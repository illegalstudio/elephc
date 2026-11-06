//! Purpose:
//! Extends the last live heap allocation without moving its payload.
//! This lets unique packed containers grow in place when they own the bump tail.
//!
//! Called from:
//! - `__rt_array_grow` before its allocate-copy-free fallback.
//!
//! Key details:
//! - An actual extension succeeds only when the block ends exactly at `_heap_buf + _heap_off`.
//! - Failed probes leave allocator metadata and GC accounting unchanged.
//! - Successful growth updates the 32-bit payload size, live bytes, and peak bytes.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_heap_grow_tail` for the active target.
///
/// Inputs are a live payload pointer and the requested new payload size. The helper
/// returns the original pointer when the current block already satisfies the request, or when
/// it owns the bump tail and the extension fits. Otherwise it returns null so the caller can
/// use ordinary allocation.
pub fn emit_heap_grow_tail(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_heap_grow_tail_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: heap_grow_tail ---");
    emitter.label_global("__rt_heap_grow_tail");

    // -- normalize and validate the requested payload size --
    emitter.instruction("cbz x0, __rt_heap_grow_tail_fail");                    // null payloads cannot identify a live heap tail
    emitter.instruction("adds x1, x1, #7");                                     // round the requested payload size toward the allocator alignment
    emitter.instruction("b.cs __rt_heap_grow_tail_fail");                       // reject an alignment addition that wrapped the machine word
    emitter.instruction("and x1, x1, #0xfffffffffffffff8");                     // keep the requested payload size eight-byte aligned
    emitter.instruction("lsr x9, x1, #32");                                     // inspect bits that do not fit the allocator's 32-bit size field
    emitter.instruction("cbnz x9, __rt_heap_grow_tail_fail");                   // let the ordinary allocator report oversized requests
    emitter.instruction("ldr w9, [x0, #-16]");                                  // load the current zero-extended payload size from the heap header
    emitter.instruction("cmp x1, x9");                                          // does the block already satisfy the requested payload size?
    emitter.instruction("b.ls __rt_heap_grow_tail_success");                    // shrinking and equal-size requests need no allocator mutation

    // -- prove tail ownership and available heap capacity --
    emitter.instruction("add x10, x0, x9");                                     // compute the current block end from its user pointer and payload size
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_heap_buf");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x12", "_heap_off");
    emitter.instruction("ldr x13, [x12]");                                      // load the current bump offset before checking tail ownership
    emitter.instruction("add x14, x11, x13");                                   // derive the current live heap end from its base and bump offset
    emitter.instruction("cmp x10, x14");                                        // does this block end exactly at the current bump tail?
    emitter.instruction("b.ne __rt_heap_grow_tail_fail");                       // non-tail blocks must use allocate-copy-free growth
    emitter.instruction("sub x10, x1, x9");                                     // compute the extra payload bytes needed by this extension
    emitter.instruction("adds x14, x13, x10");                                  // compute the tentative new bump offset with overflow detection
    emitter.instruction("b.cs __rt_heap_grow_tail_fail");                       // a wrapped bump offset cannot fit the configured heap
    crate::codegen_support::abi::emit_symbol_address(emitter, "x15", "_heap_max");
    emitter.instruction("ldr x15, [x15]");                                      // load the configured heap capacity for the extension check
    emitter.instruction("cmp x14, x15");                                        // does the extended tail remain inside the configured heap?
    emitter.instruction("b.hi __rt_heap_grow_tail_fail");                       // preserve fallback allocation when the tail has insufficient room

    // -- publish the extension and update live-byte accounting --
    emitter.instruction("str x14, [x12]");                                      // publish the advanced bump offset after every guard has passed
    emitter.instruction("str w1, [x0, #-16]");                                  // publish the extended payload size in the existing block header
    crate::codegen_support::abi::emit_symbol_address(emitter, "x12", "_gc_live");
    emitter.instruction("ldr x13, [x12]");                                      // load live-byte accounting before adding only the extension delta
    emitter.instruction("add x13, x13, x10");                                   // include the newly live tail bytes without counting another header
    emitter.instruction("str x13, [x12]");                                      // publish the updated live-byte total
    crate::codegen_support::abi::emit_symbol_address(emitter, "x12", "_gc_peak");
    emitter.instruction("ldr x14, [x12]");                                      // load the previous peak-live watermark
    emitter.instruction("cmp x13, x14");                                        // did in-place growth raise the peak-live watermark?
    emitter.instruction("csel x14, x13, x14, hi");                              // retain the larger live-byte value
    emitter.instruction("str x14, [x12]");                                      // publish the updated peak-live watermark

    // -- return either the unchanged pointer or a null fallback signal --
    emitter.label("__rt_heap_grow_tail_success");
    emitter.instruction("ret");                                                 // return the unchanged non-null payload pointer in x0
    emitter.label("__rt_heap_grow_tail_fail");
    emitter.instruction("mov x0, #0");                                          // report that the caller must use ordinary allocation
    emitter.instruction("ret");                                                 // return without changing allocator state
}

/// Emits the Linux x86_64 tail-extension helper.
fn emit_heap_grow_tail_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_grow_tail ---");
    emitter.label_global("__rt_heap_grow_tail");

    // -- normalize and validate the requested payload size --
    emitter.instruction("test rdi, rdi");                                       // null payloads cannot identify a live heap tail
    emitter.instruction("jz __rt_heap_grow_tail_fail");                         // return failure without reading a null header
    emitter.instruction("mov rax, rsi");                                        // copy the requested payload size before alignment
    emitter.instruction("add rax, 7");                                          // round the requested payload size toward the allocator alignment
    emitter.instruction("jc __rt_heap_grow_tail_fail");                         // reject an alignment addition that wrapped the machine word
    emitter.instruction("and rax, -8");                                         // keep the requested payload size eight-byte aligned
    emitter.instruction("mov r8, 0xffffffff");                                  // materialize the largest payload size representable by the header
    emitter.instruction("cmp rax, r8");                                         // does the aligned request fit the allocator's 32-bit size field?
    emitter.instruction("ja __rt_heap_grow_tail_fail");                         // let the ordinary allocator report oversized requests
    emitter.instruction("mov r10d, DWORD PTR [rdi - 16]");                      // load the current zero-extended payload size from the heap header
    emitter.instruction("cmp rax, r10");                                        // does the block already satisfy the requested payload size?
    emitter.instruction("jbe __rt_heap_grow_tail_success");                     // shrinking and equal-size requests need no allocator mutation

    // -- prove tail ownership and available heap capacity --
    emitter.instruction("lea r11, [rdi + r10]");                                // compute the current block end from its user pointer and payload size
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_heap_buf");
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_heap_off");
    emitter.instruction("mov rcx, QWORD PTR [r9]");                             // load the current bump offset before checking tail ownership
    emitter.instruction("lea r8, [r8 + rcx]");                                  // derive the current live heap end from its base and bump offset
    emitter.instruction("cmp r11, r8");                                         // does this block end exactly at the current bump tail?
    emitter.instruction("jne __rt_heap_grow_tail_fail");                        // non-tail blocks must use allocate-copy-free growth
    emitter.instruction("mov rdx, rax");                                        // copy the aligned new size before calculating the extension delta
    emitter.instruction("sub rdx, r10");                                        // compute the extra payload bytes needed by this extension
    emitter.instruction("add rcx, rdx");                                        // compute the tentative new bump offset with overflow detection
    emitter.instruction("jc __rt_heap_grow_tail_fail");                         // a wrapped bump offset cannot fit the configured heap
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_heap_max");
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // load the configured heap capacity for the extension check
    emitter.instruction("cmp rcx, r8");                                         // does the extended tail remain inside the configured heap?
    emitter.instruction("ja __rt_heap_grow_tail_fail");                         // preserve fallback allocation when the tail has insufficient room

    // -- publish the extension and update live-byte accounting --
    emitter.instruction("mov QWORD PTR [r9], rcx");                             // publish the advanced bump offset after every guard has passed
    emitter.instruction("mov DWORD PTR [rdi - 16], eax");                       // publish the extended payload size in the existing block header
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_gc_live");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load live-byte accounting before adding only the extension delta
    emitter.instruction("add r9, rdx");                                         // include the newly live tail bytes without counting another header
    emitter.instruction("mov QWORD PTR [r8], r9");                              // publish the updated live-byte total
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_gc_peak");
    emitter.instruction("mov rcx, QWORD PTR [r8]");                             // load the previous peak-live watermark
    emitter.instruction("cmp r9, rcx");                                         // did in-place growth raise the peak-live watermark?
    emitter.instruction("cmova rcx, r9");                                       // retain the larger live-byte value
    emitter.instruction("mov QWORD PTR [r8], rcx");                             // publish the updated peak-live watermark

    // -- return either the unchanged pointer or a null fallback signal --
    emitter.label("__rt_heap_grow_tail_success");
    emitter.instruction("mov rax, rdi");                                        // return the unchanged non-null payload pointer
    emitter.instruction("ret");                                                 // finish the successful extension probe
    emitter.label("__rt_heap_grow_tail_fail");
    emitter.instruction("xor eax, eax");                                        // report that the caller must use ordinary allocation
    emitter.instruction("ret");                                                 // return without changing allocator state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    /// Owns one unique assembly-test directory and removes it after the test.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        /// Removes only this test's uniquely named directory.
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Cross-assembles direct and PIC tail-growth helpers for every supported target.
    #[test]
    fn heap_tail_growth_assembles_on_all_supported_targets() {
        if Command::new("clang").arg("--version").output().is_err() {
            eprintln!("Clang unavailable; executable target CI provides assembly coverage");
            return;
        }

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the system clock must be later than the Unix epoch")
            .as_nanos();
        let dir = Scratch(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join(format!("tail-grow-assembly-{}-{nonce}", std::process::id())),
        );
        fs::create_dir_all(&dir.0).expect("the assembly scratch directory must be creatable");

        let targets = [
            ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13.0"),
            ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
            ("linux-aarch64", "aarch64-linux-gnu"),
            ("linux-x86_64", "x86_64-linux-gnu"),
        ];
        for (name, triple) in targets {
            for pic in [false, true] {
                let target = Target::parse(name)
                    .unwrap_or_else(|_| panic!("supported target {name} must parse"));
                let mut emitter = if pic {
                    Emitter::new_cdylib(target)
                } else {
                    Emitter::new(target)
                };
                if target.arch == Arch::X86_64 {
                    emitter.raw(".intel_syntax noprefix");
                }
                emitter.raw(".text");
                emit_heap_grow_tail(&mut emitter);
                let assembly = emitter.output();

                assert!(assembly.contains("_heap_off"), "{name}, pic={pic}");
                assert!(assembly.contains("_heap_max"), "{name}, pic={pic}");
                assert!(assembly.contains("_gc_live"), "{name}, pic={pic}");
                assert!(assembly.contains("_gc_peak"), "{name}, pic={pic}");
                assert!(!assembly.contains("_gc_allocs"), "{name}, pic={pic}");
                assert!(!assembly.contains("_gc_frees"), "{name}, pic={pic}");
                if target.arch == Arch::X86_64 {
                    assert!(assembly.contains("mov DWORD PTR [rdi - 16], eax"));
                } else {
                    assert!(assembly.contains("str w1, [x0, #-16]"));
                }

                let source = dir.0.join(format!("{name}-{pic}.s"));
                fs::write(&source, assembly).expect("generated assembly must be writable");
                let output = Command::new("clang")
                    .args(["-target", triple, "-c"])
                    .arg(&source)
                    .arg("-o")
                    .arg(source.with_extension("o"))
                    .output()
                    .expect("Clang must run after its availability probe succeeds");
                assert!(
                    output.status.success(),
                    "{name}, pic={pic}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}
