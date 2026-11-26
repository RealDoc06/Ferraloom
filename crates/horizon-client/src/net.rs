//! Multiplayer session: a network thread that connects to a server, logs in,
//! runs the configuration phase and then forwards decoded play packets to the
//! game thread. Keep-alives are answered on the network thread; pings wait for
//! the game thread, like ClientCommonPacketListenerImpl.handlePing, so a pong
//! cannot overtake an earlier world update or teleport acknowledgement.
//! Online-mode servers need the Microsoft account the launcher signed in
//! (crate::auth): the login joins the server through the session service and
//! encrypts the connection (ClientHandshakePacketListenerImpl.handleHello).
use horizon_protocol::text::Language;

/// The language a session resolves server-sent components with: the selected
/// one as `lang` loaded it (`en_us` under it, packs included), so a menu title
/// the server sends as a key follows the language screen. A process with no
/// language set (a dedicated server's log) reads en_us.json.
pub fn load_language(resources: &std::path::Path) -> Language {
    if let Some(lang) = crate::lang::current() {
        return lang.map().clone();
    }
    std::fs::read(resources.join("assets/minecraft/lang/en_us.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
