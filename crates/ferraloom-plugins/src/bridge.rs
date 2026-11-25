//! The JVM in the process and the server's lifecycle calls into the bridge
//! (`Bridge.java`): start, enable, the tick, disable, console commands. The
//! areas (`players`, `worlds`, `server`) bind their own natives and entry
//! points at start through [`AREAS`] and call through [`call`].
use crate::{PluginHost, events, jvm, natives};
use jni::{
    JNIEnv, JavaVM,
    objects::{
        GlobalRef, JClass, JObject, JObjectArray, JStaticMethodID, JString, JValue, JValueOwned,
    },
    signature::{Primitive, ReturnType},
    sys::jvalue,
};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

/// `Bridge.tick`'s bit for "the command labels changed".
const COMMANDS_CHANGED: u64 = 1 << 63;

/// Each area's start: its natives bound and its entry points looked up.
/// A new area adds its line here.
const AREAS: &[fn(&mut JNIEnv) -> jni::errors::Result<()>] = &[
    crate::server::load,
    crate::players::load,
    crate::worlds::load,
    crate::inventory::load,
];

/// A dimension as Bukkit sees it.
pub struct WorldSpec<'a> {
    /// `world`, `world_nether`, `world_the_end`.
    pub name: &'a str,
    /// `World.Environment`'s ordinal: 0 normal, 1 nether, 2 the end.
    pub environment: i32,
}

/// What the bridge needs to know of the server at start.
pub struct Config<'a> {
    /// The server's folder: `plugins/` is in it, and the bridge's jars in
    /// `ferraloom-bukkit/` (or `FERRALOOM_BUKKIT_DIR`).
    pub directory: &'a Path,
    /// `Server.getVersion()`: plugins parse its `(MC: 26.3)`.
    pub version: &'a str,
    /// The dimensions, in the order the host's world indices follow.
    pub worlds: &'a [WorldSpec<'a>],
    pub max_players: i32,
    pub port: i32,
    pub motd: &'a str,
    pub online_mode: bool,
    /// The server's log, `(level, thread, message)`, called from any thread.
    pub log: fn(&str, &str, &str),
}

/// A Java class's static entry points, looked up once at start; an area
/// keeps its own (`static EVENTS: OnceLock<Entry<N>>`) and names its methods
/// by index.
pub(crate) struct Entry<const N: usize> {
    class: GlobalRef,
    ids: [JStaticMethodID; N],
}

impl<const N: usize> Entry<N> {
    /// `class`'s methods by `(name, JNI signature)`.
    pub(crate) fn load(
        env: &mut JNIEnv,
        class: &str,
        methods: [(&str, String); N],
    ) -> jni::errors::Result<Self> {
        let c = env.find_class(class)?;
        let mut ids = Vec::with_capacity(N);
        for (name, sig) in methods {
            ids.push(env.get_static_method_id(&c, name, sig)?);
        }
        let Ok(ids) = <[JStaticMethodID; N]>::try_from(ids) else {
            unreachable!("one ID per method");
        };
        Ok(Self {
            class: env.new_global_ref(&c)?,
            ids,
        })
    }

    /// Calls method `i`.
    ///
    /// # Safety
    /// `ret` and `args` must match method `i`'s signature in [`Entry::load`].
    pub(crate) unsafe fn invoke<'local>(
        &self,
        env: &mut JNIEnv<'local>,
        i: usize,
        ret: ReturnType,
        args: &[jvalue],
    ) -> jni::errors::Result<JValueOwned<'local>> {
        let class: &JClass = self.class.as_obj().into();
        // SAFETY: the caller's contract, the ID from `load`.
        unsafe { env.call_static_method_unchecked(class, self.ids[i], ret, args) }
    }
}

const START: usize = 0;
const ENABLE: usize = 1;
const DISABLE: usize = 2;
const TICK: usize = 3;
const COMMANDS: usize = 4;
const CONSOLE_COMMAND: usize = 5;

/// The process's JVM and `Bridge`'s entry points.
struct Runtime {
    vm: JavaVM,
    bridge: Entry<6>,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Whether plugins run in this process (a JVM was started).
pub fn running() -> bool {
    RUNTIME.get().is_some()
}

fn jars(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut jars: Vec<PathBuf> = entries
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "jar"))
        .collect();
    jars.sort();
    jars
}

/// Starts plugins when `plugins/` holds a jar: the JVM, the bridge, then
/// every plugin's `onLoad`. False when there is no plugin (nothing is
/// loaded, not even libjvm); an error when there are plugins that cannot
/// run (no Java, no bridge), which the server logs before going on without them.
pub fn start(config: &Config, host: &mut dyn PluginHost) -> Result<bool, String> {
    if jars(&config.directory.join("plugins")).is_empty() {
        return Ok(false);
    }
    let _ = natives::LOG.set(config.log);
    let bridge_dir = std::env::var_os("FERRALOOM_BUKKIT_DIR")
        .map_or_else(|| config.directory.join("ferraloom-bukkit"), PathBuf::from);
    let classpath = jars(&bridge_dir);
    if !classpath
        .iter()
        .any(|j| j.ends_with("ferraloom-bukkit.jar"))
    {
        return Err(format!(
            "plugins/ holds plugins, but {} has no ferraloom-bukkit.jar \
             (crates/ferraloom-plugins/java/build.py --install <server folder> builds it)",
            bridge_dir.display()
        ));
    }
    let var = |name| std::env::var(name).ok();
    let libjvm = jvm::locate(
        var("FERRALOOM_JVM").as_deref(),
        var("JAVA_HOME").as_deref(),
        var("PATH").as_deref(),
    )
    .ok_or("plugins/ holds plugins, but no Java was found: set JAVA_HOME or FERRALOOM_JVM to a Java 21+ home")?;
    let classpath = std::env::join_paths(&classpath).map_err(|e| e.to_string())?;
    let mut options = vec![
        format!("-Djava.class.path={}", classpath.to_string_lossy()),
        // The server keeps its own signal handling (Ctrl-C saves and stops).
        "-Xrs".to_owned(),
        "-Dfile.encoding=UTF-8".to_owned(),
        "-Dstdout.encoding=UTF-8".to_owned(),
        "-Dstderr.encoding=UTF-8".to_owned(),
    ];
    if let Some(extra) = var("FERRALOOM_JVM_OPTIONS") {
        options.extend(extra.split_whitespace().map(str::to_owned));
    }
    (config.log)(
        "INFO",
        "Server thread",
        &format!("Starting a JVM for plugins from {}", libjvm.display()),
    );
    let vm = jvm::create(&libjvm, &options)?;
    let bridge = {
        let mut env = vm.get_env().map_err(|e| e.to_string())?;
        let loaded = AREAS
            .iter()
            .try_for_each(|load| load(&mut env))
            .and_then(|()| {
                const S: &str = "Ljava/lang/String;";
                Entry::load(
                    &mut env,
                    "dev/ferraloom/bukkit/Bridge",
                    [
                        ("start", format!("({S}{S}[{S}[III{S}Z)V")),
                        ("enable", "()J".into()),
                        ("disable", "()V".into()),
                        ("tick", "(J)J".into()),
                        ("commands", format!("()[{S}")),
                        ("consoleCommand", format!("({S})Z")),
                    ],
                )
            });
        loaded.map_err(|e| describe(&mut env, e))?
    };
    if RUNTIME.set(Runtime { vm, bridge }).is_err() {
        return Err("plugins were started twice".into());
    }
    call(host, |env| {
        let directory = env.new_string(config.directory.to_string_lossy())?;
        let version = env.new_string(config.version)?;
        let len = config.worlds.len() as i32;
        let names = env.new_object_array(len, "java/lang/String", JObject::null())?;
        for (i, w) in config.worlds.iter().enumerate() {
            let name = env.new_string(w.name)?;
            env.set_object_array_element(&names, i as i32, name)?;
        }
        let kinds = env.new_int_array(len)?;
        let environments: Vec<i32> = config.worlds.iter().map(|w| w.environment).collect();
        env.set_int_array_region(&kinds, 0, &environments)?;
        let motd = env.new_string(config.motd)?;
        let args = [
            obj(&directory),
            obj(&version),
            obj(&names),
            obj(&kinds),
            JValue::Int(config.max_players).as_jni(),
            JValue::Int(config.port).as_jni(),
            obj(&motd),
            JValue::Bool(config.online_mode.into()).as_jni(),
        ];
        // SAFETY: `start`'s signature above.
        unsafe { bridge_entry().invoke(env, START, void(), &args) }?;
        Ok(())
    });
    Ok(true)
}

fn bridge_entry() -> &'static Entry<6> {
    &RUNTIME.get().expect("plugins started").bridge
}

fn void() -> ReturnType {
    ReturnType::Primitive(Primitive::Void)
}

/// Every plugin's `onEnable`, once the worlds are loaded.
pub fn enable(host: &mut dyn PluginHost) {
    let long = ReturnType::Primitive(Primitive::Long);
    // SAFETY: `enable`'s signature in `start`.
    if let Some(bits) = call(host, |env| {
        unsafe { bridge_entry().invoke(env, ENABLE, long, &[]) }?.j()
    }) {
        events::set(bits as u64 & !COMMANDS_CHANGED);
    }
}

/// Every plugin's `onDisable`, as the server stops. The event bits go to 0.
pub fn disable(host: &mut dyn PluginHost) {
    // SAFETY: `disable`'s signature in `start`.
    call(host, |env| {
        unsafe { bridge_entry().invoke(env, DISABLE, void(), &[]) }.map(drop)
    });
    events::set(0);
}

/// The start of server tick `tick`: what plugins sent from other threads
/// goes out, then the scheduler's heartbeat runs the sync tasks due, then
/// the event bits are latched for this tick. Returns every plugin command
/// label when they changed since the last time.
pub fn tick(host: &mut dyn PluginHost, tick: i64) -> Option<Vec<String>> {
    natives::drain_deferred(host);
    let long = ReturnType::Primitive(Primitive::Long);
    let args = [JValue::Long(tick).as_jni()];
    // SAFETY: `tick`'s signature in `start`.
    let bits = call(host, |env| {
        unsafe { bridge_entry().invoke(env, TICK, long, &args) }?.j()
    })? as u64;
    events::set(bits & !COMMANDS_CHANGED);
    if bits & COMMANDS_CHANGED == 0 {
        return None;
    }
    call(host, |env| {
        // SAFETY: `commands`'s signature in `start`.
        let array = unsafe { bridge_entry().invoke(env, COMMANDS, ReturnType::Array, &[]) }?.l()?;
        let array = JObjectArray::from(array);
        let mut labels = Vec::new();
        for i in 0..env.get_array_length(&array)? {
            let label = JString::from(env.get_object_array_element(&array, i)?);
            labels.push(env.get_string(&label)?.into());
        }
        Ok(labels)
    })
}

/// A console line (no slash): true when a plugin command took it.
pub fn console_command(host: &mut dyn PluginHost, line: &str) -> bool {
    call(host, |env| {
        let line = env.new_string(line)?;
        let boolean = ReturnType::Primitive(Primitive::Boolean);
        // SAFETY: `consoleCommand`'s signature in `start`.
        unsafe { bridge_entry().invoke(env, CONSOLE_COMMAND, boolean, &[obj(&line)]) }?.z()
    })
    .unwrap_or(false)
}

/// One call into the bridge from the server thread, the host lent to it,
/// in a local frame of its own (this thread never returns to Java, so its
/// locals would otherwise pile up). A Java exception that escapes is logged.
pub(crate) fn call<R>(
    host: &mut dyn PluginHost,
    f: impl FnOnce(&mut JNIEnv) -> jni::errors::Result<R>,
) -> Option<R> {
    natives::lend(host, || call_unlent(f))
}

/// [`call`] without a host: for calls that run no plugin code on this thread.
pub(crate) fn call_unlent<R>(f: impl FnOnce(&mut JNIEnv) -> jni::errors::Result<R>) -> Option<R> {
    let rt = RUNTIME.get()?;
    let mut env = rt.vm.get_env().ok()?;
    match env.with_local_frame(16, f) {
        Ok(r) => Some(r),
        Err(e) => {
            describe(&mut env, e);
            None
        }
    }
}

/// Logs a failed JNI call, its pending Java exception (with its trace) cleared.
fn describe(env: &mut JNIEnv, e: jni::errors::Error) -> String {
    if env.exception_check().unwrap_or(false) {
        // Prints the trace through System.err, which the bridge logs.
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    let message = format!("Ferraloom's plugin bridge: {e}");
    natives::log("ERROR", "Server thread", &message);
    message
}

/// An object argument.
pub(crate) fn obj(o: &JObject) -> jvalue {
    JValue::Object(o).as_jni()
}

/// A returned `String`, which may be null.
pub(crate) fn string_or_none(
    env: &mut JNIEnv,
    value: JValueOwned,
) -> jni::errors::Result<Option<String>> {
    let obj = value.l()?;
    if obj.is_null() {
        return Ok(None);
    }
    Ok(Some(env.get_string(&JString::from(obj))?.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_jar_files_in_plugins_count() {
        let dir = std::env::temp_dir().join(format!("ferraloom-plugins-{}", std::process::id()));
        let plugins = dir.join("plugins");
        // A folder, as a plugin's data folder can be named.
        std::fs::create_dir_all(plugins.join("Foo.jar")).unwrap();
        std::fs::write(plugins.join("config.yml"), b"").unwrap();
        assert!(jars(&plugins).is_empty());
        assert!(jars(&dir.join("nowhere")).is_empty());
        std::fs::write(plugins.join("b.jar"), b"").unwrap();
        std::fs::write(plugins.join("a.jar"), b"").unwrap();
        let found: Vec<_> = jars(&plugins)
            .iter()
            .map(|p| p.file_name().unwrap().to_owned())
            .collect();
        assert_eq!(found, ["a.jar", "b.jar"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
