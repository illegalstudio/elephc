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
//! - DYNAMIC PROPERTIES are not descriptor rows. After the declared rows the walker
//!   appends the instance's dynamic-property hash (`__rt_obj_dump_dyn_props`, the
//!   same tail `get_object_vars()` copies) through `__rt_var_dump_hash`, so they
//!   render as `["name"]=>` in insertion order, and `__rt_vd_obj_count` adds the
//!   hash's entry count to `(n)`.
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

use elephc_memoir::values::incomplete_object::{CLASS_ID, PROPERTY_HASH};
use super::super::objects::emit_load_dump_dyn_hash;
use crate::codegen_support::abi;
use crate::codegen_support::sentinels::emit_resolve_tagged_scalar_property_tag;
use crate::codegen_support::{emit::Emitter, platform::Arch};
use elephc_memoir::hash_layout::{self, HeaderField};
use elephc_memoir::values::property_descriptor::{self, Field};

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

pub use elephc_memoir::values::diagnostic_stack::{emit_vd_seen_find, emit_vd_seen_pop, emit_vd_seen_push};

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
        elephc_memoir::values::object_storage::emit_load_class_id(emitter, "r9", "rdi");
        abi::emit_local_symbol_address(emitter, "r10", "_class_gc_desc_count");       // resolve the class-id table extent
        emitter.instruction("mov r10, QWORD PTR [r10]");                        // load the number of registered class ids
        emitter.instruction("cmp r9, r10");                                     // is the class id within the descriptor table?
        emitter.instruction("jae __rt_vd_obj_desc_missing_x86");                // out-of-range ids fall back to the empty descriptor
        abi::emit_local_symbol_address(emitter, "r11", "_class_vd_desc_ptrs");        // resolve the per-class descriptor pointer table
        emitter.instruction("mov rax, QWORD PTR [r11 + r9 * 8]");               // load this class's var_dump descriptor
        emitter.instruction("test rax, rax");                                   // a runtime class id may leave a null descriptor slot
        emitter.instruction("jz __rt_vd_obj_desc_missing_x86");                 // fall back to the empty descriptor
        emitter.instruction("ret");                                             // return to caller
        emitter.label("__rt_vd_obj_desc_missing_x86");
        abi::emit_local_symbol_address(emitter, "rax", "_class_vd_desc_missing");     // fall back to the zero-property descriptor
        emitter.instruction("ret");                                             // return to caller
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: vd_obj_desc ---");
    emitter.label_global("__rt_vd_obj_desc");
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "x9", "x0");
    abi::emit_local_symbol_address(emitter, "x10", "_class_gc_desc_count");           // resolve the class-id table extent
    emitter.instruction("ldr x10, [x10]");                                      // load the number of registered class ids
    emitter.instruction("cmp x9, x10");                                         // is the class id within the descriptor table?
    emitter.instruction("b.hs __rt_vd_obj_desc_missing");                       // out-of-range ids fall back to the empty descriptor
    abi::emit_local_symbol_address(emitter, "x11", "_class_vd_desc_ptrs");            // resolve the per-class descriptor pointer table
    emitter.instruction("ldr x0, [x11, x9, lsl #3]");                           // load this class's var_dump descriptor
    emitter.instruction("cbz x0, __rt_vd_obj_desc_missing");                    // a runtime class id may leave a null descriptor slot
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_vd_obj_desc_missing");
    abi::emit_local_symbol_address(emitter, "x0", "_class_vd_desc_missing");          // fall back to the zero-property descriptor
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_vd_obj_count`: count an object's INITIALIZED declared properties plus
/// its dynamic properties.
///
/// PHP's `object(C)#id (n)` header counts only properties that hold a value: a
/// typed property declared without a default is listed in the body as
/// `uninitialized(T)` but excluded from `n`. The descriptor's static property
/// count is therefore not usable directly, and this scan of every slot's
/// uninitialized marker is what produces PHP's `n`. The dynamic-property hash's
/// entry count is added on top, read inline so the helper stays a leaf.
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
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "x9", "x0");
    emitter.instruction(&format!("cmn x9, #{}", -CLASS_ID));                    // class id -2 marks an incomplete object
    emitter.instruction("b.eq __rt_vd_obj_count_incomplete");                   // count the synthetic name plus opaque properties
    abi::emit_local_symbol_address(emitter, "x10", "_class_gc_desc_count");           // resolve the class-id table extent
    emitter.instruction("ldr x10, [x10]");                                      // load the number of registered class ids
    emitter.instruction("cmp x9, x10");                                         // is the class id within the descriptor table?
    emitter.instruction("b.hs __rt_vd_obj_count_none");                         // an unknown class reports zero properties
    abi::emit_local_symbol_address(emitter, "x11", "_class_vd_desc_ptrs");            // resolve the per-class descriptor pointer table
    emitter.instruction("ldr x2, [x11, x9, lsl #3]");                           // load this class's var_dump descriptor
    emitter.instruction("cbz x2, __rt_vd_obj_count_none");                      // a runtime class id may leave a null descriptor slot
    property_descriptor::emit_load_count(emitter, "x3", "x2");                // load the declared property count
    emitter.instruction("mov x4, #0");                                          // property index = 0
    emitter.instruction("mov x5, #0");                                          // initialized property tally = 0
    emit_uninit_sentinel_aarch64(emitter, "x6");                                // materialize the uninitialized-property marker

    emitter.label("__rt_vd_obj_count_loop");
    emitter.instruction("cmp x4, x3");                                          // inspected every declared property?
    emitter.instruction("b.ge __rt_vd_obj_count_done");                         // tally complete
    property_descriptor::emit_row_address(emitter, "x7", "x2", "x4", "x10"); // select this property's descriptor row
    property_descriptor::emit_load_field(emitter, "x8", "x7", Field::ObjectOffset); // load the property's byte offset within the object
    elephc_memoir::values::object_storage::emit_slot_address(emitter, "x8", "x1", "x8"); // resolve the absolute property slot address
    property_descriptor::emit_load_field(emitter, "x10", "x7", Field::ValueTag); // load the descriptor's logical tag and native storage flags
    crate::codegen_support::runtime::reference_cells::resolve_property_payload(emitter, "x8", "x10", "x9", "__rt_vd_obj_count_slot");
    emitter.instruction("mov x8, x9");                                          // count using the marker selected by physical storage
    emitter.instruction("cmp x8, x6");                                          // is this property still uninitialized?
    emitter.instruction("b.eq __rt_vd_obj_count_skip");                         // PHP excludes uninitialized properties from `n`
    emitter.instruction("add x5, x5, #1");                                      // count this initialized property
    emitter.label("__rt_vd_obj_count_skip");
    emitter.instruction("add x4, x4, #1");                                      // advance to the next declared property
    emitter.instruction("b __rt_vd_obj_count_loop");                            // continue tallying

    emitter.label("__rt_vd_obj_count_done");
    // -- PHP's `(n)` also counts every dynamic property the body appends --
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "x9", "x1");
    emit_load_dump_dyn_hash(emitter, "x1", "x9", "x12", "x13", "__rt_vd_obj_count_ret");
    hash_layout::emit_load_header_field(emitter, "x12", "x12", HeaderField::Count);
    emitter.instruction("add x5, x5, x12");                                     // count every dynamic property too
    emitter.label("__rt_vd_obj_count_ret");
    emitter.instruction("mov x0, x5");                                          // return the initialized property count
    emitter.instruction("ret");                                                 // return to caller

    emitter.label("__rt_vd_obj_count_incomplete");
    emitter.instruction(&format!("ldr x9, [x1, #{PROPERTY_HASH}]"));            // opaque property hash
    emitter.instruction("cbz x9, __rt_vd_obj_count_incomplete_one");            // no properties still has the synthetic name
    hash_layout::emit_load_header_field(emitter, "x0", "x9", HeaderField::Count);
    emitter.instruction("add x0, x0, #1");                                      // plus __PHP_Incomplete_Class_Name
    emitter.instruction("ret");                                                 // return the complete count
    emitter.label("__rt_vd_obj_count_incomplete_one");
    emitter.instruction("mov x0, #1");                                          // just the synthetic name row
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

    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "r9", "rdi");
    emitter.instruction(&format!("cmp r9, {CLASS_ID}"));                        // class id -2 marks an incomplete object
    emitter.instruction("je __rt_vd_obj_count_incomplete_x86");                 // count the synthetic name plus opaque properties
    abi::emit_local_symbol_address(emitter, "r10", "_class_gc_desc_count");           // resolve the class-id table extent
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the number of registered class ids
    emitter.instruction("cmp r9, r10");                                         // is the class id within the descriptor table?
    emitter.instruction("jae __rt_vd_obj_count_none_x86");                      // an unknown class reports zero properties
    abi::emit_local_symbol_address(emitter, "r11", "_class_vd_desc_ptrs");            // resolve the per-class descriptor pointer table
    emitter.instruction("mov rsi, QWORD PTR [r11 + r9 * 8]");                   // load this class's var_dump descriptor
    emitter.instruction("test rsi, rsi");                                       // a runtime class id may leave a null descriptor slot
    emitter.instruction("jz __rt_vd_obj_count_none_x86");                       // report an empty body instead of dereferencing null
    property_descriptor::emit_load_count(emitter, "rdx", "rsi");              // load the declared property count
    emitter.instruction("xor rcx, rcx");                                        // property index = 0
    emitter.instruction("xor rax, rax");                                        // initialized property tally = 0
    emitter.instruction("movabs r8, 0x7ffffffffffffffd");                       // materialize the uninitialized-property marker

    emitter.label("__rt_vd_obj_count_loop_x86");
    emitter.instruction("cmp rcx, rdx");                                        // inspected every declared property?
    emitter.instruction("jge __rt_vd_obj_count_done_x86");                      // tally complete
    property_descriptor::emit_row_address(emitter, "r10", "rsi", "rcx", "r11"); // select this property's descriptor row
    property_descriptor::emit_load_field(emitter, "r11", "r10", Field::ValueTag); // load the descriptor's logical tag and native storage flags
    property_descriptor::emit_load_field(emitter, "r10", "r10", Field::ObjectOffset); // load the property's byte offset within the object
    elephc_memoir::values::object_storage::emit_slot_address(emitter, "r10", "rdi", "r10"); // resolve the absolute property slot address
    crate::codegen_support::runtime::reference_cells::resolve_property_payload(emitter, "r10", "r11", "r9", "__rt_vd_obj_count_slot");
    emitter.instruction("mov r10, r9");                                         // count using the marker selected by physical storage
    emitter.instruction("cmp r10, r8");                                         // is this property still uninitialized?
    emitter.instruction("je __rt_vd_obj_count_skip_x86");                       // PHP excludes uninitialized properties from `n`
    emitter.instruction("add rax, 1");                                          // count this initialized property
    emitter.label("__rt_vd_obj_count_skip_x86");
    emitter.instruction("add rcx, 1");                                          // advance to the next declared property
    emitter.instruction("jmp __rt_vd_obj_count_loop_x86");                      // continue tallying

    emitter.label("__rt_vd_obj_count_done_x86");
    // -- PHP's `(n)` also counts every dynamic property the body appends --
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "r9", "rdi");
    emit_load_dump_dyn_hash(emitter, "rdi", "r9", "r10", "r11", "__rt_vd_obj_count_ret_x86");
    hash_layout::emit_load_header_field(emitter, "r11", "r10", HeaderField::Count);
    emitter.instruction("add rax, r11");                                        // count every dynamic property too
    emitter.label("__rt_vd_obj_count_ret_x86");
    emitter.instruction("ret");                                                 // return the initialized property count in rax

    emitter.label("__rt_vd_obj_count_incomplete_x86");
    emitter.instruction(&format!("mov rax, QWORD PTR [rdi + {PROPERTY_HASH}]")); // opaque property hash
    emitter.instruction("test rax, rax");                                       // no properties?
    emitter.instruction("jz __rt_vd_obj_count_incomplete_one_x86");             // the synthetic name row still exists
    hash_layout::emit_load_header_field(emitter, "rax", "rax", HeaderField::Count);
    emitter.instruction("add rax, 1");                                          // plus __PHP_Incomplete_Class_Name
    emitter.instruction("ret");                                                 // return the complete count
    emitter.label("__rt_vd_obj_count_incomplete_one_x86");
    emitter.instruction("mov eax, 1");                                          // just the synthetic name row
    emitter.instruction("ret");                                                 // return to caller

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

    // Thin wrapper: count the initialized declared properties, then share the header
    // writer with the `__debugInfo()` path, which supplies its own entry count.
    emitter.instruction("sub sp, sp, #32");                                     // allocate the wrapper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the wrapper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // keep the object pointer across the count call
    emitter.instruction("bl __rt_vd_obj_count");                                // x0 = number of initialized properties
    emitter.instruction("mov x1, x0");                                          // property count → header writer
    emitter.instruction("ldr x0, [sp, #0]");                                    // object pointer → header writer
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the wrapper frame
    emitter.instruction("b __rt_var_dump_open_object_n");                       // tail-call the shared header writer

    emitter.blank();
    emitter.comment("--- runtime: var_dump_open_object_n ---");
    emitter.label_global("__rt_var_dump_open_object_n");

    // Frame (48 bytes): [0] object ptr, [8] entry count, [32] saved x29, [40] saved x30.
    emitter.instruction("sub sp, sp, #48");                                     // allocate the object-header frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the object-header frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer across the writes
    emitter.instruction("str x1, [sp, #8]");                                    // save the entry count printed in the header

    emitter.instruction("bl __rt_vd_pad");                                      // indent the object header line
    abi::emit_symbol_address(emitter, "x1", "_vd_object_prefix");               // load the `object(` prefix
    emitter.instruction("mov x2, #7");                                          // len("object(") = 7
    emitter.instruction("bl __rt_vd_write");                                    // write `object(`

    // -- class name from the shared class-id → (name ptr, name len) table --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the object pointer
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "x9", "x9");
    emitter.instruction(&format!("cmn x9, #{}", -CLASS_ID));                    // class id -2 marks an incomplete object
    emitter.instruction("b.eq __rt_vd_open_obj_incomplete");                    // render the synthetic internal class name
    elephc_memoir::values::class_metadata::emit_load_local_class_name_count(emitter, "x10"); // load the number of named class ids
    emitter.instruction("cmp x9, x10");                                         // is the class id within the name table?
    emitter.instruction("b.hs __rt_vd_open_obj_anon");                          // an unknown class id writes no name
    elephc_memoir::values::class_metadata::emit_local_class_name_table_address(emitter, "x11");
    elephc_memoir::values::class_metadata::emit_class_name_row_address(emitter, "x11", "x11", "x9", "x12");
    elephc_memoir::values::class_metadata::emit_load_class_name_pair(emitter, "x1", "x2", "x11");
    emitter.instruction("b __rt_vd_open_obj_name");                             // write the resolved name
    emitter.label("__rt_vd_open_obj_anon");
    elephc_memoir::values::class_metadata::emit_local_missing_class_name_address(emitter, "x1"); // fall back to the empty class-name slot
    emitter.instruction("mov x2, #0");                                          // a zero-length write emits nothing
    emitter.instruction("b __rt_vd_open_obj_name");                             // write the empty name slot
    emitter.label("__rt_vd_open_obj_incomplete");
    abi::emit_local_symbol_address(emitter, "x1", "_incomplete_class_name");          // __PHP_Incomplete_Class
    emitter.instruction("mov x2, #22");                                         // byte length of __PHP_Incomplete_Class
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

    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the entry count
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

    // Thin wrapper: count the initialized declared properties, then share the header
    // writer with the `__debugInfo()` path, which supplies its own entry count.
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate the wrapper frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the object pointer across the count call
    emitter.instruction("call __rt_vd_obj_count");                              // rax = number of initialized properties
    emitter.instruction("mov rsi, rax");                                        // property count → header writer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // object pointer → header writer
    emitter.instruction("add rsp, 16");                                         // release the wrapper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("jmp __rt_var_dump_open_object_n");                     // tail-call the shared header writer

    emitter.blank();
    emitter.comment("--- runtime: var_dump_open_object_n ---");
    emitter.label_global("__rt_var_dump_open_object_n");

    // rbp-relative frame: [-8] object ptr, [-16] entry count.
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the object-header frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate the object-header frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the object pointer across the writes
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the entry count printed in the header

    emitter.instruction("call __rt_vd_pad");                                    // indent the object header line
    abi::emit_symbol_address(emitter, "rsi", "_vd_object_prefix");              // load the `object(` prefix
    emitter.instruction("mov edx, 7");                                          // len("object(") = 7
    emitter.instruction("call __rt_vd_write");                                  // write `object(`

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the object pointer
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "r9", "r9");
    emitter.instruction(&format!("cmp r9, {CLASS_ID}"));                        // class id -2 marks an incomplete object
    emitter.instruction("je __rt_vd_open_obj_incomplete_x86");                  // render the synthetic internal class name
    elephc_memoir::values::class_metadata::emit_load_local_class_name_count(emitter, "r10"); // load the number of named class ids
    emitter.instruction("cmp r9, r10");                                         // is the class id within the name table?
    emitter.instruction("jae __rt_vd_open_obj_anon_x86");                       // an unknown class id writes no name
    elephc_memoir::values::class_metadata::emit_local_class_name_table_address(emitter, "r11");
    elephc_memoir::values::class_metadata::emit_class_name_row_address(emitter, "r11", "r11", "r9", "r10");
    elephc_memoir::values::class_metadata::emit_load_class_name_pair(emitter, "rsi", "rdx", "r11");
    emitter.instruction("jmp __rt_vd_open_obj_name_x86");                       // write the resolved name
    emitter.label("__rt_vd_open_obj_anon_x86");
    elephc_memoir::values::class_metadata::emit_local_missing_class_name_address(emitter, "rsi"); // fall back to the empty class-name slot
    emitter.instruction("xor rdx, rdx");                                        // a zero-length write emits nothing
    emitter.instruction("jmp __rt_vd_open_obj_name_x86");                       // write the empty name slot
    emitter.label("__rt_vd_open_obj_incomplete_x86");
    abi::emit_local_symbol_address(emitter, "rsi", "_incomplete_class_name");         // __PHP_Incomplete_Class
    emitter.instruction("mov edx, 22");                                         // byte length of __PHP_Incomplete_Class
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

    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the entry count
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
/// `<indent>[KEY]=>\n<indent>VALUE` block each, then one `["name"]=>` block per
/// dynamic property in insertion order.
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
    //   [64] saved x29, [72] saved x30.
    emitter.instruction("sub sp, sp, #80");                                     // allocate the object-walk frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the object-walk frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer
    emitter.instruction("str xzr, [sp, #48]");                                  // dynamic-properties-first flag not yet emitted
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "x9", "x0");
    emitter.instruction(&format!("cmn x9, #{}", -CLASS_ID));                    // class id -2 marks an incomplete object
    emitter.instruction("b.eq __rt_vd_obj_incomplete");                         // dump the projected name and properties

    // -- classes whose contract projects after the dynamic tail (GMP) dump it first --
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "x9", "x0");
    abi::emit_local_symbol_address(emitter, "x10", "_class_dump_dyn_first");          // resolve the dynamic-first flag table
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // read this class's ordering flag
    emitter.instruction("cbz x10, __rt_vd_obj_after_dyn_first");                // ordinary classes keep declared-then-dynamic order
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object for the dynamic hash lookup
    emitter.instruction("bl __rt_obj_dump_dyn_props");                          // x0 = dynamic-property hash, or 0
    emitter.instruction("cbz x0, __rt_vd_obj_mark_dyn_first");                  // no dynamic properties: just mark the order
    emitter.instruction("bl __rt_var_dump_hash");                               // emit the dynamic tail before the projection rows
    emitter.label("__rt_vd_obj_mark_dyn_first");
    emitter.instruction("mov x9, #1");                                          // record that the dynamic tail is already emitted
    emitter.instruction("str x9, [sp, #48]");                                   // save the ordering state for the done path
    emitter.label("__rt_vd_obj_after_dyn_first");

    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the object pointer after the dynamic walk
    emitter.instruction("bl __rt_vd_obj_desc");                                 // x0 = this class's var_dump descriptor
    emitter.instruction("str x0, [sp, #8]");                                    // save the descriptor pointer
    property_descriptor::emit_load_count(emitter, "x9", "x0");                // load the declared property count
    emitter.instruction("str x9, [sp, #24]");                                   // save the property count for the loop guard
    emitter.instruction("str xzr, [sp, #16]");                                  // property index = 0

    emitter.label("__rt_vd_obj_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the property index
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the property count
    emitter.instruction("cmp x9, x10");                                         // rendered every declared property?
    emitter.instruction("b.ge __rt_vd_obj_done");                               // walk complete

    // -- resolve this property's 48-byte descriptor row --
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the descriptor pointer
    property_descriptor::emit_row_address(emitter, "x11", "x11", "x9", "x12"); // select this property's descriptor row
    emitter.instruction("str x11, [sp, #32]");                                  // save the row pointer across the calls

    // -- resolve this property's 16-byte slot inside the instance --
    property_descriptor::emit_load_field(emitter, "x13", "x11", Field::ObjectOffset); // load the property's byte offset within the object
    emitter.instruction("ldr x14, [sp, #0]");                                   // reload the object pointer
    elephc_memoir::values::object_storage::emit_slot_address(emitter, "x13", "x14", "x13"); // resolve the absolute property slot address
    emitter.instruction("str x13, [sp, #40]");                                  // save the slot pointer across the calls

    // -- emit `<indent>[KEY]=>\n` --
    property_descriptor::emit_load_field(emitter, "x1", "x11", Field::Key);   // load the pre-rendered key pointer
    property_descriptor::emit_load_field(emitter, "x2", "x11", Field::KeyLength); // load the pre-rendered key length
    emitter.instruction("bl __rt_var_dump_emit_object_key");                    // emit `<indent>[KEY]=>\n`

    // -- an uninitialized typed property renders `uninitialized(TYPE)` --
    emitter.instruction("ldr x13, [sp, #40]");                                  // reload the property slot pointer
    emitter.instruction("ldr x11, [sp, #32]");                                  // recover the physical property's descriptor
    property_descriptor::emit_load_field(emitter, "x0", "x11", Field::ValueTag); // load the logical tag and native storage flags
    crate::codegen_support::runtime::reference_cells::resolve_property_payload(emitter, "x13", "x0", "x14", "__rt_vd_obj_marker");
    emit_uninit_sentinel_aarch64(emitter, "x15");                               // materialize the uninitialized-property marker
    emitter.instruction("cmp x14, x15");                                        // is this property still uninitialized?
    emitter.instruction("b.ne __rt_vd_obj_value");                              // initialized properties render their value

    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the descriptor row pointer
    property_descriptor::emit_load_field(emitter, "x1", "x11", Field::Auxiliary); // load the declared type-name pointer
    property_descriptor::emit_load_field(emitter, "x2", "x11", Field::AuxiliaryLength); // load the declared type-name length
    emitter.instruction("bl __rt_var_dump_emit_uninit_line");                   // emit `<indent>uninitialized(TYPE)\n`
    emitter.instruction("b __rt_vd_obj_next");                                  // continue with the next property

    emitter.label("__rt_vd_obj_value");
    // -- render the value line; __rt_var_dump_value unboxes Mixed cells (tag 7)
    //    and recurses into nested arrays/hashes/objects (tags 4/5/6) on its own --
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the descriptor row pointer
    emitter.instruction("ldr x13, [sp, #40]");                                  // reload the property slot pointer
    property_descriptor::emit_load_field(emitter, "x0", "x11", Field::ValueTag); // property value tag to value renderer
    crate::codegen_support::runtime::reference_cells::resolve_property_payload(emitter, "x13", "x0", "x14", "__rt_vd_obj_payload");
    elephc_memoir::values::object_storage::emit_load_slot_low(emitter, "x1", "x13"); // slot low word to value renderer
    elephc_memoir::values::object_storage::emit_load_slot_high(emitter, "x2", "x13"); // slot high word to value renderer
    emit_resolve_tagged_scalar_property_tag(emitter, "x0", "x2");
    emitter.instruction("bl __rt_var_dump_value");                              // emit `<indent>TYPE(VAL)\n` (recursing when needed)

    emitter.label("__rt_vd_obj_next");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the property index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next declared property
    emitter.instruction("str x9, [sp, #16]");                                   // save the updated index
    emitter.instruction("b __rt_vd_obj_loop");                                  // continue the walk

    emitter.label("__rt_vd_obj_done");
    emitter.instruction("ldr x9, [sp, #48]");                                   // was the dynamic tail emitted first?
    emitter.instruction("cbnz x9, __rt_vd_obj_exit");                           // do not emit it a second time
    // -- dynamic properties follow the declared ones, in insertion order --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("bl __rt_obj_dump_dyn_props");                          // x0 = dynamic-property hash, or 0
    emitter.instruction("cbz x0, __rt_vd_obj_exit");                            // no dynamic properties to append
    emitter.instruction("bl __rt_var_dump_hash");                               // emit one `["name"]=>` block per dynamic property
    emitter.instruction("b __rt_vd_obj_exit");                                  // skip the incomplete-object projection

    emitter.label("__rt_vd_obj_incomplete");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("mov x1, #0");                                          // get_object_vars-style visibility mode
    emitter.instruction("mov x2, #-1");                                         // global lexical scope
    emitter.instruction("bl __rt_object_to_hash");                              // project the synthetic name and opaque properties
    emitter.instruction("str x0, [sp, #48]");                                   // preserve the projected hash for release
    emitter.instruction("bl __rt_var_dump_hash");                               // dump the projected rows
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the projected hash
    emitter.instruction("bl __rt_hash_free_deep");                              // release the throwaway projection
    emitter.instruction("b __rt_vd_obj_exit");                                  // finish the object body

    emitter.label("__rt_vd_obj_exit");
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
    //   [-32] property count, [-40] descriptor row ptr, [-48] property slot ptr.
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the object-walk frame pointer
    emitter.instruction("sub rsp, 64");                                         // allocate the object-walk frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the object pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // dynamic-properties-first flag not yet emitted
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "r9", "rdi");
    emitter.instruction(&format!("cmp r9, {CLASS_ID}"));                        // class id -2 marks an incomplete object
    emitter.instruction("je __rt_vd_obj_incomplete_x86");                       // dump the projected name and properties

    // -- classes whose contract projects after the dynamic tail (GMP) dump it first --
    elephc_memoir::values::object_storage::emit_load_class_id(emitter, "r9", "rdi");
    abi::emit_local_symbol_address(emitter, "r10", "_class_dump_dyn_first");          // resolve the dynamic-first flag table
    emitter.instruction("cmp QWORD PTR [r10 + r9 * 8], 0");                     // does this class dump dynamic properties first?
    emitter.instruction("je __rt_vd_obj_after_dyn_first_x86");                  // ordinary classes keep declared-then-dynamic order
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the object for the dynamic hash lookup
    emitter.instruction("call __rt_obj_dump_dyn_props");                        // rax = dynamic-property hash, or 0
    emitter.instruction("test rax, rax");                                       // are there dynamic properties to emit?
    emitter.instruction("jz __rt_vd_obj_mark_dyn_first_x86");                   // no: just mark the order
    emitter.instruction("mov rdi, rax");                                        // dynamic-property hash -> hash walker
    emitter.instruction("call __rt_var_dump_hash");                             // emit the dynamic tail before the projection rows
    emitter.label("__rt_vd_obj_mark_dyn_first_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], 1");                         // record that the dynamic tail is already emitted
    emitter.label("__rt_vd_obj_after_dyn_first_x86");

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the object pointer after the dynamic walk
    emitter.instruction("call __rt_vd_obj_desc");                               // rax = this class's var_dump descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the descriptor pointer
    property_descriptor::emit_load_count(emitter, "r9", "rax");               // load the declared property count
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // save the property count for the loop guard
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // property index = 0

    emitter.label("__rt_vd_obj_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the property index
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the property count
    emitter.instruction("cmp r9, r10");                                         // rendered every declared property?
    emitter.instruction("jge __rt_vd_obj_done_x86");                            // walk complete

    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the descriptor pointer
    property_descriptor::emit_row_address(emitter, "r11", "r11", "r9", "r10"); // select this property's descriptor row
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the row pointer across the calls

    property_descriptor::emit_load_field(emitter, "r10", "r11", Field::ObjectOffset); // load the property's byte offset within the object
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the object pointer for slot addressing
    elephc_memoir::values::object_storage::emit_slot_address(emitter, "r10", "rax", "r10"); // resolve the absolute property slot address
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save the slot pointer across the calls

    property_descriptor::emit_load_field(emitter, "rdi", "r11", Field::Key);  // load the pre-rendered key pointer
    property_descriptor::emit_load_field(emitter, "rsi", "r11", Field::KeyLength); // load the pre-rendered key length
    emitter.instruction("call __rt_var_dump_emit_object_key");                  // emit `<indent>[KEY]=>\n`

    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the property slot pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // recover the physical property's descriptor
    property_descriptor::emit_load_field(emitter, "rcx", "r11", Field::ValueTag); // load the logical tag and native storage flags
    crate::codegen_support::runtime::reference_cells::resolve_property_payload(emitter, "r10", "rcx", "rax", "__rt_vd_obj_marker");
    emitter.instruction("mov r10, rax");                                        // compare the marker selected by physical storage
    emitter.instruction("movabs r11, 0x7ffffffffffffffd");                      // materialize the uninitialized-property marker
    emitter.instruction("cmp r10, r11");                                        // is this property still uninitialized?
    emitter.instruction("jne __rt_vd_obj_value_x86");                           // initialized properties render their value

    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the descriptor row pointer
    property_descriptor::emit_load_field(emitter, "rdi", "r11", Field::Auxiliary); // load the declared type-name pointer
    property_descriptor::emit_load_field(emitter, "rsi", "r11", Field::AuxiliaryLength); // load the declared type-name length
    emitter.instruction("call __rt_var_dump_emit_uninit_line");                 // emit `<indent>uninitialized(TYPE)\n`
    emitter.instruction("jmp __rt_vd_obj_next_x86");                            // continue with the next property

    emitter.label("__rt_vd_obj_value_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the descriptor row pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the property slot pointer
    property_descriptor::emit_load_field(emitter, "rdi", "r11", Field::ValueTag); // property value tag to value renderer
    crate::codegen_support::runtime::reference_cells::resolve_property_payload(emitter, "r10", "rdi", "r8", "__rt_vd_obj_payload");
    elephc_memoir::values::object_storage::emit_load_slot_low(emitter, "rsi", "r10"); // slot low word to value renderer
    elephc_memoir::values::object_storage::emit_load_slot_high(emitter, "rdx", "r10"); // slot high word to value renderer
    emit_resolve_tagged_scalar_property_tag(emitter, "rdi", "rdx");
    emitter.instruction("call __rt_var_dump_value");                            // emit `<indent>TYPE(VAL)\n` (recursing when needed)

    emitter.label("__rt_vd_obj_next_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the property index
    emitter.instruction("add r9, 1");                                           // advance to the next declared property
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // save the updated index
    emitter.instruction("jmp __rt_vd_obj_loop_x86");                            // continue the walk

    emitter.label("__rt_vd_obj_done_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // was the dynamic tail emitted first?
    emitter.instruction("jne __rt_vd_obj_exit_x86");                            // do not emit it a second time
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("call __rt_obj_dump_dyn_props");                        // rax = dynamic-property hash, or 0
    emitter.instruction("test rax, rax");                                       // does the object carry dynamic properties?
    emitter.instruction("jz __rt_vd_obj_exit_x86");                             // no dynamic properties to append
    emitter.instruction("mov rdi, rax");                                        // dynamic-property hash → hash walker
    emitter.instruction("call __rt_var_dump_hash");                             // emit one `["name"]=>` block per dynamic property
    emitter.instruction("jmp __rt_vd_obj_exit_x86");                            // skip the incomplete-object projection

    emitter.label("__rt_vd_obj_incomplete_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // object pointer for the x86_64 projection ABI
    emitter.instruction("xor edx, edx");                                        // get_object_vars-style visibility mode
    emitter.instruction("mov rcx, -1");                                         // global lexical scope
    emitter.instruction("call __rt_object_to_hash");                            // project the synthetic name and opaque properties
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the projected hash for release
    emitter.instruction("mov rdi, rax");                                        // pass the projected hash to the walker
    emitter.instruction("call __rt_var_dump_hash");                             // dump the projected rows
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the projected hash into the free ABI
    emitter.instruction("call __rt_hash_free_deep");                            // release the throwaway projection
    emitter.instruction("jmp __rt_vd_obj_exit_x86");                            // finish the object body

    emitter.label("__rt_vd_obj_exit_x86");
    emitter.instruction("add rsp, 64");                                         // release the object-walk frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the var_dump caller
}
