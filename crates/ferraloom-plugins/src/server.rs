//! The server area: the console, logging, vanilla commands and shutdown
//! (`java/bridge/.../ServerNative.java`; its entry points are `Bridge`'s).
use crate::natives::{self, Deferred, Fail, guard, jbool, string, with_host};
use jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::{JNI_FALSE, jboolean},
};
use std::ffi::c_void;

/// What `Server` and the console read and change of the game.
pub trait ServerHost {
    /// A line in the server's log, as a command's reply to the console.
    fn console_message(&mut self, message: &str);
    /// Runs a vanilla command line as the console; false when there is no such command.
    fn dispatch_vanilla(&mut self, line: &str) -> bool;
    /// Stops the server after this tick.
    fn shutdown(&mut self);
}

/// At start: `ServerNative`'s methods bound.
pub(crate) fn load(env: &mut JNIEnv) -> jni::errors::Result<()> {
    const S: &str = "Ljava/lang/String;";
    natives::register(
        env,
        "dev/ferraloom/bukkit/ServerNative",
        &[
            ("log", &format!("({S}{S}{S})V"), native_log as *mut c_void),
            (
                "consoleMessage",
                &format!("({S})V"),
                console_message as *mut c_void,
            ),
            (
                "dispatchVanilla",
                &format!("({S})Z"),
                dispatch_vanilla as *mut c_void,
            ),
            ("shutdown", "()V", shutdown as *mut c_void),
        ],
    )
}

extern "system" fn native_log(
    mut env: JNIEnv,
    _: JClass,
    level: JString,
    thread: JString,
    message: JString,
) {
    guard(&mut env, (), |env| {
        let (level, thread) = (string(env, &level)?, string(env, &thread)?);
        natives::log(&level, &thread, &string(env, &message)?);
        Ok(())
    })
}

extern "system" fn console_message(mut env: JNIEnv, _: JClass, message: JString) {
    guard(&mut env, (), |env| {
        let text = string(env, &message)?;
        if with_host(|h| h.console_message(&text)).is_none() {
            natives::defer(Deferred::Console(text));
        }
        Ok(())
    })
}

extern "system" fn dispatch_vanilla(mut env: JNIEnv, _: JClass, line: JString) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let line = string(env, &line)?;
        let ran = with_host(|h| h.dispatch_vanilla(&line))
            .ok_or(Fail::NoHost("Server.dispatchCommand"))?;
        Ok(jbool(ran))
    })
}

extern "system" fn shutdown(mut env: JNIEnv, _: JClass) {
    guard(&mut env, (), |_| {
        if with_host(|h| h.shutdown()).is_none() {
            natives::defer(Deferred::Shutdown);
        }
        Ok(())
    })
}
