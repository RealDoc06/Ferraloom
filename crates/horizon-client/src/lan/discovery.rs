//! LAN game discovery: LanServerPinger announces a hosted world on the
//! multicast group 224.0.2.60:4445 every 1.5 seconds as
//! `[MOTD]text[/MOTD][AD]port[/AD]`, and LanServerDetection collects those
//! announcements for the multiplayer screen.
use std::{
    io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

pub const GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 2, 60);
pub const PORT: u16 = 4445;
/// LanServerPinger sleeps 1500 ms between announcements.
const INTERVAL: Duration = Duration::from_millis(1500);

/// LanServerPinger.createPingString.
pub fn ping_string(motd: &str, port: u16) -> String {
    ping_string_text(motd, &port.to_string())
}
/// LanServerPinger.createPingString with the address as text.
pub fn ping_string_text(motd: &str, address: &str) -> String {
    format!("[MOTD]{motd}[/MOTD][AD]{address}[/AD]")
}

/// Announces one hosted world until dropped.
pub struct Pinger {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Pinger {
    pub fn start(motd: String, port: u16) -> io::Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", 0))?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let message = ping_string(&motd, port);
        let thread = std::thread::Builder::new()
            .name("horizon-lan-ping".into())
            .spawn(move || {
                let target = SocketAddr::V4(SocketAddrV4::new(GROUP, PORT));
                while !flag.load(Ordering::Relaxed) {
                    let _ = socket.send_to(message.as_bytes(), target);
                    let mut slept = Duration::ZERO;
                    while slept < INTERVAL && !flag.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(50));
                        slept += Duration::from_millis(50);
                    }
                }
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for Pinger {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
