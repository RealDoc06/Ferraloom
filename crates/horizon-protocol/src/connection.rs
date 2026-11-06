//! A blocking connection with vanilla's pipeline: optional AES/CFB8
//! encryption, VarInt21 framing and threshold compression. After login it
//! splits into a reader (for a receive thread) and a shareable writer, which
//! over TCP can also go non-blocking for an event loop that serves many
//! connections ([`ConnReader::read_some`], [`ConnWriter::set_nonblocking`]).
//! It runs over TCP, or over an in-memory pipe between two threads of one process
//! (vanilla's `LocalChannel`, which an integrated server's player uses).
use crate::{Error, Result, buf::Reader, crypto::Cfb8, frame};
use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicI64, Ordering},
    },
    time::{Duration, Instant},
};

/// One direction of a [`Pipe`]: the bytes written and not read yet, and
/// whether either end closed it.
#[derive(Default)]
struct Channel {
    state: Mutex<(std::collections::VecDeque<u8>, bool)>,
    ready: Condvar,
}
impl Channel {
    fn close(&self) {
        self.state.lock().unwrap().1 = true;
        self.ready.notify_all();
    }
}
/// The channels of one end; dropping the end's last clone closes them, as
/// a socket closes with its process.
struct End {
    incoming: Arc<Channel>,
    outgoing: Arc<Channel>,
}
impl Drop for End {
    fn drop(&mut self) {
        self.incoming.close();
        self.outgoing.close();
    }
}
/// One end of an in-memory byte pipe: what one end writes the other reads.
pub struct Pipe {
    end: Arc<End>,
    /// The read timeout (`set_read_timeout`).
    timeout: Mutex<Option<Duration>>,
}
impl Pipe {
    /// Two connected ends.
    pub fn pair() -> (Self, Self) {
        let (a, b) = (Arc::new(Channel::default()), Arc::new(Channel::default()));
        let end = |incoming, outgoing| Self {
            end: Arc::new(End { incoming, outgoing }),
            timeout: Mutex::new(None),
        };
        (end(a.clone(), b.clone()), end(b, a))
    }
    fn try_clone(&self) -> Self {
        Self {
            end: self.end.clone(),
            timeout: Mutex::new(*self.timeout.lock().unwrap()),
        }
    }
    /// Close both directions: reads at either end then see the end of the stream.
    pub fn shutdown(&self) {
        self.end.incoming.close();
        self.end.outgoing.close();
    }
}
impl Read for Pipe {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let deadline = self.timeout.lock().unwrap().map(|t| Instant::now() + t);
        let mut state = self.end.incoming.state.lock().unwrap();
        while state.0.is_empty() && !state.1 {
            state = match deadline {
                None => self.end.incoming.ready.wait(state).unwrap(),
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(std::io::ErrorKind::TimedOut.into());
                    }
                    self.end.incoming.ready.wait_timeout(state, left).unwrap().0
                }
            };
        }
        let n = out.len().min(state.0.len());
        for (o, b) in out.iter_mut().zip(state.0.drain(..n)) {
            *o = b;
        }
        Ok(n)
    }
}
impl Write for Pipe {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let mut state = self.end.outgoing.state.lock().unwrap();
        if state.1 {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        state.0.extend(data);
        self.end.outgoing.ready.notify_all();
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// What a connection runs over.
enum Stream {
    Tcp(TcpStream),
    Memory(Pipe),
}
impl Stream {
    fn try_clone(&self) -> std::io::Result<Self> {
        Ok(match self {
            Self::Tcp(s) => Self::Tcp(s.try_clone()?),
            Self::Memory(p) => Self::Memory(p.try_clone()),
        })
    }
    fn set_read_timeout(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        match self {
            Self::Tcp(s) => s.set_read_timeout(timeout),
            Self::Memory(p) => {
                *p.timeout.lock().unwrap() = timeout;
                Ok(())
            }
        }
    }
    fn shutdown(&self) {
        match self {
            Self::Tcp(s) => {
                let _ = s.shutdown(Shutdown::Both);
            }
            Self::Memory(p) => p.shutdown(),
        }
    }
}
impl Read for Stream {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(s) => s.read(out),
            Self::Memory(p) => p.read(out),
        }
    }
}
impl Write for Stream {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(s) => s.write(data),
            Self::Memory(p) => p.write(data),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(s) => s.flush(),
            Self::Memory(p) => p.flush(),
        }
    }
}

/// Packets received/sent and bytes in/out, for the debug screen.
#[derive(Debug, Default)]
pub struct Counters {
    pub received: AtomicI64,
    pub sent: AtomicI64,
    pub bytes_in: AtomicI64,
    pub bytes_out: AtomicI64,
}

pub struct ConnReader {
    stream: Stream,
    buffer: Vec<u8>,
    decrypt: Option<Cfb8>,
    pub threshold: Option<usize>,
    pub counters: Arc<Counters>,
}
pub struct ConnWriter {
    stream: Stream,
    encrypt: Option<Cfb8>,
    pub threshold: Option<usize>,
    counters: Arc<Counters>,
    /// Frames not written yet: see [`Self::start_batch`]. The first
    /// `written` bytes went out already (a non-blocking partial write).
    unsent: Vec<u8>,
    written: usize,
    batching: bool,
    /// Writes take what the socket accepts now and keep the rest.
    nonblocking: bool,
}
/// `InetAddress.getByName` under Java's default `java.net.preferIPv6Addresses=false`: IPv4
/// addresses first, whatever order the system resolver gives (macOS lists IPv6 first). Vanilla
/// connects to that first address only; the rest stay here as fallbacks.
fn ipv4_first(mut addrs: Vec<SocketAddr>) -> Vec<SocketAddr> {
    addrs.sort_by_key(SocketAddr::is_ipv6);
    addrs
}
/// Both halves before the split.
pub struct Connection {
    pub reader: ConnReader,
    pub writer: ConnWriter,
}
impl Connection {
    pub fn connect(host: &str, port: u16, timeout: Duration) -> Result<Self> {
        let addrs: Vec<_> = (host, port)
            .to_socket_addrs()
            .map_err(|e| Error::new(format!("Unknown host {host}: {e}")))?
            .collect();
        Self::connect_addresses(host, addrs, timeout)
    }
    /// Minecraft's SRV redirect applies to the socket destination only. Callers
    /// keep the player's original host and port in the protocol handshake.
    pub fn connect_server(host: &str, port: u16, timeout: Duration) -> Result<Self> {
        let addrs = crate::resolver::resolve(host, port)?;
        Self::connect_addresses(host, addrs, timeout)
    }
    fn connect_addresses(host: &str, addrs: Vec<SocketAddr>, timeout: Duration) -> Result<Self> {
        // Report the first address's failure: it is the one vanilla tries, and the IPv6
        // fallbacks after it often fail only because the client has no IPv6 route.
        let mut first = None;
        for addr in ipv4_first(addrs) {
            match TcpStream::connect_timeout(&addr, timeout) {
                Ok(stream) => return Self::from_stream(stream),
                Err(e) => {
                    first.get_or_insert_with(|| Error::new(format!("Connection refused: {e}")));
                }
            }
        }
        Err(first.unwrap_or_else(|| Error::new(format!("Unknown host {host}"))))
    }
    pub fn from_stream(stream: TcpStream) -> Result<Self> {
        stream.set_nodelay(true)?;
        Self::over(Stream::Tcp(stream))
    }
    /// A connection over one end of an in-memory [`Pipe`].
    pub fn local(pipe: Pipe) -> Self {
        Self::over(Stream::Memory(pipe)).expect("a pipe end clones")
    }
    fn over(stream: Stream) -> Result<Self> {
        let counters = Arc::new(Counters::default());
        let write = stream.try_clone()?;
        Ok(Self {
            reader: ConnReader {
                stream,
                buffer: Vec::new(),
                decrypt: None,
                threshold: None,
                counters: counters.clone(),
            },
            writer: ConnWriter {
                stream: write,
                encrypt: None,
                threshold: None,
                counters,
                unsent: Vec::new(),
                written: 0,
                batching: false,
                nonblocking: false,
            },
        })
    }
    /// `Connection.isMemoryConnection`: through a [`Pipe`] rather than a socket.
    pub fn is_memory(&self) -> bool {
        matches!(self.reader.stream, Stream::Memory(_))
    }
    /// ClientboundLoginCompressionPacket: negative thresholds disable compression.
    pub fn set_compression(&mut self, threshold: i32) {
        let t = usize::try_from(threshold).ok();
        self.reader.threshold = t;
        self.writer.threshold = t;
    }
    /// Connection.setEncryptionKey: both directions keyed with the shared secret.
    pub fn enable_encryption(&mut self, secret: &[u8; 16]) {
        let mut decrypt = Cfb8::new(secret, secret);
        // Bytes already read past the last packet were sent encrypted.
        decrypt.decrypt(&mut self.reader.buffer);
        self.reader.decrypt = Some(decrypt);
        self.writer.encrypt = Some(Cfb8::new(secret, secret));
    }
    pub fn send(&mut self, body: &[u8]) -> Result<()> {
        self.writer.send(body)
    }
    pub fn recv(&mut self) -> Result<(i32, Vec<u8>)> {
        self.reader.recv()
    }
    pub fn split(self) -> (ConnReader, ConnWriter) {
        (self.reader, self.writer)
    }
}
impl ConnReader {
    /// Block until a whole packet arrives: (id, payload).
    pub fn recv(&mut self) -> Result<(i32, Vec<u8>)> {
        loop {
            if let Some(packet) = self.buffered()? {
                return Ok(packet);
            }
            if self.read_chunk()? == 0 {
                return Err(closed());
            }
        }
    }
    /// The next packet already read, if a whole one has arrived.
    pub fn buffered(&mut self) -> Result<Option<(i32, Vec<u8>)>> {
        let Some(frame) = frame::split(&mut self.buffer)? else {
            return Ok(None);
        };
        let body = frame::decompress(frame, self.threshold)?;
        let mut r = Reader::new(&body);
        let id = r.varint()?;
        let start = r.position();
        self.counters.received.fetch_add(1, Ordering::Relaxed);
        Ok(Some((id, body[start..].to_vec())))
    }
    /// One read on a non-blocking stream, for [`Self::buffered`]: false when
    /// the peer has sent nothing more for now.
    pub fn read_some(&mut self) -> Result<bool> {
        loop {
            return match self.read_chunk() {
                Ok(0) => Err(closed()),
                Ok(_) => Ok(true),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => Err(e.into()),
            };
        }
    }
    /// Read what the stream has into the buffer, decrypted: how many bytes.
    fn read_chunk(&mut self) -> std::io::Result<usize> {
        let mut chunk = [0u8; 16384];
        let n = self.stream.read(&mut chunk)?;
        self.counters
            .bytes_in
            .fetch_add(n as i64, Ordering::Relaxed);
        let data = &mut chunk[..n];
        if let Some(c) = &mut self.decrypt {
            c.decrypt(data);
        }
        self.buffer.extend_from_slice(data);
        Ok(n)
    }
    /// The socket under a TCP connection.
    pub fn socket(&self) -> Option<&TcpStream> {
        match &self.stream {
            Stream::Tcp(s) => Some(s),
            Stream::Memory(_) => None,
        }
    }
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> Result<()> {
        Ok(self.stream.set_read_timeout(timeout)?)
    }
}
impl ConnWriter {
    /// Send one packet body (VarInt id + payload).
    pub fn send(&mut self, body: &[u8]) -> Result<()> {
        self.send_contents(&frame::contents(body, self.threshold)?)
    }
    /// Send a frame whose contents [`frame::contents`] made with this
    /// writer's threshold.
    pub fn send_contents(&mut self, contents: &[u8]) -> Result<()> {
        let start = self.unsent.len();
        frame::prefix_to(&mut self.unsent, contents);
        if let Some(c) = &mut self.encrypt {
            c.encrypt(&mut self.unsent[start..]);
        }
        self.counters.sent.fetch_add(1, Ordering::Relaxed);
        self.counters
            .bytes_out
            .fetch_add((self.unsent.len() - start) as i64, Ordering::Relaxed);
        if self.batching && self.unsent() < BATCH_BYTES {
            return Ok(());
        }
        self.write_unsent()
    }
    /// Bytes framed and not taken by the socket yet.
    pub fn unsent(&self) -> usize {
        self.unsent.len() - self.written
    }
    /// From here on writes take what a non-blocking socket accepts and keep
    /// the rest for the next ([`Self::unsent`]), so a full socket neither
    /// blocks nor fails the connection. Errors on an in-memory pipe.
    pub fn set_nonblocking(&mut self) -> Result<()> {
        let Stream::Tcp(s) = &self.stream else {
            return Err(Error::new("not a socket"));
        };
        s.set_nonblocking(true)?;
        self.nonblocking = true;
        Ok(())
    }
    /// The frames sent from here to [`Self::finish_batch`] go out together,
    /// with fewer system calls and packets on the network: the same bytes
    /// in the same order.
    pub fn start_batch(&mut self) {
        self.batching = true;
    }
    /// Write what the batch held.
    pub fn finish_batch(&mut self) -> Result<()> {
        self.batching = false;
        self.write_unsent()
    }
    fn write_unsent(&mut self) -> Result<()> {
        if self.unsent() == 0 {
            return Ok(());
        }
        let written = if self.nonblocking {
            self.write_available()
        } else {
            self.stream.write_all(&self.unsent)
        };
        if written.is_err() || !self.nonblocking || self.unsent() == 0 {
            self.unsent.clear();
            self.written = 0;
            // A chunk's frame doesn't stay for every connection's every tick.
            self.unsent.shrink_to(BATCH_BYTES);
        } else if self.written >= BATCH_BYTES {
            // A connection that never quite catches up doesn't keep what went out.
            self.unsent.drain(..self.written);
            self.written = 0;
        }
        written?;
        Ok(())
    }
    /// Write until the socket is full or nothing is left.
    fn write_available(&mut self) -> std::io::Result<()> {
        use std::io::ErrorKind;
        while self.written < self.unsent.len() {
            match self.stream.write(&self.unsent[self.written..]) {
                Ok(0) => return Err(ErrorKind::WriteZero.into()),
                Ok(n) => self.written += n,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    pub fn shutdown(&self) {
        self.stream.shutdown();
    }
}

/// What a batch holds before it writes what it has: a batch takes no more
/// memory than this per connection.
pub const BATCH_BYTES: usize = 64 << 10;

fn closed() -> Error {
    Error::new("Connection closed by the server")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn ipv4_first_keeps_each_familys_order() {
        let a = |s: &str| s.parse::<SocketAddr>().unwrap();
        let resolved = vec![
            a("[2001:db8::1]:25565"),
            a("192.0.2.1:25565"),
            a("[2001:db8::2]:25565"),
            a("10.0.0.1:25565"),
        ];
        assert_eq!(
            ipv4_first(resolved),
            [
                a("192.0.2.1:25565"),
                a("10.0.0.1:25565"),
                a("[2001:db8::1]:25565"),
                a("[2001:db8::2]:25565"),
            ]
        );
    }

    fn peer(c: &Connection) -> SocketAddr {
        match &c.reader.stream {
            Stream::Tcp(s) => s.peer_addr().unwrap(),
            Stream::Memory(_) => panic!("not a socket"),
        }
    }

    /// Binds `[::1]`, or `None` where the machine has no IPv6 loopback.
    fn ipv6_loopback() -> Option<TcpListener> {
        let listener = TcpListener::bind("[::1]:0");
        if listener.is_err() {
            eprintln!("skipped: no IPv6 loopback");
        }
        listener.ok()
    }

    #[test]
    fn connects_to_ipv4_although_the_resolver_lists_ipv6_first() {
        // Both listen, as with a server whose IPv6 address accepts and then resets.
        let Some(v6_listener) = ipv6_loopback() else {
            return;
        };
        let v4_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let v6 = v6_listener.local_addr().unwrap();
        let v4 = v4_listener.local_addr().unwrap();
        let c =
            Connection::connect_addresses("test", vec![v6, v4], Duration::from_secs(5)).unwrap();
        assert_eq!(peer(&c), v4);
    }

    #[test]
    fn falls_back_to_ipv6_when_ipv4_refuses() {
        let Some(v6_listener) = ipv6_loopback() else {
            return;
        };
        let v6 = v6_listener.local_addr().unwrap();
        let closed = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let c = Connection::connect_addresses("test", vec![v6, closed], Duration::from_secs(5))
            .unwrap();
        assert_eq!(peer(&c), v6);
    }

    /// Encrypted, compressed frames in a batch are the bytes they'd be sent
    /// one by one, nothing is left behind and a batch's size is bounded.
    #[test]
    fn a_batch_writes_the_same_bytes_as_frames_sent_one_by_one() {
        let bodies: Vec<Vec<u8>> = (0..60u32)
            .map(|i| vec![i as u8; (i as usize * 97) % 9000 + 1])
            .collect();
        let stream = |batched: bool| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let reader = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut all = Vec::new();
                std::io::Read::read_to_end(&mut stream, &mut all).unwrap();
                all
            });
            let mut c = Connection::connect("127.0.0.1", port, Duration::from_secs(5)).unwrap();
            c.enable_encryption(&[7; 16]);
            c.set_compression(64);
            let (read_half, mut writer) = c.split();
            if batched {
                writer.start_batch();
            }
            for body in &bodies {
                writer.send(body).unwrap();
                assert!(writer.unsent.len() < BATCH_BYTES);
            }
            writer.finish_batch().unwrap();
            writer.send(b"after").unwrap();
            assert!(writer.unsent.is_empty());
            writer.shutdown();
            drop((read_half, writer));
            reader.join().unwrap()
        };
        assert_eq!(stream(true), stream(false));
    }

    /// A non-blocking writer keeps what a full socket doesn't take, and the
    /// stream is the bytes a blocking one writes.
    #[test]
    fn a_full_socket_keeps_the_rest_without_blocking() {
        let bodies: Vec<Vec<u8>> = (0..300u32)
            .map(|i| {
                (0..64 << 10)
                    .map(|j: u32| (i ^ j.wrapping_mul(2_654_435_761)) as u8)
                    .collect()
            })
            .collect();
        let stream = |nonblocking: bool| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let mut c = Connection::connect("127.0.0.1", port, Duration::from_secs(5)).unwrap();
            let (mut peer, _) = listener.accept().unwrap();
            c.enable_encryption(&[7; 16]);
            let (_, mut writer) = c.split();
            if !nonblocking {
                let reader = std::thread::spawn(move || {
                    let mut all = Vec::new();
                    peer.read_to_end(&mut all).unwrap();
                    all
                });
                bodies.iter().for_each(|b| writer.send(b).unwrap());
                writer.shutdown();
                return reader.join().unwrap();
            }
            writer.set_nonblocking().unwrap();
            writer.start_batch();
            bodies.iter().for_each(|b| writer.send(b).unwrap());
            writer.finish_batch().unwrap();
            assert!(writer.unsent() > 0, "the socket took ~19 MiB at once");
            let reader = std::thread::spawn(move || {
                let mut all = Vec::new();
                peer.read_to_end(&mut all).unwrap();
                all
            });
            while writer.unsent() > 0 {
                std::thread::sleep(Duration::from_millis(1));
                writer.finish_batch().unwrap();
            }
            writer.shutdown();
            reader.join().unwrap()
        };
        assert!(stream(true) == stream(false));
    }

    /// Non-blocking reads give a packet only once all of it arrived.
    #[test]
    fn a_partial_frame_waits_for_the_rest() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        client.set_nodelay(true).unwrap();
        let (mut reader, _) = Connection::from_stream(listener.accept().unwrap().0)
            .unwrap()
            .split();
        reader.socket().unwrap().set_nonblocking(true).unwrap();
        assert!(!reader.read_some().unwrap());
        let mut frames = frame::encode(&[3, 1, 2, 3], None).unwrap();
        frames.extend(frame::encode(&[4, 5], None).unwrap());
        // Byte by byte: each read takes what arrived, the packets come whole.
        let mut got = Vec::new();
        for byte in &frames {
            client.write_all(std::slice::from_ref(byte)).unwrap();
            while !reader.read_some().unwrap() {
                std::thread::yield_now();
            }
            got.extend(std::iter::from_fn(|| reader.buffered().unwrap()));
        }
        assert_eq!(got, [(3, vec![1, 2, 3]), (4, vec![5])]);
        drop(client);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match reader.read_some() {
                Ok(false) => assert!(Instant::now() < deadline),
                Ok(true) => panic!("no more bytes were sent"),
                Err(e) => break assert_eq!(e.0, "Connection closed by the server"),
            }
        }
    }

    #[test]
    fn loopback_with_compression_and_encryption() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut c = Connection::from_stream(stream).unwrap();
            let (id, payload) = c.recv().unwrap();
            assert_eq!((id, payload.as_slice()), (0, &b"plain"[..]));
            c.enable_encryption(&[7; 16]);
            c.set_compression(64);
            let (id, payload) = c.recv().unwrap();
            assert_eq!((id, payload.len()), (5, 1000));
            let mut body = vec![9];
            body.extend_from_slice(b"reply");
            c.send(&body).unwrap();
        });
        let mut c = Connection::connect("127.0.0.1", port, Duration::from_secs(5)).unwrap();
        let mut body = vec![0];
        body.extend_from_slice(b"plain");
        c.send(&body).unwrap();
        c.enable_encryption(&[7; 16]);
        c.set_compression(64);
        let mut big = vec![5];
        big.extend(std::iter::repeat_n(3u8, 1000));
        c.send(&big).unwrap();
        let (id, payload) = c.recv().unwrap();
        assert_eq!((id, payload.as_slice()), (9, &b"reply"[..]));
        server.join().unwrap();
    }

    #[test]
    fn local_pipe_carries_packets_and_closes() {
        let (a, b) = Pipe::pair();
        let server = std::thread::spawn(move || {
            let mut c = Connection::local(b);
            c.set_compression(64);
            let (id, payload) = c.recv().unwrap();
            assert_eq!((id, payload.len()), (5, 1000));
            c.send(&[9, 1, 2]).unwrap();
            let (_, writer) = c.split();
            writer.shutdown();
        });
        let mut c = Connection::local(a);
        c.set_compression(64);
        let mut big = vec![5];
        big.extend(std::iter::repeat_n(3u8, 1000));
        c.send(&big).unwrap();
        assert_eq!(c.recv().unwrap(), (9, vec![1, 2]));
        assert_eq!(c.reader.counters.sent.load(Ordering::Relaxed), 1);
        assert_eq!(c.reader.counters.received.load(Ordering::Relaxed), 1);
        server.join().unwrap();
        assert!(c.recv().is_err());
    }

    #[test]
    fn dropping_a_pipe_end_closes_it() {
        let (a, b) = Pipe::pair();
        let mut c = Connection::local(a);
        drop(b);
        assert!(c.recv().is_err());
        assert!(c.send(&[1]).is_err());
    }

    #[test]
    fn local_pipe_read_times_out() {
        let (a, _b) = Pipe::pair();
        let mut c = Connection::local(a);
        c.reader
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        assert!(c.recv().is_err());
    }

    /// Encrypted bytes that arrive in the same read as the last plain packet
    /// are decrypted once encryption is enabled.
    #[test]
    fn encrypted_bytes_read_before_enabling_encryption() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (sent, both_sent) = std::sync::mpsc::channel();
        let client = std::thread::spawn(move || {
            let mut c = Connection::connect("127.0.0.1", port, Duration::from_secs(5)).unwrap();
            c.send(&[0, 1, 2]).unwrap();
            c.enable_encryption(&[7; 16]);
            c.send(&[5, 4, 3]).unwrap();
            sent.send(c).unwrap();
        });
        let (stream, _) = listener.accept().unwrap();
        let mut c = Connection::from_stream(stream).unwrap();
        let _keep_open = both_sent.recv().unwrap();
        assert_eq!(c.recv().unwrap(), (0, vec![1, 2]));
        c.enable_encryption(&[7; 16]);
        assert_eq!(c.recv().unwrap(), (5, vec![4, 3]));
        client.join().unwrap();
    }
}
