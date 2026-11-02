//! Purpose:
//! Combines external eval owners and native coroutine and SPL owners into one object child boundary.
//!
//! Called from:
//! - Both collectors, shared marking and structural object scan accounting.
//!
//! Key details:
//! - Fixed PHP property traversal remains separate and is never counted twice.
//! - Fiber caller pointers and visible start arguments are borrowed, not edges.
//! - Suspended activation owners use a separate typed visitor; saved registers are not scanned.
//! - Pure counting includes only the slots visited by the corresponding owner walk.

use super::gc_candidates::{load, op, proofs, restore, save};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch, runtime::fibers};

/// Emits a shared C-ABI wrapper and the native Fiber capture visitor.
pub fn emit_gc_object_children(e: &mut Emitter) {
    e.blank();
    e.label_global("__rt_gc_object_children_count_all");
    op(e, "mov x2, #2", "mov edx, 2", "prime every external and native object-owned edge");
    abi::emit_jump(e, "__rt_gc_object_children");
    e.label_global("__rt_gc_object_children");
    abi::emit_frame_prologue(e, 48);
    // -- preserve the shared C child-traversal ABI across both independent owner sources --
    arguments(e, true);
    op(e, "mov x0, #0", "xor eax, eax", "initialize the combined count without creating PHP owners");
    save(e, 24);
    for helper in ["__rt_gc_eval_object_children", "__rt_gc_fiber_children", "__rt_gc_generator_children", "__rt_gc_spl_children", "__rt_gc_incomplete_children", "__rt_gc_frame_children"] {
        arguments(e, false);
        abi::emit_call_label(e, helper);
        op(e, "ldr x9, [sp, #24]", "mov r10, QWORD PTR [rsp + 24]", "recover the preceding independent object-owner count");
        op(e, "add x0, x0, x9", "add rax, r10", "combine exact incoming counts or pure structural slot counts");
        save(e, 24);
    }
    restore(e, 24);
    abi::emit_frame_restore(e, 48);
    abi::emit_return(e);
    fiber(e);
    let mut generator_fields = vec![fibers::FIBER_TRANSFER_VALUE_OFFSET, fibers::FIBER_PENDING_THROW_OFFSET];
    generator_fields.extend((0..fibers::FIBER_START_ARGS_MAX).map(|i| fibers::FIBER_START_ARGS_OFFSET + i * 8));
    use crate::codegen_support::runtime::generators::coro;
    generator_fields.extend([coro::GEN_LAST_KEY_OFFSET, coro::GEN_LAST_VALUE_OFFSET, coro::GEN_RETURN_VALUE_OFFSET]);
    fixed(e, "generator", &["_generator_class_id"], fibers::FIBER_OBJECT_SIZE, &generator_fields);
    fixed(e, "spl", &["_spl_dll_class_id", "_spl_stack_class_id", "_spl_queue_class_id", "_spl_fixed_array_class_id"], 16,
        &[crate::codegen_support::runtime::spl::SPL_DLL_STORAGE_OFFSET as i32]);
    incomplete(e);
}

/// Saves or restores the three C traversal arguments without PHP ownership changes.
fn arguments(e: &mut Emitter, store: bool) {
    for (index, (arm, x86)) in [("x0", "rdi"), ("x1", "rsi"), ("x2", "rdx")].iter().enumerate() {
        let offset = index * 8;
        if store {
            op(e, &format!("str {arm}, [sp, #{offset}]"), &format!("mov QWORD PTR [rsp + {offset}], {x86}"), "preserve this borrowed traversal argument across the other visitor");
        } else {
            op(e, &format!("ldr {arm}, [sp, #{offset}]"), &format!("mov {x86}, QWORD PTR [rsp + {offset}]"), "restore the same owner, candidate and traversal mode");
        }
    }
}

/// Enumerates actual Fiber owners, matching the special object teardown contract.
fn fiber(e: &mut Emitter) {
    e.blank();
    e.label_global("__rt_gc_fiber_children");
    abi::emit_frame_prologue(e, 64);
    arguments(e, true);
    op(e, "mov x0, #0", "xor eax, eax", "begin with no matching capture owners");
    save(e, 24);
    restore(e, 0);
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_fiber_done");
    op(e, "ldr x9, [x0]", "mov r10, QWORD PTR [rax]", "identify the native layout before reading any Fiber-specific field");
    load(e, "x10", "r11", "_fiber_class_id");
    op(e, "cmp x9, x10", "cmp r10, r11", "ordinary PHP objects have no native Fiber owners");
    op(e, "b.ne __rt_gc_fiber_done", "jne __rt_gc_fiber_done", "leave fixed-property and eval ownership to their existing visitors");
    op(e, "ldr w9, [x0, #-16]", "mov r10d, DWORD PTR [rax - 16]", "validate the native payload extent before reading its owned fields");
    op(e, &format!("cmp x9, #{}", fibers::FIBER_OBJECT_SIZE), &format!("cmp r10d, {}", fibers::FIBER_OBJECT_SIZE), "a missing builtin class id must not make a short object a Fiber");
    op(e, "b.lo __rt_gc_fiber_done", "jb __rt_gc_fiber_done", "reject invalid native layout extents");
    op(e, &format!("ldr x9, [x0, #{}]", fibers::FIBER_USER_ARG_MAX_OFFSET), &format!("mov r10, QWORD PTR [rax + {}]", fibers::FIBER_USER_ARG_MAX_OFFSET), "legacy trailing capture slots start after the borrowed visible arguments");
    op(e, "str x9, [sp, #40]", "mov QWORD PTR [rsp + 40], r10", "preserve the ownership boundary across child calls");
    op(e, "mov x0, #3", "mov eax, 3", "count callable, transfer value and pending throwable slots");
    for index in 0..fibers::FIBER_START_ARGS_MAX {
        let next = format!("__rt_gc_fiber_cost_skip_{index}");
        op(e, &format!("cmp x9, #{index}"), &format!("cmp r10, {index}"), "visible start arguments are borrowed and must never count as incoming owners");
        op(e, &format!("b.hi {next}"), &format!("ja {next}"), "exclude borrowed visible argument storage from structural ownership");
        op(e, "add x0, x0, #1", "add rax, 1", "charge one visited legacy trailing capture slot");
        e.label(&next);
    }
    save(e, 32);
    op(e, "ldr x9, [sp, #16]", "mov r10, QWORD PTR [rsp + 16]", "pure accounting reads no captured PHP value");
    op(e, "cmp x9, #3", "cmp r10, 3", "select structural slot counting without tracing or ownership changes");
    op(e, "b.eq __rt_gc_fiber_return", "je __rt_gc_fiber_return", "return exactly the number of slots this native visitor inspects");
    // -- match the owned fields released by object_free_deep, not pointer-shaped native state --
    for (index, offset) in [fibers::FIBER_CALLABLE_OFFSET, fibers::FIBER_TRANSFER_VALUE_OFFSET, fibers::FIBER_PENDING_THROW_OFFSET].into_iter().enumerate() {
        child(e, "fiber", index, offset);
    }
    for index in 0..fibers::FIBER_START_ARGS_MAX {
        let next = format!("__rt_gc_fiber_legacy_skip_{index}");
        op(e, "ldr x9, [sp, #40]", "mov r10, QWORD PTR [rsp + 40]", "recover the boundary between borrowed arguments and owned legacy captures");
        op(e, &format!("cmp x9, #{index}"), &format!("cmp r10, {index}"), "trace only trailing slots whose teardown releases a real owner");
        op(e, &format!("b.hi {next}"), &format!("ja {next}"), "ignore ordinary start arguments, including stale borrowed pointers");
        child(e, "fiber", 3 + index as usize, fibers::FIBER_START_ARGS_OFFSET + index * 8);
        e.label(&next);
    }
    e.label("__rt_gc_fiber_done");
    restore(e, 24);
    e.label("__rt_gc_fiber_return");
    abi::emit_frame_restore(e, 64);
    abi::emit_return(e);
}

/// Processes one native owned slot using the same modes as external object children.
fn child(e: &mut Emitter, name: &str, index: usize, offset: i32) {
    let mark = format!("__rt_gc_{name}_mark_{index}");
    let note = format!("__rt_gc_{name}_note_{index}");
    let next = format!("__rt_gc_{name}_next_{index}");
    restore(e, 0);
    op(e, &format!("ldr x0, [x0, #{offset}]"), &format!("mov rax, QWORD PTR [rax + {offset}]"), "borrow one real native-owned value without acquiring an owner");
    op(e, "ldr x9, [sp, #16]", "mov r10, QWORD PTR [rsp + 16]", "select graph marking or incoming-edge accounting");
    op(e, "cmp x9, #1", "cmp r10, 1", "marking shares the normal graph discovery and proof modes");
    op(e, &format!("b.eq {mark}"), &format!("je {mark}"), "visit this actual owner through the shared marker");
    if e.target.arch == Arch::X86_64 {
        op(e, "", "cmp r10, 2", "cache priming records every child in its transient header");
        op(e, "", &format!("je {note}"), "select all-edge counting without retaining the child");
        op(e, "", "cmp rax, QWORD PTR [rsp + 8]", "an exact saturated recount counts only the selected candidate");
        op(e, "", &format!("jne {next}"), "other owned fields do not contribute to this candidate");
        op(e, "", "add QWORD PTR [rsp + 24], 1", "retain a full-width exact incoming count");
        abi::emit_jump(e, &next);
    }
    e.label(&note);
    abi::emit_call_label(e, "__rt_gc_note_child_ref");
    abi::emit_jump(e, &next);
    e.label(&mark);
    proofs::slot(e, &format!("{name}_native_{index}"), None, -1);
    abi::emit_call_label(e, "__rt_gc_mark_reachable");
    e.label(&next);
}

/// Visits fixed native owner slots using the same guarded layouts as object teardown.
fn fixed(e: &mut Emitter, name: &str, classes: &[&str], minimum: i32, offsets: &[i32]) {
    let done = format!("__rt_gc_{name}_done");
    let matched = format!("__rt_gc_{name}_matched");
    let returned = format!("__rt_gc_{name}_return");
    e.blank();
    e.label_global(&format!("__rt_gc_{name}_children"));
    abi::emit_frame_prologue(e, 64);
    arguments(e, true);
    op(e, "mov x0, #0", "xor eax, eax", "initialize the exact native incoming count");
    save(e, 24);
    restore(e, 0);
    abi::emit_branch_if_int_result_zero(e, &done);
    op(e, "ldr x9, [x0]", "mov r10, QWORD PTR [rax]", "select only a class whose native ownership layout is known");
    for class in classes {
        load(e, "x10", "r11", class);
        op(e, "cmp x9, x10", "cmp r10, r11", "compare the runtime class identity with this native owner class");
        op(e, &format!("b.eq {matched}"), &format!("je {matched}"), "visit the matching native storage layout");
    }
    abi::emit_jump(e, &done);
    e.label(&matched);
    op(e, "ldr w9, [x0, #-16]", "mov r10d, DWORD PTR [rax - 16]", "bound fixed native owner fields by the actual allocation extent");
    op(e, &format!("cmp x9, #{minimum}"), &format!("cmp r10d, {minimum}"), "require every native field to fit this allocation");
    op(e, &format!("b.lo {done}"), &format!("jb {done}"), "reject an incomplete native payload before reading any owner slot");
    op(e, "ldr x9, [sp, #16]", "mov r10, QWORD PTR [rsp + 16]", "distinguish pure slot counting from owner traversal");
    op(e, "cmp x9, #3", "cmp r10, 3", "pure accounting must not inspect children or change reference counts");
    op(e, &format!("mov x0, #{}", offsets.len()), &format!("mov eax, {}", offsets.len()), "count exactly the fixed slots visited by this native layout");
    op(e, &format!("b.eq {returned}"), &format!("je {returned}"), "return structural work without tracing or invoking PHP");
    for (index, offset) in offsets.iter().enumerate() { child(e, name, index, *offset); }
    e.label(&done);
    restore(e, 24);
    e.label(&returned);
    abi::emit_frame_restore(e, 64);
    abi::emit_return(e);
}

/// Traces the property hash of incomplete objects without interpreting their string name as a graph.
fn incomplete(e: &mut Emitter) {
    e.blank();
    e.label_global("__rt_gc_incomplete_children");
    abi::emit_frame_prologue(e, 64);
    arguments(e, true);
    op(e, "mov x0, #0", "xor eax, eax", "initialize the incomplete object's exact incoming count");
    save(e, 24);
    restore(e, 0);
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_incomplete_done");
    op(e, "ldr x9, [x0]", "mov r10, QWORD PTR [rax]", "incomplete objects use a reserved class identity outside ordinary descriptors");
    op(e, "cmn x9, #2", "cmp r10, -2", "recognize only the unserializer's incomplete-object layout");
    op(e, "b.ne __rt_gc_incomplete_done", "jne __rt_gc_incomplete_done", "ordinary classes use their existing fixed or native owner visitors");
    op(e, "ldr w9, [x0, #-16]", "mov r10d, DWORD PTR [rax - 16]", "validate the allocation before accessing the owned property hash");
    op(e, "cmp x9, #32", "cmp r10, 32", "require the complete incomplete-object payload");
    op(e, "b.lo __rt_gc_incomplete_done", "jb __rt_gc_incomplete_done", "reject undersized storage before reading native fields");
    op(e, "ldr x9, [sp, #16]", "mov r10, QWORD PTR [rsp + 16]", "select pure slot accounting without tracing the property values");
    op(e, "cmp x9, #3", "cmp r10, 3", "the persisted class name is a string and contributes no graph edge");
    op(e, "mov x0, #1", "mov eax, 1", "count exactly the one owned property-hash slot");
    op(e, "b.eq __rt_gc_incomplete_return", "je __rt_gc_incomplete_return", "return structural work without inspecting or retaining children");
    child(e, "incomplete", 0, 24);
    e.label("__rt_gc_incomplete_done");
    restore(e, 24);
    e.label("__rt_gc_incomplete_return");
    abi::emit_frame_restore(e, 64);
    abi::emit_return(e);
}
