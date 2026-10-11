//! Purpose:
//! Builds the shared native PCRE2 MIME provider for mbstring integration tests.
//!
//! Called from:
//! - INI and Core query configuration integration tests on supported native hosts.
//!
//! Key details:
//! - Uses the repository shim and aligned host development files without network access.
//! - The loaded code remains resident to satisfy the provider's process-lifetime contract.
//! - Unix uses `dlopen`; Windows uses the exact UTF-16 DLL path through `LoadLibraryW`.

use std::{ffi::{c_char, c_void, CString}, fs, path::{Path, PathBuf}, process::Command, time::{SystemTime, UNIX_EPOCH}};
#[cfg(unix)]
use std::ffi::c_int;
use elephc_builtin_contract::mbstring_abi::ini::*;

#[cfg(unix)]
#[cfg_attr(target_os = "linux", link(name = "dl"))]
unsafe extern "C" {
    /// Loads the compiled host provider and retains one dynamic-loader reference for the process.
    fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
    /// Resolves one required exact C ABI symbol from the successfully loaded provider.
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

/// Resolves one exported provider symbol through the host platform's loader.
#[cfg(unix)]
unsafe fn load_symbol(handle: *mut c_void, name: *const c_char) -> *mut c_void {
    unsafe { dlsym(handle, name) }
}

/// Resolves one exported provider symbol through the Windows loader.
#[cfg(windows)]
unsafe fn load_symbol(handle: *mut c_void, name: *const c_char) -> *mut c_void {
    unsafe { get_proc_address(handle, name) }
}

/// Removes only this test's source/shared-object files after loading; the loader retains mapped code.
struct Files(PathBuf);

impl Drop for Files {
    /// Releases the isolated temporary directory while leaving process-lifetime callback code resident.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Builds and loads the exact repository shim using an aligned native-only test PCRE2 provider.
pub(crate) fn provider() -> MbMimeRegexV1 {
    let flags = if cfg!(target_os = "macos") {
        let prefix = Command::new("brew").args(["--prefix", "pcre2"]).output().expect("Homebrew PCRE2 is required for the native INI test");
        assert!(prefix.status.success(), "PCRE2 host provider missing: {}", String::from_utf8_lossy(&prefix.stderr));
        let prefix = PathBuf::from(String::from_utf8(prefix.stdout).unwrap().trim());
        vec!["-I".into(), prefix.join("include").into_os_string(), "-L".into(), prefix.join("lib").into_os_string(),
            "-lpcre2-posix".into(), "-lpcre2-8".into()]
    } else {
        let output = Command::new("pkg-config").args(["--cflags", "--libs", "libpcre2-posix", "libpcre2-8"])
            .output().expect("pkg-config and PCRE2 development packages are required for the native INI test");
        assert!(output.status.success(), "PCRE2 host provider missing: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap().split_whitespace().map(std::ffi::OsString::from).collect()
    };
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let files = Files(std::env::temp_dir().join(format!("elephc_ini_pcre2_{}_{stamp}", std::process::id())));
    fs::create_dir(&files.0).unwrap();
    let source = files.0.join("provider.c");
    let library = files.0.join(if cfg!(windows) { "provider.dll" } else { "provider.so" });
    fs::write(&source, include_str!("../../../../src/native_deps/recipes/pcre2_shim.c")).unwrap();
    let mut command = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()));
    command.args(["-std=c11", "-Wall", "-Wextra", "-Werror"]);
    if !cfg!(windows) { command.arg("-fPIC"); }
    command.arg(if cfg!(target_os = "macos") { "-dynamiclib" } else { "-shared" });
    command.arg(&source).args(flags).arg("-o").arg(&library);
    let output = command.output().unwrap();
    assert!(output.status.success(), "PCRE2 shim compilation failed: {}", String::from_utf8_lossy(&output.stderr));
    let handle = unsafe { load_library(&library) };
    assert!(!handle.is_null(), "compiled PCRE2 provider must load with immediate symbol resolution");
    let symbol = |name: &str| {
        let name = CString::new(name).unwrap();
        let address = unsafe { load_symbol(handle, name.as_ptr()) };
        assert!(!address.is_null(), "missing provider symbol {name:?}");
        address
    };
    MbMimeRegexV1 { version: 1, size: std::mem::size_of::<MbMimeRegexV1>() as u32,
        compile: Some(unsafe { std::mem::transmute::<*mut c_void, MbMimeCompileV1>(symbol("elephc_pcre2_v1_mime_compile")) }),
        matches: Some(unsafe { std::mem::transmute::<*mut c_void, MbMimeMatchV1>(symbol("elephc_pcre2_v1_mime_match")) }),
        free: Some(unsafe { std::mem::transmute::<*mut c_void, MbMimeFreeV1>(symbol("elephc_pcre2_v1_mime_free")) }),
        error: Some(unsafe { std::mem::transmute::<*mut c_void, MbMimeErrorV1>(symbol("elephc_pcre2_v1_error_message")) }) }
}
