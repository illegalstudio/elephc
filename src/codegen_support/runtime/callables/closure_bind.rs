//! Purpose:
//! Emits the `__rt_closure_bind` runtime helper that implements PHP's
//! `Closure::bind` / `Closure::bindTo` / `Closure::call` for a closure with
//! ANY capture shape.
//!
//! Called from:
//! - `crate::codegen_support::runtime::callables`
//!
//! Key details:
//! - A runtime closure descriptor is a 64-byte static header followed by one
//!   16-byte capture slot per capture. Binding copies the whole descriptor,
//!   replaces the capture named `this` with the new receiver when one is given,
//!   and takes the copy's OWN reference to every other by-value capture so the
//!   two descriptors can be released independently.
//! - Capture ownership mirrors `descriptor_release` exactly, tag for tag: a
//!   string capture is an owned copy and is duplicated through
//!   `__rt_str_persist`; arrays, hashes, objects, Mixed boxes, nested callables
//!   and erased iterables are refcounted heap blocks and take `__rt_incref`;
//!   scalars own nothing; a by-reference capture is never replaced, and the
//!   copy takes its own lease on the managed reference cell, which
//!   `descriptor_release` retires.
//! - An eval callback adapter is rebound by Magician and wrapped again; without
//!   eval support linked it is bound like any native closure.
//! - It used to accept exactly one capture, named `this`, and abort otherwise.
//!   That refused ordinary PHP: `\Closure::bind(static function () use ($a, $b)
//!   {...}, null, null)` has four captures and no `$this`, which is what
//!   Symfony's `PhpFileLoader` writes to load a container config file, and the
//!   compiled console died there with
//!   `Closure::bind requires a closure that captures only $this`.
//! - A null receiver leaves every capture in place, which is what PHP does when
//!   only the scope is being changed.
//! - `__rt_closure_set_scope` stamps a RUNTIME-bound scope into a freshly bound descriptor: a
//!   closure whose body addresses properties by a runtime name carries a hidden last `Int`
//!   capture (`crate::names::CLOSURE_SCOPE_LOCAL`), `0` until `Closure::bind($f, $t, $scope)` names
//!   a scope that only exists as a string or object at run time. The stored value is the scope
//!   class id plus one, so `0` stays "use the lexical scope". The helper matches the capture by
//!   its 26-byte name and does nothing for any other descriptor (no environment, no metadata, an
//!   eval adapter, a closure without the capture), so callers need not know which closures have it.
//! - Verified on aarch64 (macOS/Linux) and x86_64 (Linux).

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_closure_bind` runtime helper for the active target.
///
/// Input: `x0`/`rdi` = source closure descriptor pointer, `x1`/`rsi` = the new
/// `$this` object pointer, or null to keep the captures as they are. Output:
/// `x0`/`rax` = a freshly heap-allocated descriptor copy.
pub(crate) fn emit_closure_bind(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_closure_bind_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: closure bind ---");
    emitter.label_global("__rt_closure_bind");

    // -- frame and argument save --
    emitter.instruction("sub sp, sp, #96");                                     // reserve closure-bind spill slots
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #80");                                    // establish a frame pointer for the helper
    emitter.instruction("str x0, [sp, #0]");                                    // save the source descriptor pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the new $this receiver (null keeps the captures)

    // -- an eval callback adapter is rebound by Magician, then wrapped again --
    // Its two captures are the eval context and the boxed callback, never `$this`, so the
    // native shape check below would abort. Both hooks are installed only when eval support is
    // linked; without them the adapter is bound like any native closure.
    emitter.instruction("ldr x9, [x0]");                                        // x9 = descriptor kind word
    emitter.instruction(&format!(                                               // is this an eval callback adapter?
        "cmp x9, #{}",
        crate::codegen_support::callable_descriptor::CALLABLE_DESC_KIND_CALLBACK_ADAPTER
    ));
    emitter.instruction("b.ne __rt_closure_bind_native");                       // native descriptors keep the capture-shape path
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_elephc_eval_closure_bind_fn");
    emitter.instruction("ldr x10, [x10]");                                      // x10 = Magician's rebinding callback, or null
    emitter.instruction("cbz x10, __rt_closure_bind_native");              // no eval support linked: bind it like any native closure
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the adapter descriptor
    emitter.instruction("ldr x0, [x9, #64]");                                   // pass the captured eval context
    emitter.instruction("ldr x1, [x9, #80]");                                   // pass the captured boxed callback
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the raw new $this receiver
    emitter.instruction("add x3, sp, #16");                                     // pass the bound-closure output slot
    emitter.instruction("blr x10");                                             // ask Magician to rebind $this on the eval closure
    emitter.instruction("cbz x0, __rt_closure_bind_native");               // a rejected rebinding binds it like any native closure
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_elephc_eval_wrap_callback_fn");
    emitter.instruction("ldr x10, [x10]");                                      // x10 = generated adapter wrapper, installed with the eval context
    emitter.instruction("cbz x10, __rt_closure_bind_native");              // no wrapper means no eval context ever existed
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the adapter descriptor
    emitter.instruction("ldr x0, [x9, #64]");                                   // pass the eval context to the wrapper
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the owned bound closure cell
    emitter.instruction("blr x10");                                             // x0 = adapter descriptor for the bound closure
    emitter.instruction("str x0, [sp, #24]");                                   // park the descriptor while Magician's owner is retired
    emitter.instruction("ldr x0, [sp, #16]");                                   // the wrapper retained its own capture owner
    emitter.instruction("bl __rt_decref_mixed");                                // so drop the owner Magician handed back
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the adapter descriptor
    emitter.instruction("str x0, [sp, #16]");                                   // publish it as the bind result
    emitter.instruction("b __rt_closure_bind_return");                          // return through the shared epilogue
    emitter.label("__rt_closure_bind_native");

    // -- read the capture shape; a descriptor with no environment has none --
    emitter.instruction("mov x10, #0");                                         // assume zero captures until the environment says otherwise
    emitter.instruction("mov x11, #0");                                         // assume no capture metadata table
    emitter.instruction("ldr x9, [x0, #40]");                                   // x9 = descriptor environment record pointer
    emitter.instruction("cbz x9, __rt_closure_bind_counted");                   // a captureless closure copies its header alone
    emitter.instruction("ldr x10, [x9]");                                       // x10 = capture count
    emitter.instruction("ldr x11, [x9, #16]");                                  // x11 = capture binding metadata table
    emitter.instruction("cbnz x11, __rt_closure_bind_counted");                 // metadata present: the captures are typed and can be owned
    emitter.instruction("mov x10, #0");                                         // without metadata no slot can be typed, so copy the header alone
    emitter.label("__rt_closure_bind_counted");
    emitter.instruction("str x10, [sp, #24]");                                  // save the capture count for the copy and retain loops
    emitter.instruction("str x11, [sp, #32]");                                  // save the capture metadata table

    // -- allocate a descriptor copy sized for this capture count --
    emitter.instruction("lsl x12, x10, #4");                                    // each runtime capture slot is 16 bytes
    emitter.instruction("add x0, x12, #64");                                    // plus the 64-byte static descriptor header
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = fresh descriptor block
    crate::codegen_support::callable_descriptor::emit_initialize_runtime_descriptor(emitter, "x0");
    emitter.instruction("bl __rt_object_handle_acquire");                       // Closure::bind creates a NEW Closure in PHP, so it takes a new object handle
    emitter.instruction("str x0, [sp, #16]");                                   // save the new descriptor pointer

    // -- copy header and capture slots verbatim; ownership is taken below --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source descriptor
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the capture count
    emitter.instruction("lsl x13, x10, #4");                                    // capture bytes
    emitter.instruction("add x13, x13, #64");                                   // total descriptor bytes to copy
    emitter.instruction("mov x14, #0");                                         // byte cursor
    emitter.label("__rt_closure_bind_copy");
    emitter.instruction("cmp x14, x13");                                        // copied the whole descriptor?
    emitter.instruction("b.hs __rt_closure_bind_copied");                       // yes, move on to capture ownership
    emitter.instruction("ldr x2, [x1, x14]");                                   // load the next source word
    emitter.instruction("str x2, [x0, x14]");                                   // store it into the copy
    emitter.instruction("add x14, x14, #8");                                    // advance one word
    emitter.instruction("b __rt_closure_bind_copy");                            // continue copying

    emitter.label("__rt_closure_bind_copied");
    emitter.instruction("str xzr, [sp, #40]");                                  // capture index = 0

    // -- walk the captures: replace `this`, take a reference to everything else --
    emitter.label("__rt_closure_bind_loop");
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the capture index
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the capture count
    emitter.instruction("cmp x12, x13");                                        // processed every capture?
    emitter.instruction("b.hs __rt_closure_bind_return");                       // yes, hand back the bound descriptor
    emitter.instruction("ldr x14, [sp, #32]");                                  // reload the capture metadata table
    emitter.instruction("mov x15, #32");                                        // each capture binding entry is four 8-byte words
    emitter.instruction("mul x15, x12, x15");                                   // byte offset of this capture's metadata
    emitter.instruction("add x14, x14, x15");                                   // x14 = capture metadata entry
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the destination descriptor
    emitter.instruction("mov x10, #16");                                        // each runtime capture slot is 16 bytes
    emitter.instruction("mul x10, x12, x10");                                   // byte offset of this capture's slot
    emitter.instruction("add x10, x10, #64");                                   // skip the static descriptor header
    emitter.instruction("add x9, x9, x10");                                     // x9 = capture slot pointer
    emitter.instruction("str x9, [sp, #48]");                                   // save it across the helper calls below
    emitter.instruction("ldr x15, [x14, #24]");                                 // load the by-ref flag for this capture
    emitter.instruction("cbnz x15, __rt_closure_bind_ref");                     // a by-ref capture is never replaced; it may hold a cell share

    // is this the capture named `this`?
    emitter.instruction("ldr x12, [x14, #8]");                                  // capture name length
    emitter.instruction("cmp x12, #4");                                         // "this" is four bytes long
    emitter.instruction("b.ne __rt_closure_bind_retain");                       // a different-length name is an ordinary capture
    emitter.instruction("ldr x13, [x14]");                                      // capture name byte pointer
    emitter.instruction("ldr w12, [x13]");                                      // load the first four name bytes
    emitter.instruction("movz w15, #0x6874");                                   // low half of "this" little-endian ("th")
    emitter.instruction("movk w15, #0x7369, lsl #16");                          // high half of "this" little-endian ("is")
    emitter.instruction("cmp w12, w15");                                        // is this capture named "this"?
    emitter.instruction("b.ne __rt_closure_bind_retain");                       // no: it is an ordinary capture to take a reference to
    emitter.instruction("ldr x2, [sp, #8]");                                    // the new receiver
    emitter.instruction("cbz x2, __rt_closure_bind_retain");                    // binding a null receiver leaves the captured $this in place
    emitter.instruction("ldr x15, [x14, #16]");                                 // capture type tag (6=object, 7=mixed)
    emitter.instruction("cmp x15, #7");                                         // a Mixed capture stores a boxed cell, not a raw object
    emitter.instruction("b.eq __rt_closure_bind_box_this");                     // top-level closures use a Mixed $this receiver
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the capture slot pointer
    emitter.instruction("str x2, [x9]");                                        // replace the captured object with the new receiver
    emitter.instruction("mov x0, x2");                                          // pass the new receiver to the incref helper
    emitter.instruction("bl __rt_incref");                                      // the bound descriptor now owns a reference to $this
    emitter.instruction("b __rt_closure_bind_next");                            // this capture is done

    emitter.label("__rt_closure_bind_box_this");
    emitter.instruction("mov x0, #6");                                          // boxed payload tag 6 = object
    emitter.instruction("ldr x1, [sp, #8]");                                    // payload low word = new $this object pointer
    emitter.instruction("mov x2, #0");                                          // payload high word is unused for objects
    emitter.instruction("bl __rt_mixed_from_value");                            // box (and retain) the receiver into a Mixed cell
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the capture slot pointer
    emitter.instruction("str x0, [x9]");                                        // store the boxed Mixed receiver into the slot
    emitter.instruction("b __rt_closure_bind_next");                            // this capture is done

    // -- a by-ref capture owns a managed reference-cell lease: the copy takes its own --
    emitter.label("__rt_closure_bind_ref");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the capture slot pointer
    emitter.instruction("ldr x0, [x9]");                                        // the captured reference payload address
    emitter.instruction("bl __rt_reference_cell_owner");                        // x0 = the managed cell that owns that payload
    emitter.instruction("bl __rt_incref");                                      // the copy takes its own lease, retired by descriptor_release
    emitter.instruction("b __rt_closure_bind_next");                            // this capture is done

    // -- take the copy's own reference, tag for tag with `descriptor_release` --
    emitter.label("__rt_closure_bind_retain");
    emitter.instruction("ldr x14, [sp, #32]");                                  // reload the capture metadata table
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the capture index
    emitter.instruction("mov x15, #32");                                        // each capture binding entry is four 8-byte words
    emitter.instruction("mul x15, x12, x15");                                   // byte offset of this capture's metadata
    emitter.instruction("add x14, x14, x15");                                   // x14 = capture metadata entry
    emitter.instruction("ldr x15, [x14, #16]");                                 // capture type tag
    emitter.instruction("ldr x9, [sp, #48]");                                   // capture slot pointer
    emitter.instruction("cmp x15, #1");                                         // a string capture is an OWNED copy, not a shared block
    emitter.instruction("b.eq __rt_closure_bind_retain_string");                // so the copy needs its own duplicate
    emitter.instruction("cmp x15, #4");                                         // indexed array
    emitter.instruction("b.eq __rt_closure_bind_retain_heap");
    emitter.instruction("cmp x15, #5");                                         // associative array
    emitter.instruction("b.eq __rt_closure_bind_retain_heap");
    emitter.instruction("cmp x15, #6");                                         // object
    emitter.instruction("b.eq __rt_closure_bind_retain_heap");
    emitter.instruction("cmp x15, #7");                                         // mixed or union box
    emitter.instruction("b.eq __rt_closure_bind_retain_heap");
    emitter.instruction("cmp x15, #10");                                        // nested callable descriptor
    emitter.instruction("b.eq __rt_closure_bind_retain_heap");
    emitter.instruction("cmp x15, #12");                                        // erased iterable
    emitter.instruction("b.eq __rt_closure_bind_retain_heap");
    emitter.instruction("b __rt_closure_bind_next");                            // scalar captures own nothing

    emitter.label("__rt_closure_bind_retain_string");
    emitter.instruction("ldr x1, [x9]");                                        // string payload pointer (str_persist reads x1/x2)
    emitter.instruction("ldr x2, [x9, #8]");                                    // string byte length
    emitter.instruction("bl __rt_str_persist");                                 // duplicate it so both descriptors own their own copy
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the capture slot pointer
    emitter.instruction("str x1, [x9]");                                        // store the duplicate into the copy's slot
    emitter.instruction("b __rt_closure_bind_next");                            // this capture is done

    emitter.label("__rt_closure_bind_retain_heap");
    emitter.instruction("ldr x0, [x9]");                                        // heap-backed capture payload
    emitter.instruction("bl __rt_incref");                                      // the copy takes its own reference

    emitter.label("__rt_closure_bind_next");
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the capture index after any nested call
    emitter.instruction("add x12, x12, #1");                                    // advance to the next capture
    emitter.instruction("str x12, [sp, #40]");                                  // persist the updated index
    emitter.instruction("b __rt_closure_bind_loop");                            // continue walking captures

    // -- return the new descriptor --
    emitter.label("__rt_closure_bind_return");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = bound descriptor result
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // tear down the closure-bind frame
    emitter.instruction("ret");                                                 // return the rebound closure descriptor
}

/// Emits the Linux x86_64 `__rt_closure_bind` helper (mirror of the aarch64 path).
///
/// Frame slots, all at `rsp` after the prologue: `+0` source descriptor, `+8` new receiver,
/// `+16` destination descriptor, `+24` capture count, `+32` capture metadata table, `+40`
/// capture index, `+48` capture slot pointer.
fn emit_closure_bind_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: closure bind (x86_64) ---");
    emitter.label_global("__rt_closure_bind");

    // -- frame and argument save --
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish this helper's frame
    emitter.instruction("sub rsp, 80");                                         // reserve spill slots (16-byte aligned)
    emitter.instruction("mov [rsp+0], rdi");                                    // save the source descriptor pointer
    emitter.instruction("mov [rsp+8], rsi");                                    // save the new $this receiver (null keeps the captures)

    // -- an eval callback adapter is rebound by Magician, then wrapped again (see AArch64) --
    emitter.instruction("mov r10, [rdi]");                                      // r10 = descriptor kind word
    emitter.instruction(&format!(                                               // is this an eval callback adapter?
        "cmp r10, {}",
        crate::codegen_support::callable_descriptor::CALLABLE_DESC_KIND_CALLBACK_ADAPTER
    ));
    emitter.instruction("jne __rt_closure_bind_native");                        // native descriptors keep the capture-shape path
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_elephc_eval_closure_bind_fn");
    emitter.instruction("mov r11, [r11]");                                      // r11 = Magician's rebinding callback, or null
    emitter.instruction("test r11, r11");                                       // is eval support linked at all?
    emitter.instruction("jz __rt_closure_bind_native");                    // no: bind it like any native closure
    emitter.instruction("mov r10, [rsp+0]");                                    // reload the adapter descriptor
    emitter.instruction("mov rdi, [r10+64]");                                   // pass the captured eval context
    emitter.instruction("mov rsi, [r10+80]");                                   // pass the captured boxed callback
    emitter.instruction("mov rdx, [rsp+8]");                                    // pass the raw new $this receiver
    emitter.instruction("lea rcx, [rsp+16]");                                   // pass the bound-closure output slot
    emitter.instruction("call r11");                                            // ask Magician to rebind $this on the eval closure
    emitter.instruction("test rax, rax");                                       // was the rebinding accepted?
    emitter.instruction("jz __rt_closure_bind_native");                    // no: bind it like any native closure
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_elephc_eval_wrap_callback_fn");
    emitter.instruction("mov r11, [r11]");                                      // r11 = generated adapter wrapper, installed with the eval context
    emitter.instruction("test r11, r11");                                       // did an eval context ever install it?
    emitter.instruction("jz __rt_closure_bind_native");                    // no wrapper: bind it like any native closure
    emitter.instruction("mov r10, [rsp+0]");                                    // reload the adapter descriptor
    emitter.instruction("mov rdi, [r10+64]");                                   // pass the eval context to the wrapper
    emitter.instruction("mov rsi, [rsp+16]");                                   // pass the owned bound closure cell
    emitter.instruction("call r11");                                            // rax = adapter descriptor for the bound closure
    emitter.instruction("mov [rsp+24], rax");                                   // park the descriptor while Magician's owner is retired
    emitter.instruction("mov rax, [rsp+16]");                                   // the wrapper retained its own capture owner
    emitter.instruction("call __rt_decref_mixed");                              // so drop the owner Magician handed back
    emitter.instruction("mov rax, [rsp+24]");                                   // reload the adapter descriptor
    emitter.instruction("mov [rsp+16], rax");                                   // publish it as the bind result
    emitter.instruction("jmp __rt_closure_bind_return");                        // return through the shared epilogue
    emitter.label("__rt_closure_bind_native");

    // -- read the capture shape; a descriptor with no environment has none --
    emitter.instruction("xor r9d, r9d");                                        // assume zero captures
    emitter.instruction("xor r10d, r10d");                                      // assume no capture metadata table
    emitter.instruction("mov r8, [rdi+40]");                                    // r8 = descriptor environment record pointer
    emitter.instruction("test r8, r8");                                         // does the closure capture anything?
    emitter.instruction("jz __rt_closure_bind_counted");                        // a captureless closure copies its header alone
    emitter.instruction("mov r9, [r8]");                                        // r9 = capture count
    emitter.instruction("mov r10, [r8+16]");                                    // r10 = capture binding metadata table
    emitter.instruction("test r10, r10");                                       // is the capture metadata present?
    emitter.instruction("jnz __rt_closure_bind_counted");                       // yes: the captures are typed and can be owned
    emitter.instruction("xor r9d, r9d");                                        // without metadata no slot can be typed, so copy the header alone
    emitter.label("__rt_closure_bind_counted");
    emitter.instruction("mov [rsp+24], r9");                                    // save the capture count
    emitter.instruction("mov [rsp+32], r10");                                   // save the capture metadata table

    // -- allocate a descriptor copy sized for this capture count --
    emitter.instruction("mov rax, r9");                                         // capture count
    emitter.instruction("shl rax, 4");                                          // each runtime capture slot is 16 bytes
    emitter.instruction("add rax, 64");                                         // plus the 64-byte static descriptor header
    emitter.instruction("call __rt_heap_alloc");                                // rax = fresh descriptor block
    crate::codegen_support::callable_descriptor::emit_initialize_runtime_descriptor(emitter, "rax");
    emitter.instruction("call __rt_object_handle_acquire");                     // Closure::bind creates a NEW Closure in PHP, so it takes a new object handle
    emitter.instruction("mov [rsp+16], rax");                                   // save the new descriptor pointer

    // -- copy header and capture slots verbatim; ownership is taken below --
    emitter.instruction("mov rsi, [rsp+0]");                                    // rsi = source descriptor
    emitter.instruction("mov rdi, rax");                                        // rdi = destination descriptor
    emitter.instruction("mov rcx, [rsp+24]");                                   // capture count
    emitter.instruction("shl rcx, 1");                                          // two 8-byte words per 16-byte capture slot
    emitter.instruction("add rcx, 8");                                          // plus the eight header words
    emitter.instruction("cld");                                                 // copy forward
    emitter.instruction("rep movsq");                                           // copy the descriptor payload word by word
    emitter.instruction("mov QWORD PTR [rsp+40], 0");                           // capture index = 0

    // -- walk the captures: replace `this`, take a reference to everything else --
    emitter.label("__rt_closure_bind_loop");
    emitter.instruction("mov rcx, [rsp+40]");                                   // reload the capture index
    emitter.instruction("cmp rcx, [rsp+24]");                                   // processed every capture?
    emitter.instruction("jae __rt_closure_bind_return");                        // yes, hand back the bound descriptor
    emitter.instruction("mov r10, [rsp+32]");                                   // reload the capture metadata table
    emitter.instruction("mov rax, rcx");                                        // capture index
    emitter.instruction("shl rax, 5");                                          // each capture binding entry is four 8-byte words
    emitter.instruction("add r10, rax");                                        // r10 = capture metadata entry
    emitter.instruction("mov r8, [rsp+16]");                                    // reload the destination descriptor
    emitter.instruction("mov rax, rcx");                                        // capture index
    emitter.instruction("shl rax, 4");                                          // each runtime capture slot is 16 bytes
    emitter.instruction("add rax, 64");                                         // skip the static descriptor header
    emitter.instruction("add r8, rax");                                         // r8 = capture slot pointer
    emitter.instruction("mov [rsp+48], r8");                                    // save it across the helper calls below
    emitter.instruction("mov rax, [r10+24]");                                   // load the by-ref flag for this capture
    emitter.instruction("test rax, rax");                                       // is it a by-reference capture?
    emitter.instruction("jnz __rt_closure_bind_ref");                           // yes: never replaced; it may hold a cell share

    // is this the capture named `this`?
    emitter.instruction("mov rax, [r10+8]");                                    // capture name length
    emitter.instruction("cmp rax, 4");                                          // "this" is four bytes long
    emitter.instruction("jne __rt_closure_bind_retain");                        // a different-length name is an ordinary capture
    emitter.instruction("mov r11, [r10]");                                      // capture name byte pointer
    emitter.instruction("mov eax, [r11]");                                      // load the first four name bytes
    emitter.instruction("cmp eax, 0x73696874");                                 // compare against "this" little-endian
    emitter.instruction("jne __rt_closure_bind_retain");                        // no: it is an ordinary capture to take a reference to
    emitter.instruction("mov rdx, [rsp+8]");                                    // the new receiver
    emitter.instruction("test rdx, rdx");                                       // was a receiver given?
    emitter.instruction("jz __rt_closure_bind_retain");                         // binding a null receiver leaves the captured $this in place
    emitter.instruction("mov rax, [r10+16]");                                   // capture type tag (6=object, 7=mixed)
    emitter.instruction("cmp rax, 7");                                          // a Mixed capture stores a boxed cell, not a raw object
    emitter.instruction("je __rt_closure_bind_box_this");                       // top-level closures use a Mixed $this receiver
    emitter.instruction("mov r8, [rsp+48]");                                    // reload the capture slot pointer
    emitter.instruction("mov [r8], rdx");                                       // replace the captured object with the new receiver
    emitter.instruction("mov rax, rdx");                                        // pass the new receiver in rax, the register __rt_incref reads
    emitter.instruction("call __rt_incref");                                    // the bound descriptor now owns a reference to $this
    emitter.instruction("jmp __rt_closure_bind_next");                          // this capture is done

    emitter.label("__rt_closure_bind_box_this");
    emitter.instruction("mov rax, 6");                                          // boxed payload tag 6 = object
    emitter.instruction("mov rdi, [rsp+8]");                                    // payload low word = new $this object pointer
    emitter.instruction("mov rsi, 0");                                          // payload high word is unused for objects
    emitter.instruction("call __rt_mixed_from_value");                          // box (and retain) the receiver into a Mixed cell
    emitter.instruction("mov r8, [rsp+48]");                                    // reload the capture slot pointer
    emitter.instruction("mov [r8], rax");                                       // store the boxed Mixed receiver into the slot
    emitter.instruction("jmp __rt_closure_bind_next");                          // this capture is done

    // -- a by-ref capture owns a managed reference-cell lease: the copy takes its own --
    emitter.label("__rt_closure_bind_ref");
    emitter.instruction("mov r8, [rsp+48]");                                    // reload the capture slot pointer
    emitter.instruction("mov rax, [r8]");                                       // the captured reference payload address
    emitter.instruction("call __rt_reference_cell_owner");                      // rax = the managed cell that owns that payload
    emitter.instruction("call __rt_incref");                                    // the copy takes its own lease, retired by descriptor_release
    emitter.instruction("jmp __rt_closure_bind_next");                          // this capture is done

    // -- take the copy's own reference, tag for tag with `descriptor_release` --
    emitter.label("__rt_closure_bind_retain");
    emitter.instruction("mov r10, [rsp+32]");                                   // reload the capture metadata table
    emitter.instruction("mov rcx, [rsp+40]");                                   // reload the capture index
    emitter.instruction("mov rax, rcx");                                        // capture index
    emitter.instruction("shl rax, 5");                                          // each capture binding entry is four 8-byte words
    emitter.instruction("add r10, rax");                                        // r10 = capture metadata entry
    emitter.instruction("mov rax, [r10+16]");                                   // capture type tag
    emitter.instruction("mov r8, [rsp+48]");                                    // capture slot pointer
    emitter.instruction("cmp rax, 1");                                          // a string capture is an OWNED copy, not a shared block
    emitter.instruction("je __rt_closure_bind_retain_string");                  // so the copy needs its own duplicate
    emitter.instruction("cmp rax, 4");                                          // indexed array
    emitter.instruction("je __rt_closure_bind_retain_heap");
    emitter.instruction("cmp rax, 5");                                          // associative array
    emitter.instruction("je __rt_closure_bind_retain_heap");
    emitter.instruction("cmp rax, 6");                                          // object
    emitter.instruction("je __rt_closure_bind_retain_heap");
    emitter.instruction("cmp rax, 7");                                          // mixed or union box
    emitter.instruction("je __rt_closure_bind_retain_heap");
    emitter.instruction("cmp rax, 10");                                         // nested callable descriptor
    emitter.instruction("je __rt_closure_bind_retain_heap");
    emitter.instruction("cmp rax, 12");                                         // erased iterable
    emitter.instruction("je __rt_closure_bind_retain_heap");
    emitter.instruction("jmp __rt_closure_bind_next");                          // scalar captures own nothing

    emitter.label("__rt_closure_bind_retain_string");
    emitter.instruction("mov rax, [r8]");                                       // string payload pointer (str_persist reads rax/rdx)
    emitter.instruction("mov rdx, [r8+8]");                                     // string byte length
    emitter.instruction("call __rt_str_persist");                               // duplicate it so both descriptors own their own copy
    emitter.instruction("mov r8, [rsp+48]");                                    // reload the capture slot pointer
    emitter.instruction("mov [r8], rax");                                       // store the duplicate into the copy's slot
    emitter.instruction("jmp __rt_closure_bind_next");                          // this capture is done

    emitter.label("__rt_closure_bind_retain_heap");
    emitter.instruction("mov rax, [r8]");                                       // heap-backed capture payload, in the register __rt_incref reads
    emitter.instruction("call __rt_incref");                                    // the copy takes its own reference

    emitter.label("__rt_closure_bind_next");
    emitter.instruction("mov rcx, [rsp+40]");                                   // reload the capture index after any nested call
    emitter.instruction("add rcx, 1");                                          // advance to the next capture
    emitter.instruction("mov [rsp+40], rcx");                                   // persist the updated index
    emitter.instruction("jmp __rt_closure_bind_loop");                          // continue walking captures

    // -- return the new descriptor --
    emitter.label("__rt_closure_bind_return");
    emitter.instruction("mov rax, [rsp+16]");                                   // rax = bound descriptor result
    emitter.instruction("mov rsp, rbp");                                        // tear down the closure-bind frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the rebound closure descriptor
}

/// Emits `__rt_closure_set_scope(descriptor, encoded_scope)` for the active target.
///
/// Input: `x0`/`rdi` = a closure descriptor this caller exclusively owns (a fresh
/// `__rt_closure_bind` copy), `x1`/`rsi` = the encoded scope (class id + 1, `0` = unchanged).
/// An encoded `0` is a no-op so a scope that named no class keeps the lexical one. Output: none.
pub(crate) fn emit_closure_set_scope(emitter: &mut Emitter) {
    // The 26-byte capture name `crate::names::CLOSURE_SCOPE_LOCAL` as three little-endian words.
    const NAME_WORD_0: i64 = 0x636870656c655f5f; // "__elephc"
    const NAME_WORD_1: i64 = 0x657275736f6c635f; // "_closure"
    const NAME_WORD_2: i64 = 0x672365706f63735f; // "_scope#g"
    emitter.blank();
    emitter.comment("--- runtime: closure set scope ---");
    emitter.label_global("__rt_closure_set_scope");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("test rsi, rsi");                                   // an encoded scope of zero means "unchanged"
        emitter.instruction("jz __rt_closure_set_scope_done");                  // keep the lexical scope
        emitter.instruction("mov r8, [rdi+40]");                                // r8 = descriptor environment record
        emitter.instruction("test r8, r8");                                     // a captureless closure has no scope slot
        emitter.instruction("jz __rt_closure_set_scope_done");                  // nothing to stamp
        emitter.instruction("mov r9, [r8]");                                    // r9 = capture count
        emitter.instruction("test r9, r9");                                     // an empty environment has no scope slot
        emitter.instruction("jz __rt_closure_set_scope_done");                  // nothing to stamp
        emitter.instruction("mov r10, [r8+16]");                                // r10 = capture metadata table
        emitter.instruction("test r10, r10");                                   // untyped captures cannot be named
        emitter.instruction("jz __rt_closure_set_scope_done");                  // nothing to stamp
        emitter.instruction("lea r11, [r9-1]");                                 // r11 = index of the LAST capture
        emitter.instruction("mov rax, r11");                                    // copy the index to scale it
        emitter.instruction("shl rax, 5");                                      // each metadata entry is four 8-byte words
        emitter.instruction("add rax, r10");                                    // rax = last capture's metadata entry
        emitter.instruction("cmp QWORD PTR [rax+8], 26");                       // the scope capture's name is 26 bytes long
        emitter.instruction("jne __rt_closure_set_scope_done");                 // any other name is an ordinary capture
        emitter.instruction("mov rcx, [rax]");                                  // rcx = capture name bytes
        emitter.instruction(&format!("mov rdx, {NAME_WORD_0}"));               // first eight name bytes
        emitter.instruction("cmp [rcx], rdx");                                  // compare them
        emitter.instruction("jne __rt_closure_set_scope_done");                 // not the scope capture
        emitter.instruction(&format!("mov rdx, {NAME_WORD_1}"));               // next eight name bytes
        emitter.instruction("cmp [rcx+8], rdx");                                // compare them
        emitter.instruction("jne __rt_closure_set_scope_done");                 // not the scope capture
        emitter.instruction(&format!("mov rdx, {NAME_WORD_2}"));               // next eight name bytes
        emitter.instruction("cmp [rcx+16], rdx");                               // compare them
        emitter.instruction("jne __rt_closure_set_scope_done");                 // not the scope capture
        emitter.instruction("shl r11, 4");                                      // each runtime capture slot is 16 bytes
        emitter.instruction("mov [rdi+r11+64], rsi");                           // stamp the scope into the slot after the 64-byte header
        emitter.label("__rt_closure_set_scope_done");
        emitter.instruction("ret");                                             // return to the caller
        return;
    }
    emitter.instruction("cbz x1, __rt_closure_set_scope_done");                 // an encoded scope of zero means "unchanged"
    emitter.instruction("ldr x9, [x0, #40]");                                   // x9 = descriptor environment record
    emitter.instruction("cbz x9, __rt_closure_set_scope_done");                 // a captureless closure has no scope slot
    emitter.instruction("ldr x10, [x9]");                                       // x10 = capture count
    emitter.instruction("cbz x10, __rt_closure_set_scope_done");                // an empty environment has no scope slot
    emitter.instruction("ldr x11, [x9, #16]");                                  // x11 = capture metadata table
    emitter.instruction("cbz x11, __rt_closure_set_scope_done");                // untyped captures cannot be named
    emitter.instruction("sub x12, x10, #1");                                    // x12 = index of the LAST capture
    emitter.instruction("lsl x13, x12, #5");                                    // each metadata entry is four 8-byte words
    emitter.instruction("add x13, x13, x11");                                   // x13 = last capture's metadata entry
    emitter.instruction("ldr x14, [x13, #8]");                                  // x14 = capture name length
    emitter.instruction("cmp x14, #26");                                        // the scope capture's name is 26 bytes long
    emitter.instruction("b.ne __rt_closure_set_scope_done");                    // any other name is an ordinary capture
    emitter.instruction("ldr x14, [x13]");                                      // x14 = capture name bytes
    abi::emit_load_int_immediate(emitter, "x15", NAME_WORD_0);
    emitter.instruction("ldr x16, [x14]");                                      // first eight name bytes
    emitter.instruction("cmp x16, x15");                                        // compare them
    emitter.instruction("b.ne __rt_closure_set_scope_done");                    // not the scope capture
    abi::emit_load_int_immediate(emitter, "x15", NAME_WORD_1);
    emitter.instruction("ldr x16, [x14, #8]");                                  // next eight name bytes
    emitter.instruction("cmp x16, x15");                                        // compare them
    emitter.instruction("b.ne __rt_closure_set_scope_done");                    // not the scope capture
    abi::emit_load_int_immediate(emitter, "x15", NAME_WORD_2);
    emitter.instruction("ldr x16, [x14, #16]");                                 // next eight name bytes
    emitter.instruction("cmp x16, x15");                                        // compare them
    emitter.instruction("b.ne __rt_closure_set_scope_done");                    // not the scope capture
    emitter.instruction("lsl x12, x12, #4");                                    // each runtime capture slot is 16 bytes
    emitter.instruction("add x12, x12, #64");                                   // skip the static descriptor header
    emitter.instruction("str x1, [x0, x12]");                                   // stamp the scope into the capture slot
    emitter.label("__rt_closure_set_scope_done");
    emitter.instruction("ret");                                                 // return to the caller
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Emits `__rt_closure_bind` for one target and returns the assembly text.
    fn emit_for(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_closure_bind(&mut emitter);
        emitter.output()
    }

    /// `__rt_incref` reads its argument in `x0`/`rax`, never in the SysV first argument
    /// register, so the object-capture arm has to hand it the receiver THERE.
    ///
    /// The x86_64 arm passed the receiver in `rdi` while `rax` still held the freshly allocated
    /// descriptor, so the call retained the DESCRIPTOR and never retained `$this`: one reference
    /// too many on the closure and one too few on the receiver, which is a use-after-free waiting
    /// for the caller's last release. Nothing local catches it — only the `linux-x86_64` CI shard
    /// executes this helper, and the aarch64 arm (`mov x0, x2`) has always been right — so the
    /// register is pinned here, on BOTH targets, from the emitted text.
    #[test]
    fn the_object_capture_arm_passes_this_in_the_incref_input_register() {
        for (target, expected, label) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "mov x0, x2",
                "bl __rt_incref",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "mov rax, rdx",
                "call __rt_incref",
            ),
        ] {
            let asm = emit_for(target);
            let before = asm
                .split(label)
                .next()
                .unwrap_or_else(|| panic!("no {label} in __rt_closure_bind for {target:?}:\n{asm}"));
            let last_move = before
                .lines()
                .rev()
                .find(|line| line.trim_start().starts_with("mov "))
                .unwrap_or_else(|| panic!("no register move before {label} for {target:?}:\n{asm}"));
            assert!(
                last_move.trim_start().starts_with(expected),
                "the move immediately before {label} must load the receiver into the register \
                 __rt_incref reads ({expected}), got `{}` for {target:?}",
                last_move.trim()
            );
        }
    }

    /// The receiver `__rt_incref` is handed must be the one just stored into the capture slot,
    /// not the descriptor that happens to be in the result register.
    ///
    /// The control for the test above: it would still pass if the arm moved the DESCRIPTOR into
    /// `rax`, because that is also a `mov rax, …`. Here the same register is read out of the
    /// stack slot holding the new `$this` (`[rsp+8]` / `[sp, #8]`) and stored through the CAPTURE
    /// SLOT POINTER (`[rsp+48]` / `[sp, #48]`), which is what makes it the receiver. The slot is
    /// addressed through that pointer rather than at a fixed `+64` because a descriptor now
    /// carries one slot per capture and `this` need not be the first.
    #[test]
    fn the_retained_receiver_is_the_one_stored_into_the_capture_slot() {
        for (target, load_receiver, load_slot, store) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "ldr x2, [sp, #8]",
                "ldr x9, [sp, #48]",
                "str x2, [x9]",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "mov rdx, [rsp+8]",
                "mov r8, [rsp+48]",
                "mov [r8], rdx",
            ),
        ] {
            let asm = emit_for(target);
            for needle in [load_receiver, load_slot, store] {
                assert!(
                    asm.contains(needle),
                    "the object-capture arm must `{needle}` so the retained value is the receiver \
                     stored into the capture slot ({target:?}):\n{asm}"
                );
            }
        }
    }

    /// Verifies the helper no longer refuses a capture shape: every closure binds.
    ///
    /// It used to accept exactly one capture named `this` and abort otherwise, which refused
    /// `\Closure::bind(static function () use ($a, $b) {…}, null, null)` — ordinary PHP, and what
    /// Symfony's `PhpFileLoader` writes to load a container config file.
    #[test]
    fn closure_bind_accepts_every_capture_shape() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = emit_for(target);
            assert!(
                !asm.contains("_closure_bind_unsupported_msg"),
                "closure bind must not abort on a capture shape ({target:?})"
            );
            assert!(
                asm.contains("__rt_str_persist"),
                "a string capture must be duplicated, not shared ({target:?})"
            );
            assert!(
                asm.contains("__rt_incref"),
                "a heap-backed capture must be retained by the copy ({target:?})"
            );
        }
    }
}
