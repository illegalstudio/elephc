//! Purpose:
//! Executes the emitted Stringable exception boundary against independent C callbacks.
//!
//! Called from:
//! - Focused compiler library tests on executable Linux and macOS targets.
//!
//! Key details:
//! - C uses the public context/object/output ABI and real setjmp/longjmp control flow.
//! - The formatter stub isolates argument transfer, nested boundaries, state restoration, and ownership.
//! - Full native/eval method semantics are exercised separately when the argument adapter is wired.

use super::*;
use std::{ffi::{CStr, CString}, os::unix::ffi::OsStrExt, process::Command};
use crate::codegen_support::platform::{Platform, Target};

/// Provides the formatter's register returns while the C fixture owns callback behavior and buffers.
fn emit_formatter_fixture(emitter: &mut Emitter) {
    emitter.label_global("__rt_sprintf_mixed_to_string");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #48");                             // reserve a C output tuple and independent frame linkage
            emitter.instruction("stp x29, x30, [sp, #32]");                     // preserve the fixture caller across its C callback
            emitter.instruction("add x29, sp, #32");                            // establish an aligned fixture frame
            emitter.instruction("mov x3, sp");                                  // use the first three words for the independent C result tuple
            emitter.bl_c("mb_test_convert");
            emitter.instruction("ldp x1, x2, [sp]");                            // return binary bytes and length in formatter registers
            emitter.instruction("ldr x0, [sp, #16]");                           // return the separate native owner
            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore callback fixture linkage
            emitter.instruction("add sp, sp, #48");                             // release the C result tuple
            emitter.instruction("ret");                                         // return the formatter-compatible native tuple
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // align the stack before the independent C callback
            emitter.instruction("mov rbp, rsp");                                // establish the fixture frame
            emitter.instruction("sub rsp, 32");                                 // reserve an aligned C output tuple
            emitter.instruction("mov rcx, rsp");                                // pass tuple storage in the fourth SysV argument register
            emitter.bl_c("mb_test_convert");
            emitter.instruction("mov rax, QWORD PTR [rsp]");                    // return the binary string pointer
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // return the byte length
            emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");               // return the distinct native owner
            emitter.instruction("leave");                                       // release the fixture tuple and restore caller linkage
            emitter.instruction("ret");                                         // return the formatter-compatible native tuple
        }
    }
}

/// Runs real host machine code through ordinary, throwing, nested, and invalid-input C calls.
#[test]
fn mbstring_stringable_boundary_preserves_host_state() {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("elephc-mbstring-stringable-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let target = Target::detect_host();
    let mut emitter = Emitter::new(target);
    if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
    emitter.raw(".text");
    emit(&mut emitter);
    emit_formatter_fixture(&mut emitter);
    if target.platform == Platform::Linux { emitter.raw(".section .note.GNU-stack,\"\",@progbits"); }
    std::fs::write(directory.join("boundary.s"), emitter.output()).unwrap();
    std::fs::write(directory.join("boundary.c"), include_str!("native_boundary.c")).unwrap();
    let library = directory.join("boundary.so");
    let output = Command::new("cc").current_dir(&directory)
        .arg(if target.platform == Platform::MacOS { "-dynamiclib" } else { "-shared" })
        .args(["-fPIC", "-Wall", "-Wextra", "-Werror", "boundary.c", "boundary.s", "-o"])
        .arg(&library).output().unwrap();
    assert!(output.status.success(), "boundary build failed: {}", String::from_utf8_lossy(&output.stderr));
    let name = CString::new(library.as_os_str().as_bytes()).unwrap();
    let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if handle.is_null() {
        let error = unsafe { CStr::from_ptr(libc::dlerror()) };
        panic!("boundary load failed: {}", error.to_string_lossy());
    }
    let name = CString::new("mb_test_stringable_boundary").unwrap();
    let symbol = unsafe { libc::dlsym(handle, name.as_ptr()) };
    assert!(!symbol.is_null(), "missing boundary test entry");
    let check: unsafe extern "C" fn() -> i32 = unsafe { std::mem::transmute(symbol) };
    let result = unsafe { check() };
    unsafe { libc::dlclose(handle); }
    let _ = std::fs::remove_dir_all(directory);
    assert_eq!(result, 0, "C fixture failed at the reported line");
}
