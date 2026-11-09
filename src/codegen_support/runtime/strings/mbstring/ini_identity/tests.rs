//! Purpose:
//! Exercises emitted INI identity hooks through real persistence and allocator release paths.
//!
//! Called from:
//! - Focused compiler library tests on executable supported Unix hosts.
//!
//! Key details:
//! - C owns an independent bounded heap and forwards provider calls to the real Rust bridge.
//! - Shared fixture runtime symbols use production hidden visibility before register probes call them.
//! - Structural target checks cover both architectures, PIC modes, and all five supported targets.

use super::*;
use std::{ffi::{CStr, CString}, os::unix::ffi::OsStrExt, process::Command};
use crate::codegen_support::{platform::{Platform, Target}, runtime::arrays};
use elephc_mbstring::abi::*;

/// Emits C-callable fixture entries without changing the runtime's native register conventions.
fn entry(emitter: &mut Emitter, name: &str, runtime: &str, arguments: usize) {
    emitter.label_global(&emitter.target.extern_symbol(name));
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // preserve the fixture's C return linkage
        if matches!(name, "mb_test_persist" | "mb_test_literal" | "mb_test_lower" | "mb_test_upper") {
            emitter.instruction("mov x2, x1");                                  // adapt the C byte count to native string persistence
            emitter.instruction("mov x1, x0");                                  // adapt the C source pointer to native string persistence
        }
        abi::emit_call_label(emitter, runtime);
        if matches!(name, "mb_test_persist" | "mb_test_lower" | "mb_test_upper") { emitter.instruction("mov x0, x1"); } // return native string ownership through the C pointer result
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore fixture linkage after the real native helper returns
    } else {
        emitter.instruction("push rbp");                                        // preserve the C frame and align the fixture call site
        emitter.instruction("mov rbp, rsp");                                    // establish the C entry's stable frame
        if arguments >= 3 { emitter.instruction("mov rcx, rdx"); }              // preserve the third C argument before staging the native second register
        if arguments >= 2 { emitter.instruction("mov rdx, rsi"); }              // adapt byte length or source pointer to the native convention
        if arguments >= 1 { emitter.instruction("mov rax, rdi"); }              // adapt the owner or destination pointer to the native convention
        abi::emit_call_label(emitter, runtime);
        emitter.instruction("pop rbp");                                         // restore the fixture's C linkage after the native return
    }
    emitter.instruction("ret");                                                 // return only after the real native operation completes
}

/// Captures volatile GPRs and all vector lanes before and after a deliberately clobbering C provider.
fn register_probe(emitter: &mut Emitter) {
    emitter.label_global(&emitter.target.extern_symbol("mb_test_registers"));
    let arm = emitter.target.arch == Arch::AArch64;
    if arm {
        emitter.instruction("stp x29, x30, [sp, #-160]!");                      // reserve original C linkage and callee-saved vector storage
        emitter.instruction("stp x19, x20, [sp, #16]");                         // preserve the fixture's snapshot base registers
        for register in (8..16).step_by(2) {
            emitter.instruction(&format!("stp q{register}, q{}, [sp, #{}]", register + 1, 32 + (register - 8) * 16));// preserve C callee-saved lanes before filling test vectors
        }
        abi::emit_symbol_address(emitter, "x19", "_ini_register_snapshots");
        for register in 3..18 { emitter.instruction(&format!("mov x{register}, #{}", register + 101)); }// assign distinct volatile integer sentinels
        for register in 0..32 { emitter.instruction(&format!("movi v{register}.16b, #{}", register + 17)); }// initialize both halves of every vector with distinct bytes
    } else {
        emitter.instruction("push rbp");                                        // retain C frame linkage before filling volatile registers
        emitter.instruction("push rbx");                                        // preserve the snapshot base register owned by the C caller
        emitter.instruction("sub rsp, 8");                                      // align the fixture's nested native call
        abi::emit_symbol_address(emitter, "rbx", "_ini_register_snapshots");
        emitter.instruction("mov rcx, rdx");                                    // retain the incoming identity as the native third argument
        emitter.instruction("mov rdx, rsi");                                    // adapt the incoming byte length to native bind
        emitter.instruction("mov rax, rdi");                                    // adapt the source allocation to native bind
        for register in 0..16 {
            emitter.instruction(&format!("mov r11, {}", register + 17));        // assign a distinct integer pattern for this vector
            emitter.instruction(&format!("movq xmm{register}, r11"));           // seed the low vector lane without disturbing bind arguments
            emitter.instruction(&format!("punpcklqdq xmm{register}, xmm{register}"));// fill the high lane so full-width preservation is observable
        }
        for register in 8..12 { emitter.instruction(&format!("mov r{register}, {}", register + 101)); }// set distinct volatile integer register sentinels
    }
    for phase in 0..2 {
        let base = phase * 672;
        if arm {
            for register in 0..18 { emitter.instruction(&format!("str x{register}, [x19, #{}]", base + register * 8)); }// snapshot every volatile integer argument and scratch register
            for register in 0..32 { emitter.instruction(&format!("str q{register}, [x19, #{}]", base + 160 + register * 16)); }// snapshot every full vector without clobbering registers
        } else {
            for (index, register) in ["rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"].iter().enumerate() {
                emitter.instruction(&format!("mov QWORD PTR [rbx + {}], {register}", base + index * 8));// snapshot volatile integer state without changing it
            }
            for register in 0..16 { emitter.instruction(&format!("movdqu XMMWORD PTR [rbx + {}], xmm{register}", base + 160 + register * 16)); }// capture both vector lanes before comparison in C
        }
        if phase == 0 { abi::emit_call_label(emitter, "__rt_mbstring_ini_bind"); }
    }
    if arm {
        for register in (8..16).step_by(2) {
            emitter.instruction(&format!("ldp q{register}, q{}, [sp, #{}]", register + 1, 32 + (register - 8) * 16));// restore the C caller's original callee-saved vectors
        }
        emitter.instruction("ldp x19, x20, [sp, #16]");                         // restore the fixture's C base registers
        emitter.instruction("ldp x29, x30, [sp], #160");                        // restore linkage and release the probe frame
    } else {
        emitter.instruction("add rsp, 8");                                      // release alignment before restoring the C caller
        emitter.instruction("pop rbx");                                         // restore the original callee-saved base register
        emitter.instruction("pop rbp");                                         // restore original C linkage
    }
    emitter.instruction("ret");                                                 // let independent C checks compare the snapshots
}

/// Builds the tested hooks and real persistence/free helpers with independent C allocation support.
fn fixture(target: Target) -> String {
    let mut emitter = Emitter::new(target);
    emitter.pic_data_refs = true;
    if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
    emitter.raw(".text");
    emit(&mut emitter);
    crate::codegen_support::runtime::eval_bridge::string_literal::emit(&mut emitter);
    register_probe(&mut emitter);
    crate::codegen_support::runtime::strings::emit_str_persist(&mut emitter, true);
    crate::codegen_support::runtime::strings::emit_strtolower(&mut emitter, true);
    crate::codegen_support::runtime::strings::emit_strtoupper(&mut emitter, true);
    arrays::emit_heap_free(&mut emitter, true, true);
    arrays::emit_heap_kind(&mut emitter);
    for name in ["__rt_object_handle_release", "__rt_heap_debug_validate_free_list", "__rt_weak_reference_retire"] {
        emitter.label_global(name);
        emitter.instruction("ret");                                             // keep unrelated metadata and debug checks inert in this bounded heap fixture
    }
    emitter.label_global("__rt_heap_debug_fail");
    emitter.instruction(if target.arch == Arch::AArch64 { "mov x0, #91" } else { "mov edi, 91" });// expose an unexpected allocator debug failure as a distinct status
    emitter.bl_c("exit");
    emitter.label_global("__rt_heap_alloc");
    if target.arch == Arch::X86_64 { emitter.instruction("mov rdi, rax"); }     // adapt the runtime allocation size to the fixture C allocator
    let destination = target.extern_symbol("mb_test_allocate");
    emitter.instruction(&format!("{} {destination}", if target.arch == Arch::AArch64 { "b" } else { "jmp" }));// delegate storage only, retaining real native persistence and free behavior
    for (name, runtime, arguments) in [("mb_test_bind", "__rt_mbstring_ini_bind", 3), ("mb_test_persist", "__rt_str_persist", 2),
        ("mb_test_literal", "__rt_mbstring_ini_literal", 2),
        ("mb_test_lower", "__rt_strtolower", 2), ("mb_test_upper", "__rt_strtoupper", 2),
        ("mb_test_free", "__rt_heap_free", 1), ("mb_test_reset", "__rt_mbstring_ini_reset", 0)] { entry(&mut emitter, name, runtime, arguments); }
    if target.platform == Platform::Linux { emitter.raw(".section .note.GNU-stack,\"\",@progbits"); }
    crate::codegen_support::visibility::append_hidden_directives(
        &emitter.output(),
        &crate::fast_hash::FastSet::default(),
        target.platform,
    )
}

/// Executes the emitted persistence/free hooks against real identity leases, including native address reuse.
#[test]
fn mbstring_ini_native_identity_survives_persistence_and_final_free() {
    let target = Target::detect_host();
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("elephc-ini-native-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("native.s"), fixture(target)).unwrap();
    std::fs::write(directory.join("native.c"), include_str!("native.c")).unwrap();
    let library = directory.join("native.so");
    let output = Command::new("cc").current_dir(&directory)
        .arg(if target.platform == Platform::MacOS { "-dynamiclib" } else { "-shared" })
        .args(["-fPIC", "-Wall", "-Wextra", "-Werror", "native.c", "native.s", "-o"]).arg(&library).output().unwrap();
    assert!(output.status.success(), "native identity fixture build failed: {}", String::from_utf8_lossy(&output.stderr));
    let name = CString::new(library.as_os_str().as_bytes()).unwrap();
    let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if handle.is_null() { panic!("native identity fixture load failed: {}", unsafe { CStr::from_ptr(libc::dlerror()) }.to_string_lossy()); }
    let name = CString::new("mb_test_native").unwrap();
    let symbol = unsafe { libc::dlsym(handle, name.as_ptr()) };
    assert!(!symbol.is_null());
    let functions = [elephc_mbstring_ini_v1 as *const (), elephc_mbstring_release_v1 as *const (),
        elephc_mbstring_ini_string_retain_v1 as *const (), elephc_mbstring_ini_string_release_v1 as *const (),
        elephc_mbstring_native_string_bind_v1 as *const (), elephc_mbstring_native_string_copy_v1 as *const (),
        elephc_mbstring_native_string_lookup_v1 as *const (), elephc_mbstring_native_string_forget_v1 as *const (),
        elephc_mbstring_native_string_reset_v1 as *const (), elephc_mbstring_native_string_fresh_v1 as *const (),
        elephc_mbstring_native_string_literal_v1 as *const (), elephc_mbstring_native_string_persist_v1 as *const (),
        elephc_mbstring_native_string_resolve_v1 as *const (),
        &ELEPHC_MBSTRING_NATIVE_ENTRIES as *const _ as *const ()];
    let check: unsafe extern "C" fn(*const *const ()) -> i32 = unsafe { std::mem::transmute(symbol) };
    let result = unsafe { check(functions.as_ptr()) };
    unsafe { libc::dlclose(handle); }
    let _ = std::fs::remove_dir_all(directory);
    assert_eq!(result, 0, "C native identity fixture failed at the reported line");
}

/// Checks platform symbols and preservation frames in ordinary and PIC variants of every supported target.
#[test]
fn mbstring_ini_identity_hooks_cover_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for pic in [false, true] {
            let mut emitter = Emitter::new(target);
            emitter.pic_data_refs = pic;
            emit(&mut emitter);
            let assembly = emitter.output();
            for operation in ["bind", "copy", "fresh", "literal", "persist", "forget", "reset"] {
                assert!(assembly.contains(&format!("__rt_mbstring_ini_{operation}:")), "{name}, PIC {pic}");
                // `literal` records nothing, so it has no Rust call (see `emit_literal_noop`).
                let calls = assembly.contains(&target.extern_symbol(&format!("elephc_mbstring_native_string_{operation}_v1")));
                assert_eq!(calls, operation != "literal", "{name}, PIC {pic}: {operation}");
            }
            assert!(assembly.contains(if target.arch == Arch::AArch64 { "stp q30, q31" } else { "and rsp, -16" }));
            assert!(assembly.contains(if target.arch == Arch::AArch64 { "ldp q30, q31" } else { "movdqu xmm15" }));
        }
        let shared_fixture = fixture(target);
        let visibility = if target.platform == Platform::MacOS {
            ".private_extern"
        } else {
            ".hidden"
        };
        assert!(
            shared_fixture.contains(&format!("{visibility} __rt_mbstring_ini_bind\n")),
            "{name} shared fixture must bind the register-preserving hook locally"
        );
    }
}

/// `forget` runs on every freed string once identity tracking is active, and its only caller is
/// `__rt_heap_free`: it must not carry the full 30-GPR + 32-Q register save of the other hooks
/// (that save was most of the mbstring cost of a compiled Symfony request). Float argument
/// registers stay preserved.
#[test]
fn mbstring_ini_forget_hook_keeps_a_lean_frame() {
    for name in ["macos-aarch64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        emit(&mut emitter);
        let assembly = emitter.output();
        let body = assembly
            .split_once("__rt_mbstring_ini_forget:")
            .and_then(|(_, rest)| rest.split_once("__rt_mbstring_ini_reset:"))
            .map(|(body, _)| body)
            .unwrap_or_else(|| panic!("{name}: forget body"));
        let (full_save, float_kept) = if target.arch == Arch::AArch64 {
            ("sub sp, sp, #768", "stp d6, d7")
        } else {
            ("push r15", "movdqu XMMWORD PTR [rsp + 112], xmm7")
        };
        assert!(!body.contains(full_save), "{name}: forget must not save every register");
        assert!(!body.contains("q31") && !body.contains("xmm15"), "{name}: no full vector save");
        assert!(body.contains(float_kept), "{name}: float argument registers stay preserved");
    }
}

/// Until an INI input activates the registry, the creation hooks return before any Rust call, and
/// the hooks that only retire or share existing metadata return while no allocation is tracked: a
/// request must not pay a Rust call per persisted or freed string (that bookkeeping was a fifth of
/// a compiled Symfony request). Only `bind` runs unconditionally.
#[test]
fn mbstring_ini_creation_hooks_stay_dormant_until_ini_input() {
    for name in ["macos-aarch64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        emit(&mut emitter);
        let assembly = emitter.output();
        let operations = ["bind", "copy", "fresh", "literal", "persist", "forget", "reset"];
        for (index, operation) in operations.iter().enumerate() {
            let start = format!("__rt_mbstring_ini_{operation}:");
            let body = assembly.split_once(start.as_str()).map(|(_, rest)| rest).unwrap();
            let body = match operations.get(index + 1) {
                Some(next) => body.split_once(format!("__rt_mbstring_ini_{next}:").as_str()).unwrap().0,
                None => body,
            };
            if *operation == "literal" {
                let first = body.lines().map(str::trim).find(|line| !line.is_empty());
                assert_eq!(first, Some("ret"), "{name}: literal records nothing");
                continue;
            }
            let gate = if needs_tracked_entries(operation) {
                body.find("ELEPHC_MBSTRING_NATIVE_ENTRIES")
            } else {
                body.find("_mbstring_ini_native_active")
            };
            let call = body.find(&format!("elephc_mbstring_native_string_{operation}_v1")).unwrap();
            if *operation == "bind" {
                assert!(gate.is_none_or(|gate| gate > call), "{name}: bind activates the registry");
            } else {
                assert!(gate.is_some_and(|gate| gate < call), "{name}: {operation} is gated before its Rust call");
            }
        }
    }
    for name in ["macos-aarch64", "linux-x86_64"] {
        let mut emitter = Emitter::new(Target::parse(name).unwrap());
        super::super::ini_input::emit(&mut emitter);
        assert!(emitter.output().contains("_mbstring_ini_native_active"), "{name}: INI input activates the registry");
    }
}
