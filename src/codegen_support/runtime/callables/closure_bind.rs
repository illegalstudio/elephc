//! Purpose:
//! Emits the `__rt_closure_bind` runtime helper that implements PHP's
//! `Closure::bind` / `Closure::bindTo` / `Closure::call` for closures that
//! capture `$this` and, when needed, the compiler's called-class id.
//!
//! Called from:
//! - `crate::codegen_support::runtime::callables`
//!
//! Key details:
//! - A closure that uses `$this` carries a first runtime capture named "this"
//!   (appended by EIR lowering). Class-scope closures may also carry the
//!   compiler-owned integer `CALLED_CLASS_ID_LOCAL` capture in slot one.
//! - Binding copies the complete 80- or 96-byte runtime descriptor, overwrites
//!   the captured object with the new receiver, and increfs it so the bound
//!   descriptor owns its own reference (balanced against descriptor release).
//! - Closures with any other capture shape (extra `use` variables, no `$this`)
//!   are not yet supported and abort with a fatal diagnostic rather than
//!   copying an unretained user capture.
//! - Verified on macOS, iOS device/Simulator, and Linux AArch64 plus Linux x86_64.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::names::CALLED_CLASS_ID_LOCAL;

const CALLED_CLASS_ID_SUFFIX: u32 = u32::from_le_bytes(*b"#gen");
const CALLED_CLASS_ID_SUFFIX_OFFSET: usize = CALLED_CLASS_ID_LOCAL.len() - size_of::<u32>();

/// Emits the `__rt_closure_bind` runtime helper for the active target.
///
/// Input: `x0`/`rdi` = source closure descriptor pointer, `x1`/`rsi` = the new
/// `$this` object pointer. Output: `x0`/`rax` = a freshly heap-allocated
/// descriptor copy whose `this` capture is the new receiver. Aborts (exit 1)
/// unless the source captures `$this` alone or followed by the compiler-owned
/// called-class id.
pub(crate) fn emit_closure_bind(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_closure_bind_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: closure bind ($this-only) ---");
    emitter.label_global("__rt_closure_bind");

    // -- frame and argument save --
    emitter.instruction("sub sp, sp, #64");                                     // reserve closure-bind spill slots
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #48");                                    // establish a frame pointer for the helper
    emitter.instruction("str x0, [sp, #0]");                                    // save the source descriptor pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the new $this receiver

    // -- validate capture shape: $this, optionally followed by called-class id --
    emitter.instruction("ldr x9, [x0, #40]");                                   // x9 = descriptor environment record pointer
    emitter.instruction("cbz x9, __rt_closure_bind_unsupported");               // no captures means there is no $this to rebind
    emitter.instruction("ldr x10, [x9]");                                       // x10 = capture count
    emitter.instruction("str x10, [sp, #32]");                                  // preserve the validated descriptor size across helper calls
    emitter.instruction("cmp x10, #1");                                         // is this the ordinary $this-only shape?
    emitter.instruction("b.eq __rt_closure_bind_validate_this");                // validate capture slot zero
    emitter.instruction("cmp x10, #2");                                         // is there one compiler-owned hidden capture?
    emitter.instruction("b.ne __rt_closure_bind_unsupported");                  // user captures are not retained by this helper
    emitter.label("__rt_closure_bind_validate_this");
    emitter.instruction("ldr x11, [x9, #16]");                                  // x11 = capture binding metadata table
    emitter.instruction("cbz x11, __rt_closure_bind_unsupported");              // missing metadata means the capture name is unknown
    emitter.instruction("ldr x14, [x11, #16]");                                 // x14 = capture type tag (6=object, 7=mixed)
    emitter.instruction("str x14, [sp, #24]");                                  // save the capture type tag for the store phase
    emitter.instruction("ldr x12, [x11, #8]");                                  // x12 = capture name length
    emitter.instruction("cmp x12, #4");                                         // "this" is four bytes long
    emitter.instruction("b.ne __rt_closure_bind_unsupported");                  // a different-length name is not $this
    emitter.instruction("ldr x13, [x11]");                                      // x13 = capture name byte pointer
    emitter.instruction("ldr w14, [x13]");                                      // load the first four name bytes
    emitter.instruction("movz w15, #0x6874");                                   // low half of "this" little-endian (\"th\")
    emitter.instruction("movk w15, #0x7369, lsl #16");                          // high half of "this" little-endian (\"is\")
    emitter.instruction("cmp w14, w15");                                        // is the sole capture named "this"?
    emitter.instruction("b.ne __rt_closure_bind_unsupported");                  // a non-$this single capture is not supported

    // -- validate the optional compiler-owned called-class id capture --
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the capture count after validating $this
    emitter.instruction("cmp x10, #1");                                         // does this descriptor omit the hidden capture?
    emitter.instruction("b.eq __rt_closure_bind_shape_valid");                  // the $this-only shape is complete
    emitter.instruction("ldr x12, [x11, #40]");                                 // x12 = second capture name length
    emitter.instruction(&format!("cmp x12, #{}", CALLED_CLASS_ID_LOCAL.len())); // require the complete generated called-class capture name
    emitter.instruction("b.ne __rt_closure_bind_unsupported");                  // reject an arbitrary user capture
    emitter.instruction("ldr x12, [x11, #48]");                                 // x12 = second capture type tag
    emitter.instruction("cbnz x12, __rt_closure_bind_unsupported");             // called-class id must use integer tag zero
    emitter.instruction("ldr x12, [x11, #56]");                                 // x12 = second capture by-reference flag
    emitter.instruction("cbnz x12, __rt_closure_bind_unsupported");             // called-class id is captured by value
    emitter.instruction("ldr x13, [x11, #32]");                                 // x13 = second capture name bytes
    emitter.instruction("cbz x13, __rt_closure_bind_unsupported");              // missing metadata cannot identify the hidden capture
    emitter.instruction("ldr x14, [x13]");                                      // load "__elephc" from the hidden capture name
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x15", 0x6368_7065_6c65_5f5f);
    emitter.instruction("cmp x14, x15");                                        // does the hidden name start with "__elephc"?
    emitter.instruction("b.ne __rt_closure_bind_unsupported");                  // reject a different second capture
    emitter.instruction("ldr x14, [x13, #8]");                                  // load "_called_" from the hidden capture name
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x15", 0x5f64_656c_6c61_635f);
    emitter.instruction("cmp x14, x15");                                        // does the hidden name continue with "_called_"?
    emitter.instruction("b.ne __rt_closure_bind_unsupported");                  // reject a different second capture
    emitter.instruction("ldr x14, [x13, #16]");                                 // load "class_id" from the hidden capture name
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x15", 0x6469_5f73_7361_6c63);
    emitter.instruction("cmp x14, x15");                                        // does the hidden name continue with "class_id"?
    emitter.instruction("b.ne __rt_closure_bind_unsupported");                  // reject a different second capture
    emitter.instruction(&format!("ldr w14, [x13, #{}]", CALLED_CLASS_ID_SUFFIX_OFFSET)); // load the generated "#gen" suffix
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x15", i64::from(CALLED_CLASS_ID_SUFFIX));
    emitter.instruction("cmp w14, w15");                                        // does the hidden name end with "#gen"?
    emitter.instruction("b.ne __rt_closure_bind_unsupported");                  // reject a different second capture

    // -- allocate a complete runtime descriptor copy --
    emitter.label("__rt_closure_bind_shape_valid");
    emitter.instruction("mov x0, #80");                                         // default to header plus the $this capture
    emitter.instruction("cmp x10, #2");                                         // does the descriptor carry called-class id too?
    emitter.instruction("b.ne __rt_closure_bind_allocate");                     // keep the 80-byte allocation for top-level closures
    emitter.instruction("mov x0, #96");                                         // include the hidden 16-byte called-class capture
    emitter.label("__rt_closure_bind_allocate");
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = fresh descriptor block
    emitter.instruction("bl __rt_object_handle_acquire");                       // Closure::bind creates a NEW Closure in PHP, so it takes a new object handle
    emitter.instruction("str x0, [sp, #16]");                                   // save the new descriptor pointer

    // -- copy the descriptor payload --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source descriptor
    emitter.instruction("ldp x2, x3, [x1, #0]");                                // copy header words 0-1 (kind, entry)
    emitter.instruction("stp x2, x3, [x0, #0]");                                // store header words 0-1
    emitter.instruction("ldp x2, x3, [x1, #16]");                               // copy header words 2-3 (name, name_len)
    emitter.instruction("stp x2, x3, [x0, #16]");                               // store header words 2-3
    emitter.instruction("ldp x2, x3, [x1, #32]");                               // copy header words 4-5 (signature, environment)
    emitter.instruction("stp x2, x3, [x0, #32]");                               // store header words 4-5
    emitter.instruction("ldp x2, x3, [x1, #48]");                               // copy header words 6-7 (invocation, invoker)
    emitter.instruction("stp x2, x3, [x0, #48]");                               // store header words 6-7
    emitter.instruction("ldp x2, x3, [x1, #64]");                               // copy the single capture slot (value, tag/len)
    emitter.instruction("stp x2, x3, [x0, #64]");                               // store the capture slot into the copy
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the capture count for the optional slot
    emitter.instruction("cmp x10, #2");                                         // is called-class id present?
    emitter.instruction("b.ne __rt_closure_bind_captures_copied");              // the $this-only descriptor is complete
    emitter.instruction("ldp x2, x3, [x1, #80]");                               // copy called-class id and its unused high word
    emitter.instruction("stp x2, x3, [x0, #80]");                               // preserve late-static dispatch in the bound closure
    emitter.label("__rt_closure_bind_captures_copied");

    // -- overwrite the captured $this, matching the capture representation --
    emitter.instruction("ldr x14, [sp, #24]");                                  // x14 = capture type tag
    emitter.instruction("cmp x14, #7");                                         // a Mixed capture stores a boxed cell, not a raw object
    emitter.instruction("b.eq __rt_closure_bind_box_this");                     // top-level closures use a Mixed $this receiver
    // object capture (method-defined closure): store the raw object and retain it
    emitter.instruction("ldr x2, [sp, #8]");                                    // x2 = new $this receiver
    emitter.instruction("str x2, [x0, #64]");                                   // replace the captured object with the new receiver
    emitter.instruction("mov x0, x2");                                          // pass the new receiver to the incref helper
    emitter.instruction("bl __rt_incref");                                      // the bound descriptor now owns a reference to $this
    emitter.instruction("b __rt_closure_bind_return");                          // skip the Mixed boxing path

    // mixed capture (top-level closure): box the object into a Mixed cell
    emitter.label("__rt_closure_bind_box_this");
    emitter.instruction("mov x0, #6");                                          // boxed payload tag 6 = object
    emitter.instruction("ldr x1, [sp, #8]");                                    // payload low word = new $this object pointer
    emitter.instruction("mov x2, #0");                                          // payload high word is unused for objects
    emitter.instruction("bl __rt_mixed_from_value");                            // box (and retain) the receiver into a Mixed cell
    emitter.instruction("ldr x16, [sp, #16]");                                  // reload the bound descriptor pointer
    emitter.instruction("str x0, [x16, #64]");                                  // store the boxed Mixed receiver into the capture slot

    // -- return the new descriptor --
    emitter.label("__rt_closure_bind_return");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = bound descriptor result
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // tear down the closure-bind frame
    emitter.instruction("ret");                                                 // return the rebound closure descriptor

    // -- unsupported capture shape: fatal --
    emitter.label("__rt_closure_bind_unsupported");
    emitter.instruction("mov x0, #2");                                          // write the unsupported-bind fatal to stderr
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_closure_bind_unsupported_msg");
    emitter.instruction("mov x2, #71");                                         // byte length of the unsupported-bind fatal message
    emitter.syscall(4);
    crate::codegen_support::abi::emit_exit(emitter, 1);
}

/// Emits the Linux x86_64 `__rt_closure_bind` helper (mirror of the aarch64 path).
fn emit_closure_bind_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: closure bind ($this-only, x86_64) ---");
    emitter.label_global("__rt_closure_bind");

    // -- frame and argument save --
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish this helper's frame
    emitter.instruction("sub rsp, 64");                                         // reserve spill slots (16-byte aligned)
    emitter.instruction("mov [rsp+0], rdi");                                    // save the source descriptor pointer
    emitter.instruction("mov [rsp+8], rsi");                                    // save the new $this receiver

    // -- validate capture shape: $this, optionally followed by called-class id --
    emitter.instruction("mov r8, [rdi+40]");                                    // r8 = descriptor environment record pointer
    emitter.instruction("test r8, r8");                                         // are there any captures?
    emitter.instruction("jz __rt_closure_bind_unsupported");                    // no captures means there is no $this to rebind
    emitter.instruction("mov r9, [r8]");                                        // r9 = capture count
    emitter.instruction("mov [rsp+32], r9");                                    // preserve the validated descriptor size across helper calls
    emitter.instruction("cmp r9, 1");                                           // is this the ordinary $this-only shape?
    emitter.instruction("je __rt_closure_bind_validate_this");                  // validate capture slot zero
    emitter.instruction("cmp r9, 2");                                           // is there one compiler-owned hidden capture?
    emitter.instruction("jne __rt_closure_bind_unsupported");                   // user captures are not retained by this helper
    emitter.label("__rt_closure_bind_validate_this");
    emitter.instruction("mov r10, [r8+16]");                                    // r10 = capture binding metadata table
    emitter.instruction("test r10, r10");                                       // is the capture metadata present?
    emitter.instruction("jz __rt_closure_bind_unsupported");                    // missing metadata means the capture name is unknown
    emitter.instruction("mov rax, [r10+16]");                                   // rax = capture type tag (6=object, 7=mixed)
    emitter.instruction("mov [rsp+24], rax");                                   // save the capture type tag for the store phase
    emitter.instruction("mov r11, [r10+8]");                                    // r11 = capture name length
    emitter.instruction("cmp r11, 4");                                          // "this" is four bytes long
    emitter.instruction("jne __rt_closure_bind_unsupported");                   // a different-length name is not $this
    emitter.instruction("mov r11, [r10]");                                      // r11 = capture name byte pointer
    emitter.instruction("mov eax, [r11]");                                      // load the first four name bytes
    emitter.instruction("cmp eax, 0x73696874");                                 // compare against "this" little-endian
    emitter.instruction("jne __rt_closure_bind_unsupported");                   // a non-$this single capture is not supported

    // -- validate the optional compiler-owned called-class id capture --
    emitter.instruction("cmp QWORD PTR [rsp+32], 1");                           // does this descriptor omit the hidden capture?
    emitter.instruction("je __rt_closure_bind_shape_valid");                    // the $this-only shape is complete
    emitter.instruction("mov r11, [r10+40]");                                   // r11 = second capture name length
    emitter.instruction(&format!("cmp r11, {}", CALLED_CLASS_ID_LOCAL.len()));  // require the complete generated called-class capture name
    emitter.instruction("jne __rt_closure_bind_unsupported");                   // reject an arbitrary user capture
    emitter.instruction("mov r11, [r10+48]");                                   // r11 = second capture type tag
    emitter.instruction("test r11, r11");                                       // called-class id must use integer tag zero
    emitter.instruction("jnz __rt_closure_bind_unsupported");                   // reject a non-integer hidden capture
    emitter.instruction("mov r11, [r10+56]");                                   // r11 = second capture by-reference flag
    emitter.instruction("test r11, r11");                                       // called-class id must be captured by value
    emitter.instruction("jnz __rt_closure_bind_unsupported");                   // reject a by-reference hidden capture
    emitter.instruction("mov r10, [r10+32]");                                   // r10 = second capture name bytes
    emitter.instruction("test r10, r10");                                       // is the hidden capture name present?
    emitter.instruction("jz __rt_closure_bind_unsupported");                    // missing metadata cannot identify the hidden capture
    emitter.instruction("mov r11, [r10]");                                      // load "__elephc" from the hidden capture name
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "rax", 0x6368_7065_6c65_5f5f);
    emitter.instruction("cmp r11, rax");                                        // does the hidden name start with "__elephc"?
    emitter.instruction("jne __rt_closure_bind_unsupported");                   // reject a different second capture
    emitter.instruction("mov r11, [r10+8]");                                    // load "_called_" from the hidden capture name
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "rax", 0x5f64_656c_6c61_635f);
    emitter.instruction("cmp r11, rax");                                        // does the hidden name continue with "_called_"?
    emitter.instruction("jne __rt_closure_bind_unsupported");                   // reject a different second capture
    emitter.instruction("mov r11, [r10+16]");                                   // load "class_id" from the hidden capture name
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "rax", 0x6469_5f73_7361_6c63);
    emitter.instruction("cmp r11, rax");                                        // does the hidden name continue with "class_id"?
    emitter.instruction("jne __rt_closure_bind_unsupported");                   // reject a different second capture
    emitter.instruction(&format!("mov r11d, [r10+{}]", CALLED_CLASS_ID_SUFFIX_OFFSET)); // load the generated "#gen" suffix
    emitter.instruction(&format!("cmp r11d, {}", CALLED_CLASS_ID_SUFFIX));      // does the hidden name end with "#gen"?
    emitter.instruction("jne __rt_closure_bind_unsupported");                   // reject a different second capture

    // -- allocate a complete runtime descriptor copy --
    emitter.label("__rt_closure_bind_shape_valid");
    emitter.instruction("mov rax, 80");                                         // default to header plus the $this capture
    emitter.instruction("cmp QWORD PTR [rsp+32], 2");                           // does the descriptor carry called-class id too?
    emitter.instruction("jne __rt_closure_bind_allocate");                      // keep the 80-byte allocation for top-level closures
    emitter.instruction("mov rax, 96");                                         // include the hidden 16-byte called-class capture
    emitter.label("__rt_closure_bind_allocate");
    emitter.instruction("call __rt_heap_alloc");                                // rax = fresh descriptor block
    emitter.instruction("call __rt_object_handle_acquire");                     // Closure::bind creates a NEW Closure in PHP, so it takes a new object handle
    emitter.instruction("mov [rsp+16], rax");                                   // save the new descriptor pointer

    // -- copy the descriptor payload --
    emitter.instruction("mov rsi, [rsp+0]");                                    // rsi = source descriptor
    emitter.instruction("mov rdi, rax");                                        // rdi = destination descriptor
    emitter.instruction("mov rcx, 10");                                         // $this-only descriptor is ten 8-byte words
    emitter.instruction("cmp QWORD PTR [rsp+32], 2");                           // is called-class id present?
    emitter.instruction("jne __rt_closure_bind_copy");                          // copy the 80-byte descriptor
    emitter.instruction("mov rcx, 12");                                         // 96 bytes = twelve words with called-class id
    emitter.label("__rt_closure_bind_copy");
    emitter.instruction("cld");                                                 // copy forward
    emitter.instruction("rep movsq");                                           // copy the descriptor payload word by word

    // -- overwrite the captured $this, matching the capture representation --
    emitter.instruction("mov rax, [rsp+24]");                                   // rax = capture type tag
    emitter.instruction("cmp rax, 7");                                          // a Mixed capture stores a boxed cell, not a raw object
    emitter.instruction("je __rt_closure_bind_box_this");                       // top-level closures use a Mixed $this receiver
    // object capture (method-defined closure): store the raw object and retain it
    emitter.instruction("mov rax, [rsp+16]");                                   // rax = new descriptor
    emitter.instruction("mov rdx, [rsp+8]");                                    // rdx = new $this receiver
    emitter.instruction("mov [rax+64], rdx");                                   // replace the captured object with the new receiver
    emitter.instruction("mov rdi, rdx");                                        // pass the new receiver to the incref helper
    emitter.instruction("call __rt_incref");                                    // the bound descriptor now owns a reference to $this
    emitter.instruction("jmp __rt_closure_bind_return");                        // skip the Mixed boxing path

    // mixed capture (top-level closure): box the object into a Mixed cell
    emitter.label("__rt_closure_bind_box_this");
    emitter.instruction("mov rax, 6");                                          // boxed payload tag 6 = object
    emitter.instruction("mov rdi, [rsp+8]");                                    // payload low word = new $this object pointer
    emitter.instruction("mov rsi, 0");                                          // payload high word is unused for objects
    emitter.instruction("call __rt_mixed_from_value");                          // box (and retain) the receiver into a Mixed cell
    emitter.instruction("mov rdx, [rsp+16]");                                   // reload the bound descriptor pointer
    emitter.instruction("mov [rdx+64], rax");                                   // store the boxed Mixed receiver into the capture slot

    // -- return the new descriptor --
    emitter.label("__rt_closure_bind_return");
    emitter.instruction("mov rax, [rsp+16]");                                   // rax = bound descriptor result
    emitter.instruction("mov rsp, rbp");                                        // tear down the closure-bind frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the rebound closure descriptor

    // -- unsupported capture shape: fatal --
    emitter.label("__rt_closure_bind_unsupported");
    emitter.instruction("mov edi, 2");                                          // write the unsupported-bind fatal to stderr
    crate::codegen_support::abi::emit_symbol_address(emitter, "rsi", "_closure_bind_unsupported_msg");
    emitter.instruction("mov edx, 71");                                         // byte length of the unsupported-bind fatal message
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the fatal before exiting
    crate::codegen_support::abi::emit_exit(emitter, 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    /// Verifies every supported target validates and copies the optional
    /// called-class capture while retaining the top-level `$this`-only size.
    #[test]
    fn test_closure_bind_emits_both_supported_descriptor_sizes() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_closure_bind(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_closure_bind_validate_this:"), "{target:?}: {asm}");
            assert!(asm.contains("__rt_closure_bind_shape_valid:"), "{target:?}: {asm}");
            if target.arch == Arch::X86_64 {
                assert!(asm.contains(&format!("cmp r11, {}", CALLED_CLASS_ID_LOCAL.len())), "{target:?}: {asm}");
                assert!(asm.contains(&format!("mov r11d, [r10+{}]", CALLED_CLASS_ID_SUFFIX_OFFSET)), "{target:?}: {asm}");
                assert!(asm.contains(&format!("cmp r11d, {}", CALLED_CLASS_ID_SUFFIX)), "{target:?}: {asm}");
                assert!(asm.contains("mov rax, 80"), "{target:?}: {asm}");
                assert!(asm.contains("mov rax, 96"), "{target:?}: {asm}");
                assert!(asm.contains("mov rcx, 12"), "{target:?}: {asm}");
            } else {
                assert!(asm.contains(&format!("cmp x12, #{}", CALLED_CLASS_ID_LOCAL.len())), "{target:?}: {asm}");
                assert!(asm.contains(&format!("ldr w14, [x13, #{}]", CALLED_CLASS_ID_SUFFIX_OFFSET)), "{target:?}: {asm}");
                assert!(asm.contains("movz x15, #0x6723"), "{target:?}: {asm}");
                assert!(asm.contains("movk x15, #0x6e65, lsl #16"), "{target:?}: {asm}");
                assert!(asm.contains("cmp w14, w15"), "{target:?}: {asm}");
                assert!(asm.contains("mov x0, #80"), "{target:?}: {asm}");
                assert!(asm.contains("mov x0, #96"), "{target:?}: {asm}");
                assert!(
                    asm.contains("ldp x2, x3, [x1, #80]"),
                    "{target:?}: {asm}"
                );
            }
        }
    }
}
