//! Purpose:
//! Emits `__rt_array_map_numeric`, the stored-result kernel for
//! `array_map(fn($v) => $v + C | $v - C | $v * C, $packedInts)`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime`.
//!
//! Key details:
//! - The first pass builds a raw 8-byte `array<int>`. It stops at the first
//!   int-to-float overflow, releases that partial array, and rebuilds a boxed
//!   Mixed array so PHP's per-element int/float result is preserved exactly.
//! - The safe result keeps value_type 0, so the stored array has no per-element
//!   heap cell. The overflow result keeps value_type 7, the ordinary Mixed layout.
//! - Both arrays are ordinary copy-on-write heap containers created by
//!   `__rt_array_new`; callers own exactly the returned reference.
//! - The op selector is 0 = add, 1 = subtract, 2 = multiply. The callback is pure,
//!   so rebuilding after an overflow cannot repeat an observable effect.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the stored numeric map kernel for the active target.
pub fn emit_array_map_numeric(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emit_aarch64(emitter);
}

/// Emits the AArch64 kernel.
fn emit_aarch64(emitter: &mut Emitter) {
    let loop_label = "__rt_array_map_numeric_loop";
    let add_label = "__rt_array_map_numeric_add";
    let sub_label = "__rt_array_map_numeric_sub";
    let store_label = "__rt_array_map_numeric_store";
    let overflow_label = "__rt_array_map_numeric_overflow";
    let mixed_loop = "__rt_array_map_numeric_mixed_loop";
    let mixed_add = "__rt_array_map_numeric_mixed_add";
    let mixed_sub = "__rt_array_map_numeric_mixed_sub";
    let mixed_mul = "__rt_array_map_numeric_mixed_mul";
    let mixed_add_float = "__rt_array_map_numeric_mixed_add_float";
    let mixed_sub_float = "__rt_array_map_numeric_mixed_sub_float";
    let mixed_store_int = "__rt_array_map_numeric_mixed_store_int";
    let mixed_store_float = "__rt_array_map_numeric_mixed_store_float";
    let mixed_store_cell = "__rt_array_map_numeric_mixed_store_cell";
    let mixed_done = "__rt_array_map_numeric_mixed_done";
    let done = "__rt_array_map_numeric_done";

    emitter.blank();
    emitter.comment("--- runtime: array_map_numeric (stored int map) ---");
    emitter.label_global("__rt_array_map_numeric");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller frame and return address
    emitter.instruction("mov x29, sp");                                         // establish a stable helper frame
    emitter.instruction("stp x19, x20, [sp, #-16]!");                           // preserve the source and literal operand
    emitter.instruction("stp x21, x22, [sp, #-16]!");                           // preserve the operation selector and element count
    emitter.instruction("stp x23, x24, [sp, #-16]!");                           // preserve the element index and result array
    emitter.instruction("mov x19, x0");                                         // x19 = packed source array
    emitter.instruction("mov x20, x1");                                         // x20 = literal arithmetic operand
    emitter.instruction("mov x21, x2");                                         // x21 = 0 add / 1 subtract / 2 multiply
    emitter.instruction("ldr x22, [x19]");                                      // x22 = source element count
    emitter.instruction("mov x0, x22");                                         // allocate exactly one slot per source element
    emitter.instruction("mov x1, #8");                                          // raw integer slots are pointer-sized
    emitter.instruction("bl __rt_array_new");                                   // allocate the copy-on-write destination container
    emitter.instruction("mov x24, x0");                                         // x24 = raw int destination array
    emitter.instruction("mov x23, xzr");                                        // x23 = current element index

    emitter.label(loop_label);
    emitter.instruction("cmp x23, x22");                                        // has every element been mapped?
    emitter.instruction(&format!("b.ge {done}"));                               // finish once the loop exhausts the source
    emitter.instruction("add x9, x19, #24");                                    // skip the source header to the payload base
    emitter.instruction("ldr x10, [x9, x23, lsl #3]");                          // load the current source integer
    emitter.instruction("cmp x21, #0");                                         // is this the addition selector?
    emitter.instruction(&format!("b.eq {add_label}"));                          // use the checked add path
    emitter.instruction("cmp x21, #1");                                         // is this the subtraction selector?
    emitter.instruction(&format!("b.eq {sub_label}"));                          // use the checked subtract path
    emitter.instruction("mul x11, x10, x20");                                   // compute the low product for multiply
    emitter.instruction("smulh x12, x10, x20");                                 // compute the signed high product for overflow detection
    emitter.instruction("cmp x12, x11, asr #63");                               // does the high word differ from the sign extension?
    emitter.instruction(&format!("b.ne {overflow_label}"));                     // a mismatched high word means int-to-float promotion
    emitter.instruction(&format!("b {store_label}"));                           // store the in-range product

    emitter.label(add_label);
    emitter.instruction("adds x11, x10, x20");                                  // add the literal with signed overflow flags
    emitter.instruction(&format!("b.vs {overflow_label}"));                     // overflow promotes the result to float
    emitter.instruction(&format!("b {store_label}"));                           // store the in-range sum

    emitter.label(sub_label);
    emitter.instruction("subs x11, x10, x20");                                  // subtract the literal with signed overflow flags
    emitter.instruction(&format!("b.vs {overflow_label}"));                     // overflow promotes the result to float

    emitter.label(store_label);
    emitter.instruction("add x9, x24, #24");                                    // skip the destination header to the payload base
    emitter.instruction("str x11, [x9, x23, lsl #3]");                          // store the mapped raw integer
    emitter.instruction("add x23, x23, #1");                                    // advance to the next source element
    emitter.instruction(&format!("b {loop_label}"));                            // continue until every element is mapped

    emitter.label(done);
    emitter.instruction("str x22, [x24]");                                      // publish the completed logical length
    emitter.instruction("mov x0, x24");                                         // return the raw int array
    emitter.instruction("ldp x23, x24, [sp], #16");                             // restore the element index and result array
    emitter.instruction("ldp x21, x22, [sp], #16");                             // restore the selector and element count
    emitter.instruction("ldp x19, x20, [sp], #16");                             // restore the source and literal operand
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame and return address
    emitter.instruction("ret");                                                 // return the stored map result

    emitter.label(overflow_label);
    emitter.instruction("mov x0, x24");                                         // pass the partial raw array for release
    emitter.instruction("bl __rt_decref_array");                                // retire every slot built before the promoting element
    emitter.instruction("mov x0, x22");                                         // allocate one Mixed slot per source element
    emitter.instruction("mov x1, #8");                                          // boxed Mixed slots are pointer-sized
    emitter.instruction("bl __rt_array_new");                                   // allocate the overflow destination container
    emitter.instruction("mov x24, x0");                                         // x24 = boxed Mixed destination array
    emitter.instruction("ldr x9, [x24, #-8]");                                  // load the packed indexed-array kind word
    emitter.instruction("mov x10, #0x80ff");                                    // preserve heap kind and copy-on-write metadata
    emitter.instruction("and x9, x9, x10");                                     // clear the raw-int value_type bits
    emitter.instruction("mov x10, #7");                                         // value_type 7 = boxed Mixed slots
    emitter.instruction("lsl x10, x10, #8");                                    // move the Mixed tag into the packed kind lane
    emitter.instruction("orr x9, x9, x10");                                     // combine stable metadata with the Mixed tag
    emitter.instruction("str x9, [x24, #-8]");                                  // publish the Mixed indexed-array metadata
    emitter.instruction("mov x23, xzr");                                        // restart at the first source element

    emitter.label(mixed_loop);
    emitter.instruction("cmp x23, x22");                                        // has every element been rebuilt?
    emitter.instruction(&format!("b.ge {mixed_done}"));                         // finish once the Mixed array is complete
    emitter.instruction("add x9, x19, #24");                                    // skip the source header to the payload base
    emitter.instruction("ldr x10, [x9, x23, lsl #3]");                          // load the current source integer
    emitter.instruction("cmp x21, #0");                                         // is this the addition selector?
    emitter.instruction(&format!("b.eq {mixed_add}"));                          // use the add classification
    emitter.instruction("cmp x21, #1");                                         // is this the subtraction selector?
    emitter.instruction(&format!("b.eq {mixed_sub}"));                          // use the subtract classification
    emitter.instruction("mul x11, x10, x20");                                   // compute the low product for multiply
    emitter.instruction("smulh x12, x10, x20");                                 // compute the signed high product
    emitter.instruction("cmp x12, x11, asr #63");                               // does the product fit a signed 64-bit integer?
    emitter.instruction(&format!("b.ne {mixed_mul}"));                          // a mismatch requires the float product
    emitter.instruction(&format!("b {mixed_store_int}"));                       // box the in-range product as an integer

    emitter.label(mixed_add);
    emitter.instruction("adds x11, x10, x20");                                  // add the literal with signed overflow flags
    emitter.instruction(&format!("b.vs {mixed_add_float}"));                    // overflow requires the float sum
    emitter.instruction(&format!("b {mixed_store_int}"));                       // box the in-range sum as an integer

    emitter.label(mixed_sub);
    emitter.instruction("subs x11, x10, x20");                                  // subtract the literal with signed overflow flags
    emitter.instruction(&format!("b.vs {mixed_sub_float}"));                    // overflow requires the float difference
    emitter.instruction(&format!("b {mixed_store_int}"));                       // box the in-range difference as an integer

    emitter.label(mixed_mul);
    emitter.instruction("scvtf d0, x10");                                       // convert the source integer to double
    emitter.instruction("scvtf d1, x20");                                       // convert the literal operand to double
    emitter.instruction("fmul d0, d0, d1");                                     // build the promoted float product
    emitter.instruction(&format!("b {mixed_store_float}"));                     // box the promoted double

    // The add/sub overflow edges reuse this shared float conversion.
    emitter.label(mixed_add_float);
    emitter.instruction("scvtf d0, x10");                                       // convert the source integer to double
    emitter.instruction("scvtf d1, x20");                                       // convert the literal operand to double
    emitter.instruction("fadd d0, d0, d1");                                     // build the promoted float sum
    emitter.instruction(&format!("b {mixed_store_float}"));                     // box the promoted double
    emitter.label(mixed_sub_float);
    emitter.instruction("scvtf d0, x10");                                       // convert the source integer to double
    emitter.instruction("scvtf d1, x20");                                       // convert the literal operand to double
    emitter.instruction("fsub d0, d0, d1");                                     // build the promoted float difference
    emitter.instruction(&format!("b {mixed_store_float}"));                     // box the promoted double

    emitter.label(mixed_store_int);
    emitter.instruction("mov x0, #0");                                          // Mixed tag 0 = integer
    emitter.instruction("mov x1, x11");                                         // integer payload
    emitter.instruction("mov x2, xzr");                                         // scalar payloads do not use a second word
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate the boxed integer cell
    emitter.instruction(&format!("b {mixed_store_cell}"));                      // share the destination store

    emitter.label(mixed_store_float);
    emitter.instruction("mov x0, #2");                                          // Mixed tag 2 = float
    emitter.instruction("fmov x1, d0");                                         // float bits as the payload word
    emitter.instruction("mov x2, xzr");                                         // scalar payloads do not use a second word
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate the boxed float cell

    emitter.label(mixed_store_cell);
    emitter.instruction("add x9, x24, #24");                                    // skip the Mixed destination header to the payload base
    emitter.instruction("str x0, [x9, x23, lsl #3]");                           // store the owned Mixed cell pointer
    emitter.instruction("add x23, x23, #1");                                    // advance to the next source element
    emitter.instruction(&format!("b {mixed_loop}"));                            // continue until every element is boxed

    emitter.label(mixed_done);
    emitter.instruction("str x22, [x24]");                                      // publish the completed logical length
    emitter.instruction("mov x0, x24");                                         // return the boxed Mixed array
    emitter.instruction("ldp x23, x24, [sp], #16");                             // restore the element index and result array
    emitter.instruction("ldp x21, x22, [sp], #16");                             // restore the selector and element count
    emitter.instruction("ldp x19, x20, [sp], #16");                             // restore the source and literal operand
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame and return address
    emitter.instruction("ret");                                                 // return the stored map result
}

/// Emits the x86_64 kernel.
fn emit_x86_64(emitter: &mut Emitter) {
    let loop_label = "__rt_array_map_numeric_loop_x86";
    let add_label = "__rt_array_map_numeric_add_x86";
    let sub_label = "__rt_array_map_numeric_sub_x86";
    let store_label = "__rt_array_map_numeric_store_x86";
    let overflow_label = "__rt_array_map_numeric_overflow_x86";
    let mixed_loop = "__rt_array_map_numeric_mixed_loop_x86";
    let mixed_add = "__rt_array_map_numeric_mixed_add_x86";
    let mixed_sub = "__rt_array_map_numeric_mixed_sub_x86";
    let mixed_mul = "__rt_array_map_numeric_mixed_mul_x86";
    let mixed_add_float = "__rt_array_map_numeric_mixed_add_float_x86";
    let mixed_sub_float = "__rt_array_map_numeric_mixed_sub_float_x86";
    let mixed_store_int = "__rt_array_map_numeric_mixed_store_int_x86";
    let mixed_store_float = "__rt_array_map_numeric_mixed_store_float_x86";
    let mixed_done = "__rt_array_map_numeric_mixed_done_x86";
    let done = "__rt_array_map_numeric_done_x86";

    emitter.blank();
    emitter.comment("--- runtime: array_map_numeric (stored int map) ---");
    emitter.label_global("__rt_array_map_numeric");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("push r12");                                            // preserve the packed source array
    emitter.instruction("push r13");                                            // preserve the literal arithmetic operand
    emitter.instruction("push r14");                                            // preserve the operation selector
    emitter.instruction("push r15");                                            // preserve the source element count
    emitter.instruction("push rbx");                                            // preserve the result array pointer
    emitter.instruction("sub rsp, 8");                                          // reserve the loop index and keep calls aligned
    emitter.instruction("mov r12, rdi");                                        // r12 = packed source array
    emitter.instruction("mov r13, rsi");                                        // r13 = literal arithmetic operand
    emitter.instruction("mov r14, rdx");                                        // r14 = 0 add / 1 subtract / 2 multiply
    emitter.instruction("mov r15, QWORD PTR [r12]");                            // r15 = source element count
    emitter.instruction("mov rdi, r15");                                        // allocate exactly one slot per source element
    emitter.instruction("mov rsi, 8");                                          // raw integer slots are pointer-sized
    emitter.instruction("call __rt_array_new");                                 // allocate the copy-on-write destination container
    emitter.instruction("mov rbx, rax");                                        // rbx = raw int destination array
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the element index to zero

    emitter.label(loop_label);
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the current element index
    emitter.instruction("cmp rcx, r15");                                        // has every element been mapped?
    emitter.instruction(&format!("jge {done}"));                                // finish once the loop exhausts the source
    emitter.instruction("lea r10, [r12 + 24]");                                 // skip the source header to the payload base
    emitter.instruction("mov r11, QWORD PTR [r10 + rcx * 8]");                  // load the current source integer
    emitter.instruction("cmp r14, 0");                                          // is this the addition selector?
    emitter.instruction(&format!("je {add_label}"));                            // use the checked add path
    emitter.instruction("cmp r14, 1");                                          // is this the subtraction selector?
    emitter.instruction(&format!("je {sub_label}"));                            // use the checked subtract path
    emitter.instruction("mov rax, r11");                                        // stage the source integer for multiply
    emitter.instruction("imul rax, r13");                                       // compute the signed product with overflow flags
    emitter.instruction(&format!("jo {overflow_label}"));                       // overflow promotes the result to float
    emitter.instruction(&format!("jmp {store_label}"));                         // store the in-range product

    emitter.label(add_label);
    emitter.instruction("mov rax, r11");                                        // stage the source integer for addition
    emitter.instruction("add rax, r13");                                        // add the literal with overflow flags
    emitter.instruction(&format!("jo {overflow_label}"));                       // overflow promotes the result to float
    emitter.instruction(&format!("jmp {store_label}"));                         // store the in-range sum

    emitter.label(sub_label);
    emitter.instruction("mov rax, r11");                                        // stage the source integer for subtraction
    emitter.instruction("sub rax, r13");                                        // subtract the literal with overflow flags
    emitter.instruction(&format!("jo {overflow_label}"));                       // overflow promotes the result to float

    emitter.label(store_label);
    emitter.instruction("lea r10, [rbx + 24]");                                 // skip the destination header to the payload base
    emitter.instruction("mov QWORD PTR [r10 + rcx * 8], rax");                  // store the mapped raw integer
    emitter.instruction("add rcx, 1");                                          // advance to the next source element
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the loop index
    emitter.instruction(&format!("jmp {loop_label}"));                          // continue until every element is mapped

    emitter.label(done);
    emitter.instruction("mov QWORD PTR [rbx], r15");                            // publish the completed logical length
    emitter.instruction("mov rax, rbx");                                        // return the raw int array
    emitter.instruction("add rsp, 8");                                          // release the loop index slot
    emitter.instruction("pop rbx");                                             // restore the result array pointer
    emitter.instruction("pop r15");                                             // restore the source element count
    emitter.instruction("pop r14");                                             // restore the operation selector
    emitter.instruction("pop r13");                                             // restore the literal operand
    emitter.instruction("pop r12");                                             // restore the packed source array
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the stored map result

    emitter.label(overflow_label);
    emitter.instruction("mov rdi, rbx");                                        // pass the partial raw array for release
    emitter.instruction("call __rt_decref_array");                              // retire every slot built before the promoting element
    emitter.instruction("mov rdi, r15");                                        // allocate one Mixed slot per source element
    emitter.instruction("mov rsi, 8");                                          // boxed Mixed slots are pointer-sized
    emitter.instruction("call __rt_array_new");                                 // allocate the overflow destination container
    emitter.instruction("mov rbx, rax");                                        // rbx = boxed Mixed destination array
    emitter.instruction("mov r10, QWORD PTR [rbx - 8]");                        // load the packed indexed-array kind word
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // preserve the heap marker, kind, and copy-on-write bits
    emitter.instruction("and r10, r11");                                        // clear the raw-int value_type bits
    emitter.instruction("or r10, 0x700");                                       // value_type 7 = boxed Mixed slots
    emitter.instruction("mov QWORD PTR [rbx - 8], r10");                        // publish the Mixed indexed-array metadata
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // restart at the first source element

    emitter.label(mixed_loop);
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the current element index
    emitter.instruction("cmp rcx, r15");                                        // has every element been rebuilt?
    emitter.instruction(&format!("jge {mixed_done}"));                          // finish once the Mixed array is complete
    emitter.instruction("lea r10, [r12 + 24]");                                 // skip the source header to the payload base
    emitter.instruction("mov r11, QWORD PTR [r10 + rcx * 8]");                  // load the current source integer
    emitter.instruction("cmp r14, 0");                                          // is this the addition selector?
    emitter.instruction(&format!("je {mixed_add}"));                            // use the add classification
    emitter.instruction("cmp r14, 1");                                          // is this the subtraction selector?
    emitter.instruction(&format!("je {mixed_sub}"));                            // use the subtract classification
    emitter.instruction("mov rax, r11");                                        // stage the source integer for multiply
    emitter.instruction("imul rax, r13");                                       // compute the signed product with overflow flags
    emitter.instruction(&format!("jo {mixed_mul}"));                            // overflow requires the float product
    emitter.instruction(&format!("jmp {mixed_store_int}"));                     // box the in-range product as an integer

    emitter.label(mixed_add);
    emitter.instruction("mov rax, r11");                                        // stage the source integer for addition
    emitter.instruction("add rax, r13");                                        // add the literal with overflow flags
    emitter.instruction(&format!("jo {mixed_add_float}"));                      // overflow requires the float sum
    emitter.instruction(&format!("jmp {mixed_store_int}"));                     // box the in-range sum as an integer

    emitter.label(mixed_sub);
    emitter.instruction("mov rax, r11");                                        // stage the source integer for subtraction
    emitter.instruction("sub rax, r13");                                        // subtract the literal with overflow flags
    emitter.instruction(&format!("jo {mixed_sub_float}"));                      // overflow requires the float difference
    emitter.instruction(&format!("jmp {mixed_store_int}"));                     // box the in-range difference as an integer

    emitter.label(mixed_add_float);
    emitter.instruction("cvtsi2sd xmm0, r11");                                  // convert the source integer to double
    emitter.instruction("cvtsi2sd xmm1, r13");                                  // convert the literal operand to double
    emitter.instruction("addsd xmm0, xmm1");                                    // build the promoted float sum
    emitter.instruction(&format!("jmp {mixed_store_float}"));                   // box the promoted double

    emitter.label(mixed_sub_float);
    emitter.instruction("cvtsi2sd xmm0, r11");                                  // convert the source integer to double
    emitter.instruction("cvtsi2sd xmm1, r13");                                  // convert the literal operand to double
    emitter.instruction("subsd xmm0, xmm1");                                    // build the promoted float difference
    emitter.instruction(&format!("jmp {mixed_store_float}"));                   // box the promoted double

    emitter.label(mixed_mul);
    emitter.instruction("cvtsi2sd xmm0, r11");                                  // convert the source integer to double
    emitter.instruction("cvtsi2sd xmm1, r13");                                  // convert the literal operand to double
    emitter.instruction("mulsd xmm0, xmm1");                                    // build the promoted float product
    emitter.instruction(&format!("jmp {mixed_store_float}"));                   // box the promoted double

    emitter.label(mixed_store_int);
    emitter.instruction("mov rdi, rax");                                        // integer payload word
    emitter.instruction("xor esi, esi");                                        // scalar payloads do not use a second word
    emitter.instruction("xor eax, eax");                                        // Mixed tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the boxed integer cell
    emitter.instruction("jmp __rt_array_map_numeric_mixed_store_cell_x86");     // share the destination store

    emitter.label(mixed_store_float);
    emitter.instruction("movq rdi, xmm0");                                      // move the float bits into the payload word
    emitter.instruction("xor esi, esi");                                        // scalar payloads do not use a second word
    emitter.instruction("mov eax, 2");                                          // Mixed tag 2 = float
    emitter.instruction("call __rt_mixed_from_value");                          // allocate the boxed float cell

    emitter.label("__rt_array_map_numeric_mixed_store_cell_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the current element index
    emitter.instruction("lea r10, [rbx + 24]");                                 // skip the Mixed destination header to the payload base
    emitter.instruction("mov QWORD PTR [r10 + rcx * 8], rax");                  // store the owned Mixed cell pointer
    emitter.instruction("add rcx, 1");                                          // advance to the next source element
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the loop index
    emitter.instruction(&format!("jmp {mixed_loop}"));                          // continue until every element is boxed

    emitter.label(mixed_done);
    emitter.instruction("mov QWORD PTR [rbx], r15");                            // publish the completed logical length
    emitter.instruction("mov rax, rbx");                                        // return the boxed Mixed array
    emitter.instruction("add rsp, 8");                                          // release the loop index slot
    emitter.instruction("pop rbx");                                             // restore the result array pointer
    emitter.instruction("pop r15");                                             // restore the source element count
    emitter.instruction("pop r14");                                             // restore the operation selector
    emitter.instruction("pop r13");                                             // restore the literal operand
    emitter.instruction("pop r12");                                             // restore the packed source array
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the stored map result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target emits both the raw-int and Mixed overflow paths.
    #[test]
    fn array_map_numeric_emits_all_targets() {
        for target in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_array_map_numeric(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_array_map_numeric"), "{target}");
            assert!(asm.contains("__rt_array_new"), "{target}");
            assert!(asm.contains("__rt_mixed_from_value"), "{target}");
            assert!(asm.contains("__rt_decref_array"), "{target}");
            if target == "linux-x86_64" {
                assert!(asm.contains("mov rdi, rax"), "{target}");
                assert!(asm.contains("movq rdi, xmm0"), "{target}");
                assert!(asm.contains("mov eax, 2"), "{target}");
                assert!(asm.contains("xor eax, eax"), "{target}");
            } else {
                assert!(asm.contains("fmov x1, d0"), "{target}");
                assert!(asm.contains("mov x0, #2"), "{target}");
            }
        }
    }
}
