//! Purpose:
//! Emits `__rt_vsprintf`, the array→variadic bridge behind `vsprintf()`,
//! `vprintf()`, and `vfprintf()`. Given a format string and an arguments
//! array, it pushes one 16-byte tagged record per array element (the exact
//! layout `__rt_sprintf` consumes) and tail-calls `__rt_sprintf`, which formats
//! the string and pops the caller-owned records on return.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//! - The `vsprintf`/`vprintf`/`vfprintf` builtin emitters.
//!
//! Key details:
//! - Record tags match `__rt_sprintf`: int=0, string=1|(len<<8), float=2,
//!   bool=3. Records are pushed in reverse element order so element 0 lands at
//!   the lowest address (the first argument), as `__rt_sprintf` expects.
//! - The arguments array is read by its runtime value_type (kind word at
//!   `[arr-8]`, bits 8..14): a Mixed array (7) holds boxed-cell pointers that
//!   are unboxed per element; typed arrays (int/float/bool = 8-byte slots,
//!   string = 16-byte ptr+len slots) are read directly. The array is NOT
//!   mutated — elements are only read.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_vsprintf(array_ptr, fmt_ptr, fmt_len) -> string`.
///
/// Inputs: x0 = array pointer, x1 = format pointer, x2 = format length
/// (AArch64); rdi = array pointer, rax = format pointer, rdx = format length
/// (x86_64). Output: the formatted string in the standard string-result
/// registers (x1/x2 on AArch64, rax/rdx on x86_64), via `__rt_sprintf`.
pub fn emit_vsprintf(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_vsprintf_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: vsprintf ---");
    emitter.label_global("__rt_vsprintf");

    // Frame: 80 bytes. [x29+0..16] saved x29/x30; locals above: [x29+16] fmt
    //   ptr, [x29+24] fmt len, [x29+32] count, [x29+40] slot base, [x29+48]
    //   value_type, [x29+56] slot size, [x29+64] loop index.
    emitter.instruction("sub sp, sp, #80");                                     // helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // fixed frame pointer (sp moves while records are pushed)
    emitter.instruction("str x1, [x29, #16]");                                  // save the format pointer
    emitter.instruction("str x2, [x29, #24]");                                  // save the format length

    // -- read the array length and runtime value_type --
    emitter.instruction("cbz x0, __rt_vsprintf_empty");                         // null array → format with zero args
    emitter.instruction("ldr x9, [x0]");                                        // array logical length
    emitter.instruction("str x9, [x29, #32]");                                  // save the argument count
    emitter.instruction("add x10, x0, #24");                                    // skip the 24-byte array header to the slot payload
    emitter.instruction("str x10, [x29, #40]");                                 // save the slot base pointer
    emitter.instruction("ldr x11, [x0, #-8]");                                  // packed array kind word
    emitter.instruction("lsr x11, x11, #8");                                    // shift the runtime value_type into the low byte
    emitter.instruction("and x11, x11, #0x7f");                                 // isolate the value_type tag (drop the COW bit)
    emitter.instruction("str x11, [x29, #48]");                                 // save the value_type
    emitter.instruction("mov x12, #8");                                         // default slot size = 8 bytes
    emitter.instruction("cmp x11, #1");                                         // string-typed array (16-byte ptr+len slots)?
    emitter.instruction("mov x13, #16");                                        // string slot size = 16 bytes
    emitter.instruction("csel x12, x13, x12, eq");                              // pick 16 for string arrays, else 8
    emitter.instruction("str x12, [x29, #56]");                                 // save the slot size

    // -- push one 16-byte tagged record per element, in reverse order --
    emitter.instruction("sub x14, x9, #1");                                     // loop index = count - 1 (last element first)
    emitter.instruction("str x14, [x29, #64]");                                 // save the loop index
    emitter.label("__rt_vsprintf_loop");
    emitter.instruction("ldr x14, [x29, #64]");                                 // current index
    emitter.instruction("cmp x14, #0");                                         // exhausted the array?
    emitter.instruction("b.lt __rt_vsprintf_format");                           // all elements pushed → format
    emitter.instruction("ldr x10, [x29, #40]");                                 // slot base
    emitter.instruction("ldr x12, [x29, #56]");                                 // slot size
    emitter.instruction("madd x15, x14, x12, x10");                             // slot address = base + index * size
    emitter.instruction("ldr x11, [x29, #48]");                                 // value_type
    // x16 = record payload, x17 = record tag (built below per value_type)
    emitter.instruction("cmp x11, #7");                                         // boxed-Mixed array slot?
    emitter.instruction("b.eq __rt_vsprintf_mixed");                            // unbox the boxed cell
    emitter.instruction("cmp x11, #1");                                         // string array slot (ptr+len)?
    emitter.instruction("b.eq __rt_vsprintf_str");                              // build a string record
    // int (0) / float (2) / bool (3): one payload word, tag == value_type
    emitter.instruction("ldr x16, [x15]");                                      // payload = the typed slot value
    emitter.instruction("mov x17, x11");                                        // record tag = value_type (0/2/3 map 1:1)
    emitter.instruction("b __rt_vsprintf_push");                                // push the record
    emitter.label("__rt_vsprintf_str");
    emitter.instruction("ldr x16, [x15]");                                      // string pointer payload
    emitter.instruction("ldr x9, [x15, #8]");                                   // string length
    emitter.instruction("lsl x17, x9, #8");                                     // pack the length into the metadata word
    emitter.instruction("orr x17, x17, #1");                                    // tag 1 = string operand
    emitter.instruction("b __rt_vsprintf_push");                                // push the record
    emitter.label("__rt_vsprintf_mixed");
    emitter.instruction("ldr x15, [x15]");                                      // boxed Mixed cell pointer
    emitter.instruction("cbz x15, __rt_vsprintf_mixed_zero");                   // null cell → integer zero record
    emitter.instruction("ldr x9, [x15]");                                       // cell runtime tag
    emitter.instruction("ldr x16, [x15, #8]");                                  // cell low payload word
    emitter.instruction("cmp x9, #1");                                          // string cell?
    emitter.instruction("b.eq __rt_vsprintf_mixed_str");                        // build a string record from the cell
    emitter.instruction("cmp x9, #2");                                          // float cell?
    emitter.instruction("mov x17, #2");                                         // tag 2 = float operand
    emitter.instruction("b.eq __rt_vsprintf_push");                             // float payload already in x16
    emitter.instruction("cmp x9, #3");                                          // bool cell?
    emitter.instruction("mov x17, #3");                                         // tag 3 = bool operand
    emitter.instruction("b.eq __rt_vsprintf_push");                             // bool payload already in x16
    emitter.instruction("mov x17, #0");                                         // anything else → integer operand
    emitter.instruction("b __rt_vsprintf_push");                                // push the record
    emitter.label("__rt_vsprintf_mixed_str");
    emitter.instruction("ldr x9, [x15, #16]");                                  // cell high word = string length
    emitter.instruction("lsl x17, x9, #8");                                     // pack the length into the metadata word
    emitter.instruction("orr x17, x17, #1");                                    // tag 1 = string operand
    emitter.instruction("b __rt_vsprintf_push");                                // push the record
    emitter.label("__rt_vsprintf_mixed_zero");
    emitter.instruction("mov x16, #0");                                         // zero payload for a null cell
    emitter.instruction("mov x17, #0");                                         // integer-zero tag
    emitter.label("__rt_vsprintf_push");
    emitter.instruction("sub sp, sp, #16");                                     // reserve one 16-byte tagged record
    emitter.instruction("str x16, [sp, #0]");                                   // store the payload word
    emitter.instruction("str x17, [sp, #8]");                                   // store the tag/metadata word
    emitter.instruction("ldr x14, [x29, #64]");                                 // reload the loop index
    emitter.instruction("sub x14, x14, #1");                                    // step to the previous element
    emitter.instruction("str x14, [x29, #64]");                                 // store the loop index
    emitter.instruction("b __rt_vsprintf_loop");                                // push the next record

    emitter.label("__rt_vsprintf_empty");
    emitter.instruction("str xzr, [x29, #32]");                                 // zero argument count

    emitter.label("__rt_vsprintf_format");
    emitter.instruction("ldr x0, [x29, #32]");                                  // arg count
    emitter.instruction("ldr x1, [x29, #16]");                                  // format pointer
    emitter.instruction("ldr x2, [x29, #24]");                                  // format length
    emitter.instruction("bl __rt_sprintf");                                     // format; pops the count*16 records, returns x1/x2
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the formatted string (x1/x2)
}

/// Emits the Linux x86_64 string runtime helper for vsprintf.
fn emit_vsprintf_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: vsprintf ---");
    emitter.label_global("__rt_vsprintf");

    // Frame: [rbp-8] fmt ptr, [rbp-16] fmt len, [rbp-24] count, [rbp-32] slot
    //   base, [rbp-40] value_type, [rbp-48] slot size, [rbp-56] loop index.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // fixed frame pointer (rsp moves while records are pushed)
    emitter.instruction("sub rsp, 64");                                         // reserve the helper locals
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the format pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the format length

    // -- read the array length and runtime value_type (array ptr in rdi) --
    emitter.instruction("test rdi, rdi");                                       // null array?
    emitter.instruction("jz __rt_vsprintf_empty_x86");                          // → format with zero args
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // array logical length
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // save the argument count
    emitter.instruction("lea r10, [rdi + 24]");                                 // slot base = array + 24-byte header
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the slot base
    emitter.instruction("mov r11, QWORD PTR [rdi - 8]");                        // packed array kind word
    emitter.instruction("shr r11, 8");                                          // shift the runtime value_type into the low byte
    emitter.instruction("and r11, 0x7f");                                       // isolate the value_type (drop the COW bit)
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the value_type
    emitter.instruction("mov r12, 8");                                          // default slot size = 8 bytes
    emitter.instruction("cmp r11, 1");                                          // string-typed array (16-byte slots)?
    emitter.instruction("mov r13, 16");                                         // string slot size = 16 bytes
    emitter.instruction("cmove r12, r13");                                      // pick 16 for string arrays, else 8
    emitter.instruction("mov QWORD PTR [rbp - 48], r12");                       // save the slot size

    // -- push one 16-byte tagged record per element, in reverse order --
    emitter.instruction("mov r14, r9");                                         // loop index = count
    emitter.instruction("dec r14");                                             // = count - 1 (last element first)
    emitter.instruction("mov QWORD PTR [rbp - 56], r14");                       // save the loop index
    emitter.label("__rt_vsprintf_loop_x86");
    emitter.instruction("mov r14, QWORD PTR [rbp - 56]");                       // current index
    emitter.instruction("cmp r14, 0");                                          // exhausted the array?
    emitter.instruction("jl __rt_vsprintf_format_x86");                         // all elements pushed → format
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // slot base
    emitter.instruction("mov r12, QWORD PTR [rbp - 48]");                       // slot size
    emitter.instruction("mov rax, r14");                                        // index
    emitter.instruction("imul rax, r12");                                       // index * slot size
    emitter.instruction("add rax, r10");                                        // slot address = base + index * size
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // value_type
    // rsi = record payload, rcx = record tag (built below per value_type)
    emitter.instruction("cmp r11, 7");                                          // boxed-Mixed array slot?
    emitter.instruction("je __rt_vsprintf_mixed_x86");                          // unbox the boxed cell
    emitter.instruction("cmp r11, 1");                                          // string array slot (ptr+len)?
    emitter.instruction("je __rt_vsprintf_str_x86");                            // build a string record
    emitter.instruction("mov rsi, QWORD PTR [rax]");                            // payload = the typed slot value
    emitter.instruction("mov rcx, r11");                                        // record tag = value_type (0/2/3 map 1:1)
    emitter.instruction("jmp __rt_vsprintf_push_x86");                          // push the record
    emitter.label("__rt_vsprintf_str_x86");
    emitter.instruction("mov rsi, QWORD PTR [rax]");                            // string pointer payload
    emitter.instruction("mov rcx, QWORD PTR [rax + 8]");                        // string length
    emitter.instruction("shl rcx, 8");                                          // pack the length into the metadata word
    emitter.instruction("or rcx, 1");                                           // tag 1 = string operand
    emitter.instruction("jmp __rt_vsprintf_push_x86");                          // push the record
    emitter.label("__rt_vsprintf_mixed_x86");
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // boxed Mixed cell pointer
    emitter.instruction("test rax, rax");                                       // null cell?
    emitter.instruction("jz __rt_vsprintf_mixed_zero_x86");                     // → integer zero record
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // cell runtime tag
    emitter.instruction("mov rsi, QWORD PTR [rax + 8]");                        // cell low payload word
    emitter.instruction("cmp r9, 1");                                           // string cell?
    emitter.instruction("je __rt_vsprintf_mixed_str_x86");                      // build a string record from the cell
    emitter.instruction("mov rcx, 2");                                          // tag 2 = float operand
    emitter.instruction("cmp r9, 2");                                           // float cell?
    emitter.instruction("je __rt_vsprintf_push_x86");                           // float payload already in rsi
    emitter.instruction("mov rcx, 3");                                          // tag 3 = bool operand
    emitter.instruction("cmp r9, 3");                                           // bool cell?
    emitter.instruction("je __rt_vsprintf_push_x86");                           // bool payload already in rsi
    emitter.instruction("xor ecx, ecx");                                        // anything else → integer operand
    emitter.instruction("jmp __rt_vsprintf_push_x86");                          // push the record
    emitter.label("__rt_vsprintf_mixed_str_x86");
    emitter.instruction("mov r9, QWORD PTR [rax + 16]");                        // cell high word = string length
    emitter.instruction("mov rcx, r9");                                         // length into the metadata word
    emitter.instruction("shl rcx, 8");                                          // pack the length
    emitter.instruction("or rcx, 1");                                           // tag 1 = string operand
    emitter.instruction("jmp __rt_vsprintf_push_x86");                          // push the record
    emitter.label("__rt_vsprintf_mixed_zero_x86");
    emitter.instruction("xor esi, esi");                                        // zero payload for a null cell
    emitter.instruction("xor ecx, ecx");                                        // integer-zero tag
    emitter.label("__rt_vsprintf_push_x86");
    emitter.instruction("sub rsp, 16");                                         // reserve one 16-byte tagged record
    emitter.instruction("mov QWORD PTR [rsp], rsi");                            // store the payload word
    emitter.instruction("mov QWORD PTR [rsp + 8], rcx");                        // store the tag/metadata word
    emitter.instruction("mov r14, QWORD PTR [rbp - 56]");                       // reload the loop index
    emitter.instruction("dec r14");                                             // step to the previous element
    emitter.instruction("mov QWORD PTR [rbp - 56], r14");                       // store the loop index
    emitter.instruction("jmp __rt_vsprintf_loop_x86");                          // push the next record

    emitter.label("__rt_vsprintf_empty_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // zero argument count

    emitter.label("__rt_vsprintf_format_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // arg count
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // format pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // format length
    emitter.instruction("call __rt_sprintf");                                   // format; pops the count*16 records, returns rax/rdx
    emitter.instruction("leave");                                               // restore rsp/rbp (records already discarded by __rt_sprintf)
    emitter.instruction("ret");                                                 // return the formatted string (rax/rdx)
}
