//! Finding and starting a JVM in the process, through the JNI invocation API.
//!
//! libjvm is loaded with `dlopen` when the server first needs it, never
//! linked: a server without Java starts as it always has. Where it looks, in
//! order: `FERRALOOM_JVM` (a libjvm file or a Java home), `JAVA_HOME`, then
//! the `java` on `PATH` (its real path's Java home).
use jni::{JavaVM, sys};
use std::{
    ffi::{CString, c_void},
    path::{Path, PathBuf},
};

#[cfg(target_os = "windows")]
const LIBJVM: &[&str] = &["bin/server/jvm.dll", "jre/bin/server/jvm.dll"];
#[cfg(target_os = "macos")]
const LIBJVM: &[&str] = &["lib/server/libjvm.dylib", "jre/lib/server/libjvm.dylib"];
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const LIBJVM: &[&str] = &["lib/server/libjvm.so", "jre/lib/server/libjvm.so"];

/// libjvm in a Java home, or the file itself.
fn in_home(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_owned());
    }
    LIBJVM.iter().map(|l| path.join(l)).find(|p| p.is_file())
}

/// Where libjvm is, from the environment as given (`FERRALOOM_JVM`,
/// `JAVA_HOME`, `PATH`).
pub(crate) fn locate(
    ferraloom_jvm: Option<&str>,
    java_home: Option<&str>,
    path: Option<&str>,
) -> Option<PathBuf> {
    if let Some(found) = [ferraloom_jvm, java_home]
        .into_iter()
        .flatten()
        .filter(|v| !v.is_empty())
        .find_map(|v| in_home(Path::new(v)))
    {
        return Some(found);
    }
    let java = std::env::split_paths(path?)
        .map(|dir| dir.join(if cfg!(windows) { "java.exe" } else { "java" }))
        .find(|p| p.is_file())?;
    // bin/java's home, through the alternatives' symlinks.
    let home = java.canonicalize().ok()?.parent()?.parent()?.to_owned();
    in_home(&home)
}

type CreateJavaVm =
    unsafe extern "system" fn(*mut *mut sys::JavaVM, *mut *mut c_void, *mut c_void) -> sys::jint;

/// Loads libjvm and creates the process's one JVM, the calling thread
/// attached to it (it stays attached: it is the server thread). `options`
/// are the JVM's own (`-Djava.class.path=...`, `-Xmx2G`).
pub(crate) fn create(libjvm: &Path, options: &[String]) -> Result<JavaVM, String> {
    // The library stays loaded for the life of the process, as the JVM does.
    let library = unsafe { libloading::Library::new(libjvm) }
        .map_err(|e| format!("could not load {}: {e}", libjvm.display()))?;
    let library: &'static libloading::Library = Box::leak(Box::new(library));
    let create: libloading::Symbol<CreateJavaVm> = unsafe { library.get(b"JNI_CreateJavaVM\0") }
        .map_err(|e| format!("{} has no JNI_CreateJavaVM: {e}", libjvm.display()))?;
    let strings = options
        .iter()
        .map(|o| CString::new(o.as_str()).map_err(|_| format!("JVM option {o:?} has a NUL")))
        .collect::<Result<Vec<_>, _>>()?;
    let mut raw: Vec<sys::JavaVMOption> = strings
        .iter()
        .map(|s| sys::JavaVMOption {
            optionString: s.as_ptr().cast_mut(),
            extraInfo: std::ptr::null_mut(),
        })
        .collect();
    let mut args = sys::JavaVMInitArgs {
        version: sys::JNI_VERSION_1_8,
        nOptions: raw.len() as sys::jint,
        options: raw.as_mut_ptr(),
        ignoreUnrecognized: sys::JNI_FALSE,
    };
    let mut vm: *mut sys::JavaVM = std::ptr::null_mut();
    let mut env: *mut c_void = std::ptr::null_mut();
    let status = unsafe { create(&mut vm, &mut env, (&raw mut args).cast()) };
    if status != sys::JNI_OK || vm.is_null() {
        return Err(format!("JNI_CreateJavaVM failed ({status})"));
    }
    unsafe { JavaVM::from_raw(vm) }.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_libjvm_in_a_java_home_then_on_path() {
        let root = std::env::temp_dir().join(format!("ferraloom-jvm-{}", std::process::id()));
        let home = root.join("jdk");
        let lib = home.join(LIBJVM[0]);
        std::fs::create_dir_all(lib.parent().unwrap()).unwrap();
        std::fs::write(&lib, b"").unwrap();
        std::fs::create_dir_all(home.join("bin")).unwrap();
        let java = home
            .join("bin")
            .join(if cfg!(windows) { "java.exe" } else { "java" });
        std::fs::write(&java, b"").unwrap();
        let home_s = home.to_str().unwrap();
        let bin = home.join("bin");
        let path = bin.to_str().unwrap();
        let lib = lib.canonicalize().unwrap();
        let found = |f, j, p| locate(f, j, p).map(|p| p.canonicalize().unwrap());

        assert_eq!(found(Some(home_s), None, None), Some(lib.clone()));
        assert_eq!(found(None, Some(home_s), None), Some(lib.clone()));
        assert_eq!(
            found(Some(""), Some("/nonexistent"), Some(path)),
            Some(lib.clone())
        );
        assert_eq!(found(None, None, None), None);
        assert_eq!(
            found(None, Some("/nonexistent"), Some("/nonexistent")),
            None
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
