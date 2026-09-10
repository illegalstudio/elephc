//! Purpose:
//! Emits the `__rt_var_dump_object` family: the runtime walkers that render a PHP
//! `var_dump` OBJECT body — `object(C)#1 (n) {\n  ["p"]=>\n  int(1)\n}` — plus the
//! visited-pointer stack that turns a self-referential object graph into PHP's
//! `*RECURSION*` marker instead of an unbounded walk.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - `__rt_var_dump_value` (tag 6) in `super::var_dump_walk`, which is itself
//!   reached from the array/hash walkers and from the `var_dump` builtin for a
//!   top-level object.
//!
//! Key details:
//! - Property enumeration is driven by the per-class descriptor table
//!   `_class_vd_desc_ptrs[class_id]` emitted by
//!   `crate::codegen_support::runtime::data::user`. Layout: a property count at
//!   offset 0, then one 48-byte row per DECLARED property in layout order:
//!   `(key_ptr, key_len, byte_offset, value_tag, type_ptr, type_len)`. `key` is
//!   the text PHP prints between the `[` and `]`, already carrying the
//!   visibility annotation (`"p"`, `"p":protected`, `"p":"C":private`), so the
//!   walker never has to reason about visibility at runtime.
//! - Object layout (see `codegen::lower_inst::objects::emit_object_allocation`):
//!   class id at offset 0, then a UNIFORM 16-byte slot per property at
//!   `8 + index * 16` — low word = payload, high word = string length / the
//!   uninitialized-typed-property marker. The descriptor carries the byte offset
//!   outright so this walker never recomputes it.
//! - A typed property declared without a default carries
//!   `UNINITIALIZED_TYPED_PROPERTY_SENTINEL` (`0x7fff_ffff_ffff_fffd`) in its
//!   slot's HIGH word, the same marker `emit_typed_property_initialized_bool`
//!   tests. Such a property renders `uninitialized(TYPE)` and is EXCLUDED from
//!   the `(n)` count, matching PHP — which is why the count is computed at
//!   runtime by `__rt_vd_obj_count` rather than baked into the descriptor.
//! - THE `#N` OBJECT HANDLE: PHP's `object(C)#N` handle is real here. `#N` is read
//!   from `__rt_object_handle_of`, the SAME helper `spl_object_id()` calls, so the
//!   printed handle and `spl_object_id()` can never disagree. Handles are small
//!   dense integers starting at 1, and they are REUSED LIFO after an object dies,
//!   matching php-src. The pool, the direct-mapped side table that carries a handle
//!   without touching the allocator header or the property layout, and the exact
//!   ownership argument (it stores no pointers, so it can neither keep an object
//!   alive nor resurrect a freed one) all live in `runtime::objects::handles`.
//!   Handle 0 renders as `#0` and means the block never went through
//!   `__rt_object_handle_acquire` — a missed allocation site is meant to be
//!   glaring rather than silently plausible.
//! - CLOSURES CONSUME A HANDLE TOO, because in PHP a Closure IS an object:
//!   `$f = function () {}; var_dump(new P());` prints `object(P)#2`. elephc
//!   represents a closure as a callable DESCRIPTOR rather than a class-id-headed
//!   object, so `codegen::lower_inst::lower_closure_new` allocates a runtime
//!   descriptor for EVERY closure — including the capture-free ones that used to
//!   collapse to a static `.data` address — purely so the closure has heap storage
//!   whose lifetime can carry a handle. `__rt_callable_descriptor_release` frees
//!   that storage through `__rt_heap_free`, which is the same release chokepoint
//!   objects use, so a closure hands its handle back exactly when PHP destroys the
//!   Closure. `tests/var_dump_object_tests.rs` pins this against reference PHP.
//! - RECURSION GUARD: `_vd_seen` is a bounded stack of the object pointers
//!   currently being walked. `__rt_var_dump_value` consults it before opening an
//!   object; a hit — or a full stack — renders `*RECURSION*` and returns, so the
//!   walk always terminates. The stack is pushed/popped around the body only,
//!   so two SIBLING references to the same object still both render in full,
//!   exactly like PHP.
//! - Every helper here saves and restores the frame pointer and return address
//!   around its internal calls, matching the rest of the var_dump walkers; the
//!   leaf helpers (`__rt_vd_seen_*`, `__rt_vd_obj_desc`, `__rt_vd_obj_count`)
//!   make no calls at all and touch caller-saved scratch only.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Capacity of the `_vd_seen` visited-object stack, in pointers. Kept in sync
/// with the `.comm _vd_seen` reservation in `runtime::data::fixed`.
const VD_SEEN_CAPACITY: u64 = 256;

/// Byte width of one `_class_vd_desc_*` property row.
const VD_DESC_ROW_BYTES: u64 = 48;

/// Materializes the uninitialized-typed-property sentinel into an AArch64 register.
///
/// The value (`0x7fff_ffff_ffff_fffd`) needs the full four-halfword `movz`/`movk`
/// sequence; it must match `codegen_support::sentinels::
/// UNINITIALIZED_TYPED_PROPERTY_SENTINEL` exactly or an initialized property
/// would be misread as uninitialized.
fn emit_uninit_sentinel_aarch64(emitter: &mut Emitter, reg: &str) {
    emitter.instruction(&format!("movz {}, #0xfffd", reg));                     // low halfword of the uninitialized-typed-property sentinel
    emitter.instruction(&format!("movk {}, #0xffff, lsl #16", reg));            // second halfword of the uninitialized sentinel
    emitter.instruction(&format!("movk {}, #0xffff, lsl #32", reg));            // third halfword of the uninitialized sentinel
    emitter.instruction(&format!("movk {}, #0x7fff, lsl #48", reg));            // top halfword of the uninitialized sentinel
}

/// `__rt_vd_seen_find`: report whether an object pointer is already being walked.
///
/// Input: AArch64 x0=object x1=optional debug-info Mixed / x86_64
/// rdi=object rsi=optional debug-info Mixed. A present array/hash projection
/// supplies PHP's displayed property count.
/// Output: AArch64 x0 / x86_64 rax = 1 when the pointer is on the visited stack
/// OR the stack is full, 0 otherwise. Treating a full stack as a hit is what
/// bounds the walk: the caller renders `*RECURSION*` and stops descending.
pub fn emit_vd_seen_find(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_vd_seen_find_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: vd_seen_find ---");
    emitter.label_global("__rt_vd_seen_find");

    abi::emit_symbol_address(emitter, "x9", "_vd_seen_n");                      // resolve the visited-stack depth counter
    emitter.instruction("ldr x10, [x9]");                                       // load the current visited-stack depth
    emitter.instruction(&format!("cmp x10, #{}", VD_SEEN_CAPACITY));            // is the visited stack already full?
    emitter.instruction("b.ge __rt_vd_seen_find_hit");                          // a full stack reports a hit so the walk terminates
    abi::emit_symbol_address(emitter, "x11", "_vd_seen");                       // resolve the visited-pointer stack base
    emitter.instruction("mov x12, #0");                                         // start the scan at the bottom of the stack

    emitter.label("__rt_vd_seen_find_loop");
    emitter.instruction("cmp x12, x10");                                        // scanned every live entry?
    emitter.instruction("b.ge __rt_vd_seen_find_miss");                         // the pointer is not currently being walked
    emitter.instruction("ldr x13, [x11, x12, lsl #3]");                         // load the visited pointer at this depth
    emitter.instruction("cmp x13, x0");                                         // does it match the candidate object?
    emitter.instruction("b.eq __rt_vd_seen_find_hit");                          // already on the walk stack → recursion
    emitter.instruction("add x12, x12, #1");                                    // advance to the next visited entry
    emitter.instruction("b __rt_vd_seen_find_loop");                            // continue scanning

    emitter.label("__rt_vd_seen_find_hit");
    emitter.instruction("mov x0, #1");                                          // report recursion (or exhausted guard capacity)
    emitter.instruction("ret");                                                 // return to caller

    emitter.label("__rt_vd_seen_find_miss");
    emitter.instruction("mov x0, #0");                                          // the object is safe to descend into
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 visited-object lookup helper.
fn emit_vd_seen_find_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: vd_seen_find ---");
    emitter.label_global("__rt_vd_seen_find");

    abi::emit_symbol_address(emitter, "r9", "_vd_seen_n");                      // resolve the visited-stack depth counter
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current visited-stack depth
    emitter.instruction(&format!("cmp r10, {}", VD_SEEN_CAPACITY));             // is the visited stack already full?
    emitter.instruction("jge __rt_vd_seen_find_hit_x86");                       // a full stack reports a hit so the walk terminates
    abi::emit_symbol_address(emitter, "r11", "_vd_seen");                       // resolve the visited-pointer stack base
    emitter.instruction("xor rcx, rcx");                                        // start the scan at the bottom of the stack

    emitter.label("__rt_vd_seen_find_loop_x86");
    emitter.instruction("cmp rcx, r10");                                        // scanned every live entry?
    emitter.instruction("jge __rt_vd_seen_find_miss_x86");                      // the pointer is not currently being walked
    emitter.instruction("mov rax, QWORD PTR [r11 + rcx * 8]");                  // load the visited pointer at this depth
    emitter.instruction("cmp rax, rdi");                                        // does it match the candidate object?
    emitter.instruction("je __rt_vd_seen_find_hit_x86");                        // already on the walk stack → recursion
    emitter.instruction("add rcx, 1");                                          // advance to the next visited entry
    emitter.instruction("jmp __rt_vd_seen_find_loop_x86");                      // continue scanning

    emitter.label("__rt_vd_seen_find_hit_x86");
    emitter.instruction("mov rax, 1");                                          // report recursion (or exhausted guard capacity)
    emitter.instruction("ret");                                                 // return to caller

    emitter.label("__rt_vd_seen_find_miss_x86");
    emitter.instruction("xor rax, rax");                                        // the object is safe to descend into
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_vd_seen_push`: record an object pointer as being walked.
///
/// Silently drops the entry when the stack is full — `__rt_vd_seen_find` already
/// refuses to descend at that depth, so the pop below can never underflow.
/// Input: AArch64 x0=object x1=optional debug-info Mixed / x86_64
/// rdi=object rsi=optional debug-info Mixed. A present array/hash projection is
/// authoritative and is walked instead of the static declared-property descriptor.
pub fn emit_vd_seen_push(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: vd_seen_push ---");
        emitter.label_global("__rt_vd_seen_push");
        abi::emit_symbol_address(emitter, "r9", "_vd_seen_n");                  // resolve the visited-stack depth counter
        emitter.instruction("mov r10, QWORD PTR [r9]");                         // load the current visited-stack depth
        emitter.instruction(&format!("cmp r10, {}", VD_SEEN_CAPACITY));         // is there room for another entry?
        emitter.instruction("jge __rt_vd_seen_push_done_x86");                  // a full stack is already refusing to descend
        abi::emit_symbol_address(emitter, "r11", "_vd_seen");                   // resolve the visited-pointer stack base
        emitter.instruction("mov QWORD PTR [r11 + r10 * 8], rdi");              // store the object pointer at the top of the stack
        emitter.instruction("add r10, 1");                                      // grow the visited-stack depth
        emitter.instruction("mov QWORD PTR [r9], r10");                         // publish the new depth
        emitter.label("__rt_vd_seen_push_done_x86");
        emitter.instruction("ret");                                             // return to caller
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: vd_seen_push ---");
    emitter.label_global("__rt_vd_seen_push");
    abi::emit_symbol_address(emitter, "x9", "_vd_seen_n");                      // resolve the visited-stack depth counter
    emitter.instruction("ldr x10, [x9]");                                       // load the current visited-stack depth
    emitter.instruction(&format!("cmp x10, #{}", VD_SEEN_CAPACITY));            // is there room for another entry?
    emitter.instruction("b.ge __rt_vd_seen_push_done");                         // a full stack is already refusing to descend
    abi::emit_symbol_address(emitter, "x11", "_vd_seen");                       // resolve the visited-pointer stack base
    emitter.instruction("str x0, [x11, x10, lsl #3]");                          // store the object pointer at the top of the stack
    emitter.instruction("add x10, x10, #1");                                    // grow the visited-stack depth
    emitter.instruction("str x10, [x9]");                                       // publish the new depth
    emitter.label("__rt_vd_seen_push_done");
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_vd_seen_pop`: drop the innermost visited-object entry. Takes no arguments.
///
/// Clamps at zero so an unbalanced pop can never make the depth negative and turn
/// the next `__rt_vd_seen_find` scan into an out-of-bounds read.
pub fn emit_vd_seen_pop(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: vd_seen_pop ---");
        emitter.label_global("__rt_vd_seen_pop");
        abi::emit_symbol_address(emitter, "r9", "_vd_seen_n");                  // resolve the visited-stack depth counter
        emitter.instruction("mov r10, QWORD PTR [r9]");                         // load the current visited-stack depth
        emitter.instruction("cmp r10, 0");                                      // is the stack already empty?
        emitter.instruction("jle __rt_vd_seen_pop_done_x86");                   // clamp at zero rather than underflow
        emitter.instruction("sub r10, 1");                                      // leave the innermost object
        emitter.instruction("mov QWORD PTR [r9], r10");                         // publish the new depth
        emitter.label("__rt_vd_seen_pop_done_x86");
        emitter.instruction("ret");                                             // return to caller
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: vd_seen_pop ---");
    emitter.label_global("__rt_vd_seen_pop");
    abi::emit_symbol_address(emitter, "x9", "_vd_seen_n");                      // resolve the visited-stack depth counter
    emitter.instruction("ldr x10, [x9]");                                       // load the current visited-stack depth
    emitter.instruction("cmp x10, #0");                                         // is the stack already empty?
    emitter.instruction("b.le __rt_vd_seen_pop_done");                          // clamp at zero rather than underflow
    emitter.instruction("sub x10, x10, #1");                                    // leave the innermost object
    emitter.instruction("str x10, [x9]");                                       // publish the new depth
    emitter.label("__rt_vd_seen_pop_done");
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_vd_obj_desc`: resolve an object's var_dump property descriptor.
///
/// Bounds-checks the header class id against `_class_gc_desc_count` (the shared
/// class-id table extent) so a stale or synthetic id lands on the empty
/// `_class_vd_desc_missing` descriptor instead of reading past the table.
/// Input: AArch64 x0 / x86_64 rdi = object pointer.
/// Output: AArch64 x0 / x86_64 rax = descriptor pointer.
pub fn emit_vd_obj_desc(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: vd_obj_desc ---");
        emitter.label_global("__rt_vd_obj_desc");
        emitter.instruction("mov r9, QWORD PTR [rdi]");                         // load the runtime class id from the object header
        abi::emit_symbol_address(emitter, "r10", "_class_gc_desc_count");       // resolve the class-id table extent
        emitter.instruction("mov r10, QWORD PTR [r10]");                        // load the number of registered class ids
        emitter.instruction("cmp r9, r10");                                     // is the class id within the descriptor table?
        emitter.instruction("jae __rt_vd_obj_desc_missing_x86");                // out-of-range ids fall back to the empty descriptor
        abi::emit_symbol_address(emitter, "r11", "_class_vd_desc_ptrs");        // resolve the per-class descriptor pointer table
        emitter.instruction("mov rax, QWORD PTR [r11 + r9 * 8]");               // load this class's var_dump descriptor
        emitter.instruction("ret");                                             // return to caller
        emitter.label("__rt_vd_obj_desc_missing_x86");
        abi::emit_symbol_address(emitter, "rax", "_class_vd_desc_missing");     // fall back to the zero-property descriptor
        emitter.instruction("ret");                                             // return to caller
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: vd_obj_desc ---");
    emitter.label_global("__rt_vd_obj_desc");
    emitter.instruction("ldr x9, [x0]");                                        // load the runtime class id from the object header
    abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");           // resolve the class-id table extent
    emitter.instruction("ldr x10, [x10]");                                      // load the number of registered class ids
    emitter.instruction("cmp x9, x10");                                         // is the class id within the descriptor table?
    emitter.instruction("b.hs __rt_vd_obj_desc_missing");                       // out-of-range ids fall back to the empty descriptor
    abi::emit_symbol_address(emitter, "x11", "_class_vd_desc_ptrs");            // resolve the per-class descriptor pointer table
    emitter.instruction("ldr x0, [x11, x9, lsl #3]");                           // load this class's var_dump descriptor
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_vd_obj_desc_missing");
    abi::emit_symbol_address(emitter, "x0", "_class_vd_desc_missing");          // fall back to the zero-property descriptor
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_vd_obj_count`: count an object's INITIALIZED declared properties.
///
/// PHP's `object(C)#id (n)` header counts only properties that hold a value: a
/// typed property declared without a default is listed in the body as
/// `uninitialized(T)` but excluded from `n`. The descriptor's static property
/// count is therefore not usable directly, and this scan of every slot's
/// uninitialized marker is what produces PHP's `n`.
///
/// Input: AArch64 x0 / x86_64 rdi = object pointer.
/// Output: AArch64 x0 / x86_64 rax = initialized property count.
pub fn emit_vd_obj_count(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_vd_obj_count_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: vd_obj_count ---");
    emitter.label_global("__rt_vd_obj_count");

    emitter.instruction("mov x1, x0");                                          // keep the object pointer for slot addressing
    emitter.instruction("ldr x9, [x0]");                                        // load the runtime class id from the object header
    abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");           // resolve the class-id table extent
    emitter.instruction("ldr x10, [x10]");                                      // load the number of registered class ids
    emitter.instruction("cmp x9, x10");                                         // is the class id within the descriptor table?
    emitter.instruction("b.hs __rt_vd_obj_count_none");                         // an unknown class reports zero properties
    abi::emit_symbol_address(emitter, "x11", "_class_vd_desc_ptrs");            // resolve the per-class descriptor pointer table
    emitter.instruction("ldr x2, [x11, x9, lsl #3]");                           // load this class's var_dump descriptor
    emitter.instruction("ldr x3, [x2]");                                        // load the declared property count
    emitter.instruction("mov x4, #0");                                          // property index = 0
    emitter.instruction("mov x5, #0");                                          // initialized property tally = 0
    emit_uninit_sentinel_aarch64(emitter, "x6");                                // materialize the uninitialized-property marker

    emitter.label("__rt_vd_obj_count_loop");
    emitter.instruction("cmp x4, x3");                                          // inspected every declared property?
    emitter.instruction("b.ge __rt_vd_obj_count_done");                         // tally complete
    emitter.instruction(&format!("mov x7, #{}", VD_DESC_ROW_BYTES));            // each descriptor row occupies 48 bytes
    emitter.instruction("mul x7, x4, x7");                                      // byte offset of this property's descriptor row
    emitter.instruction("add x7, x2, x7");                                      // advance into the descriptor
    emitter.instruction("add x7, x7, #8");                                      // skip the leading property-count word
    emitter.instruction("ldr x8, [x7, #16]");                                   // load the property's byte offset within the object
    emitter.instruction("add x8, x1, x8");                                      // resolve the absolute property slot address
    emitter.instruction("ldr x8, [x8, #8]");                                    // load the slot's high word (the init marker)
    emitter.instruction("cmp x8, x6");                                          // is this property still uninitialized?
    emitter.instruction("b.eq __rt_vd_obj_count_skip");                         // PHP excludes uninitialized properties from `n`
    emitter.instruction("add x5, x5, #1");                                      // count this initialized property
    emitter.label("__rt_vd_obj_count_skip");
    emitter.instruction("add x4, x4, #1");                                      // advance to the next declared property
    emitter.instruction("b __rt_vd_obj_count_loop");                            // continue tallying

    emitter.label("__rt_vd_obj_count_done");
    emitter.instruction("ldr x9, [x1]");                                        // reload the runtime class id for dynamic-tail metadata
    abi::emit_symbol_address(emitter, "x10", "_class_object_dynamic_prop_flags");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                         // load this class's dynamic-tail flag
    emitter.instruction("cbz x10, __rt_vd_obj_count_return");                  // fixed-layout classes contribute only declared slots
    abi::emit_symbol_address(emitter, "x10", "_class_object_payload_sizes");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                         // load the full object payload size
    emitter.instruction("cmp x10, #8");                                        // can the payload contain the trailing hash pointer?
    emitter.instruction("b.lo __rt_vd_obj_count_return");                      // malformed metadata contributes no dynamic properties
    emitter.instruction("sub x10, x10, #8");                                   // compute the dynamic hash-slot offset
    emitter.instruction("ldr x10, [x1, x10]");                                 // load the optional dynamic-property hash
    emitter.instruction("cbz x10, __rt_vd_obj_count_return");                  // an unallocated tail contributes zero properties
    emitter.instruction("ldr x10, [x10]");                                     // load the insertion-ordered hash entry count
    emitter.instruction("add x5, x5, x10");                                    // include dynamic public properties in the object header count
    emitter.label("__rt_vd_obj_count_return");
    emitter.instruction("mov x0, x5");                                          // return the initialized property count
    emitter.instruction("ret");                                                 // return to caller

    emitter.label("__rt_vd_obj_count_none");
    emitter.instruction("mov x0, #0");                                          // an unknown class dumps an empty body
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 initialized-property tally helper.
fn emit_vd_obj_count_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: vd_obj_count ---");
    emitter.label_global("__rt_vd_obj_count");

    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // load the runtime class id from the object header
    abi::emit_symbol_address(emitter, "r10", "_class_gc_desc_count");           // resolve the class-id table extent
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the number of registered class ids
    emitter.instruction("cmp r9, r10");                                         // is the class id within the descriptor table?
    emitter.instruction("jae __rt_vd_obj_count_none_x86");                      // an unknown class reports zero properties
    abi::emit_symbol_address(emitter, "r11", "_class_vd_desc_ptrs");            // resolve the per-class descriptor pointer table
    emitter.instruction("mov rsi, QWORD PTR [r11 + r9 * 8]");                   // load this class's var_dump descriptor
    emitter.instruction("mov rdx, QWORD PTR [rsi]");                            // load the declared property count
    emitter.instruction("xor rcx, rcx");                                        // property index = 0
    emitter.instruction("xor rax, rax");                                        // initialized property tally = 0
    emitter.instruction("movabs r8, 0x7ffffffffffffffd");                       // materialize the uninitialized-property marker

    emitter.label("__rt_vd_obj_count_loop_x86");
    emitter.instruction("cmp rcx, rdx");                                        // inspected every declared property?
    emitter.instruction("jge __rt_vd_obj_count_done_x86");                      // tally complete
    emitter.instruction("mov r10, rcx");                                        // copy the index for row-offset scaling
    emitter.instruction(&format!("imul r10, r10, {}", VD_DESC_ROW_BYTES));      // each descriptor row occupies 48 bytes
    emitter.instruction("add r10, rsi");                                        // advance into the descriptor
    emitter.instruction("add r10, 8");                                          // skip the leading property-count word
    emitter.instruction("mov r10, QWORD PTR [r10 + 16]");                       // load the property's byte offset within the object
    emitter.instruction("add r10, rdi");                                        // resolve the absolute property slot address
    emitter.instruction("mov r10, QWORD PTR [r10 + 8]");                        // load the slot's high word (the init marker)
    emitter.instruction("cmp r10, r8");                                         // is this property still uninitialized?
    emitter.instruction("je __rt_vd_obj_count_skip_x86");                       // PHP excludes uninitialized properties from `n`
    emitter.instruction("add rax, 1");                                          // count this initialized property
    emitter.label("__rt_vd_obj_count_skip_x86");
    emitter.instruction("add rcx, 1");                                          // advance to the next declared property
    emitter.instruction("jmp __rt_vd_obj_count_loop_x86");                      // continue tallying

    emitter.label("__rt_vd_obj_count_done_x86");
    emitter.instruction("mov r9, QWORD PTR [rdi]");                            // reload the runtime class id for dynamic-tail metadata
    abi::emit_symbol_address(emitter, "r10", "_class_object_dynamic_prop_flags");
    emitter.instruction("cmp QWORD PTR [r10 + r9*8], 0");                      // does this class reserve the dynamic hash tail?
    emitter.instruction("je __rt_vd_obj_count_return_x86");                    // fixed-layout classes contribute only declared slots
    abi::emit_symbol_address(emitter, "r10", "_class_object_payload_sizes");
    emitter.instruction("mov r10, QWORD PTR [r10 + r9*8]");                    // load the full object payload size
    emitter.instruction("cmp r10, 8");                                         // can the payload contain the trailing hash pointer?
    emitter.instruction("jb __rt_vd_obj_count_return_x86");                    // malformed metadata contributes no dynamic properties
    emitter.instruction("sub r10, 8");                                         // compute the dynamic hash-slot offset
    emitter.instruction("mov r10, QWORD PTR [rdi + r10]");                     // load the optional dynamic-property hash
    emitter.instruction("test r10, r10");                                      // is the tail allocated?
    emitter.instruction("jz __rt_vd_obj_count_return_x86");                    // an unallocated tail contributes zero properties
    emitter.instruction("add rax, QWORD PTR [r10]");                           // include dynamic public properties in the header count
    emitter.label("__rt_vd_obj_count_return_x86");
    emitter.instruction("ret");                                                 // return the initialized property count in rax

    emitter.label("__rt_vd_obj_count_none_x86");
    emitter.instruction("xor rax, rax");                                        // an unknown class dumps an empty body
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_open_object`: emit `<indent>object(NAME) (COUNT) {\n`.
///
/// The class name comes from the shared `_class_name_entries` `(ptr, len)` table
/// that `get_class()` reads, so a class renders under exactly one spelling. There
/// is deliberately NO `#id` between the name and the count — see the module
/// preamble.
/// Input: AArch64 x0 / x86_64 rdi = object pointer.
pub fn emit_var_dump_open_object(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_open_object_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_open_object ---");
    emitter.label_global("__rt_var_dump_open_object");

    // Frame (48 bytes): [0] object ptr, [8] debug-info Mixed, [32] saved x29, [40] saved x30.
    emitter.instruction("sub sp, sp, #48");                                     // allocate the object-header frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the object-header frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer across the writes
    emitter.instruction("str x1, [sp, #8]");                                    // save the optional runtime debug projection

    emitter.instruction("bl __rt_vd_pad");                                      // indent the object header line
    abi::emit_symbol_address(emitter, "x1", "_vd_object_prefix");               // load the `object(` prefix
    emitter.instruction("mov x2, #7");                                          // len("object(") = 7
    emitter.instruction("bl __rt_vd_write");                                    // write `object(`

    // -- class name from the shared class-id → (name ptr, name len) table --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("ldr x9, [x9]");                                        // load the runtime class id from the object header
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");              // resolve the class-name table extent
    emitter.instruction("ldr x10, [x10]");                                      // load the number of named class ids
    emitter.instruction("cmp x9, x10");                                         // is the class id within the name table?
    emitter.instruction("b.hs __rt_vd_open_obj_anon");                          // an unknown class id writes no name
    abi::emit_symbol_address(emitter, "x11", "_class_name_entries");            // resolve the class-name entry table
    emitter.instruction("add x11, x11, x9, lsl #4");                            // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("ldr x1, [x11]");                                       // load the class-name pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // load the class-name length
    emitter.instruction("b __rt_vd_open_obj_name");                             // write the resolved name
    emitter.label("__rt_vd_open_obj_anon");
    abi::emit_symbol_address(emitter, "x1", "_class_name_missing");             // fall back to the empty class-name slot
    emitter.instruction("mov x2, #0");                                          // a zero-length write emits nothing
    emitter.label("__rt_vd_open_obj_name");
    emitter.instruction("bl __rt_vd_write");                                    // write the class name

    abi::emit_symbol_address(emitter, "x1", "_vd_object_mid");                  // load the `)#` handle separator
    emitter.instruction("mov x2, #2");                                          // len(")#") = 2
    emitter.instruction("bl __rt_vd_write");                                    // write `)#`

    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("bl __rt_object_handle_of");                            // x0 = this object's PHP handle (same value spl_object_id returns)
    emitter.instruction("bl __rt_itoa");                                        // x1=digits ptr, x2=digits len
    emitter.instruction("bl __rt_vd_write");                                    // write the object handle digits

    abi::emit_symbol_address(emitter, "x1", "_vd_object_count_open");           // load the ` (` property-count opener
    emitter.instruction("mov x2, #2");                                          // len(" (") = 2
    emitter.instruction("bl __rt_vd_write");                                    // write ` (`

    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the optional runtime debug projection
    emitter.instruction("cbz x9, __rt_vd_open_obj_declared_count");             // absent projection falls back to declared properties
    emitter.instruction("ldr x10, [x9]");                                       // read the boxed projection's runtime value tag
    emitter.instruction("cmp x10, #4");                                         // indexed-array debug projection?
    emitter.instruction("b.eq __rt_vd_open_obj_debug_count");                   // both array layouts store count at raw offset zero
    emitter.instruction("cmp x10, #5");                                         // associative-array debug projection?
    emitter.instruction("b.eq __rt_vd_open_obj_debug_count");                   // use the projected hash entry count
    emitter.instruction("mov x0, #0");                                          // null or unsupported projections expose no properties
    emitter.instruction("b __rt_vd_open_obj_count_ready");                     // continue with the normalized count
    emitter.label("__rt_vd_open_obj_debug_count");
    emitter.instruction("ldr x9, [x9, #8]");                                    // load the raw projected container pointer
    emitter.instruction("ldr x0, [x9]");                                        // load the projected entry count
    emitter.instruction("b __rt_vd_open_obj_count_ready");                     // skip declared-property enumeration
    emitter.label("__rt_vd_open_obj_declared_count");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("bl __rt_vd_obj_count");                                // x0 = number of initialized declared properties
    emitter.label("__rt_vd_open_obj_count_ready");
    emitter.instruction("bl __rt_itoa");                                        // x1=digits ptr, x2=digits len
    emitter.instruction("bl __rt_vd_write");                                    // write the property count digits

    abi::emit_symbol_address(emitter, "x1", "_vd_brace_open");                  // load the `) {\n` opener
    emitter.instruction("mov x2, #4");                                          // len(") {\n") = 4
    emitter.instruction("bl __rt_vd_write");                                    // write `) {\n`

    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the object-header frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 object-header helper.
fn emit_var_dump_open_object_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_open_object ---");
    emitter.label_global("__rt_var_dump_open_object");

    // rbp-relative frame: [-8] object ptr, [-16] debug-info Mixed.
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the object-header frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate the object-header frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the object pointer across the writes
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the optional runtime debug projection

    emitter.instruction("call __rt_vd_pad");                                    // indent the object header line
    abi::emit_symbol_address(emitter, "rsi", "_vd_object_prefix");              // load the `object(` prefix
    emitter.instruction("mov edx, 7");                                          // len("object(") = 7
    emitter.instruction("call __rt_vd_write");                                  // write `object(`

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the object pointer
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the runtime class id from the object header
    abi::emit_symbol_address(emitter, "r10", "_class_name_count");              // resolve the class-name table extent
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the number of named class ids
    emitter.instruction("cmp r9, r10");                                         // is the class id within the name table?
    emitter.instruction("jae __rt_vd_open_obj_anon_x86");                       // an unknown class id writes no name
    abi::emit_symbol_address(emitter, "r11", "_class_name_entries");            // resolve the class-name entry table
    emitter.instruction("imul r9, r9, 16");                                     // each entry is a 16-byte (ptr, len) pair
    emitter.instruction("add r11, r9");                                         // advance to this class's entry
    emitter.instruction("mov rsi, QWORD PTR [r11]");                            // load the class-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                        // load the class-name length
    emitter.instruction("jmp __rt_vd_open_obj_name_x86");                       // write the resolved name
    emitter.label("__rt_vd_open_obj_anon_x86");
    abi::emit_symbol_address(emitter, "rsi", "_class_name_missing");            // fall back to the empty class-name slot
    emitter.instruction("xor rdx, rdx");                                        // a zero-length write emits nothing
    emitter.label("__rt_vd_open_obj_name_x86");
    emitter.instruction("call __rt_vd_write");                                  // write the class name

    abi::emit_symbol_address(emitter, "rsi", "_vd_object_mid");                 // load the `)#` handle separator
    emitter.instruction("mov edx, 2");                                          // len(")#") = 2
    emitter.instruction("call __rt_vd_write");                                  // write `)#`

    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("call __rt_object_handle_of");                          // rax = this object's PHP handle (same value spl_object_id returns)
    emitter.instruction("call __rt_itoa");                                      // rax=digits ptr, rdx=digits len
    emitter.instruction("mov rsi, rax");                                        // digits ptr → write buffer
    emitter.instruction("call __rt_vd_write");                                  // write the object handle digits

    abi::emit_symbol_address(emitter, "rsi", "_vd_object_count_open");          // load the ` (` property-count opener
    emitter.instruction("mov edx, 2");                                          // len(" (") = 2
    emitter.instruction("call __rt_vd_write");                                  // write ` (`

    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the optional runtime debug projection
    emitter.instruction("test r9, r9");                                         // did dynamic dispatch return a projection?
    emitter.instruction("jz __rt_vd_open_obj_declared_count_x86");              // absent projection falls back to declared properties
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // read the boxed projection's runtime value tag
    emitter.instruction("cmp r10, 4");                                          // indexed-array debug projection?
    emitter.instruction("je __rt_vd_open_obj_debug_count_x86");                 // both array layouts store count at raw offset zero
    emitter.instruction("cmp r10, 5");                                          // associative-array debug projection?
    emitter.instruction("je __rt_vd_open_obj_debug_count_x86");                 // use the projected hash entry count
    emitter.instruction("xor eax, eax");                                        // null or unsupported projections expose no properties
    emitter.instruction("jmp __rt_vd_open_obj_count_ready_x86");                // continue with the normalized count
    emitter.label("__rt_vd_open_obj_debug_count_x86");
    emitter.instruction("mov r9, QWORD PTR [r9 + 8]");                          // load the raw projected container pointer
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the projected entry count
    emitter.instruction("jmp __rt_vd_open_obj_count_ready_x86");                // skip declared-property enumeration
    emitter.label("__rt_vd_open_obj_declared_count_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("call __rt_vd_obj_count");                              // rax = number of initialized declared properties
    emitter.label("__rt_vd_open_obj_count_ready_x86");
    emitter.instruction("call __rt_itoa");                                      // rax=digits ptr, rdx=digits len
    emitter.instruction("mov rsi, rax");                                        // digits ptr → write buffer
    emitter.instruction("call __rt_vd_write");                                  // write the property count digits

    abi::emit_symbol_address(emitter, "rsi", "_vd_brace_open");                 // load the `) {\n` opener
    emitter.instruction("mov edx, 4");                                          // len(") {\n") = 4
    emitter.instruction("call __rt_vd_write");                                  // write `) {\n`

    emitter.instruction("add rsp, 16");                                         // release the object-header frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_object_key`: emit `<indent>[KEY]=>\n` for a property.
///
/// `KEY` arrives pre-rendered from the class descriptor and already carries its
/// quotes and any visibility annotation, so this reuses the bare `[` / `]=>\n`
/// delimiters rather than the hash walker's quoting pair.
/// Input: AArch64 x1=key ptr, x2=key len / x86_64 rdi=key ptr, rsi=key len.
pub fn emit_var_dump_emit_object_key(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_emit_object_key_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_object_key ---");
    emitter.label_global("__rt_var_dump_emit_object_key");

    emitter.instruction("sub sp, sp, #32");                                     // allocate the key-line frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the key-line frame pointer
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save key ptr/len across the writes

    emitter.instruction("bl __rt_vd_pad");                                      // indent the key line
    abi::emit_symbol_address(emitter, "x1", "_vd_indent_open");                 // load the `[` opener
    emitter.instruction("mov x2, #1");                                          // len("[") = 1
    emitter.instruction("bl __rt_vd_write");                                    // write `[`
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the rendered key pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the rendered key length
    emitter.instruction("bl __rt_vd_write");                                    // write the pre-rendered key text
    abi::emit_symbol_address(emitter, "x1", "_vd_close_arrow");                 // load the `]=>\n` closer
    emitter.instruction("mov x2, #4");                                          // len("]=>\n") = 4
    emitter.instruction("bl __rt_vd_write");                                    // write `]=>\n`

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the key-line frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 object property-key line helper.
fn emit_var_dump_emit_object_key_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_object_key ---");
    emitter.label_global("__rt_var_dump_emit_object_key");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the key-line frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate the key-line frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the rendered key pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the rendered key length

    emitter.instruction("call __rt_vd_pad");                                    // indent the key line
    abi::emit_symbol_address(emitter, "rsi", "_vd_indent_open");                // load the `[` opener
    emitter.instruction("mov edx, 1");                                          // len("[") = 1
    emitter.instruction("call __rt_vd_write");                                  // write `[`
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the rendered key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the rendered key length
    emitter.instruction("call __rt_vd_write");                                  // write the pre-rendered key text
    abi::emit_symbol_address(emitter, "rsi", "_vd_close_arrow");                // load the `]=>\n` closer
    emitter.instruction("mov edx, 4");                                          // len("]=>\n") = 4
    emitter.instruction("call __rt_vd_write");                                  // write `]=>\n`

    emitter.instruction("add rsp, 16");                                         // release the key-line frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_uninit_line`: emit `<indent>uninitialized(TYPE)\n`.
///
/// PHP renders a typed property read before its first write this way instead of
/// a value line, and omits it from the object's property count.
/// Input: AArch64 x1=type ptr, x2=type len / x86_64 rdi=type ptr, rsi=type len.
pub fn emit_var_dump_emit_uninit_line(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_emit_uninit_line_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_uninit_line ---");
    emitter.label_global("__rt_var_dump_emit_uninit_line");

    emitter.instruction("sub sp, sp, #32");                                     // allocate the uninitialized-line frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the uninitialized-line frame pointer
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save type ptr/len across the writes

    emitter.instruction("bl __rt_vd_pad");                                      // indent the value line
    abi::emit_symbol_address(emitter, "x1", "_vd_uninit_prefix");               // load the `uninitialized(` prefix
    emitter.instruction("mov x2, #14");                                         // len("uninitialized(") = 14
    emitter.instruction("bl __rt_vd_write");                                    // write `uninitialized(`
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the declared type-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the declared type-name length
    emitter.instruction("bl __rt_vd_write");                                    // write the declared type name
    abi::emit_symbol_address(emitter, "x1", "_vd_close_paren");                 // load the `)\n` closer
    emitter.instruction("mov x2, #2");                                          // len(")\n") = 2
    emitter.instruction("bl __rt_vd_write");                                    // write `)\n`

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the uninitialized-line frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 uninitialized-property line helper.
fn emit_var_dump_emit_uninit_line_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_uninit_line ---");
    emitter.label_global("__rt_var_dump_emit_uninit_line");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the uninitialized-line frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate the uninitialized-line frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the declared type-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the declared type-name length

    emitter.instruction("call __rt_vd_pad");                                    // indent the value line
    abi::emit_symbol_address(emitter, "rsi", "_vd_uninit_prefix");              // load the `uninitialized(` prefix
    emitter.instruction("mov edx, 14");                                         // len("uninitialized(") = 14
    emitter.instruction("call __rt_vd_write");                                  // write `uninitialized(`
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the declared type-name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the declared type-name length
    emitter.instruction("call __rt_vd_write");                                  // write the declared type name
    abi::emit_symbol_address(emitter, "rsi", "_vd_close_paren");                // load the `)\n` closer
    emitter.instruction("mov edx, 2");                                          // len(")\n") = 2
    emitter.instruction("call __rt_vd_write");                                  // write `)\n`

    emitter.instruction("add rsp, 16");                                         // release the uninitialized-line frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_emit_recursion_line`: emit `<indent>*RECURSION*\n`.
///
/// PHP prints this in place of the value whenever a container is already on the
/// walk stack. Takes no arguments.
pub fn emit_var_dump_emit_recursion_line(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: var_dump_emit_recursion_line ---");
        emitter.label_global("__rt_var_dump_emit_recursion_line");
        emitter.instruction("push rbp");                                        // save caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish the recursion-line frame pointer
        emitter.instruction("call __rt_vd_pad");                                // indent the recursion marker
        abi::emit_symbol_address(emitter, "rsi", "_vd_recursion_line");         // load the `*RECURSION*\n` marker
        emitter.instruction("mov edx, 12");                                     // len("*RECURSION*\n") = 12
        emitter.instruction("call __rt_vd_write");                              // write `*RECURSION*\n`
        emitter.instruction("pop rbp");                                         // restore caller frame pointer
        emitter.instruction("ret");                                             // return to caller
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_emit_recursion_line ---");
    emitter.label_global("__rt_var_dump_emit_recursion_line");
    emitter.instruction("sub sp, sp, #16");                                     // allocate the recursion-line frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the recursion-line frame pointer
    emitter.instruction("bl __rt_vd_pad");                                      // indent the recursion marker
    abi::emit_symbol_address(emitter, "x1", "_vd_recursion_line");              // load the `*RECURSION*\n` marker
    emitter.instruction("mov x2, #12");                                         // len("*RECURSION*\n") = 12
    emitter.instruction("bl __rt_vd_write");                                    // write `*RECURSION*\n`
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the recursion-line frame
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_var_dump_object`: walk an object's declared properties and emit one
/// `<indent>[KEY]=>\n<indent>VALUE` block each.
///
/// Every value is handed to `__rt_var_dump_value`, which unboxes Mixed cells and
/// recurses into nested arrays, hashes and objects — so an object graph nests to
/// arbitrary depth, bounded only by the `_vd_seen` recursion guard. Properties
/// still carrying the uninitialized marker in their slot's high word render
/// `uninitialized(TYPE)` instead.
///
/// This walker does NOT emit the surrounding `object(C) (n) {` / `}` frame; the
/// tag-6 branch of `__rt_var_dump_value` owns that, exactly as the array frame is
/// owned by the tag-4/5 branches.
///
/// Input: AArch64 x0 / x86_64 rdi = object pointer.
pub fn emit_var_dump_object(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_var_dump_object_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: var_dump_object ---");
    emitter.label_global("__rt_var_dump_object");

    // Frame (80 bytes): [0] object ptr, [8] descriptor ptr, [16] property index,
    //   [24] property count, [32] descriptor row ptr, [40] property slot ptr,
    //   [48] debug-info Mixed,
    //   [64] saved x29, [72] saved x30.
    emitter.instruction("sub sp, sp, #80");                                     // allocate the object-walk frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the object-walk frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer
    emitter.instruction("str x1, [sp, #48]");                                   // save the optional dynamic debug projection

    // -- dynamic __debugInfo() projections supersede declared properties --
    emitter.instruction("cbz x1, __rt_vd_obj_declared");                        // no runtime projection means ordinary property enumeration
    emitter.instruction("ldr x9, [x1]");                                        // read the boxed projection's runtime value tag
    emitter.instruction("cmp x9, #4");                                          // indexed array projection?
    emitter.instruction("b.eq __rt_vd_obj_debug_indexed");                      // render its numeric-key entries directly
    emitter.instruction("cmp x9, #5");                                          // associative array projection?
    emitter.instruction("b.eq __rt_vd_obj_debug_hash");                         // render its keyed entries directly
    emitter.instruction("b __rt_vd_obj_done");                                  // null or unsupported projections expose an empty body
    emitter.label("__rt_vd_obj_debug_indexed");
    emitter.instruction("ldr x0, [x1, #8]");                                    // load the raw indexed-array payload
    emitter.instruction("bl __rt_var_dump_indexed");                            // render projected entries at the current object indent
    emitter.instruction("b __rt_vd_obj_done");                                  // dynamic projection walk complete
    emitter.label("__rt_vd_obj_debug_hash");
    emitter.instruction("ldr x0, [x1, #8]");                                    // load the raw associative-array payload
    emitter.instruction("bl __rt_var_dump_debug_hash");                         // render and demangle projected property keys
    emitter.instruction("b __rt_vd_obj_done");                                  // dynamic projection walk complete

    emitter.label("__rt_vd_obj_declared");

    emitter.instruction("bl __rt_vd_obj_desc");                                 // x0 = this class's var_dump descriptor
    emitter.instruction("str x0, [sp, #8]");                                    // save the descriptor pointer
    emitter.instruction("ldr x9, [x0]");                                        // load the declared property count
    emitter.instruction("str x9, [sp, #24]");                                   // save the property count for the loop guard
    emitter.instruction("str xzr, [sp, #16]");                                  // property index = 0

    emitter.label("__rt_vd_obj_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the property index
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the property count
    emitter.instruction("cmp x9, x10");                                         // rendered every declared property?
    emitter.instruction("b.ge __rt_vd_obj_declared_done");                      // append public dynamic properties after declared slots

    // -- resolve this property's 48-byte descriptor row --
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the descriptor pointer
    emitter.instruction(&format!("mov x12, #{}", VD_DESC_ROW_BYTES));           // each descriptor row occupies 48 bytes
    emitter.instruction("mul x12, x9, x12");                                    // byte offset of this property's row
    emitter.instruction("add x11, x11, x12");                                   // advance into the descriptor
    emitter.instruction("add x11, x11, #8");                                    // skip the leading property-count word
    emitter.instruction("str x11, [sp, #32]");                                  // save the row pointer across the calls

    // -- resolve this property's 16-byte slot inside the instance --
    emitter.instruction("ldr x13, [x11, #16]");                                 // load the property's byte offset within the object
    emitter.instruction("ldr x14, [sp, #0]");                                   // reload the object pointer
    emitter.instruction("add x13, x14, x13");                                   // resolve the absolute property slot address
    emitter.instruction("str x13, [sp, #40]");                                  // save the slot pointer across the calls

    // -- emit `<indent>[KEY]=>\n` --
    emitter.instruction("ldr x1, [x11]");                                       // load the pre-rendered key pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // load the pre-rendered key length
    emitter.instruction("bl __rt_var_dump_emit_object_key");                    // emit `<indent>[KEY]=>\n`

    // -- an uninitialized typed property renders `uninitialized(TYPE)` --
    emitter.instruction("ldr x13, [sp, #40]");                                  // reload the property slot pointer
    emitter.instruction("ldr x14, [x13, #8]");                                  // load the slot's high word (the init marker)
    emit_uninit_sentinel_aarch64(emitter, "x15");                               // materialize the uninitialized-property marker
    emitter.instruction("cmp x14, x15");                                        // is this property still uninitialized?
    emitter.instruction("b.ne __rt_vd_obj_value");                              // initialized properties render their value

    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the descriptor row pointer
    emitter.instruction("ldr x1, [x11, #32]");                                  // load the declared type-name pointer
    emitter.instruction("ldr x2, [x11, #40]");                                  // load the declared type-name length
    emitter.instruction("bl __rt_var_dump_emit_uninit_line");                   // emit `<indent>uninitialized(TYPE)\n`
    emitter.instruction("b __rt_vd_obj_next");                                  // continue with the next property

    emitter.label("__rt_vd_obj_value");
    // -- render the value line; __rt_var_dump_value unboxes Mixed cells (tag 7)
    //    and recurses into nested arrays/hashes/objects (tags 4/5/6) on its own --
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the descriptor row pointer
    emitter.instruction("ldr x13, [sp, #40]");                                  // reload the property slot pointer
    emitter.instruction("ldr x0, [x11, #24]");                                  // property value tag → value renderer
    emitter.instruction("ldr x1, [x13]");                                       // slot low word → value renderer
    emitter.instruction("ldr x2, [x13, #8]");                                   // slot high word → value renderer
    emitter.instruction("bl __rt_var_dump_value");                              // emit `<indent>TYPE(VAL)\n` (recursing when needed)

    emitter.label("__rt_vd_obj_next");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the property index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next declared property
    emitter.instruction("str x9, [sp, #16]");                                   // save the updated index
    emitter.instruction("b __rt_vd_obj_loop");                                  // continue the walk

    emitter.label("__rt_vd_obj_declared_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("bl __rt_object_dynamic_hash");                         // resolve the optional dynamic-property hash tail
    emitter.instruction("cbz x0, __rt_vd_obj_done");                            // classes without dynamic entries are complete
    emitter.instruction("bl __rt_var_dump_hash");                               // append public dynamic properties in insertion order
    emitter.label("__rt_vd_obj_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the object-walk frame
    emitter.instruction("ret");                                                 // return to the var_dump caller
}

/// Emits the Linux x86_64 object property walker.
fn emit_var_dump_object_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: var_dump_object ---");
    emitter.label_global("__rt_var_dump_object");

    // rbp-relative frame: [-8] object ptr, [-16] descriptor ptr, [-24] index,
    //   [-32] property count, [-40] descriptor row ptr, [-48] property slot ptr,
    //   [-56] debug-info Mixed.
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the object-walk frame pointer
    emitter.instruction("sub rsp, 64");                                         // allocate the object-walk frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the object pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rsi");                       // save the optional dynamic debug projection

    emitter.instruction("test rsi, rsi");                                       // did runtime dispatch return a debug projection?
    emitter.instruction("jz __rt_vd_obj_declared_x86");                         // absent projection means ordinary property enumeration
    emitter.instruction("mov r9, QWORD PTR [rsi]");                             // read the boxed projection's runtime value tag
    emitter.instruction("cmp r9, 4");                                           // indexed array projection?
    emitter.instruction("je __rt_vd_obj_debug_indexed_x86");                    // render its numeric-key entries directly
    emitter.instruction("cmp r9, 5");                                           // associative array projection?
    emitter.instruction("je __rt_vd_obj_debug_hash_x86");                       // render its keyed entries directly
    emitter.instruction("jmp __rt_vd_obj_done_x86");                            // null or unsupported projections expose an empty body
    emitter.label("__rt_vd_obj_debug_indexed_x86");
    emitter.instruction("mov rdi, QWORD PTR [rsi + 8]");                        // load the raw indexed-array payload
    emitter.instruction("call __rt_var_dump_indexed");                          // render projected entries at the current object indent
    emitter.instruction("jmp __rt_vd_obj_done_x86");                            // dynamic projection walk complete
    emitter.label("__rt_vd_obj_debug_hash_x86");
    emitter.instruction("mov rdi, QWORD PTR [rsi + 8]");                        // load the raw associative-array payload
    emitter.instruction("call __rt_var_dump_debug_hash");                       // render and demangle projected property keys
    emitter.instruction("jmp __rt_vd_obj_done_x86");                            // dynamic projection walk complete

    emitter.label("__rt_vd_obj_declared_x86");

    emitter.instruction("call __rt_vd_obj_desc");                               // rax = this class's var_dump descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the descriptor pointer
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // load the declared property count
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // save the property count for the loop guard
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // property index = 0

    emitter.label("__rt_vd_obj_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the property index
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the property count
    emitter.instruction("cmp r9, r10");                                         // rendered every declared property?
    emitter.instruction("jge __rt_vd_obj_declared_done_x86");                   // append public dynamic properties after declared slots

    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the descriptor pointer
    emitter.instruction(&format!("imul r9, r9, {}", VD_DESC_ROW_BYTES));        // each descriptor row occupies 48 bytes
    emitter.instruction("add r11, r9");                                         // advance into the descriptor
    emitter.instruction("add r11, 8");                                          // skip the leading property-count word
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the row pointer across the calls

    emitter.instruction("mov r10, QWORD PTR [r11 + 16]");                       // load the property's byte offset within the object
    emitter.instruction("add r10, QWORD PTR [rbp - 8]");                        // resolve the absolute property slot address
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save the slot pointer across the calls

    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // load the pre-rendered key pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                        // load the pre-rendered key length
    emitter.instruction("call __rt_var_dump_emit_object_key");                  // emit `<indent>[KEY]=>\n`

    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the property slot pointer
    emitter.instruction("mov r10, QWORD PTR [r10 + 8]");                        // load the slot's high word (the init marker)
    emitter.instruction("movabs r11, 0x7ffffffffffffffd");                      // materialize the uninitialized-property marker
    emitter.instruction("cmp r10, r11");                                        // is this property still uninitialized?
    emitter.instruction("jne __rt_vd_obj_value_x86");                           // initialized properties render their value

    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the descriptor row pointer
    emitter.instruction("mov rdi, QWORD PTR [r11 + 32]");                       // load the declared type-name pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + 40]");                       // load the declared type-name length
    emitter.instruction("call __rt_var_dump_emit_uninit_line");                 // emit `<indent>uninitialized(TYPE)\n`
    emitter.instruction("jmp __rt_vd_obj_next_x86");                            // continue with the next property

    emitter.label("__rt_vd_obj_value_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the descriptor row pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the property slot pointer
    emitter.instruction("mov rdi, QWORD PTR [r11 + 24]");                       // property value tag → value renderer
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // slot low word → value renderer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // slot high word → value renderer
    emitter.instruction("call __rt_var_dump_value");                            // emit `<indent>TYPE(VAL)\n` (recursing when needed)

    emitter.label("__rt_vd_obj_next_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the property index
    emitter.instruction("add r9, 1");                                           // advance to the next declared property
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // save the updated index
    emitter.instruction("jmp __rt_vd_obj_loop_x86");                            // continue the walk

    emitter.label("__rt_vd_obj_declared_done_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("call __rt_object_dynamic_hash");                      // resolve the optional dynamic-property hash tail
    emitter.instruction("test rax, rax");                                       // does this object own dynamic entries?
    emitter.instruction("jz __rt_vd_obj_done_x86");                             // classes without dynamic entries are complete
    emitter.instruction("mov rdi, rax");                                        // hash pointer → body-only hash walker
    emitter.instruction("call __rt_var_dump_hash");                            // append public dynamic properties in insertion order
    emitter.label("__rt_vd_obj_done_x86");
    emitter.instruction("add rsp, 64");                                         // release the object-walk frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the var_dump caller
}
