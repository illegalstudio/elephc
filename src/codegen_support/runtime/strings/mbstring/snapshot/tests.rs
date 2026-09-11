//! Purpose:
//! Executes the emitted native array reader against independent C layouts and the real Rust bridge.
//!
//! Called from:
//! - Focused compiler library tests on Linux and macOS executable targets.
//!
//! Key details:
//! - A temporary shared library contains only the reader and its two nonallocating runtime helpers.
//! - Native slots, sparse hashes, cycles, aliases, and null representations cross the actual C ABI.
//! - The library remains loaded until all borrowed descriptors have been copied and released.

use super::*;
use std::{ffi::{c_void, CStr, CString}, os::unix::ffi::OsStrExt, path::PathBuf, process::Command};
use crate::codegen_support::{platform::{Platform, Target}, runtime::arrays};
use elephc_builtin_contract::mbstring_abi::{*, array::{ArrayGraph, Key, Value}, host::*};
use elephc_mbstring::abi::{elephc_mbstring_snapshot_v1, elephc_mbstring_release_v1};

/// Owns the temporary native reader library and keeps its borrowed fixtures alive.
struct NativeReader { directory: PathBuf, handle: *mut c_void }

impl NativeReader {
    /// Compiles the actual selected-target emitter and independent C fixtures into a shared library.
    fn build() -> Self {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let directory = std::env::temp_dir().join(format!("elephc-mbstring-reader-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let target = Target::detect_host();
        let mut emitter = Emitter::new(target);
        if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
        emitter.raw(".text");
        emit(&mut emitter);
        arrays::emit_mixed_unbox(&mut emitter);
        arrays::emit_hash_iter(&mut emitter);
        if target.platform == Platform::Linux { emitter.raw(".section .note.GNU-stack,\"\",@progbits"); }
        std::fs::write(directory.join("reader.s"), emitter.output()).unwrap();
        std::fs::write(directory.join("reader.c"), include_str!("native_reader.c")).unwrap();
        let library = directory.join("reader.so");
        let output = Command::new("cc").current_dir(&directory)
            .arg(if target.platform == Platform::MacOS { "-dynamiclib" } else { "-shared" })
            .args(["-fPIC", "-Wall", "-Wextra", "-Werror", "reader.c", "reader.s", "-o"])
            .arg(&library).output().unwrap();
        assert!(output.status.success(), "reader build failed: {}", String::from_utf8_lossy(&output.stderr));
        let name = CString::new(library.as_os_str().as_bytes()).unwrap();
        let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            let error = unsafe { CStr::from_ptr(libc::dlerror()) };
            panic!("reader load failed: {}", error.to_string_lossy());
        }
        Self { directory, handle }
    }

    /// Resolves a fixture function while the shared library remains owned by this guard.
    fn symbol(&self, name: &str) -> *mut c_void {
        let name = CString::new(name).unwrap();
        let pointer = unsafe { libc::dlsym(self.handle, name.as_ptr()) };
        assert!(!pointer.is_null(), "missing native reader fixture symbol");
        pointer
    }
}

impl Drop for NativeReader {
    /// Unloads fixture storage only after the Rust bridge has copied every borrowed value.
    fn drop(&mut self) {
        unsafe { libc::dlclose(self.handle); }
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// Verifies the actual host machine-code reader and complete reader-to-engine snapshot path.
#[test]
fn mbstring_native_array_snapshot_matches_host_graph() {
    let library = NativeReader::build();
    let check: unsafe extern "C" fn() -> i32 = unsafe { std::mem::transmute(library.symbol("mb_test_reader")) };
    assert_eq!(unsafe { check() }, 0, "native reader C fixture failed at the reported line");
    let root: unsafe extern "C" fn() -> *const MbHostValueV1 = unsafe { std::mem::transmute(library.symbol("mb_test_root")) };
    let next: MbArrayNextV1 = unsafe { std::mem::transmute(library.symbol("mb_test_next")) };
    let mut output = MbResultV1::default();
    unsafe { elephc_mbstring_snapshot_v1(root(), Some(next), std::ptr::null_mut(), &mut output); }
    assert_eq!(output.kind, RESULT_ARRAY);
    let bytes = unsafe { std::slice::from_raw_parts(output.bytes, output.len as usize) };
    let graph = ArrayGraph::decode(bytes).expect("valid native snapshot framing");
    unsafe { elephc_mbstring_release_v1(&mut output); }
    let expected = ArrayGraph::new(0, vec![vec![
        (Key::Int(0), Value::String(b"a\0\xff".to_vec())), (Key::Int(1), Value::Int(i64::MIN)),
        (Key::Int(2), Value::Array(1)), (Key::Int(3), Value::Array(1)),
        (Key::Int(4), Value::Array(0)), (Key::Int(5), Value::Unsupported),
    ], vec![
        (Key::String(b"42".to_vec()), Value::Float(0x7ff8000000000042)),
        (Key::Int(42), Value::Null), (Key::String(b"k\0".to_vec()), Value::Array(0)),
    ]]).unwrap();
    assert_eq!(graph, expected);
}
