//! Purpose:
//! Emits the `__rt_closure_bind` runtime helper that implements PHP's
//! `Closure::bind` / `Closure::bindTo` / `Closure::call` for closures that
//! capture only `$this`.
//!
//! Called from:
//! - `crate::codegen::runtime::callables`
//!
//! Key details:
//! - A closure that uses `$this` carries exactly one runtime capture named
//!   "this" (appended by EIR lowering). Binding copies the 80-byte runtime
//!   descriptor (64-byte static header + one 16-byte capture slot), overwrites
//!   the captured object with the new receiver, and increfs it so the bound
//!   descriptor owns its own reference (balanced against descriptor release).
//! - Closures with any other capture shape (extra `use` variables, no `$this`)
//!   are not yet supported and abort with a fatal diagnostic rather than
//!   corrupt a capture slot.
//! - Verified on aarch64 (macOS/Linux) and x86_64 (Linux).

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_closure_bind` runtime helper for the active target.
///
/// Input: `x0`/`rdi` = source closure descriptor pointer, `x1`/`rsi` = the new
/// `$this` object pointer. Output: `x0`/`rax` = a freshly heap-allocated
/// descriptor copy whose `this` capture is the new receiver. Aborts (exit 1)
/// when the source closure does not capture exactly one `$this`.
pub(crate) fn emit_closure_bind(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_closure_bind_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: closure bind ($this-only) ---");
    emitter.label_global("__rt_closure_bind");

    // -- frame and argument save --
    emitter.instruction("sub sp, sp, #48");                                     // reserve closure-bind spill slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #32");                                    // establish a frame pointer for the helper
    emitter.instruction("str x0, [sp, #0]");                                    // save the source descriptor pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the new $this receiver

    // -- validate capture shape: exactly one capture named "this" --
    emitter.instruction("ldr x9, [x0, #40]");                                   // x9 = descriptor environment record pointer
    emitter.instruction("cbz x9, __rt_closure_bind_unsupported");               // no captures means there is no $this to rebind
    emitter.instruction("ldr x10, [x9]");                                       // x10 = capture count
    emitter.instruction("cmp x10, #1");                                         // only the single-$this capture shape is supported
    emitter.instruction("b.ne __rt_closure_bind_unsupported");                  // extra captures are not yet handled
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

    // -- allocate an 80-byte runtime descriptor copy --
    emitter.instruction("mov x0, #80");                                         // 64-byte static header + one 16-byte capture slot
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = fresh descriptor block
    emitter.instruction("str x0, [sp, #16]");                                   // save the new descriptor pointer

    // -- copy the 80-byte descriptor payload --
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
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // tear down the closure-bind frame
    emitter.instruction("ret");                                                 // return the rebound closure descriptor

    // -- unsupported capture shape: fatal --
    emitter.label("__rt_closure_bind_unsupported");
    emitter.instruction("mov x0, #2");                                          // write the unsupported-bind fatal to stderr
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_closure_bind_unsupported_msg");
    emitter.instruction("mov x2, #71");                                         // byte length of the unsupported-bind fatal message
    emitter.syscall(4);
    crate::codegen::abi::emit_exit(emitter, 1);
}

/// Emits the Linux x86_64 `__rt_closure_bind` helper (mirror of the aarch64 path).
fn emit_closure_bind_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: closure bind ($this-only, x86_64) ---");
    emitter.label_global("__rt_closure_bind");

    // -- frame and argument save --
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish this helper's frame
    emitter.instruction("sub rsp, 48");                                         // reserve spill slots (16-byte aligned)
    emitter.instruction("mov [rsp+0], rdi");                                    // save the source descriptor pointer
    emitter.instruction("mov [rsp+8], rsi");                                    // save the new $this receiver

    // -- validate capture shape: exactly one capture named "this" --
    emitter.instruction("mov r8, [rdi+40]");                                    // r8 = descriptor environment record pointer
    emitter.instruction("test r8, r8");                                         // are there any captures?
    emitter.instruction("jz __rt_closure_bind_unsupported");                    // no captures means there is no $this to rebind
    emitter.instruction("mov r9, [r8]");                                        // r9 = capture count
    emitter.instruction("cmp r9, 1");                                           // only the single-$this capture shape is supported
    emitter.instruction("jne __rt_closure_bind_unsupported");                   // extra captures are not yet handled
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

    // -- allocate an 80-byte runtime descriptor copy --
    emitter.instruction("mov rax, 80");                                         // 64-byte static header + one 16-byte capture slot
    emitter.instruction("call __rt_heap_alloc");                                // rax = fresh descriptor block
    emitter.instruction("mov [rsp+16], rax");                                   // save the new descriptor pointer

    // -- copy the 80-byte descriptor payload --
    emitter.instruction("mov rsi, [rsp+0]");                                    // rsi = source descriptor
    emitter.instruction("mov rdi, rax");                                        // rdi = destination descriptor
    emitter.instruction("mov rcx, 10");                                         // 80 bytes = ten 8-byte words
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
    crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_closure_bind_unsupported_msg");
    emitter.instruction("mov edx, 71");                                         // byte length of the unsupported-bind fatal message
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the fatal before exiting
    crate::codegen::abi::emit_exit(emitter, 1);
}
