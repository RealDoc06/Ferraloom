//! The play state's sockets on a few event-driven threads (Linux). Each lane
//! thread waits in one epoll set on its connections' sockets and on an
//! eventfd rung when the game queues packets or fills a slot, then reads and
//! writes whatever is ready: a handful of threads rather than a reader and a
//! writer per player, so 500 players don't cost ~20 000 thread wake-ups a
//! second. A connection does what its threads in conn.rs (`read_loop`,
//! `write_loop`) do, with the same bytes in the same order; only which
//! thread does it, and when, changes. Handshake, login and configuration
//! stay on the connection's own thread.
use super::{
    Joined, NetEvent, Out,
    conn::{
        DISCONNECT_LINGER, Due, Ended, KEEP_ALIVE_TIMEOUT, KeepAlive, PacketBody, Player, Wake,
        disconnect_packet, frame_body, read_failed,
    },
};
use horizon_protocol::connection::{BATCH_BYTES, ConnReader, ConnWriter, Connection};
use rustix::event::{EventfdFlags, Timespec, epoll, eventfd};
use std::{
    collections::{HashMap, VecDeque},
    os::fd::{AsFd, OwnedFd},
    sync::{
        Arc, Mutex, OnceLock, PoisonError,
        atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU64, AtomicUsize, Ordering},
        mpsc::{Receiver, TryRecvError},
    },
    time::{Duration, Instant},
};

/// The eventfd's token in a lane's epoll set; connections count from 1.
const RING: u64 = 0;
/// How often a lane looks at every connection's keep-alive and read timeout:
/// what a writer thread's `recv_timeout` waits.
const SWEEP: Duration = Duration::from_millis(200);
/// Reads of one connection per round, so a flood doesn't hold its lane up:
/// the rest is read on the next.
const READS_PER_ROUND: usize = 8;

/// A connection's place on a lane, until it takes it ([`Seat::serve`]).
pub(super) struct Seat {
    lane: Arc<Lane>,
    token: u64,
    /// Whether the lane has the connection on its list to write already.
    queued: Arc<AtomicBool>,
}

/// A seat on one of the lanes, which start with the first player; `None`
/// where they could not, and the connection's threads serve it instead.
pub(super) fn seat() -> Option<Seat> {
    static LANES: OnceLock<Option<Vec<Arc<Lane>>>> = OnceLock::new();
    static TOKENS: AtomicU64 = AtomicU64::new(RING + 1);
    let lanes = LANES.get_or_init(start).as_ref()?;
    let token = TOKENS.fetch_add(1, Ordering::Relaxed);
    Some(Seat {
        lane: lanes[(token % lanes.len() as u64) as usize].clone(),
        token,
        queued: Arc::default(),
    })
}

/// `HORIZON_IO_THREADS` lanes, or half the hardware threads (2 to 8): they
/// compress and encrypt every player's packets as well as wait for them.
fn lane_count() -> usize {
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get());
    std::env::var("HORIZON_IO_THREADS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or((threads / 2).clamp(2, 8))
        .clamp(1, 64)
}

fn start() -> Option<Vec<Arc<Lane>>> {
    (0..lane_count())
        .map(|i| {
            let lane = Arc::new(Lane::new().ok()?);
            let run = lane.clone();
            std::thread::Builder::new()
                .name(format!("horizon-lan-io-{i}"))
                .spawn(move || {
                    // As the writer threads: behind ticks and frames, but
                    // not left to idle time.
                    horizon_threads::behind_server_io();
                    run.run();
                })
                .ok()?;
            Some(lane)
        })
        .collect()
}

impl Seat {
    /// What the connection's [`PlayerSender`](super::conn::PlayerSender) and
    /// slots tell when the writer has something to do.
    pub fn wake(&self) -> Wake {
        let (lane, token, queued) = (self.lane.clone(), self.token, self.queued.clone());
        Arc::new(move || {
            if !queued.swap(true, Ordering::AcqRel) {
                lane.post(|inbox| inbox.ready.push(token));
            }
        })
    }
    /// Hand the connection to its lane, which tells the game it joined and
    /// serves it until it closes.
    pub fn serve(self, c: Connection, rx: Receiver<Out>, player: Player, joined: Joined) {
        let (reader, mut writer) = c.split();
        if let Err(e) = writer.set_nonblocking() {
            // Blocking, it would hold the lane up: closed, it ends at once.
            log::warn!("connection {}: {e}", player.conn);
            writer.shutdown();
        }
        let conn = Conn {
            token: self.token,
            queued: self.queued,
            reader,
            out: Outgoing::new(
                writer,
                rx,
                player.alive.clone(),
                player.latency.clone(),
                player.pending.clone(),
            ),
            joined: Some(Box::new(joined)),
            player,
            last_read: Instant::now(),
            reading: true,
            interest: None,
        };
        self.lane.post(|inbox| inbox.added.push(conn));
    }
}

/// One lane: the side other threads hand it connections and wake it through.
struct Lane {
    epoll: OwnedFd,
    ring: OwnedFd,
    /// The eventfd was written and the lane hasn't looked at its inbox since.
    rung: AtomicBool,
    inbox: Mutex<Inbox>,
}
#[derive(Default)]
struct Inbox {
    added: Vec<Conn>,
    /// Connections whose writer has something to do.
    ready: Vec<u64>,
}

impl Lane {
    fn new() -> rustix::io::Result<Self> {
        let epoll = epoll::create(epoll::CreateFlags::CLOEXEC)?;
        let ring = eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)?;
        epoll::add(
            &epoll,
            &ring,
            epoll::EventData::new_u64(RING),
            epoll::EventFlags::IN,
        )?;
        Ok(Self {
            epoll,
            ring,
            rung: AtomicBool::new(false),
            inbox: Mutex::default(),
        })
    }
    /// Change the inbox and wake the lane, unless it is awake to see it.
    fn post(&self, change: impl FnOnce(&mut Inbox)) {
        change(&mut self.inbox.lock().unwrap_or_else(PoisonError::into_inner));
        if !self.rung.swap(true, Ordering::AcqRel) {
            let _ = rustix::io::write(&self.ring, &1u64.to_ne_bytes());
        }
    }

    fn run(&self) {
        let mut conns: HashMap<u64, Conn> = HashMap::new();
        let mut events = Vec::with_capacity(256);
        let mut sweep_at = Instant::now() + SWEEP;
        // The sweep, or sooner a disconnect's linger.
        let mut due = sweep_at;
        // The connections something happened to this round.
        let mut touched = Vec::new();
        loop {
            let wait = due.saturating_duration_since(Instant::now());
            let timeout = Timespec {
                tv_sec: wait.as_secs() as _,
                tv_nsec: wait.subsec_nanos() as _,
            };
            events.clear();
            let _ = epoll::wait(
                &self.epoll,
                rustix::buffer::spare_capacity(&mut events),
                Some(&timeout),
            );
            let now = Instant::now();
            for event in &events {
                let token = event.data.u64();
                if token == RING {
                    let mut count = [0; 8];
                    let _ = rustix::io::read(&self.ring, &mut count);
                } else if let Some(conn) = conns.get_mut(&token) {
                    let flags = event.flags;
                    if flags.intersects(
                        epoll::EventFlags::IN | epoll::EventFlags::HUP | epoll::EventFlags::ERR,
                    ) {
                        conn.read(now);
                    }
                    conn.out.write(now);
                    touched.push(token);
                }
            }
            self.rung.store(false, Ordering::Release);
            let inbox =
                std::mem::take(&mut *self.inbox.lock().unwrap_or_else(PoisonError::into_inner));
            for mut conn in inbox.added {
                let token = conn.token;
                conn.start(now);
                conns.insert(token, conn);
                touched.push(token);
            }
            for token in inbox.ready {
                if let Some(conn) = conns.get_mut(&token) {
                    conn.queued.store(false, Ordering::Release);
                    conn.out.write(now);
                    touched.push(token);
                }
            }
            if now >= due {
                due = sweep_at.max(now);
                if now >= sweep_at {
                    for conn in conns.values_mut() {
                        conn.sweep(now);
                    }
                    sweep_at = now + SWEEP;
                    due = sweep_at;
                }
                // Lingering disconnects come due before the next sweep.
                touched.extend(conns.keys().copied());
            }
            touched.sort_unstable();
            touched.dedup();
            for token in touched.drain(..) {
                let Some(conn) = conns.get_mut(&token) else {
                    continue;
                };
                if let Some(at) = conn.out.linger_until() {
                    if now >= at {
                        conn.out.write(now);
                    } else {
                        due = due.min(at);
                    }
                }
                if conn.finished() {
                    let conn = conns.remove(&token).expect("looked up above");
                    if conn.interest.is_some() {
                        let _ = epoll::delete(&self.epoll, conn.socket());
                    }
                    conn.player.shared.release(conn.player.uuid);
                } else {
                    conn.watch(&self.epoll);
                }
            }
        }
    }
}

/// Where a connection's writer is.
enum Writing {
    Open,
    /// Writes what it holds, waits `linger` and closes; or closes anyway at
    /// `give_up`, for a client that never reads.
    Closing {
        linger: Duration,
        until: Option<Instant>,
        give_up: Instant,
    },
    /// Shut down.
    Done,
}

/// One connection on a lane.
struct Conn {
    token: u64,
    queued: Arc<AtomicBool>,
    reader: ConnReader,
    out: Outgoing,
    /// Sent to the game when the lane takes the connection.
    joined: Option<Box<Joined>>,
    player: Player,
    /// When the client last sent anything: the reader threads' read timeout.
    last_read: Instant,
    reading: bool,
    /// What the epoll set waits for on the socket, if it is in it.
    interest: Option<epoll::EventFlags>,
}

impl Conn {
    fn socket(&self) -> impl AsFd + '_ {
        self.reader
            .socket()
            .expect("only sockets are pooled")
            .as_fd()
    }

    /// Tell the game the player joined, then take what arrived with the end
    /// of configuration and send what was queued meanwhile.
    fn start(&mut self, now: Instant) {
        self.queued.store(false, Ordering::Release);
        let joined = self.joined.take().expect("started once");
        if self.player.events.send(NetEvent::Joined(joined)).is_ok() {
            self.read(now);
        } else {
            self.stop_reading(Ok("server closed".to_owned()), now);
        }
        self.out.write(now);
    }

    /// Forward what the client sent until the socket has nothing more for
    /// now: what `read_loop` does between blocking reads.
    fn read(&mut self, now: Instant) {
        if !self.reading {
            return;
        }
        let before = self.reader.counters.bytes_in.load(Ordering::Relaxed);
        let ended = self.read_round();
        if self.reader.counters.bytes_in.load(Ordering::Relaxed) != before {
            self.last_read = now;
        }
        if let Some(ended) = ended {
            self.stop_reading(ended, now);
        }
    }
    fn read_round(&mut self) -> Option<Ended> {
        for round in 0..=READS_PER_ROUND {
            loop {
                match self.reader.buffered() {
                    Ok(Some((id, payload))) => {
                        if let Err(ended) = self.player.receive(id, &payload) {
                            return Some(ended);
                        }
                    }
                    Ok(None) => break,
                    Err(e) => return Some(Ok(read_failed(e))),
                }
            }
            if round == READS_PER_ROUND {
                break;
            }
            match self.reader.read_some() {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => return Some(Ok(read_failed(e))),
            }
        }
        None
    }
    fn stop_reading(&mut self, ended: Ended, now: Instant) {
        self.reading = false;
        self.player.reader_ended(ended);
        self.out.write(now);
    }

    /// The read timeout and the keep-alive, which come due without any event.
    fn sweep(&mut self, now: Instant) {
        if self.reading && now.duration_since(self.last_read) > KEEP_ALIVE_TIMEOUT {
            // What a blocking read that timed out fails with.
            let timed_out = std::io::Error::from(rustix::io::Errno::AGAIN);
            self.stop_reading(Ok(timed_out.to_string()), now);
        }
        self.out.write(now);
    }

    fn finished(&self) -> bool {
        !self.reading && matches!(self.out.writing, Writing::Done)
    }

    /// Wait for what the connection needs next: the client's packets while
    /// reading, room in the socket while the writer holds bytes. Out of the
    /// set when neither, so a closed socket doesn't keep waking the lane.
    fn watch(&mut self, epoll_fd: &OwnedFd) {
        let mut want = epoll::EventFlags::empty();
        if self.reading {
            want |= epoll::EventFlags::IN;
        }
        if self.out.waits_for_room() {
            want |= epoll::EventFlags::OUT;
        }
        let want = (!want.is_empty()).then_some(want);
        if want == self.interest {
            return;
        }
        let data = epoll::EventData::new_u64(self.token);
        let changed = match (self.interest, want) {
            (None, Some(flags)) => epoll::add(epoll_fd, self.socket(), data, flags),
            (Some(_), Some(flags)) => epoll::modify(epoll_fd, self.socket(), data, flags),
            (Some(_), None) => epoll::delete(epoll_fd, self.socket()),
            (None, None) => Ok(()),
        };
        match changed {
            Ok(()) => self.interest = want,
            // Not watched, it would never be served: close it.
            Err(e) => {
                log::warn!("connection {}: epoll: {e}", self.player.conn);
                self.out.done();
                if self.reading {
                    self.reading = false;
                    self.player.reader_ended(Ok(e.to_string()));
                }
            }
        }
    }
}

/// A connection's writing side: what `write_loop` does, without blocking.
struct Outgoing {
    writer: ConnWriter,
    rx: Receiver<Out>,
    /// Bodies taken from `rx` and not framed yet, in order.
    queue: VecDeque<PacketBody>,
    /// How many of the front slot's bodies are framed.
    in_slot: usize,
    keep_alive: KeepAlive,
    alive: Arc<AtomicI64>,
    latency: Arc<AtomicI32>,
    pending: Arc<AtomicUsize>,
    writing: Writing,
}

impl Outgoing {
    /// `alive`, `latency` and `pending` as [`Player`] has them.
    fn new(
        writer: ConnWriter,
        rx: Receiver<Out>,
        alive: Arc<AtomicI64>,
        latency: Arc<AtomicI32>,
        pending: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            writer,
            rx,
            queue: VecDeque::new(),
            in_slot: 0,
            keep_alive: KeepAlive::new(),
            alive,
            latency,
            pending,
            writing: Writing::Open,
        }
    }

    /// Frame and write what the game queued, as far as the socket takes it
    /// and the slots are filled.
    fn write(&mut self, now: Instant) {
        if matches!(self.writing, Writing::Done) {
            return;
        }
        self.writer.start_batch();
        if matches!(self.writing, Writing::Open)
            && let Err(linger) = self.frame()
        {
            self.writing = Writing::Closing {
                linger,
                until: None,
                give_up: now + KEEP_ALIVE_TIMEOUT,
            };
        }
        if self.writer.finish_batch().is_err() {
            self.done();
            return;
        }
        let unsent = self.writer.unsent();
        if let Writing::Closing {
            linger,
            until,
            give_up,
        } = &mut self.writing
            && ((unsent == 0 && now >= *until.get_or_insert(now + *linger)) || now >= *give_up)
        {
            self.done();
        }
    }

    /// Frame bodies until the queue is empty, a slot isn't filled yet or the
    /// socket is behind. `Err` with how long to linger when the writer is to
    /// close after what it framed.
    fn frame(&mut self) -> Result<(), Duration> {
        loop {
            if self.writer.unsent() >= BATCH_BYTES {
                // Write it now: if the socket takes it, carry on.
                self.writer.finish_batch().map_err(|_| Duration::ZERO)?;
                self.writer.start_batch();
                if self.writer.unsent() >= BATCH_BYTES {
                    return Ok(());
                }
            }
            let Some(front) = self.queue.front() else {
                // Between what the game queued, as a writer thread checks.
                match self.keep_alive.due(&self.alive, &self.latency) {
                    Some(Due::Send(packet)) => {
                        self.writer.send(&packet).map_err(|_| Duration::ZERO)?;
                    }
                    Some(Due::TimedOut(packet)) => {
                        let _ = self.writer.send(&packet);
                        return Err(Duration::ZERO);
                    }
                    None => {}
                }
                match self.rx.try_recv() {
                    Ok(Out::Packet(body)) => self.queue.push_back(PacketBody::Owned(body)),
                    Ok(Out::Packets(bodies)) => self.queue.extend(bodies),
                    Ok(Out::Disconnect(reason)) => {
                        let _ = self.writer.send(&disconnect_packet(reason));
                        return Err(DISCONNECT_LINGER);
                    }
                    Ok(Out::Close) | Err(TryRecvError::Disconnected) => {
                        return Err(Duration::ZERO);
                    }
                    Err(TryRecvError::Empty) => return Ok(()),
                }
                continue;
            };
            let framed = match front {
                PacketBody::Slot(slot) => {
                    // Not filled: what comes before it goes out meanwhile,
                    // and its filler wakes the lane.
                    let Some(bodies) = slot.bodies.get() else {
                        return Ok(());
                    };
                    let mut framed = true;
                    while framed
                        && self.in_slot < bodies.len()
                        && self.writer.unsent() < BATCH_BYTES
                    {
                        framed = frame_body(&mut self.writer, &self.pending, &bodies[self.in_slot]);
                        self.in_slot += 1;
                    }
                    if self.in_slot == bodies.len() {
                        self.queue.pop_front();
                        self.in_slot = 0;
                    }
                    framed
                }
                body => {
                    let framed = frame_body(&mut self.writer, &self.pending, body);
                    self.queue.pop_front();
                    framed
                }
            };
            if !framed {
                return Err(Duration::ZERO);
            }
        }
    }

    fn done(&mut self) {
        self.writer.shutdown();
        self.writing = Writing::Done;
    }
    /// Bytes are waiting for room in the socket.
    fn waits_for_room(&self) -> bool {
        !matches!(self.writing, Writing::Done) && self.writer.unsent() > 0
    }
    /// When a disconnect that went out has lingered enough.
    fn linger_until(&self) -> Option<Instant> {
        match self.writing {
            Writing::Closing { until, .. } => until,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lan::conn::{Holding, PlayerSender};
    use std::{net::TcpListener, sync::mpsc::channel};

    /// A pooled writer for a player's sender, and the client's end.
    /// Encrypted and compressed when `secure`.
    fn pooled(secure: bool) -> (Outgoing, PlayerSender, Connection) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut client = Connection::connect("127.0.0.1", port, Duration::from_secs(5)).unwrap();
        let mut server = Connection::from_stream(listener.accept().unwrap().0).unwrap();
        if secure {
            server.enable_encryption(&[7; 16]);
            client.enable_encryption(&[7; 16]);
            server.set_compression(64);
            client.set_compression(64);
        }
        let (_, mut writer) = server.split();
        writer.set_nonblocking().unwrap();
        let (tx, rx) = channel();
        let pending = Arc::new(AtomicUsize::new(0));
        let sender = PlayerSender::new(tx, pending.clone());
        let out = Outgoing::new(
            writer,
            rx,
            Arc::new(AtomicI64::new(-1)),
            Arc::default(),
            pending,
        );
        (out, sender, client)
    }

    #[test]
    fn an_unfilled_slot_parks_the_writer_after_what_came_before() {
        let (mut out, sender, mut client) = pooled(true);
        let holding = Holding::start();
        sender.send(vec![1, 10]);
        let filler = sender.reserve();
        sender.send(vec![4, 40]);
        drop(holding);
        sender.flush();
        // Returns although the slot is empty, having written what is ahead of it.
        out.write(Instant::now());
        assert_eq!(client.recv().unwrap(), (1, vec![10]));
        assert_eq!(out.queue.len(), 2);
        filler.fill(vec![PacketBody::Run {
            bytes: vec![2, 20, 3, 30],
            ends: vec![2, 4],
        }]);
        out.write(Instant::now());
        for packet in [(2, vec![20]), (3, vec![30]), (4, vec![40])] {
            assert_eq!(client.recv().unwrap(), packet);
        }
        assert!(out.queue.is_empty());
        assert_eq!(sender.pending.load(Ordering::Relaxed), 0);
    }

    /// More than the socket takes: the writer keeps the rest, never blocks,
    /// and the client gets every packet in order.
    #[test]
    fn a_full_socket_holds_the_rest_for_later() {
        let (mut out, sender, mut client) = pooled(false);
        let mut seed = 1u32;
        let bodies: Vec<Vec<u8>> = (0..400u32)
            .map(|i| {
                let mut body = vec![(i % 100) as u8];
                body.extend((0..48 << 10).map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (seed >> 24) as u8
                }));
                body
            })
            .collect();
        let holding = Holding::start();
        sender.send_many(bodies.clone());
        drop(holding);
        sender.flush();
        out.write(Instant::now());
        assert!(out.writer.unsent() > 0 && !out.queue.is_empty());
        let count = bodies.len();
        let reader = std::thread::spawn(move || {
            (0..count)
                .map(|_| {
                    let (id, payload) = client.recv().unwrap();
                    let mut body = vec![id as u8];
                    body.extend(payload);
                    body
                })
                .collect::<Vec<_>>()
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        while out.writer.unsent() > 0 || !out.queue.is_empty() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
            out.write(Instant::now());
        }
        assert!(reader.join().unwrap() == bodies);
        assert_eq!(sender.pending.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_disconnect_lingers_then_closes() {
        let (mut out, sender, mut client) = pooled(false);
        sender.disconnect(horizon_world::nbt::Tag::String("bye".into()));
        let now = Instant::now();
        out.write(now);
        assert!(matches!(out.writing, Writing::Closing { .. }));
        out.write(now + DISCONNECT_LINGER / 2);
        assert!(matches!(out.writing, Writing::Closing { .. }));
        out.write(now + DISCONNECT_LINGER);
        assert!(matches!(out.writing, Writing::Done));
        let (id, _) = client.recv().unwrap();
        let disconnect = disconnect_packet(horizon_world::nbt::Tag::String("bye".into()));
        assert_eq!(id, i32::from(disconnect[0]));
        assert!(client.recv().is_err());
    }
}
