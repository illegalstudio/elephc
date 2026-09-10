//! Purpose:
//! Emits the eval concat wrapper with independently owned string conversions.
//!
//! Called from:
//! - The target-specific eval comparison and string wrapper emitters.
//!
//! Key details:
//! - Each operand is persisted before converting the next, including numeric scratch results.
//! - Converted buffers are released after the result has acquired its own string storage.
//! - Original boxed operands remain borrowed from the interpreter.

use super::*;
use crate::codegen_support::platform::Arch;

/// Emits the C concat wrapper and its internal owned-string conversion helper for the selected target.
pub(super) fn emit(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Preserves the two input boxes and releases independent ARM64 conversion buffers after boxing the result.
fn emit_aarch64(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_concat");
    emitter.instruction("sub sp, sp, #64");                                     // reserve aligned argument, string-owner, and result slots
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the Rust caller frame and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the wrapper frame pointer
    emitter.instruction("str x1, [sp]");                                        // preserve the borrowed right box during left conversion
    emitter.instruction("bl __rt_eval_concat_owned_string");                    // capture the left string before right-side scratch conversion
    emitter.instruction("stp x1, x2, [sp, #8]");                                // retain the owned left pointer and length
    emitter.instruction("ldr x0, [sp]");                                        // recover the borrowed right box
    emitter.instruction("bl __rt_eval_concat_owned_string");                    // acquire an independent right conversion owner
    emitter.instruction("str x1, [sp, #24]");                                   // keep the right owner for post-concat cleanup
    emitter.instruction("mov x3, x1");                                          // materialize the right string pointer
    emitter.instruction("mov x4, x2");                                          // materialize the right byte length
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // recover the stable left string pair
    emitter.instruction("bl __rt_concat");                                      // concatenate both stable byte ranges
    emitter.instruction("mov x0, #1");                                          // select the PHP string tag
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the result independently of operand owners
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the result during conversion cleanup
    emitter.instruction("ldr x0, [sp, #8]");                                    // select the owned left conversion buffer
    emitter.instruction("bl __rt_heap_free_safe");                              // release the left buffer or ignore an empty static string
    emitter.instruction("ldr x0, [sp, #24]");                                   // select the owned right conversion buffer
    emitter.instruction("bl __rt_heap_free_safe");                              // release the right buffer after all result bytes are copied
    emitter.instruction("ldr x0, [sp, #32]");                                   // restore the independently owned boxed result
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #64");                                     // discard the concat wrapper slots
    emitter.instruction("ret");                                                 // return the result through the C ABI

    emitter.label("__rt_eval_concat_owned_string");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the source box and the conversion helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve return state across unboxing and persistence
    emitter.instruction("add x29, sp, #16");                                    // establish the conversion helper frame
    emitter.instruction("str x0, [sp]");                                        // preserve the source box for non-string conversion
    emitter.instruction("bl __rt_mixed_unbox");                                 // inspect the value and borrow existing string bytes
    emitter.instruction("cmp x0, #1");                                          // existing strings need only one owned persistence step
    emitter.instruction("b.eq __rt_eval_concat_persist_string");                // avoid the allocating string arm of mixed_cast_string
    emitter.instruction("ldr x0, [sp]");                                        // restore the non-string source box
    emitter.instruction("bl __rt_mixed_cast_string");                           // preserve ordinary PHP scalar and array string conversions
    emitter.label("__rt_eval_concat_persist_string");
    emitter.instruction("bl __rt_str_persist");                                 // capture borrowed bytes before another cast can reuse scratch storage
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore conversion helper return state
    emitter.instruction("add sp, sp, #32");                                     // release the conversion helper frame
    emitter.instruction("ret");                                                 // return the independently owned pointer and length
}

/// Preserves the two input boxes and releases independent x86_64 conversion buffers after boxing the result.
fn emit_x86_64(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_concat");
    emitter.instruction("push rbp");                                            // align the C caller stack for internal runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish stable local slot addressing
    emitter.instruction("sub rsp, 48");                                         // reserve the right argument, conversion owners, and result
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the borrowed right box during left conversion
    emitter.instruction("mov rax, rdi");                                        // materialize the left box in the internal input register
    emitter.instruction("call __rt_eval_concat_owned_string");                  // snapshot the left string before converting the right value
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // retain the independently owned left string pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // retain the left byte length
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // recover the borrowed right box
    emitter.instruction("call __rt_eval_concat_owned_string");                  // acquire an independent right conversion buffer
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // keep the right owner for post-concat cleanup
    emitter.instruction("mov rdi, rax");                                        // materialize the right string pointer
    emitter.instruction("mov rsi, rdx");                                        // materialize the right byte length
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // recover the stable left pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // recover the stable left byte length
    emitter.instruction("call __rt_concat");                                    // concatenate both independently captured strings
    emitter.instruction("mov rdi, rax");                                        // materialize the result string pointer for boxing
    emitter.instruction("mov rsi, rdx");                                        // materialize the result byte length for boxing
    emitter.instruction("mov eax, 1");                                          // select the PHP string tag
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the complete result
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the result through conversion cleanup
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // select the owned left conversion buffer
    emitter.instruction("call __rt_heap_free_safe");                            // release the left conversion without touching the caller's source box
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // select the owned right conversion buffer
    emitter.instruction("call __rt_heap_free_safe");                            // release the right conversion after result persistence
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // recover the independently owned boxed result
    emitter.instruction("add rsp, 48");                                         // discard the concat wrapper local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result through the C ABI

    emitter.label("__rt_eval_concat_owned_string");
    emitter.instruction("push rbp");                                            // align the internal helper stack before runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish stable source-box storage
    emitter.instruction("sub rsp, 16");                                         // reserve one source box slot with ABI alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the original source for non-string conversion
    emitter.instruction("call __rt_mixed_unbox");                               // inspect the value and borrow existing string bytes
    emitter.instruction("cmp rax, 1");                                          // existing strings need one persistence step
    emitter.instruction("je __rt_eval_concat_existing_string");                 // skip the allocating string cast arm
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the non-string source box
    emitter.instruction("call __rt_mixed_cast_string");                         // preserve ordinary scalar and array conversion semantics
    emitter.instruction("jmp __rt_eval_concat_persist_string");                 // snapshot the returned borrowed or scratch byte range
    emitter.label("__rt_eval_concat_existing_string");
    emitter.instruction("mov rax, rdi");                                        // adapt the unboxed string pointer to the persistence ABI
    emitter.label("__rt_eval_concat_persist_string");
    emitter.instruction("call __rt_str_persist");                               // acquire stable bytes before another conversion reuses scratch storage
    emitter.instruction("add rsp, 16");                                         // discard source-box storage
    emitter.instruction("pop rbp");                                             // restore the internal caller frame pointer
    emitter.instruction("ret");                                                 // return the owned pointer and byte length
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Assembles both conversion and cleanup paths for every supported ABI with real target assemblers.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn eval_concat_assembles_supported_targets() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let directory = std::env::temp_dir().join(format!("eval-concat-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        for (name, triple) in [
            ("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
            ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
            ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
        ] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            if target.arch == Arch::X86_64 { emitter.raw(".intel_syntax noprefix"); }
            emitter.raw(".text");
            emit(&mut emitter);
            let input = directory.join(format!("{name}.s"));
            std::fs::write(&input, emitter.output()).unwrap();
            let result = std::process::Command::new("clang").args(["-target", triple, "-c"])
                .arg(&input).arg("-o").arg(input.with_extension("o")).output().expect("clang is required for this explicit target check");
            assert!(result.status.success(), "{name}: {}", String::from_utf8_lossy(&result.stderr));
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
