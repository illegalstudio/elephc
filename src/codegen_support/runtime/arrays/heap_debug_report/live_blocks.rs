//! Purpose:
//! Emits bounded live-allocation diagnostics after a non-clean heap summary.
//!
//! Called from:
//! - `super::emit_heap_debug_report()` on every supported target.
//!
//! Key details:
//! - Walks validated physical headers without allocating or exposing payload contents.
//! - Reports at most 64 blocks, with offsets relative to the managed heap rather than addresses.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Reports each live header's footprint, heap kind, and reference count without mutating owners.
pub(super) fn emit_live_blocks(emitter: &mut Emitter) {
    emitter.label_global("__rt_heap_debug_live_blocks");
    abi::emit_frame_prologue(emitter, 80);
    let value = abi::int_result_reg(emitter);
    abi::emit_symbol_address(emitter, value, "_heap_buf");
    abi::store_at_offset(emitter, value, 8);
    abi::emit_load_symbol_to_reg(emitter, value, "_heap_off", 0);
    let scratch = match emitter.target.arch { Arch::AArch64 => "x9", Arch::X86_64 => "r10" };
    abi::emit_symbol_address(emitter, scratch, "_heap_buf");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("add x0, x0, x9"),                 // compute the exclusive managed-heap end
        Arch::X86_64 => emitter.instruction("add rax, r10"),                    // compute the exclusive managed-heap end
    }
    abi::store_at_offset(emitter, value, 16);
    abi::emit_load_int_immediate(emitter, value, 0);
    abi::store_at_offset(emitter, value, 32);

    emitter.label("__rt_heap_debug_live_blocks_loop");
    abi::load_at_offset(emitter, value, 8);
    abi::load_at_offset(emitter, scratch, 16);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, x9");                                  // stop before dereferencing beyond the bump extent
            emitter.instruction("b.hs __rt_heap_debug_live_blocks_done");       // no physical header remains
            emitter.instruction("sub x10, x9, x0");                             // calculate bytes available for a complete header
            emitter.instruction("cmp x10, #16");                                // validate all header words before reading them
            emitter.instruction("b.lo __rt_heap_debug_live_blocks_done");       // stop safely on a truncated header
            emitter.instruction("ldr w10, [x0]");                               // read the unsigned physical payload size
            emitter.instruction("cmp x10, #8");                                 // every reusable allocation has at least eight payload bytes
            emitter.instruction("b.lo __rt_heap_debug_live_blocks_done");       // reject zero-size or malformed blocks
            emitter.instruction("add x10, x10, #16");                           // include the uniform header in the reported footprint
            emitter.instruction("add x11, x0, x10");                            // derive the next physical block boundary
            emitter.instruction("cmp x11, x9");                                 // validate the full block against the saved heap end
            emitter.instruction("b.hi __rt_heap_debug_live_blocks_done");       // do not follow an oversized or corrupted header
            abi::store_at_offset(emitter, "x11", 24);
            abi::store_at_offset(emitter, "x10", 40);
            emitter.instruction("ldr w10, [x0, #4]");                           // inspect the live reference count
            emitter.instruction("cbz x10, __rt_heap_debug_live_blocks_next");   // free-list blocks do not contribute live diagnostics
            abi::store_at_offset(emitter, "x10", 56);
            emitter.instruction("ldr x10, [x0, #8]");                           // read heap kind and internal flags
            emitter.instruction("and x10, x10, #0xff");                         // report only the public heap-kind byte
            abi::store_at_offset(emitter, "x10", 48);
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, r10");                                // stop before dereferencing beyond the bump extent
            emitter.instruction("jae __rt_heap_debug_live_blocks_done");        // no physical header remains
            emitter.instruction("mov r11, r10");                                // preserve the managed end while deriving remaining bytes
            emitter.instruction("sub r11, rax");                                // calculate bytes available for a complete header
            emitter.instruction("cmp r11, 16");                                 // validate all header words before reading them
            emitter.instruction("jb __rt_heap_debug_live_blocks_done");         // stop safely on a truncated header
            emitter.instruction("mov r11d, DWORD PTR [rax]");                   // read the unsigned physical payload size
            emitter.instruction("cmp r11, 8");                                  // every reusable allocation has at least eight payload bytes
            emitter.instruction("jb __rt_heap_debug_live_blocks_done");         // reject zero-size or malformed blocks
            emitter.instruction("add r11, 16");                                 // include the uniform header in the reported footprint
            abi::store_at_offset(emitter, "r11", 40);
            emitter.instruction("add r11, rax");                                // derive the next physical block boundary
            emitter.instruction("cmp r11, r10");                                // validate the full block against the saved heap end
            emitter.instruction("ja __rt_heap_debug_live_blocks_done");         // do not follow an oversized or corrupted header
            abi::store_at_offset(emitter, "r11", 24);
            emitter.instruction("mov r11d, DWORD PTR [rax + 4]");               // inspect the live reference count
            emitter.instruction("test r11d, r11d");                             // zero reference counts identify free-list blocks
            emitter.instruction("jz __rt_heap_debug_live_blocks_next");         // skip freed storage in the diagnostic output
            abi::store_at_offset(emitter, "r11", 56);
            emitter.instruction("movzx r11d, BYTE PTR [rax + 8]");              // discard the heap marker and internal flags
            abi::store_at_offset(emitter, "r11", 48);
        }
    }
    write_label(emitter, "_heap_dbg_block_offset", 24);
    abi::load_at_offset(emitter, value, 8);
    abi::emit_symbol_address(emitter, scratch, "_heap_buf");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("sub x0, x0, x9"),                 // report a heap-relative offset without disclosing an address
        Arch::X86_64 => emitter.instruction("sub rax, r10"),                    // report a heap-relative offset without disclosing an address
    }
    write_integer(emitter);
    for (label, len, offset) in [
        ("_heap_dbg_block_bytes", 7, 40),
        ("_heap_dbg_block_kind", 6, 48),
        ("_heap_dbg_block_refs", 6, 56),
    ] {
        write_label(emitter, label, len);
        abi::load_at_offset(emitter, value, offset);
        write_integer(emitter);
    }
    write_label(emitter, "_heap_dbg_newline", 1);
    abi::load_at_offset(emitter, value, 32);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("add x0, x0, #1");                              // count this reported live block
            emitter.instruction("cmp x0, #64");                                 // cap diagnostics independently of total heap occupancy
            emitter.instruction("b.hs __rt_heap_debug_live_blocks_done");       // keep failure logs bounded
        }
        Arch::X86_64 => {
            emitter.instruction("add rax, 1");                                  // count this reported live block
            emitter.instruction("cmp rax, 64");                                 // cap diagnostics independently of total heap occupancy
            emitter.instruction("jae __rt_heap_debug_live_blocks_done");        // keep failure logs bounded
        }
    }
    abi::store_at_offset(emitter, value, 32);
    emitter.label("__rt_heap_debug_live_blocks_next");
    abi::load_at_offset(emitter, value, 24);
    abi::store_at_offset(emitter, value, 8);
    abi::emit_jump(emitter, "__rt_heap_debug_live_blocks_loop");
    emitter.label("__rt_heap_debug_live_blocks_done");
    abi::emit_frame_restore(emitter, 80);
    abi::emit_return(emitter);
}

/// Writes one fixed diagnostic label through the target's stderr syscall ABI.
fn write_label(emitter: &mut Emitter, label: &str, len: i64) {
    let (ptr, length) = abi::string_result_regs(emitter);
    abi::emit_symbol_address(emitter, ptr, label);
    abi::emit_load_int_immediate(emitter, length, len);
    write_string(emitter);
}

/// Formats a header field in the existing static conversion buffer, then writes it to stderr.
fn write_integer(emitter: &mut Emitter) {
    abi::emit_call_label(emitter, "__rt_itoa");
    write_string(emitter);
}

/// Writes the native string result without heap allocation or payload inspection.
fn write_string(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // route live-allocation diagnostics to stderr
            emitter.syscall(4);
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // move the native string pointer into the Linux write ABI
            emitter.instruction("mov edi, 2");                                  // route live-allocation diagnostics to stderr
            emitter.instruction("mov eax, 1");                                  // select Linux write without libc or heap allocation
            emitter.instruction("syscall");                                     // write one field from fixed or static conversion storage
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target bounds the scan and log size without allocating while inspecting leaks.
    #[test]
    fn live_block_reports_are_bounded_and_non_allocating_on_all_targets() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_live_blocks(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("_heap_off") && asm.contains("__rt_heap_debug_live_blocks_next:"), "{target}");
            assert!(asm.contains("cmp x0, #64") || asm.contains("cmp rax, 64"), "{target}");
            assert_eq!(asm.matches("__rt_itoa").count(), 4, "{target}");
            assert!(!asm.contains("__rt_heap_alloc") && !asm.contains("__rt_incref"), "{target}");
            assert_eq!("HEAP DEBUG: live offset=".len(), 24);
        }
    }
}
