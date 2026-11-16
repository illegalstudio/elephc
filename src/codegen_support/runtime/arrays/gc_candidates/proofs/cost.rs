//! Purpose:
//! Counts structural scan work, including non-graph slots that a visitor must inspect.
//!
//! Called from:
//! - Proof construction, candidate discovery and surviving-candidate rebuild charging.
//!
//! Key details:
//! - Hash capacity is charged even when every entry is empty, scalar or string.
//! - Scalar indexed arrays contribute no element loop, matching the shared visitor.
//! - Native retained edges are counted through the side-effect-free enumeration mode.

use super::{load, op, reg, save};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch, sentinels::{REFERENCE_CELL_HEAP_KIND, CALLABLE_HEAP_KIND}};

/// Returns one node unit plus the logical slots actually inspected by its child visitor.
pub(super) fn emit(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_node_scan_cost ---");
    e.label_global("__rt_gc_node_scan_cost");
    abi::emit_frame_prologue(e, 48);
    save(e, 0);
    // -- use the same physical kinds and payload descriptors as child discovery --
    op(e, "ldrb w9, [x0, #-8]", "movzx r10d, BYTE PTR [rax - 8]", "select the current allocation's typed child layout");
    for (kind, label) in [(2, "array"), (3, "hash"), (4, "object"), (5, "box"), (REFERENCE_CELL_HEAP_KIND as u64, "reference"), (CALLABLE_HEAP_KIND as u64, "callable")] {
        op(e, &format!("cmp x9, #{kind}"), &format!("cmp r10d, {kind}"), "classify scan work without interpreting pointer-shaped scalar data");
        abi::emit_branch_if_equal_wide(e, &format!("__rt_gc_scan_cost_{label}"));
    }
    abi::emit_jump(e, "__rt_gc_scan_cost_one");

    // -- array loops visit elements only for reference-bearing payload types --
    e.label("__rt_gc_scan_cost_array");
    descriptor(e);
    range(e, "array");
    op(e, "ldr x0, [x0]", "mov rax, QWORD PTR [rax]", "charge every initialized pointer slot traversed by the array visitor");
    op(e, "add x0, x0, #1", "add rax, 1", "include the array node itself in the scan estimate");
    abi::emit_jump(e, "__rt_gc_scan_cost_done");
    e.label("__rt_gc_scan_cost_hash");
    op(e, "ldr x0, [x0, #8]", "mov rax, QWORD PTR [rax + 8]", "charge the complete capacity scan, including scalar values and tombstones");
    op(e, "add x0, x0, #1", "add rax, 1", "include the stable hash header in the scan estimate");
    abi::emit_jump(e, "__rt_gc_scan_cost_done");

    // -- wrappers have one possible child slot rather than a variable-size loop --
    e.label("__rt_gc_scan_cost_reference");
    descriptor(e);
    abi::emit_jump(e, "__rt_gc_scan_cost_cell");
    e.label("__rt_gc_scan_cost_box");
    op(e, "ldr x9, [x0]", "mov r10, QWORD PTR [rax]", "inspect whether the Mixed payload has a graph child");
    e.label("__rt_gc_scan_cost_cell");
    range(e, "cell");
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 2);
    abi::emit_jump(e, "__rt_gc_scan_cost_done");

    // -- callable accounting includes scalar captures as well as graph owners --
    e.label("__rt_gc_scan_cost_callable");
    op(e, "mov x0, x0", "mov rdi, rax", "pass the descriptor through the C child-visitor ABI");
    abi::emit_load_int_immediate(e, reg(e, "x1", "rsi"), 0);
    abi::emit_load_int_immediate(e, reg(e, "x2", "rdx"), 3);
    abi::emit_call_label(e, "__rt_gc_callable_children");
    op(e, "add x0, x0, #1", "add rax, 1", "include the descriptor alongside every capture slot inspected");
    abi::emit_jump(e, "__rt_gc_scan_cost_done");

    // -- fixed object properties are inspected regardless of their declared value tags --
    e.label("__rt_gc_scan_cost_object");
    op(e, "ldr x10, [x0]", "mov r10, QWORD PTR [rax]", "read the current class before consulting immutable layout metadata");
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 1);
    save(e, 8);
    load(e, "x11", "r11", "_class_gc_desc_count");
    op(e, "cmp x10, x11", "cmp r10, r11", "exclude unknown class layouts while retaining native-edge accounting");
    op(e, "b.hs __rt_gc_scan_cost_native", "jae __rt_gc_scan_cost_native", "unknown layouts have no emitted fixed-property walk");
    address(e, "x11", "r11", "_class_object_payload_sizes");
    op(e, "ldr x11, [x11, x10, lsl #3]", "mov rcx, QWORD PTR [r11 + r10 * 8]", "use declared property storage rather than recycled heap capacity");
    address(e, "x12", "r11", "_class_object_dynamic_prop_flags");
    op(e, "ldr x12, [x12, x10, lsl #3]", "mov r9, QWORD PTR [r11 + r10 * 8]", "identify the separately owned dynamic-property tail");
    op(e, "sub x11, x11, #8", "sub rcx, 8", "exclude the leading class identity from property slots");
    op(e, "sub x11, x11, x12, lsl #3", "lea r11, [r9 * 8]", "exclude the optional tail before dividing the fixed-property region");
    if e.target.arch == Arch::X86_64 { op(e, "", "sub rcx, r11", "remove the dynamic tail from the fixed region"); }
    op(e, "lsr x11, x11, #4", "shr rcx, 4", "count all fixed property slots, including scalar and string properties");
    op(e, "add x11, x11, x12", "add rcx, r9", "include the optional owned dynamic-property slot");
    op(e, "add x0, x11, #1", "lea rax, [rcx + 1]", "include the object itself in its structural scan estimate");
    save(e, 8);
    // -- native enumeration returns only a count and never marks or retains its children --
    e.label("__rt_gc_scan_cost_native");
    op(e, "ldr x0, [sp]", "mov rdi, QWORD PTR [rsp]", "provide the validated owning object using the callback helper ABI");
    abi::emit_load_int_immediate(e, reg(e, "x1", "rsi"), 0);
    abi::emit_load_int_immediate(e, reg(e, "x2", "rdx"), 3);
    abi::emit_call_label(e, "__rt_gc_object_children");
    op(e, "ldr x9, [sp, #8]", "mov r10, QWORD PTR [rsp + 8]", "recover the ordinary object-slot cost after native enumeration");
    op(e, "adds x0, x0, x9", "add rax, r10", "include native retained slots in the total work charge");
    op(e, "b.cc __rt_gc_scan_cost_done", "jnc __rt_gc_scan_cost_done", "keep the exact sum when it fits the work counter");
    op(e, "mov x0, #-1", "mov rax, -1", "saturate impossible-to-represent work rather than undercharging it");
    abi::emit_jump(e, "__rt_gc_scan_cost_done");
    e.label("__rt_gc_scan_cost_one");
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 1);
    e.label("__rt_gc_scan_cost_done");
    abi::emit_frame_restore(e, 48);
    abi::emit_return(e);
}

/// Reads the array or reference cell payload descriptor without collector flags.
fn descriptor(e: &mut Emitter) {
    op(e, "ldr x9, [x0, #-8]", "mov r10, QWORD PTR [rax - 8]", "inspect whether this layout actually traverses pointer slots");
    op(e, "ubfx x9, x9, #8, #7", "shr r10, 8", "isolate the packed payload descriptor");
    if e.target.arch == Arch::X86_64 { op(e, "", "and r10d, 0x7f", "discard COW and collector flags before choosing loop work"); }
}

/// Skips an element loop when its representation cannot contain graph pointers.
fn range(e: &mut Emitter, name: &str) {
    let ready = format!("__rt_gc_scan_cost_range_{name}");
    op(e, "cmp x9, #10", "cmp r10, 10", "callable payloads own descriptors with typed capture edges");
    abi::emit_branch_if_equal_wide(e, &ready);
    op(e, "cmp x9, #4", "cmp r10, 4", "scalar and string representations have no child pointer loop");
    op(e, "b.lo __rt_gc_scan_cost_one", "jb __rt_gc_scan_cost_one", "charge only the node for a non-graph leaf");
    op(e, "cmp x9, #7", "cmp r10, 7", "bound the representations visited by the shared child scanner");
    op(e, "b.hi __rt_gc_scan_cost_one", "ja __rt_gc_scan_cost_one", "unsupported representations do not contribute pointer-slot work");
    e.label(&ready);
}

/// Addresses class metadata without borrowing the live class index.
fn address(e: &mut Emitter, arm: &str, x86: &str, symbol: &str) {
    abi::emit_symbol_address(e, reg(e, arm, x86), symbol);
}
