//! What every area's natives share: the host they reach, the queue for
//! calls from other threads, and the wall between Rust and Java.
//!
//! A call into Java that can run plugin code lends the host for its length
//! ([`lend`]): a native called on that thread during it reaches the game
//! through it, as the call's own continuation. Anywhere else (a plugin's
//! async task, the chat thread, the server thread between calls) there is no
//! host: messages, log lines and shutdown are queued ([`Deferred`]) for the
//! start of the next tick, and reads or changes of the game throw
//! `IllegalStateException`, as Spigot's AsyncCatcher does.
use crate::{PlayerId, PluginHost};
use jni::{
    JNIEnv, NativeMethod,
    objects::JString,
    sys::{JNI_FALSE, JNI_TRUE, jboolean, jlong},
};
use std::{
    cell::Cell,
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Mutex, OnceLock},
};

type HostPtr = *mut (dyn PluginHost + 'static);

thread_local! {
    /// The host lent to the call into Java running on this thread, if any.
    static HOST: Cell<Option<HostPtr>> = const { Cell::new(None) };
}

/// Lends `host` to the natives this thread calls during `f`; the previous
/// one (an outer call's) is back after it.
pub(crate) fn lend<R>(host: &mut dyn PluginHost, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<HostPtr>);
    impl Drop for Restore {
        fn drop(&mut self) {
            HOST.set(self.0);
        }
    }
    let ptr: *mut (dyn PluginHost + '_) = host;
    // SAFETY: only the lifetime is erased. The pointer is used on this
    // thread only, by `with_host` inside `f`, while `host` stays borrowed
    // by this call; `Restore` takes it away again before `lend` returns.
    let ptr = unsafe { std::mem::transmute::<*mut (dyn PluginHost + '_), HostPtr>(ptr) };
    let _restore = Restore(HOST.replace(Some(ptr)));
    f()
}

/// The lent host for one native's work. It is taken out while in use, so a
/// native can never reach it twice at once.
pub(crate) fn with_host<R>(f: impl FnOnce(&mut dyn PluginHost) -> R) -> Option<R> {
    struct Restore(HostPtr);
    impl Drop for Restore {
        fn drop(&mut self) {
            HOST.set(Some(self.0));
        }
    }
    let restore = Restore(HOST.take()?);
    // SAFETY: `lend` keeps the host borrowed for as long as it is in HOST,
    // and HOST is empty while this reference lives.
    Some(f(unsafe { &mut *restore.0 }))
}

/// What a plugin sent from where no host was lent, for the start of the next tick.
#[derive(Debug, PartialEq)]
pub(crate) enum Deferred {
    Message { player: PlayerId, text: String },
    Console(String),
    Shutdown,
}

static OUTBOX: Mutex<Vec<Deferred>> = Mutex::new(Vec::new());

pub(crate) fn defer(d: Deferred) {
    OUTBOX.lock().unwrap_or_else(|e| e.into_inner()).push(d);
}

/// Hands the queued messages to the host, in the order they were sent.
pub(crate) fn drain_deferred(host: &mut dyn PluginHost) {
    let queued = std::mem::take(&mut *OUTBOX.lock().unwrap_or_else(|e| e.into_inner()));
    for d in queued {
        match d {
            Deferred::Message { player, text } => host.send_message(player, &text),
            Deferred::Console(text) => host.console_message(&text),
            Deferred::Shutdown => host.shutdown(),
        }
    }
}

/// Where the bridge's log lines go: the server's log, from any thread.
pub(crate) static LOG: OnceLock<fn(&str, &str, &str)> = OnceLock::new();

pub(crate) fn log(level: &str, thread: &str, message: &str) {
    match LOG.get() {
        Some(sink) => sink(level, thread, message),
        None => eprintln!("[{thread}/{level}]: {message}"),
    }
}

/// `UUID`'s two halves as one value.
pub(crate) fn player_id(most: jlong, least: jlong) -> PlayerId {
    (u128::from(most as u64) << 64) | u128::from(least as u64)
}

/// The two halves of a player's UUID, as `new UUID(most, least)` takes them.
pub(crate) fn halves(id: PlayerId) -> (jlong, jlong) {
    ((id >> 64) as u64 as jlong, id as u64 as jlong)
}

pub(crate) enum Fail {
    /// Not on the server thread inside a call: `IllegalStateException` naming the API method.
    NoHost(&'static str),
    /// A JNI call failed; its Java exception, if any, is pending already.
    Jni,
}

impl From<jni::errors::Error> for Fail {
    fn from(_: jni::errors::Error) -> Self {
        Fail::Jni
    }
}

/// A native's body: its failures become Java exceptions and a panic never
/// unwinds into the JVM.
pub(crate) fn guard<R: Copy>(
    env: &mut JNIEnv,
    default: R,
    f: impl FnOnce(&mut JNIEnv) -> Result<R, Fail>,
) -> R {
    match catch_unwind(AssertUnwindSafe(|| f(env))) {
        Ok(Ok(r)) => r,
        Ok(Err(Fail::NoHost(what))) => {
            let message =
                format!("{what} is answered on the server thread only (schedule a sync task)");
            let _ = env.throw_new("java/lang/IllegalStateException", message);
            default
        }
        Ok(Err(Fail::Jni)) => default,
        Err(panic) => {
            let what = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            let _ = env.throw_new(
                "java/lang/RuntimeException",
                format!("Ferraloom panicked: {what}"),
            );
            default
        }
    }
}

pub(crate) fn string(env: &mut JNIEnv, s: &JString) -> Result<String, Fail> {
    Ok(env.get_string(s)?.into())
}

pub(crate) fn jbool(b: bool) -> jboolean {
    if b { JNI_TRUE } else { JNI_FALSE }
}

/// Binds a Java class's `native` methods: `(name, JNI signature, function)`.
pub(crate) fn register(
    env: &mut JNIEnv,
    class: &str,
    methods: &[(&str, &str, *mut c_void)],
) -> jni::errors::Result<()> {
    let methods: Vec<NativeMethod> = methods
        .iter()
        .map(|&(name, sig, fn_ptr)| NativeMethod {
            name: name.into(),
            sig: sig.into(),
            fn_ptr,
        })
        .collect();
    let class = env.find_class(class)?;
    env.register_native_methods(&class, &methods)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Location, PlayerHost, ServerHost, WorldHost};

    /// A host that records what it is told.
    #[derive(Default)]
    struct Host {
        said: Vec<String>,
        stopped: bool,
    }
    impl PlayerHost for Host {
        fn send_message(&mut self, player: PlayerId, message: &str) {
            self.said.push(format!("{player:x}: {message}"));
        }
        fn location(&mut self, _: PlayerId) -> Option<Location> {
            None
        }
        fn teleport(&mut self, _: PlayerId, _: Location) -> bool {
            false
        }
    }
    impl WorldHost for Host {
        fn world_time(&mut self, _: usize, _: bool) -> i64 {
            0
        }
        fn block_at(&mut self, _: usize, _: [i32; 3]) -> Option<String> {
            None
        }
    }
    impl ServerHost for Host {
        fn console_message(&mut self, message: &str) {
            self.said.push(format!("console: {message}"));
        }
        fn dispatch_vanilla(&mut self, _: &str) -> bool {
            false
        }
        fn shutdown(&mut self) {
            self.stopped = true;
        }
    }

    #[test]
    fn uuid_halves_round_trip() {
        let id = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210_u128;
        let (most, least) = halves(id);
        assert_eq!(most, 0x0123_4567_89ab_cdef);
        assert_eq!(least as u64, 0xfedc_ba98_7654_3210);
        assert_eq!(player_id(most, least), id);
    }

    #[test]
    fn the_host_is_lent_for_the_call_only_and_nests() {
        assert!(with_host(|_| ()).is_none());
        let mut outer = Host::default();
        lend(&mut outer, || {
            with_host(|h| h.console_message("outer")).unwrap();
            // Taken while in use: a second reach finds nothing.
            with_host(|_| assert!(with_host(|_| ()).is_none())).unwrap();
            let mut inner = Host::default();
            lend(&mut inner, || {
                with_host(|h| h.console_message("inner")).unwrap()
            });
            assert_eq!(inner.said, ["console: inner"]);
            with_host(|h| h.console_message("outer again")).unwrap();
        });
        assert!(with_host(|_| ()).is_none());
        assert_eq!(outer.said, ["console: outer", "console: outer again"]);
    }

    #[test]
    fn queued_messages_go_out_in_order() {
        defer(Deferred::Message {
            player: 1,
            text: "a".into(),
        });
        defer(Deferred::Console("b".into()));
        defer(Deferred::Shutdown);
        let mut host = Host::default();
        drain_deferred(&mut host);
        assert_eq!(host.said, ["1: a", "console: b"]);
        assert!(host.stopped);
        assert!(OUTBOX.lock().unwrap().is_empty());
    }
}
