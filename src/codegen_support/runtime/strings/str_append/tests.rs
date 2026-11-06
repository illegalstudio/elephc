//! Purpose:
//! Exercises the emitted append ABI, ownership transitions, failures and copy volume.
//!
//! Called from:
//! - Focused `cargo test --lib str_append::tests` on executable host targets.
//!
//! Key details:
//! - A C allocator model supplies deterministic sharing and allocation failure.
//! - Byte counters are injected only into test assembly, never production runtime code.
//! - Scratch directories are unique and removed through RAII on every exit path.

use super::*;
use crate::codegen_support::{abi, platform::Target};
use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

/// Owns only one test-created scratch directory under the workspace build directory.
struct Scratch(PathBuf);

impl Scratch {
    /// Creates an isolated directory without modifying shared runtime caches.
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
            .join(format!("append-harness-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    /// Removes only this harness's resolved directory, including failure artifacts.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Adds a C-callable adapter plus the model's custom-runtime ABI bridges.
fn bridges(emitter: &mut Emitter) {
    emitter.label_global("append_under_test");
    match emitter.target.arch {
        // -- map the C caller into the x86_64 string ABI --
        Arch::X86_64 => {
            emitter.instruction("mov rax, rdi");                                // map the C left pointer into the runtime string ABI
            emitter.instruction("mov r8, rdx");                                 // preserve the C RHS pointer while moving the left length
            emitter.instruction("mov r9, rcx");                                 // preserve the C RHS length
            emitter.instruction("mov rdx, rsi");                                // install the runtime left length
            emitter.instruction("mov rdi, r8");                                 // install the runtime RHS pointer
            emitter.instruction("mov rsi, r9");                                 // install the runtime RHS length
            emitter.instruction("jmp __rt_str_append");                         // tail-call with a C-compatible two-word result pair
            // -- bridge ownership and allocation helpers to the C model --
            for (label, callee) in [("__rt_heap_kind", "append_test_kind"),
                ("__rt_heap_alloc", "append_test_alloc"), ("__rt_decref_any", "append_test_decref")]
            {
                emitter.label_global(label);
                emitter.instruction("mov rdi, rax");                            // move the runtime pointer or size into C argument zero
                emitter.instruction("sub rsp, 8");                              // align the stack for the C model call
                abi::emit_call_label(emitter, callee);
                emitter.instruction("add rsp, 8");                              // restore runtime entry stack alignment
                emitter.instruction("ret");                                     // return the C model result in rax
            }
            // -- bridge origin metadata invalidation to the C model --
            emitter.label_global("__rt_mbstring_ini_fresh");
            emitter.instruction("mov rdi, rax");                                // pass the changed runtime string pointer to C
            emitter.instruction("mov rsi, rdx");                                // pass its complete logical length to C
            emitter.instruction("sub rsp, 8");                                  // align the metadata model call
            abi::emit_call_label(emitter, "append_test_fresh");
            emitter.instruction("add rsp, 8");                                  // restore the runtime helper stack
            emitter.instruction("ret");                                         // return with caller-saved registers deliberately unconstrained
            // -- forward allocation failure without returning --
            emitter.label_global("__rt_alloc_overflow");
            abi::emit_call_label(emitter, "append_test_overflow");
            emitter.instruction("ud2");                                         // the model's overflow boundary never returns
        }
        // -- preserve the ARM64 caller while mapping the string ABI --
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // preserve the C caller's frame and return link
            emitter.instruction("mov x4, x3");                                  // install the runtime RHS length before overwriting x3
            emitter.instruction("mov x3, x2");                                  // install the runtime RHS pointer
            emitter.instruction("mov x2, x1");                                  // install the runtime left length
            emitter.instruction("mov x1, x0");                                  // install the runtime left pointer
            abi::emit_call_label(emitter, "__rt_str_append");
            emitter.instruction("mov x0, x1");                                  // map the runtime pointer to C result word zero
            emitter.instruction("mov x1, x2");                                  // map the runtime length to C result word one
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore the caller frame and link
            emitter.instruction("ret");                                         // return the C-compatible pair
            // -- bridge origin metadata invalidation to the C model --
            emitter.label_global("__rt_mbstring_ini_fresh");
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // preserve the runtime return link across the C model call
            emitter.instruction("mov x0, x1");                                  // pass the changed string pointer in C argument zero
            emitter.instruction("mov x1, x2");                                  // pass its logical length in C argument one
            abi::emit_call_label(emitter, "append_test_fresh");
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore the caller frame and link
            emitter.instruction("ret");                                         // return with caller-saved registers deliberately unconstrained
            // -- forward allocation failure without returning --
            emitter.label_global("__rt_alloc_overflow");
            abi::emit_call_label(emitter, "append_test_overflow");
            emitter.instruction("brk #0");                                      // the model's overflow boundary never returns
        }
    }
}

/// Produces standalone assembler input without changing runtime helper behavior.
fn assembly(target: Target, mbstring: bool) -> String {
    let mut emitter = Emitter::new(target);
    emit_str_append(&mut emitter, mbstring);
    let syntax = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
    format!("{syntax}.text\n{}", emitter.output())
}

/// Executes real helper instructions against sharing, overlap, metadata and failure controls.
#[test]
fn str_append_native_abi_ownership_and_copy_volume() {
    let target = Target::detect_host();
    let mut emitter = Emitter::new(target);
    emit_str_append(&mut emitter, true);
    bridges(&mut emitter);
    let mut body = emitter.output();
    let (store, counter) = match target.arch {
        Arch::X86_64 => ("mov BYTE PTR [r8], r10b", "\n    add QWORD PTR append_test_copy_bytes[rip], 1\n".to_string()),
        Arch::AArch64 => {
            // -- count copied bytes without changing the runtime control flow --
            let mut counter = Emitter::new(target);
            abi::emit_symbol_address(&mut counter, "x12", "append_test_copy_bytes");
            counter.instruction("ldr x13, [x12]");                              // read the test-only copied-byte counter
            counter.instruction("add x13, x13, #1");                            // count one executed payload-byte store without changing flags
            counter.instruction("str x13, [x12]");                              // publish the test-only byte count
            ("strb w11, [x10], #1", format!("\n{}", counter.output()))
        }
    };
    assert_eq!(body.matches(store).count(), 2, "both prefix and RHS loops are instrumented");
    body = body.replace(store, &format!("{store}{counter}"));
    let prefix = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n.text\n" } else { ".text\n" };
    let suffix = if cfg!(target_os = "linux") { "\n.section .note.GNU-stack,\"\",@progbits\n" } else { "" };
    let dir = Scratch::new();
    fs::write(dir.0.join("runtime.s"), format!("{prefix}{body}{suffix}")).unwrap();
    fs::write(dir.0.join("harness.c"), include_str!("harness.c")).unwrap();
    let binary = dir.0.join("harness");
    let compile = Command::new("cc").args(["-std=c11", "-O0", "-Wall", "-Wextra", "-Werror"])
        .arg(dir.0.join("harness.c")).arg(dir.0.join("runtime.s")).arg("-o").arg(&binary)
        .output().unwrap();
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let output = Command::new(binary).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("n=20000"));
    println!("{stdout}");
}

/// Cross-assembles both metadata variants for the complete target matrix when Clang is available.
#[test]
fn str_append_all_supported_target_assemblers() {
    if Command::new("clang").arg("--version").output().is_err() {
        eprintln!("Clang unavailable; native ABI and CI target runtime assembly provide coverage");
        return;
    }
    let dir = Scratch::new();
    for (name, triple) in [("macos-aarch64", "arm64-apple-macos11"),
        ("ios-arm64", "arm64-apple-ios13.0"), ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu")]
    {
        for mbstring in [false, true] {
            let source = dir.0.join(format!("{name}-{mbstring}.s"));
            let object = source.with_extension("o");
            fs::write(&source, assembly(Target::parse(name).unwrap(), mbstring)).unwrap();
            let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                .arg("-o").arg(object).output().unwrap();
            assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
