//! Purpose:
//! Supplies native and eval object metadata to mbstring input description.
//!
//! Called from:
//! - The architecture-specific __rt_mbstring_input classifier.
//!
//! Key details:
//! - Eval ownership takes precedence because dynamic classes may use native stdClass payloads.
//! - Native tables describe objects without an available eval context.
//! - Eval lookup returns an owned class-name cell retained until the shared planner returns.
//! - Metadata lookup must not invoke __toString or traverse the original argument.

use super::*;

/// Describes a native AArch64 object or delegates synthetic class identities to eval metadata.
pub(super) fn aarch64(emitter: &mut Emitter, eval_bridge: bool) {
    emitter.label("__rt_mbstring_input_object");
    if eval_bridge {
        emitter.instruction("ldr x0, [sp]");                                    // pass the optional active caller context to object ownership lookup
        emitter.instruction("ldr x1, [sp, #8]");                                // identify the original boxed receiver before consulting native class tables
        emitter.bl_c("__elephc_eval_object_context");
        emitter.instruction("str x0, [sp]");                                    // retain the resolved object owner context for class and method metadata
        emitter.instruction("cbnz x0, __rt_mbstring_input_dynamic");            // eval identity takes precedence over a native stdClass payload
        emitter.instruction("ldr x10, [sp, #16]");                              // recover the concrete descriptor after the C metadata lookup
        emitter.instruction("ldr x1, [x10, #8]");                               // recover the raw native object for context-free class-table lookup
    }
    emitter.instruction("ldr x11, [x1]");                                       // read the actual object's class identity rather than its static type
    emitter.instruction("str x11, [sp, #32]");                                  // retain class identity across symbol-address helpers
    abi::emit_load_symbol_to_reg(emitter, "x12", "_class_name_count", 0);
    emitter.instruction("cmp x11, x12");                                        // bound-check dense native class metadata, including negative eval ids
    emitter.instruction("b.hs __rt_mbstring_input_dynamic");                    // resolve synthetic classes through their active eval context
    abi::emit_symbol_address(emitter, "x12", "_class_name_entries");
    emitter.instruction("add x12, x12, x11, lsl #4");                           // address the immutable class-name pointer and length pair
    emitter.instruction("ldp x1, x2, [x12]");                                   // borrow the actual class spelling without allocating
    emitter.instruction("cbz x1, __rt_mbstring_input_dynamic");                 // table holes may require dynamic metadata
    emitter.instruction("cbz x2, __rt_mbstring_input_dynamic");                 // empty native names cannot describe PHP objects
    emitter.instruction("ldr x10, [sp, #16]");                                  // recover the output descriptor
    emitter.instruction("stp x1, x2, [x10, #16]");                              // retain binary class-name bytes until preparation finishes
    abi::emit_load_symbol_to_reg(emitter, "x12", "_class_tostring_count", 0);
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the class id after name lookup
    emitter.instruction("cmp x11, x12");                                        // guard the independently sized Stringable method table
    emitter.instruction("b.hs __rt_mbstring_input_done");                       // out-of-range native classes have no conversion method
    abi::emit_symbol_address(emitter, "x12", "_class_tostring_ptrs");
    emitter.instruction("ldr x12, [x12, x11, lsl #3]");                         // resolve inherited native __toString capability without calling it
    emitter.instruction("cmp x12, #0");                                         // distinguish a conversion handler from a method-table hole
    emitter.instruction("cset x9, ne");                                         // publish the neutral Stringable capability bit
    emitter.instruction("str x9, [x10, #32]");                                  // leave ownership with the original borrowed object
    emitter.instruction("b __rt_mbstring_input_done");                          // native class metadata needs no owner allocation
    emitter.label("__rt_mbstring_input_dynamic");
    if !eval_bridge {
        emitter.instruction("b __rt_mbstring_input_fatal");                     // synthetic objects require an available eval metadata provider
        return;
    }
    emitter.instruction("ldr x0, [sp]");                                        // recover the active caller's eval context
    emitter.instruction("cbz x0, __rt_mbstring_input_fatal");                   // dynamic class lookup requires a live context
    emitter.instruction("ldr x1, [sp, #8]");                                    // pass the original borrowed object box
    emitter.instruction("mov x2, #0");                                          // select the existing get_class metadata lookup operation
    emitter.instruction("add x3, sp, #48");                                     // use the local versioned eval result tuple
    emitter.bl_c("__elephc_eval_object_class_name");
    emitter.instruction("str x0, [sp, #112]");                                  // preserve the returned eval status during owner transfer
    emitter.instruction("ldr x0, [sp, #56]");                                   // take the returned class-name cell even on malformed responses
    emitter.instruction("ldr x10, [sp, #24]");                                  // locate the caller's metadata-owner slot
    emitter.instruction("str x0, [x10]");                                       // transfer class-name cell ownership before validating its contents
    emitter.instruction("ldr x9, [sp, #112]");                                  // reload the metadata lookup status
    emitter.instruction("cbnz x9, __rt_mbstring_input_fatal");                  // reject unavailable metadata after balancing any owned outputs
    emitter.instruction("cbz x0, __rt_mbstring_input_fatal");                   // successful metadata lookup must supply a class-name value
    emitter.instruction("bl __rt_mixed_unbox");                                 // borrow the class-name string inside its retained native cell
    emitter.instruction("cmp x0, #1");                                          // metadata must preserve a binary string class name
    emitter.instruction("b.ne __rt_mbstring_input_fatal");                      // reject unexpected values after releasing their owner
    emitter.instruction("ldr x10, [sp, #16]");                                  // recover writable input metadata
    emitter.instruction("stp x1, x2, [x10, #16]");                              // retain class bytes through the caller-owned class-name cell
    emitter.instruction("mov x9, #1");                                          // construct a borrowed string cell for method-name lookup
    emitter.instruction("str x9, [sp, #80]");                                   // the method argument is an actual PHP string
    abi::emit_symbol_address(emitter, "x9", "_mbstring_tostring_name");
    emitter.instruction("str x9, [sp, #88]");                                   // borrow the immutable __toString name
    emitter.instruction("mov x9, #10");                                         // preserve its complete case-sensitive spelling
    emitter.instruction("str x9, [sp, #96]");                                   // complete the non-owning method-name cell
    emitter.instruction("ldr x0, [sp]");                                        // use the same active eval context for method metadata
    emitter.instruction("ldr x1, [sp, #8]");                                    // inspect the original receiver without converting it
    emitter.instruction("add x2, sp, #80");                                     // pass the borrowed method-name cell
    emitter.instruction("mov x3, #0");                                          // select method_exists without invoking the method
    emitter.bl_c("__elephc_eval_member_exists");
    emitter.instruction("cmp w0, #0");                                          // normalize the metadata predicate into one capability bit
    emitter.instruction("cset x9, ne");                                         // a declared __toString method makes the object Stringable
    emitter.instruction("ldr x10, [sp, #16]");                                  // recover the input descriptor after eval metadata lookup
    emitter.instruction("str x9, [x10, #32]");                                  // publish capability while retaining the original object identity
    emitter.instruction("b __rt_mbstring_input_done");                          // caller releases the separate class-name owner after preparation
}

/// Describes a native SysV object or obtains synthetic class metadata through existing eval APIs.
pub(super) fn x86_64(emitter: &mut Emitter, eval_bridge: bool) {
    emitter.label("__rt_mbstring_input_object");
    if eval_bridge {
        emitter.instruction("mov rdi, QWORD PTR [rsp]");                        // pass the active caller context, which may be absent in native code
        emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                    // identify the original boxed receiver through eval ownership metadata
        emitter.bl_c("__elephc_eval_object_context");
        emitter.instruction("mov QWORD PTR [rsp], rax");                        // retain the resolved context for class and method lookup
        emitter.instruction("test rax, rax");                                   // determine whether eval metadata is available for this object
        emitter.instruction("jnz __rt_mbstring_input_dynamic");                 // prefer the actual eval class over its native backing payload
        emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                   // recover the writable concrete-value descriptor
        emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                    // recover the raw object for context-free native class lookup
    }
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // inspect the actual object's runtime class identity
    emitter.instruction("mov QWORD PTR [rsp + 32], r11");                       // retain the class id through symbol-address materialization
    abi::emit_load_symbol_to_reg(emitter, "r9", "_class_name_count", 0);
    emitter.instruction("cmp r11, r9");                                         // bound-check native metadata and reject negative synthetic ids
    emitter.instruction("jae __rt_mbstring_input_dynamic");                     // use eval metadata for synthetic class identities
    abi::emit_symbol_address(emitter, "r9", "_class_name_entries");
    emitter.instruction("shl r11, 4");                                          // scale the class id to a pointer/length entry
    emitter.instruction("add r9, r11");                                         // address the actual native class-name tuple
    emitter.instruction("mov rdi, QWORD PTR [r9]");                             // borrow the binary class-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r9 + 8]");                         // preserve the complete class-name length
    emitter.instruction("test rdi, rdi");                                       // table holes require dynamic metadata
    emitter.instruction("jz __rt_mbstring_input_dynamic");                      // do not publish an absent native class spelling
    emitter.instruction("test rdx, rdx");                                       // empty native class names are not valid object metadata
    emitter.instruction("jz __rt_mbstring_input_dynamic");                      // resolve a possible synthetic class instead
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // recover the descriptor after native lookup
    emitter.instruction("mov QWORD PTR [r10 + 16], rdi");                       // borrow native class-name bytes
    emitter.instruction("mov QWORD PTR [r10 + 24], rdx");                       // preserve their complete byte length
    abi::emit_load_symbol_to_reg(emitter, "r9", "_class_tostring_count", 0);
    emitter.instruction("mov r11, QWORD PTR [rsp + 32]");                       // recover the unscaled class id
    emitter.instruction("cmp r11, r9");                                         // guard the independently sized native method table
    emitter.instruction("jae __rt_mbstring_input_done");                        // out-of-range native classes have no string conversion method
    abi::emit_symbol_address(emitter, "r9", "_class_tostring_ptrs");
    emitter.instruction("cmp QWORD PTR [r9 + r11 * 8], 0");                     // inspect inherited __toString capability without executing PHP
    emitter.instruction("setne r9b");                                           // preserve one boolean capability bit
    emitter.instruction("movzx r9, r9b");                                       // clear non-boolean upper bits before exporting flags
    emitter.instruction("mov QWORD PTR [r10 + 32], r9");                        // publish Stringable capability
    emitter.instruction("jmp __rt_mbstring_input_done");                        // static class metadata acquires no owner
    emitter.label("__rt_mbstring_input_dynamic");
    if !eval_bridge {
        emitter.instruction("jmp __rt_mbstring_input_fatal");                   // synthetic classes require the optional eval metadata provider
        return;
    }
    emitter.instruction("mov rdi, QWORD PTR [rsp]");                            // recover the caller's eval context
    emitter.instruction("test rdi, rdi");                                       // dynamic metadata requires a live context
    emitter.instruction("jz __rt_mbstring_input_fatal");                        // reject missing metadata context before crossing the C ABI
    emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                        // pass the original borrowed object box
    emitter.instruction("xor edx, edx");                                        // select get_class metadata lookup
    emitter.instruction("lea rcx, [rsp + 48]");                                 // provide local storage for the eval result tuple
    emitter.bl_c("__elephc_eval_object_class_name");
    emitter.instruction("mov QWORD PTR [rsp + 112], rax");                      // preserve status while taking metadata ownership
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // take the returned class-name cell
    emitter.instruction("mov r10, QWORD PTR [rsp + 24]");                       // recover the metadata-owner output slot
    emitter.instruction("mov QWORD PTR [r10], rax");                            // transfer any class-name owner before validating the response
    emitter.instruction("cmp QWORD PTR [rsp + 112], 0");                        // require successful metadata lookup
    emitter.instruction("jne __rt_mbstring_input_fatal");                       // release optional outputs before failing
    emitter.instruction("test rax, rax");                                       // successful metadata lookup requires a value cell
    emitter.instruction("jz __rt_mbstring_input_fatal");                        // reject a missing class-name value
    emitter.instruction("call __rt_mixed_unbox");                               // inspect the retained class-name cell's concrete value
    emitter.instruction("cmp rax, 1");                                          // require a binary PHP string
    emitter.instruction("jne __rt_mbstring_input_fatal");                       // reject malformed metadata after owner cleanup
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // recover writable input metadata
    emitter.instruction("mov QWORD PTR [r10 + 16], rdi");                       // borrow the class bytes from the retained native cell
    emitter.instruction("mov QWORD PTR [r10 + 24], rdx");                       // preserve binary class-name length
    emitter.instruction("mov QWORD PTR [rsp + 80], 1");                         // build a non-owning PHP string cell for the method name
    abi::emit_symbol_address(emitter, "r9", "_mbstring_tostring_name");
    emitter.instruction("mov QWORD PTR [rsp + 88], r9");                        // borrow immutable __toString bytes
    emitter.instruction("mov QWORD PTR [rsp + 96], 10");                        // preserve the complete method-name length
    emitter.instruction("mov rdi, QWORD PTR [rsp]");                            // retain the same active eval context
    emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                        // inspect the original receiver without a string conversion
    emitter.instruction("lea rdx, [rsp + 80]");                                 // pass the borrowed method-name cell
    emitter.instruction("xor ecx, ecx");                                        // select method_exists metadata lookup
    emitter.bl_c("__elephc_eval_member_exists");
    emitter.instruction("test eax, eax");                                       // read the metadata predicate without invoking the method
    emitter.instruction("setne r9b");                                           // normalize the Stringable capability bit
    emitter.instruction("movzx r9, r9b");                                       // discard unrelated upper register bits
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // recover the concrete-value descriptor
    emitter.instruction("mov QWORD PTR [r10 + 32], r9");                        // publish capability while retaining the original receiver identity
    emitter.instruction("jmp __rt_mbstring_input_done");                        // caller retains the class-name owner until parameter preparation returns
}
