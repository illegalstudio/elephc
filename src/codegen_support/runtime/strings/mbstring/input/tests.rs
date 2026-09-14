//! Purpose:
//! Tests emitted host-value descriptions against independent C layouts and real shared preparation.
//!
//! Called from:
//! - Focused compiler library tests on executable Linux and macOS targets.
//!
//! Key details:
//! - The actual Mixed unboxer and input classifier execute as native machine code.
//! - C metadata providers expose binary names and track every acquired/released owner.
//! - Synthetic lookup errors, nested cells, null sentinels, and Stringable flags cross the real ABI.

use super::*;
use std::{ffi::{CStr, CString}, os::unix::ffi::OsStrExt, process::Command};
use crate::codegen_support::{platform::{Platform, Target}, runtime::arrays};
use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::{coercion::MbCoercionInputV1, MbResultV1}};
use elephc_mbstring::abi::{elephc_mbstring_prepare_v1, elephc_mbstring_release_v1};

/// Adapts the internal GC argument register to the fixture's independent C ownership counter.
fn emit_release_fixture(emitter: &mut Emitter) {
    emitter.label_global("__rt_decref_any");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // adapt the native GC pointer to the first SysV C argument
    }
    let symbol = emitter.target.extern_symbol("mb_test_release");
    let instruction = if emitter.target.arch == Arch::AArch64 { "b" } else { "jmp" };
    emitter.instruction(&format!("{instruction} {symbol}"));                    // let the C fixture release only metadata it explicitly allocated
}

/// Runs actual machine-code descriptions through PHP parameter preparation and balanced metadata cleanup.
#[test]
fn mbstring_native_input_descriptors_preserve_php_values() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("elephc-mbstring-input-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let target = Target::detect_host();
    let mut emitter = Emitter::new(target);
    if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
    emitter.raw(".text");
    emit(&mut emitter, true);
    arrays::emit_mixed_unbox(&mut emitter);
    emit_release_fixture(&mut emitter);
    if target.platform == Platform::Linux { emitter.raw(".section .note.GNU-stack,\"\",@progbits"); }
    std::fs::write(directory.join("input.s"), emitter.output()).unwrap();
    std::fs::write(directory.join("input.c"), include_str!("native_input.c")).unwrap();
    let library = directory.join("input.so");
    let output = Command::new("cc").current_dir(&directory)
        .arg(if target.platform == Platform::MacOS { "-dynamiclib" } else { "-shared" })
        .args(["-fPIC", "-Wall", "-Wextra", "-Werror", "input.c", "input.s", "-o"])
        .arg(&library).output().unwrap();
    assert!(output.status.success(), "input build failed: {}", String::from_utf8_lossy(&output.stderr));
    let name = CString::new(library.as_os_str().as_bytes()).unwrap();
    let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if handle.is_null() {
        let error = unsafe { CStr::from_ptr(libc::dlerror()) };
        panic!("input load failed: {}", error.to_string_lossy());
    }
    let name = CString::new("mb_test_input").unwrap();
    let symbol = unsafe { libc::dlsym(handle, name.as_ptr()) };
    assert!(!symbol.is_null(), "missing input fixture entry");
    type Prepare = unsafe extern "C" fn(u32, u32, *const MbCoercionInputV1, u32, *mut MbResultV1);
    type Release = unsafe extern "C" fn(*mut MbResultV1);
    let check: unsafe extern "C" fn(u32, Prepare, Release) -> i32 = unsafe { std::mem::transmute(symbol) };
    let result = unsafe { check(RuntimeBuiltinId::MbStrlen.as_u32(), elephc_mbstring_prepare_v1, elephc_mbstring_release_v1) };
    unsafe { libc::dlclose(handle); }
    let _ = std::fs::remove_dir_all(directory);
    assert_eq!(result, 0, "C input fixture failed at the reported line");
}
