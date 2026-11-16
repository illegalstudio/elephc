//! Purpose:
//! Emits the `__rt_ptoa`, `__rt_ptoa_find_start` runtime helper assembly for pointer-to-string formatting.
//! Keeps compiler pointer extension conversions and fatal checks aligned with generated pointer operations.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::pointers`.
//!
//! Key details:
//! - Pointer helpers must keep null checks and C-string conversions aligned with the pointer extension ABI.
//! - `__rt_ptoa` reserves its at most 18 bytes through `__rt_concat_reserve` and publishes the
//!   written length through `__rt_concat_publish`, so its result can feed a concatenation
//!   without overwriting earlier pieces and never writes past the 64 KiB `_concat_buf`.
//! - When the scratch buffer is full the result lands in a heap block stamped with
//!   `CONCAT_TEMP_HEAP_KIND`, the same transient kind `__rt_concat` uses: `i_to_str` results
//!   are scratch strings that codegen never releases, so `__rt_str_persist` adopts the block
//!   in place instead of duplicating it.

use crate::codegen_support::sentinels::CONCAT_TEMP_HEAP_KIND;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Longest pointer spelling: `0x` plus sixteen hexadecimal digits.
const PTOA_MAX_LEN: usize = 18;

/// Emits the `__rt_ptoa` runtime helper that formats a pointer address as a lowercase hex string.
///
/// Input: x0 = pointer value (64-bit address)
/// Output: x1 = pointer to the string, x2 = string length
///
/// Formats the pointer as "0x" followed by hexadecimal digits, skipping leading zeros.
/// Null pointers are formatted as "0x0". Uses a loop to emit each nibble, converting
/// values 0-9 to '0'-'9' and 10-15 to 'a'-'f'. The result (at most 18 bytes) goes into
/// storage from `__rt_concat_reserve`: the current concat scratch cursor while it fits, a
/// transient heap block otherwise. `__rt_concat_publish` then advances `_concat_off` only for
/// scratch results, so `echo`, string casts, interpolation and concatenation operands all
/// share one formatter without clobbering a partially built concatenation or running past
/// the scratch buffer. Clobbers every caller-saved register, because the reservation can
/// reach `__rt_heap_alloc`. Dispatches to the x86_64 Linux implementation when targeting
/// that architecture.
pub(crate) fn emit_ptoa(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ptoa_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for ARM64 instructions
    emitter.comment("--- runtime: ptoa (pointer to hex string) ---");
    emitter.label_global("__rt_ptoa");

    // -- set up a frame that keeps the pointer value across the reservation --
    emitter.instruction("sub sp, sp, #32");                                     // allocate a frame for the pointer value and the saved frame registers
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save the frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the ptoa frame pointer
    emitter.instruction("str x0, [sp]");                                        // keep the pointer value across the reservation call

    // -- reserve bounded storage instead of appending blindly at _concat_off --
    emitter.instruction(&format!("mov x0, #{}", PTOA_MAX_LEN));                 // request room for the longest pointer spelling
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or heap storage for the digits

    // -- stamp heap-backed results so __rt_str_persist can take them over in place --
    abi::emit_symbol_address(emitter, "x9", "_concat_buf");
    emitter.instruction("sub x10, x0, x9");                                     // compute the candidate scratch offset of the reservation
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x11", emitter.concat_capacity as i64); // load the concat scratch capacity in bytes
    emitter.instruction("cmp x10, x11");                                        // is the reservation outside the shared scratch window (unsigned)?
    emitter.instruction("b.lo __rt_ptoa_dest_ready");                           // scratch-backed reservations carry no heap header to stamp
    emitter.instruction(&format!("mov x9, #{}", CONCAT_TEMP_HEAP_KIND));        // heap kind 7 = transient string temporary
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the heap reservation as a transient string
    emitter.label("__rt_ptoa_dest_ready");
    emitter.instruction("mov x1, x0");                                          // x1 = output start
    emitter.instruction("mov x3, x0");                                          // x3 = write cursor
    emitter.instruction("ldr x0, [sp]");                                        // reload the pointer value to format

    // -- write "0x" prefix --
    emitter.instruction("mov w4, #0x30");                                       // ASCII '0'
    emitter.instruction("strb w4, [x3], #1");                                   // write '0', advance cursor
    emitter.instruction("mov w4, #0x78");                                       // ASCII 'x'
    emitter.instruction("strb w4, [x3], #1");                                   // write 'x', advance cursor

    // -- handle zero specially --
    emitter.instruction("cbnz x0, __rt_ptoa_find_start");                       // non-zero, find first nibble
    emitter.instruction("mov w4, #0x30");                                       // ASCII '0'
    emitter.instruction("strb w4, [x3], #1");                                   // write single '0' for null pointer
    emitter.instruction("b __rt_ptoa_done");                                    // skip to end

    // -- find first non-zero nibble (skip leading zeros) --
    emitter.label("__rt_ptoa_find_start");
    emitter.instruction("clz x5, x0");                                          // count leading zero bits
    emitter.instruction("lsr x5, x5, #2");                                      // divide by 4 = leading zero nibbles
    emitter.instruction("mov x6, #16");                                         // total nibbles in 64-bit value
    emitter.instruction("sub x6, x6, x5");                                      // x6 = significant nibbles to emit
    emitter.instruction("lsl x5, x5, #2");                                      // x5 = bits to shift left to align first nibble
    emitter.instruction("lsl x0, x0, x5");                                      // shift value so first significant nibble is at top

    // -- emit hex digits loop --
    emitter.label("__rt_ptoa_loop");
    emitter.instruction("cbz x6, __rt_ptoa_done");                              // all nibbles emitted
    emitter.instruction("lsr x4, x0, #60");                                     // extract top 4 bits (current nibble)
    emitter.instruction("cmp x4, #10");                                         // is it >= 10?
    emitter.instruction("b.ge __rt_ptoa_hex_letter");                           // yes, use a-f
    emitter.instruction("add x4, x4, #0x30");                                   // convert 0-9 to ASCII '0'-'9'
    emitter.instruction("b __rt_ptoa_store");                                   // go store the digit

    emitter.label("__rt_ptoa_hex_letter");
    emitter.instruction("add x4, x4, #0x57");                                   // convert 10-15 to ASCII 'a'-'f' (10+0x57=0x61='a')

    emitter.label("__rt_ptoa_store");
    emitter.instruction("strb w4, [x3], #1");                                   // store hex digit, advance cursor
    emitter.instruction("lsl x0, x0, #4");                                      // shift next nibble into top position
    emitter.instruction("sub x6, x6, #1");                                      // decrement remaining nibble count
    emitter.instruction("b __rt_ptoa_loop");                                    // continue loop

    // -- compute length and return --
    emitter.label("__rt_ptoa_done");
    emitter.instruction("sub x2, x3, x1");                                      // x2 = length (cursor - start)
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch results
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the ptoa frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux implementation of the `__rt_ptoa` runtime helper.
///
/// Mirrors the ARM64 logic but uses x86_64 registers and instruction encoding.
/// Preserves the incoming pointer in r11 since the call site passes it in rax (the
/// integer result register). Formats the pointer as "0x" followed by lowercase hex
/// digits, with null pointers rendered as "0x0". Uses BSR to find the highest set
/// bit, then shifts the value left by `60 - (msb & !3)` so the most significant nibble
/// sits in bits 60..63 before the loop peels one nibble per digit. Rounding `60 - msb`
/// down instead (the earlier form) was one nibble short whenever the top set bit was not
/// the lowest bit of its nibble, so `0xf` printed `0x0` and typical addresses lost a digit.
///
/// Output: rax = pointer to the string, rdx = string length. The bytes go into storage from
/// `__rt_concat_reserve` (heap fallback stamped `CONCAT_TEMP_HEAP_KIND`) and
/// `__rt_concat_publish` advances `_concat_off` only for scratch results.
fn emit_ptoa_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ptoa (pointer to hex string) ---");
    emitter.label_global("__rt_ptoa");

    // -- set up a frame that keeps the pointer value across the reservation --
    emitter.instruction("push rbp");                                            // save the caller frame pointer and realign the stack
    emitter.instruction("mov rbp, rsp");                                        // establish the ptoa frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve an aligned slot for the pointer value
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the incoming pointer value across the reservation

    // -- reserve bounded storage instead of appending blindly at _concat_off --
    emitter.instruction(&format!("mov eax, {}", PTOA_MAX_LEN));                 // request room for the longest pointer spelling
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or heap storage for the digits

    // -- stamp heap-backed results so __rt_str_persist can take them over in place --
    abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("mov r11, rax");                                        // copy the reservation before deriving its scratch offset
    emitter.instruction("sub r11, r10");                                        // compute the candidate scratch offset of the reservation
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "r9", emitter.concat_capacity as i64); // load the shared scratch capacity
    emitter.instruction("cmp r11, r9");                                         // is the reservation outside the shared scratch window?
    emitter.instruction("jb __rt_ptoa_dest_ready_linux_x86_64");                // scratch reservations carry no heap header to stamp
    abi::emit_load_int_immediate(emitter, "r10",
        crate::codegen_support::sentinels::x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND) as i64);
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the heap reservation as a transient string
    emitter.label("__rt_ptoa_dest_ready_linux_x86_64");
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the pointer value to format
    emitter.instruction("mov rsi, rax");                                        // seed the write cursor from the output start
    emitter.instruction("mov BYTE PTR [rsi], 0x30");                            // write the leading '0' of the hexadecimal pointer prefix
    emitter.instruction("add rsi, 1");                                          // advance the write cursor after storing the leading '0'
    emitter.instruction("mov BYTE PTR [rsi], 0x78");                            // write the trailing 'x' of the hexadecimal pointer prefix
    emitter.instruction("add rsi, 1");                                          // advance the write cursor after storing the trailing 'x'
    emitter.instruction("test r11, r11");                                       // detect the null pointer case before scanning for the first significant nibble
    emitter.instruction("jnz __rt_ptoa_find_start_linux_x86_64");               // skip the null fast path when the incoming pointer value is non-zero
    emitter.instruction("mov BYTE PTR [rsi], 0x30");                            // encode a single trailing '0' digit when the incoming pointer value is null
    emitter.instruction("add rsi, 1");                                          // advance the write cursor after materializing the null pointer payload digit
    emitter.instruction("jmp __rt_ptoa_done_linux_x86_64");                     // finish once the hexadecimal null pointer spelling has been emitted

    emitter.label("__rt_ptoa_find_start_linux_x86_64");
    emitter.instruction("bsr r8, r11");                                         // locate the highest set bit so the helper can skip leading zero nibbles
    emitter.instruction("mov r9, r8");                                          // copy the highest-set-bit index before converting it into a nibble count
    emitter.instruction("shr r9, 2");                                           // divide the highest-set-bit index by four to obtain the most significant nibble index
    emitter.instruction("add r9, 1");                                           // convert the nibble index into the total number of significant hexadecimal digits
    emitter.instruction("mov r10, r8");                                         // copy the highest-set-bit index before aligning it to its nibble
    emitter.instruction("and r10, -4");                                         // round the highest-set-bit index down to the first bit of its nibble
    emitter.instruction("neg r10");                                             // negate the nibble start so the shift can be computed as 60 minus it
    emitter.instruction("add r10, 60");                                         // shift = 60 - nibble start, which lifts the most significant nibble to bits 60..63
    emitter.instruction("mov rdx, r11");                                        // copy the incoming pointer value into a scratch register that can be shifted during digit emission
    emitter.instruction("mov cl, r10b");                                        // move the initial nibble-alignment shift into the x86 variable-shift register
    emitter.instruction("shl rdx, cl");                                         // align the first significant nibble at the top of the working pointer value

    emitter.label("__rt_ptoa_loop_linux_x86_64");
    emitter.instruction("test r9, r9");                                         // stop once every significant hexadecimal digit has been emitted
    emitter.instruction("jz __rt_ptoa_done_linux_x86_64");                      // finish once no significant hexadecimal digits remain
    emitter.instruction("mov rcx, rdx");                                        // copy the shifted working pointer value before extracting the current high nibble
    emitter.instruction("shr rcx, 60");                                         // isolate the current hexadecimal digit in the low bits of the scratch register
    emitter.instruction("cmp rcx, 10");                                         // decide whether the current nibble maps to '0'-'9' or 'a'-'f'
    emitter.instruction("jge __rt_ptoa_hex_letter_linux_x86_64");               // branch to the alphabetic hexadecimal digit path for nibble values ten through fifteen
    emitter.instruction("add ecx, 0x30");                                       // convert nibble values zero through nine into ASCII '0' through '9'
    emitter.instruction("jmp __rt_ptoa_store_linux_x86_64");                    // skip the alphabetic adjustment once the numeric digit has been materialized

    emitter.label("__rt_ptoa_hex_letter_linux_x86_64");
    emitter.instruction("add ecx, 0x57");                                       // convert nibble values ten through fifteen into ASCII 'a' through 'f'

    emitter.label("__rt_ptoa_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [rsi], cl");                              // store the current hexadecimal digit into the concat buffer
    emitter.instruction("add rsi, 1");                                          // advance the write cursor after storing one hexadecimal digit
    emitter.instruction("shl rdx, 4");                                          // shift the next nibble into the high position for the following loop iteration
    emitter.instruction("sub r9, 1");                                           // decrement the count of significant hexadecimal digits still left to emit
    emitter.instruction("jmp __rt_ptoa_loop_linux_x86_64");                     // continue emitting the remaining hexadecimal digits

    emitter.label("__rt_ptoa_done_linux_x86_64");
    emitter.instruction("mov rdx, rsi");                                        // copy the final write cursor before converting it into the emitted string length
    emitter.instruction("sub rdx, rax");                                        // compute the emitted string length as write cursor minus output start
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch results
    emitter.instruction("mov rsp, rbp");                                        // release the pointer value slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the string pointer and length in rax and rdx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target reserves the pointer spelling through `__rt_concat_reserve` before the first
    /// byte store and publishes it through `__rt_concat_publish`, so a full concat scratch buffer
    /// moves the result to the heap instead of writing past `_concat_buf`.
    #[test]
    fn ptoa_reserves_before_writing_and_publishes_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_ptoa(&mut emitter);
            let asm = emitter.output();
            let (reserve, publish, first_store) = if target.arch == Arch::X86_64 {
                ("call __rt_concat_reserve\n", "call __rt_concat_publish\n", "mov BYTE PTR [rsi]")
            } else {
                ("bl __rt_concat_reserve\n", "bl __rt_concat_publish\n", "strb w4")
            };
            let reserve_at = asm.find(reserve).unwrap_or_else(|| panic!("{name}: no reservation"));
            let store_at = asm.find(first_store).unwrap_or_else(|| panic!("{name}: no byte store"));
            assert!(reserve_at < store_at, "{name}: ptoa stores before reserving");
            assert!(asm.contains(publish), "{name}: ptoa does not publish its length");
            assert!(!asm.contains("_concat_off"), "{name}: ptoa still moves the cursor itself");
        }
    }

    /// A heap-backed reservation is stamped with the transient concat kind on both ABIs, so
    /// `__rt_str_persist` adopts it instead of leaving a duplicated block behind.
    #[test]
    fn ptoa_marks_heap_fallback_results_transient() {
        let mut arm = Emitter::new(Target::parse("linux-aarch64").unwrap());
        emit_ptoa(&mut arm);
        assert!(arm.output().contains(&format!("mov x9, #{}", CONCAT_TEMP_HEAP_KIND)));

        let mut x86 = Emitter::new(Target::parse("linux-x86_64").unwrap());
        emit_ptoa(&mut x86);
        let x86_asm = x86.output();
        let kind = crate::codegen_support::sentinels::x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND);
        assert!(
            x86_asm.contains(&format!("0x{:x}", kind)) || x86_asm.contains(&kind.to_string()),
            "x86_64 ptoa does not stamp the transient concat kind"
        );
    }
}
