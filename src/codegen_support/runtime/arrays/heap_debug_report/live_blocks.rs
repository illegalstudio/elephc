//! Purpose:
//! Emits bounded live-allocation diagnostics after a non-clean heap summary.
//!
//! Called from:
//! - `super::emit_heap_debug_report()` on every supported target.
//!
//! Key details:
//! - Walks validated physical headers without allocating or exposing strings or payload pointers.
//! - Object class ids and Mixed tags are range checked before reporting their type identity.
//! - Reports at most 64 blocks, with offsets relative to the managed heap rather than addresses.
//! - String fingerprints inspect at most 32 validated payload bytes without printing their content.

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
    emit_type_identity(emitter);
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

/// Reports only bounded object and Mixed discriminants after the complete block was validated.
fn emit_type_identity(emitter: &mut Emitter) {
    let value = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, value, 48);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #1");                                  // recognize strings without exposing their bytes
            emitter.instruction("b.eq __rt_heap_debug_live_string");            // fingerprint only a bounded prefix of validated storage
            emitter.instruction("cmp x0, #2");                                  // recognize indexed arrays with bounded scalar metadata
            emitter.instruction("b.eq __rt_heap_debug_live_array");             // array lengths distinguish collectors from stored child lists
            emitter.instruction("cmp x0, #4");                                  // recognize native object allocations
            emitter.instruction("b.eq __rt_heap_debug_live_class");             // only object payloads carry a class id
            emitter.instruction("cmp x0, #5");                                  // recognize boxed Mixed allocations
            emitter.instruction("b.eq __rt_heap_debug_live_tag");               // only Mixed payloads carry a value tag
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 1");                                  // recognize strings without exposing their bytes
            emitter.instruction("je __rt_heap_debug_live_string");              // fingerprint only a bounded prefix of validated storage
            emitter.instruction("cmp rax, 2");                                  // recognize indexed arrays with bounded scalar metadata
            emitter.instruction("je __rt_heap_debug_live_array");               // array lengths distinguish collectors from stored child lists
            emitter.instruction("cmp rax, 4");                                  // recognize native object allocations
            emitter.instruction("je __rt_heap_debug_live_class");               // only object payloads carry a class id
            emitter.instruction("cmp rax, 5");                                  // recognize boxed Mixed allocations
            emitter.instruction("je __rt_heap_debug_live_tag");                 // only Mixed payloads carry a value tag
        }
    }
    abi::emit_jump(emitter, "__rt_heap_debug_live_type_done");
    emitter.label("__rt_heap_debug_live_string");
    emit_string_fingerprint(emitter);
    abi::emit_jump(emitter, "__rt_heap_debug_live_type_done");
    emitter.label("__rt_heap_debug_live_array");
    emit_array_identity(emitter);
    abi::emit_jump(emitter, "__rt_heap_debug_live_type_done");
    emitter.label("__rt_heap_debug_live_class");
    write_label(emitter, "_heap_dbg_block_class", 10);
    emit_first_payload_word(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", "_class_name_count", 0);
            emitter.instruction("cmp x0, x9");                                  // accept only ids represented in this program's class table
            emitter.instruction("b.lo __rt_heap_debug_live_class_valid");       // prevent malformed metadata from exposing a pointer
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", "_class_name_count", 0);
            emitter.instruction("cmp rax, r10");                                // accept only ids represented in this program's class table
            emitter.instruction("jb __rt_heap_debug_live_class_valid");         // prevent malformed metadata from exposing a pointer
        }
    }
    abi::emit_load_int_immediate(emitter, value, -1);
    emitter.label("__rt_heap_debug_live_class_valid");
    write_integer(emitter);
    emit_class_name(emitter);
    abi::emit_jump(emitter, "__rt_heap_debug_live_type_done");
    emitter.label("__rt_heap_debug_live_tag");
    write_label(emitter, "_heap_dbg_block_tag", 5);
    emit_first_payload_word(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #11");                                 // runtime tags run from int zero through invoker reference eleven
            emitter.instruction("b.ls __rt_heap_debug_live_tag_valid");         // report a type discriminant rather than arbitrary payload contents
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 11");                                 // runtime tags run from int zero through invoker reference eleven
            emitter.instruction("jbe __rt_heap_debug_live_tag_valid");          // report a type discriminant rather than arbitrary payload contents
        }
    }
    abi::emit_load_int_immediate(emitter, value, -1);
    emitter.label("__rt_heap_debug_live_tag_valid");
    write_integer(emitter);
    emitter.label("__rt_heap_debug_live_type_done");
}

/// Prints a 32-bit FNV-1a prefix fingerprint, stopping at NUL or the validated payload boundary.
fn emit_string_fingerprint(emitter: &mut Emitter) {
    write_label(emitter, "_heap_dbg_block_fingerprint", 13);
    let value = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, value, 40);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub x10, x0, #16");                            // derive the validated physical payload size
            emitter.instruction("mov x11, #32");                                // cap the inspected prefix independently of allocation size
            emitter.instruction("cmp x10, x11");                                // choose the smaller safe byte count
            emitter.instruction("csel x10, x10, x11, lo");                      // never inspect padding outside this block
            abi::load_at_offset(emitter, "x9", 8);
            emitter.instruction("add x9, x9, #16");                             // address this string's own payload without following pointers
            emitter.instruction("movz w0, #0x9dc5");                            // materialize the low half of the FNV-1a basis
            emitter.instruction("movk w0, #0x811c, lsl #16");                   // complete the unsigned 32-bit FNV-1a basis
            emitter.instruction("movz w12, #0x0193");                           // materialize the low half of the FNV-1a multiplier
            emitter.instruction("movk w12, #0x0100, lsl #16");                  // complete the unsigned 32-bit multiplier
            emitter.label("__rt_heap_debug_live_string_loop");
            emitter.instruction("cbz x10, __rt_heap_debug_live_string_done");   // stop at the bounded physical prefix length
            emitter.instruction("ldrb w11, [x9], #1");                          // inspect only one byte within the validated allocation
            emitter.instruction("cbz w11, __rt_heap_debug_live_string_done");   // avoid reading unused storage after a terminating NUL
            emitter.instruction("eor w0, w0, w11");                             // mix the next byte without retaining or displaying it
            emitter.instruction("mul w0, w0, w12");                             // apply wrapping 32-bit FNV-1a multiplication
            emitter.instruction("sub x10, x10, #1");                            // consume one validated prefix byte
        }
        Arch::X86_64 => {
            emitter.instruction("sub rax, 16");                                 // derive the validated physical payload size
            emitter.instruction("mov edx, 32");                                 // cap the inspected prefix independently of allocation size
            emitter.instruction("cmp rax, rdx");                                // choose the smaller safe byte count
            emitter.instruction("cmovb rdx, rax");                              // never inspect padding outside this block
            abi::load_at_offset(emitter, "r10", 8);
            emitter.instruction("add r10, 16");                                 // address this string's own payload without following pointers
            emitter.instruction("mov eax, 2166136261");                         // initialize the unsigned 32-bit FNV-1a basis
            emitter.label("__rt_heap_debug_live_string_loop");
            emitter.instruction("test rdx, rdx");                               // stop at the bounded physical prefix length
            emitter.instruction("jz __rt_heap_debug_live_string_done");         // no further byte belongs to the inspected prefix
            emitter.instruction("movzx ecx, BYTE PTR [r10]");                   // inspect only one byte within the validated allocation
            emitter.instruction("test ecx, ecx");                               // a terminating NUL ends the useful string prefix
            emitter.instruction("jz __rt_heap_debug_live_string_done");         // avoid reading unused storage after the terminator
            emitter.instruction("xor eax, ecx");                                // mix the next byte without retaining or displaying it
            emitter.instruction("imul eax, eax, 16777619");                     // apply wrapping 32-bit FNV-1a multiplication
            emitter.instruction("inc r10");                                     // advance within the validated payload
            emitter.instruction("dec rdx");                                     // consume one validated prefix byte
        }
    }
    abi::emit_jump(emitter, "__rt_heap_debug_live_string_loop");
    emitter.label("__rt_heap_debug_live_string_done");
    write_integer(emitter);
}

/// Reports array metadata only when the complete header and element capacity fit its block.
fn emit_array_identity(emitter: &mut Emitter) {
    let value = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, value, 40);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #40");                                 // require the allocation header and all three array header words
            emitter.instruction("b.lo __rt_heap_debug_live_type_done");         // malformed short arrays have no safe metadata to report
            emitter.instruction("sub x9, x0, #40");                             // isolate the available element payload bytes
            emitter.instruction("lsr x9, x9, #3");                              // bound capacity by the minimum eight-byte element width
            abi::load_at_offset(emitter, "x0", 8);
            emitter.instruction("ldr x10, [x0, #24]");                          // read array capacity from its validated header
            emitter.instruction("cmp x10, x9");                                 // capacity must fit the physical allocation
            emitter.instruction("b.hi __rt_heap_debug_live_type_done");         // skip invalid capacities without reading any element
            emitter.instruction("ldr x0, [x0, #16]");                           // read array length without following child pointers
            emitter.instruction("cmp x0, x10");                                 // length cannot exceed the validated capacity
            emitter.instruction("b.hi __rt_heap_debug_live_type_done");         // do not report malformed metadata as a meaningful length
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 40");                                 // require the allocation header and all three array header words
            emitter.instruction("jb __rt_heap_debug_live_type_done");           // malformed short arrays have no safe metadata to report
            emitter.instruction("mov r10, rax");                                // preserve the validated footprint while loading its header
            emitter.instruction("sub r10, 40");                                 // isolate the available element payload bytes
            emitter.instruction("shr r10, 3");                                  // bound capacity by the minimum eight-byte element width
            abi::load_at_offset(emitter, "rax", 8);
            emitter.instruction("mov r11, QWORD PTR [rax + 24]");               // read array capacity from its validated header
            emitter.instruction("cmp r11, r10");                                // capacity must fit the physical allocation
            emitter.instruction("ja __rt_heap_debug_live_type_done");           // skip invalid capacities without reading any element
            emitter.instruction("mov rax, QWORD PTR [rax + 16]");               // read array length without following child pointers
            emitter.instruction("cmp rax, r11");                                // length cannot exceed the validated capacity
            emitter.instruction("ja __rt_heap_debug_live_type_done");           // do not report malformed metadata as a meaningful length
        }
    }
    abi::store_at_offset(emitter, value, 64);
    write_label(emitter, "_heap_dbg_block_length", 8);
    abi::load_at_offset(emitter, value, 64);
    write_integer(emitter);
}

/// Adds a static class name only when its id addresses a row in the generated metadata table.
fn emit_class_name(emitter: &mut Emitter) {
    emit_first_payload_word(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", "_class_name_count", 0);
            emitter.instruction("cmp x0, x9");                                  // validate the id again after integer output clobbered its registers
            emitter.instruction("b.hs __rt_heap_debug_live_type_done");         // no class-name pointer is read for an invalid id
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", "_class_name_count", 0);
            emitter.instruction("cmp rax, r10");                                // validate the id again after integer output clobbered its registers
            emitter.instruction("jae __rt_heap_debug_live_type_done");          // no class-name pointer is read for an invalid id
        }
    }
    write_label(emitter, "_heap_dbg_block_class_name", 7);
    emit_first_payload_word(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_class_name_entries");
            emitter.instruction("add x9, x9, x0, lsl #4");                      // address the validated sixteen-byte class-name row
            emitter.instruction("ldp x1, x2, [x9]");                            // borrow the generated name pointer and exact length
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", "_class_name_entries");
            emitter.instruction("shl rax, 4");                                  // scale the validated class id by the row size
            emitter.instruction("add r10, rax");                                // select this program's static class-name row
            emitter.instruction("mov rax, QWORD PTR [r10]");                    // borrow the generated name pointer
            emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                // borrow the generated name length
        }
    }
    write_string(emitter);
}

/// Reads the first word of a block whose header, minimum payload and end were already checked.
fn emit_first_payload_word(emitter: &mut Emitter) {
    let value = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, value, 8);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("ldr x0, [x0, #16]"),              // skip the validated sixteen-byte header
        Arch::X86_64 => emitter.instruction("mov rax, QWORD PTR [rax + 16]"),   // skip the validated sixteen-byte header
    }
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
            assert_eq!(asm.matches("__rt_itoa").count(), 8, "{target}");
            assert!(asm.contains("_heap_dbg_block_class") && asm.contains("_heap_dbg_block_tag"), "{target}");
            assert!(asm.contains("_heap_dbg_block_length"), "{target}");
            assert!(asm.contains("_heap_dbg_block_fingerprint"), "{target}");
            assert!(asm.contains("mov x11, #32") || asm.contains("mov edx, 32"), "{target}");
            assert!(asm.contains("_class_name_count") && asm.contains("_class_name_entries"), "{target}");
            assert!(!asm.contains("__rt_heap_alloc") && !asm.contains("__rt_incref"), "{target}");
            assert_eq!("HEAP DEBUG: live offset=".len(), 24);
        }
    }
}
