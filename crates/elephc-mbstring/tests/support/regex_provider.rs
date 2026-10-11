//! Purpose:
//! Loads the pinned native mbregex provider for independent engine and request integration tests.
//!
//! Called from:
//! - The regex and regex_request test binaries through an explicit shared support module.
//!
//! Key details:
//! - Managed archives take precedence; mapped provider code remains live until process exit.
//! - Unix uses `dlopen`; Windows uses the exact UTF-16 DLL path through `LoadLibraryW`.

use std::{ffi::{c_char, c_void}, path::Path, process::Command, sync::OnceLock};
#[cfg(unix)]
use std::ffi::{c_int, CString};
use elephc_builtin_contract::mbstring_abi::regex::MbRegexProviderV1;

#[cfg(unix)]
#[cfg_attr(target_os = "linux", link(name = "dl"))]
unsafe extern "C" {
    /// Loads an isolated provider with immediate native symbol resolution.
    fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
    /// Locates the provider getter in its retained shared library.
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "LoadLibraryW"]
    fn load_library_w(path: *const u16) -> *mut c_void;
    #[link_name = "GetProcAddress"]
    fn get_proc_address(module: *mut c_void, name: *const c_char) -> *mut c_void;
}

/// Loads one test provider through the host platform's native dynamic loader.
#[cfg(unix)]
unsafe fn load_library(path: &Path) -> *mut c_void {
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(path.as_os_str().as_bytes()).unwrap();
    unsafe { dlopen(path.as_ptr(), 2) }
}

/// Loads one test provider by its exact UTF-16 Windows path.
#[cfg(windows)]
unsafe fn load_library(path: &Path) -> *mut c_void {
    use std::os::windows::ffi::OsStrExt;

    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe { load_library_w(path.as_ptr()) }
}

/// Resolves one provider symbol through the host platform's native loader.
#[cfg(unix)]
unsafe fn load_symbol(handle: *mut c_void, name: *const c_char) -> *mut c_void {
    unsafe { dlsym(handle, name) }
}

/// Resolves one provider symbol through the Windows loader.
#[cfg(windows)]
unsafe fn load_symbol(handle: *mut c_void, name: *const c_char) -> *mut c_void {
    unsafe { get_proc_address(handle, name) }
}

/// Runs native matches against subjects ending at an inaccessible page boundary.
pub fn guarded_subjects() -> i32 {
    let address = provider_symbol(c"elephc_test_guarded_subjects");
    let test = unsafe { std::mem::transmute::<*mut c_void, unsafe extern "C" fn() -> i32>(address) };
    unsafe { test() }
}

/// Returns one retained callback table so independent tests never replace the active native allocator.
pub fn provider() -> MbRegexProviderV1 {
    static PROVIDER: OnceLock<MbRegexProviderV1> = OnceLock::new();
    *PROVIDER.get_or_init(load)
}

/// Loads managed static archives when supplied, otherwise builds the exact shim against pinned host files.
fn load() -> MbRegexProviderV1 {
    let managed = std::env::var_os("ELEPHC_ONIGURUMA_TEST_PREFIX").map(std::path::PathBuf::from);
    let flags = if let Some(prefix) = &managed {
        vec![prefix.join("lib/libelephc_oniguruma_shim.a").into_os_string(), prefix.join("lib/libonig.a").into_os_string()]
    } else {
        let version = Command::new("pkg-config").args(["--modversion", "oniguruma"]).output().expect("pkg-config and Oniguruma 6.9.10 are required");
        assert!(version.status.success());
        assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), "6.9.10");
        let flags = Command::new("pkg-config").args(["--cflags", "--libs", "oniguruma"]).output().unwrap();
        assert!(flags.status.success());
        String::from_utf8(flags.stdout).unwrap().split_whitespace().map(std::ffi::OsString::from).collect()
    };
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("elephc_onig_{}_{stamp}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("elephc_oniguruma.h"), include_str!("../../../../src/native_deps/recipes/oniguruma_shim.h")).unwrap();
    let source = directory.join("provider.c");
    let guard = directory.join("guard.c");
    let library = directory.join(if cfg!(windows) { "provider.dll" } else { "provider.so" });
    let provider_source = if managed.is_some() {
        "#include \"elephc_oniguruma.h\"\nconst void *elephc_test_provider(void) { return elephc_oniguruma_v1_provider(); }\n"
    } else { include_str!("../../../../src/native_deps/recipes/oniguruma_shim.c") };
    std::fs::write(&source, provider_source).unwrap();
    std::fs::write(&guard, include_str!("regex_provider.c")).unwrap();
    let mut cc = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()));
    cc.args(["-std=c11", "-Wall", "-Wextra", "-Werror"]);
    if !cfg!(windows) { cc.arg("-fPIC"); }
    cc
        .arg(if cfg!(target_os = "macos") { "-dynamiclib" } else { "-shared" })
        .arg("-I").arg(&directory).arg(&source).arg(&guard).args(flags)
        .arg("-o").arg(&library);
    let compiled = cc.output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let handle = unsafe { load_library(&library) };
    assert!(!handle.is_null());
    assert!(LIBRARY.set(handle as usize).is_ok());
    let address = unsafe { load_symbol(handle, c"elephc_oniguruma_v1_provider".as_ptr()) };
    assert!(!address.is_null());
    let getter = unsafe { std::mem::transmute::<*mut c_void, unsafe extern "C" fn() -> *const MbRegexProviderV1>(address) };
    let pointer = unsafe { getter() };
    assert!(!pointer.is_null());
    let header = pointer.cast::<u32>();
    assert_eq!(unsafe { header.read() }, 1);
    assert_eq!(unsafe { header.add(1).read() } as usize, std::mem::size_of::<MbRegexProviderV1>(), "rebuild the managed provider after an ABI change");
    let provider = unsafe { *pointer };
    // Unix can unlink a loaded shared object. Windows keeps the DLL path locked until process
    // exit, so cleanup is best-effort there and the process-lifetime provider remains valid.
    let _ = std::fs::remove_dir_all(directory);
    provider
}

static LIBRARY: OnceLock<usize> = OnceLock::new();

/// Resolves one symbol from the process-lifetime provider test library.
fn provider_symbol(name: &std::ffi::CStr) -> *mut c_void {
    let handle = *LIBRARY.get().expect("regex provider library loaded") as *mut c_void;
    let address = unsafe { load_symbol(handle, name.as_ptr()) };
    assert!(!address.is_null());
    address
}
