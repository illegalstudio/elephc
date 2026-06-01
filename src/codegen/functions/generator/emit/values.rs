//! Purpose:
//! Expression lowering, Mixed-cell boxing, and the refcount-replace pattern
//! for boxed pointer slots in the generator frame. Shared by statement
//! lowering and yield emission.
//!
//! Called from:
//!  - `super::stmts` (body statement and switch subject emission).
//!  - `super::yields` (yield value/key boxing, send resume helpers).
//!  - `super::dispatcher` (return-value boxing on terminal `return`).
//!
//! Key details:
//!  - All boxed Mixed-cell allocations go through `__rt_mixed_from_value` so
//!    the runtime tag/payload contract stays consistent with the rest of the
//!    type system.
//!  - `emit_replace_mixed_slot` is the canonical refcount-replace pattern:
//!    park the previous pointer in `x20`, produce the new pointer in `x0`,
//!    overwrite the slot, then decref the previous pointer (NULL is safe).

use super::{preserved_scratch_reg, slot_offset};
use super::super::model::*;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime::generators::frame as gen_frame;

/// Loads an integer value from `src` into `dest_reg` on ARM64.
/// Supports literal values, generator frame slots, binary operations (add/sub/mul/div),
/// and integer function calls. For x86_64, delegates to `emit_load_int_source_x86_64`.
pub(super) fn emit_load_int_source(emitter: &mut Emitter, dest_reg: &str, src: &IntSource) {
    if emitter.target.arch == Arch::X86_64 {
        emit_load_int_source_x86_64(emitter, dest_reg, src);
        return;
    }

    match src {
        IntSource::Literal(n) => {
            emitter.instruction(&format!("mov {}, #{}", dest_reg, n));          // load the literal int into the destination register
        }
        IntSource::Slot(idx) => {
            emitter.instruction(&format!("ldr {}, [x19, #{}]", dest_reg, slot_offset(*idx))); // load the int slot from the generator frame
        }
        IntSource::BinaryOp(left, op, right) => {
            emit_load_int_source(emitter, dest_reg, left);
            emitter.instruction("sub sp, sp, #16");                             // reserve aligned temporary storage for the left operand
            emitter.instruction(&format!("str {}, [sp, #0]", dest_reg));        // preserve the left operand while evaluating the right operand
            let right_reg = if dest_reg == "x12" { "x13" } else { "x12" };
            emit_load_int_source(emitter, right_reg, right);
            emitter.instruction(&format!("ldr {}, [sp, #0]", dest_reg));        // restore the left operand after right-side evaluation
            emitter.instruction("add sp, sp, #16");                             // release the temporary operand storage
            let mnem = match op {
                IntBinOp::Add => "add",
                IntBinOp::Sub => "sub",
                IntBinOp::Mul => "mul",
                IntBinOp::Div => "sdiv",
            };
            emitter.instruction(&format!("{} {}, {}, {}", mnem, dest_reg, dest_reg, right_reg)); // combine left and right with the chosen op
        }
        IntSource::Call { fn_name, args } => {
            emit_int_function_call(emitter, fn_name, args);
            if dest_reg != "x0" {
                emitter.instruction(&format!("mov {}, x0", dest_reg));          // move the function return value to the destination register
            }
        }
    }
}

/// x86_64-specific integer loading. Handles the same `IntSource` variants as
/// the ARM64 path but uses r12-based frame addressing and r11/r10 scratch
/// registers instead of x12/x13.
fn emit_load_int_source_x86_64(emitter: &mut Emitter, dest_reg: &str, src: &IntSource) {
    match src {
        IntSource::Literal(n) => {
            emitter.instruction(&format!("mov {}, {}", dest_reg, n));           // load the literal int into the destination register
        }
        IntSource::Slot(idx) => {
            emitter.instruction(&format!("mov {}, QWORD PTR [r12 + {}]", dest_reg, slot_offset(*idx))); // load the int slot from the generator frame
        }
        IntSource::BinaryOp(left, op, right) => {
            if matches!(op, IntBinOp::Div) {
                emit_load_int_source_x86_64(emitter, "rax", left);
                emitter.instruction("sub rsp, 16");                             // reserve aligned temporary storage for the left operand
                emitter.instruction("mov QWORD PTR [rsp], rax");                // preserve the dividend while evaluating the divisor
                emit_load_int_source_x86_64(emitter, "r11", right);
                emitter.instruction("mov rax, QWORD PTR [rsp]");                // restore the dividend after divisor evaluation
                emitter.instruction("add rsp, 16");                             // release the temporary operand storage
                emitter.instruction("cqo");                                     // sign-extend the dividend before signed division
                emitter.instruction("idiv r11");                                // divide the left value by the right value
                if dest_reg != "rax" {
                    emitter.instruction(&format!("mov {}, rax", dest_reg));     // move the quotient into the requested destination register
                }
                return;
            }
            emit_load_int_source_x86_64(emitter, dest_reg, left);
            emitter.instruction("sub rsp, 16");                                 // reserve aligned temporary storage for the left operand
            emitter.instruction(&format!("mov QWORD PTR [rsp], {}", dest_reg)); // preserve the left operand while evaluating the right operand
            let right_reg = if dest_reg == "r11" { "r10" } else { "r11" };
            emit_load_int_source_x86_64(emitter, right_reg, right);
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", dest_reg)); // restore the left operand after right-side evaluation
            emitter.instruction("add rsp, 16");                                 // release the temporary operand storage
            match op {
                IntBinOp::Add => {
                    emitter.instruction(&format!("add {}, {}", dest_reg, right_reg)); // add the right operand into the left value
                }
                IntBinOp::Sub => {
                    emitter.instruction(&format!("sub {}, {}", dest_reg, right_reg)); // subtract the right operand from the left value
                }
                IntBinOp::Mul => {
                    emitter.instruction(&format!("imul {}, {}", dest_reg, right_reg)); // multiply the left value by the right operand
                }
                IntBinOp::Div => unreachable!(),
            }
        }
        IntSource::Call { fn_name, args } => {
            emit_int_function_call(emitter, fn_name, args);
            if dest_reg != "rax" {
                emitter.instruction(&format!("mov {}, rax", dest_reg));         // move the function return value to the destination register
            }
        }
    }
}

/// Evaluates `args` into a 16-byte-aligned stack stash, then pops them into
/// ABI integer registers (`x0..x{n-1}` on ARM64, `rdi..r9` on x86_64) before
/// branching to `fn_name`. The return value remains in `x0`/`rax`.
pub(super) fn emit_int_function_call(emitter: &mut Emitter, fn_name: &str, args: &[IntSource]) {
    if emitter.target.arch == Arch::X86_64 {
        emit_int_function_call_x86_64(emitter, fn_name, args);
        return;
    }

    let n = args.len();
    let stash_bytes = if n == 0 { 0 } else { ((n * 8) + 15) & !15 };
    if stash_bytes > 0 {
        emitter.instruction(&format!("sub sp, sp, #{}", stash_bytes));          // reserve a 16-byte aligned slab for evaluated arguments
        for (i, arg) in args.iter().enumerate() {
            emit_load_int_source(emitter, "x9", arg);                       // x9 = computed argument value
            emitter.instruction(&format!("str x9, [sp, #{}]", i * 8));          // park argument i in its stash slot
        }
        for i in 0..n {
            emitter.instruction(&format!("ldr x{}, [sp, #{}]", i, i * 8));      // load argument i into its ABI register
        }
    }
    let label = crate::names::function_symbol(fn_name);
    emitter.instruction(&format!("bl {}", label));                              // branch with link into the user function
    if stash_bytes > 0 {
        emitter.instruction(&format!("add sp, sp, #{}", stash_bytes));          // release the argument stash
    }
}

/// x86_64-specific integer function call. Overflow arguments (beyond `rdi..r9`)
/// are stashed on the stack with an additional alignment reservation before
/// being placed in their outgoing stack slots.
fn emit_int_function_call_x86_64(emitter: &mut Emitter, fn_name: &str, args: &[IntSource]) {
    let n = args.len();
    let stash_bytes = if n == 0 { 0 } else { ((n * 8) + 15) & !15 };
    let regs = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];
    let reg_count = n.min(regs.len());
    let overflow_count = n.saturating_sub(regs.len());
    let overflow_bytes = if overflow_count == 0 {
        0
    } else {
        ((overflow_count * 8) + 15) & !15
    };
    if stash_bytes > 0 {
        emitter.instruction(&format!("sub rsp, {}", stash_bytes));              // reserve a 16-byte aligned slab for evaluated arguments
        for (i, arg) in args.iter().enumerate() {
            emit_load_int_source_x86_64(emitter, "r10", arg);
            emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", i * 8)); // park argument i in its stash slot
        }
        if overflow_bytes > 0 {
            emitter.instruction(&format!("sub rsp, {}", overflow_bytes));       // reserve aligned outgoing stack slots for overflow arguments
            for i in 0..overflow_count {
                let src_off = overflow_bytes + (regs.len() + i) * 8;
                emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", src_off)); // reload overflow argument from the evaluation stash
                emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", i * 8)); // place overflow argument in its outgoing stack slot
            }
        }
        for (i, reg) in regs.iter().take(reg_count).enumerate() {
            let src_off = overflow_bytes + i * 8;
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp + {}]", reg, src_off)); // load argument i into its ABI register
        }
    }
    let label = crate::names::function_symbol(fn_name);
    crate::codegen::abi::emit_call_label(emitter, &label);
    if overflow_bytes > 0 {
        emitter.instruction(&format!("add rsp, {}", overflow_bytes));           // release outgoing stack slots for overflow arguments
    }
    if stash_bytes > 0 {
        emitter.instruction(&format!("add rsp, {}", stash_bytes));              // release the argument stash
    }
}

/// Materialize a Mixed-cell pointer for `src` in `x0`. Invoked at yield
/// time to box the payload before stashing it in the frame's
/// `last_key`/`last_value` slot.
pub(super) fn emit_box_mixed_source(emitter: &mut Emitter, src: &MixedSource) {
    if emitter.target.arch == Arch::X86_64 {
        emit_box_mixed_source_x86_64(emitter, src);
        return;
    }

    match src {
        MixedSource::Null => {
            emitter.instruction("mov x1, xzr");                                 // null has no low payload word
            emitter.instruction("mov x2, xzr");                                 // null has no high payload word
            emitter.instruction("mov x0, #8");                                  // x0 = tag (8 = null)
            emitter.instruction("bl __rt_mixed_from_value");                    // x0 = boxed Mixed pointer
        }
        MixedSource::Int(int_src) => {
            emit_load_int_source(emitter, "x1", int_src);                   // x1 = lo (the int payload)
            emitter.instruction("mov x2, xzr");                                 // x2 = hi (unused for ints)
            emitter.instruction("mov x0, #0");                                  // x0 = tag (0 = int)
            emitter.instruction("bl __rt_mixed_from_value");                    // x0 = boxed Mixed pointer
        }
        MixedSource::Str { label, len } => {
            // `adr` only reaches ±1 MB; go through `adrp + add :lo12:`.
            crate::codegen::abi::emit_symbol_address(emitter, "x1", label); // x1 = pointer to interned string bytes
            emitter.instruction(&format!("mov x2, #{}", len));                  // x2 = string length in bytes
            emitter.instruction("mov x0, #1");                                  // x0 = tag (1 = string)
            emitter.instruction("bl __rt_mixed_from_value");                    // x0 = boxed Mixed pointer
        }
        MixedSource::IntArrayLit(values) => {
            emit_box_int_array_literal(emitter, values);
        }
        MixedSource::MixedSlot(idx) => {
            // The slot already holds a boxed Mixed pointer; we share it
            // with the destination by incref'ing — the slot keeps its
            // own reference and the new owner gets one too.
            emitter.instruction(&format!("ldr x0, [x19, #{}]", slot_offset(*idx))); // load the boxed Mixed pointer from the slot
            emitter.instruction("bl __rt_incref");                              // retain a refcount for the new owner
        }
    }
}

/// x86_64-specific Mixed-cell boxing. Matches the behavior of `emit_box_mixed_source`
/// but uses r12-based frame addressing and System V ABI register conventions (`rdi`, `rsi`, `rax`).
fn emit_box_mixed_source_x86_64(emitter: &mut Emitter, src: &MixedSource) {
    match src {
        MixedSource::Null => {
            emitter.instruction("xor rdi, rdi");                                // null has no low payload word
            emitter.instruction("xor rsi, rsi");                                // null has no high payload word
            emitter.instruction("mov rax, 8");                                  // rax = tag (8 = null)
            emitter.instruction("call __rt_mixed_from_value");                  // rax = boxed Mixed pointer
        }
        MixedSource::Int(int_src) => {
            emit_load_int_source_x86_64(emitter, "rdi", int_src);
            emitter.instruction("xor rsi, rsi");                                // rsi = hi (unused for ints)
            emitter.instruction("xor rax, rax");                                // rax = tag (0 = int)
            emitter.instruction("call __rt_mixed_from_value");                  // rax = boxed Mixed pointer
        }
        MixedSource::Str { label, len } => {
            crate::codegen::abi::emit_symbol_address(emitter, "rdi", label);
            emitter.instruction(&format!("mov rsi, {}", len));                  // rsi = string length in bytes
            emitter.instruction("mov rax, 1");                                  // rax = tag (1 = string)
            emitter.instruction("call __rt_mixed_from_value");                  // rax = boxed Mixed pointer
        }
        MixedSource::IntArrayLit(values) => {
            emit_box_int_array_literal(emitter, values);
        }
        MixedSource::MixedSlot(idx) => {
            emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", slot_offset(*idx))); // load the boxed Mixed pointer from the slot
            emitter.instruction("call __rt_incref");                            // retain a refcount for the new owner
        }
    }
}

/// Allocates a heap buffer for an integer array literal, writes the 24-byte
/// header (kind=1, length, capacity, reserved=0), stores each `i64` element
/// at its offset, then boxes the pointer as a Mixed cell with tag 4 (indexed array).
/// On x86_64, stamps the heap header with `X86_64_HEAP_MAGIC_HI32` to distinguish
/// from ARM64 allocations.
fn emit_box_int_array_literal(emitter: &mut Emitter, values: &[i64]) {
    if emitter.target.arch == Arch::X86_64 {
        emit_box_int_array_literal_x86_64(emitter, values);
        return;
    }

    let n = values.len();
    let payload_bytes = 24 + n * 8;
    emitter.instruction(&format!("mov x0, #{}", payload_bytes));                // request bytes for the array header + slots
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = pointer to array body
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = indexed-int array
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the heap header kind
    emitter.instruction(&format!("mov x9, #{}", n));                            // length value
    emitter.instruction("str x9, [x0, #0]");                                    // store array length at +0
    emitter.instruction(&format!("mov x9, #{}", n));                            // capacity = length for a literal
    emitter.instruction("str x9, [x0, #8]");                                    // store array capacity at +8
    emitter.instruction("str xzr, [x0, #16]");                                  // zero the reserved third header word
    for (i, v) in values.iter().enumerate() {
        let off = 24 + i * 8;
        emitter.instruction(&format!("mov x9, #{}", v));                        // load element value
        emitter.instruction(&format!("str x9, [x0, #{}]", off));                // store element into the array body
    }
    // Box the array pointer as a Mixed cell with tag = 4 (indexed array).
    emitter.instruction("mov x1, x0");                                          // x1 = lo = array pointer
    emitter.instruction("mov x2, xzr");                                         // x2 = hi unused
    emitter.instruction("mov x0, #4");                                          // x0 = tag (4 = indexed array)
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = boxed Mixed pointer
}

/// x86_64-specific array literal boxing. Same semantics as `emit_box_int_array_literal`
/// but emits x86_64 instructions with rax-based addressing and `call` instead of `bl`.
fn emit_box_int_array_literal_x86_64(emitter: &mut Emitter, values: &[i64]) {
    let n = values.len();
    let payload_bytes = 24 + n * 8;
    emitter.instruction(&format!("mov rax, {}", payload_bytes));                // request bytes for the array header plus slots
    emitter.instruction("call __rt_heap_alloc");                                // rax = pointer to array body
    emitter.instruction(&format!("mov r10, 0x{:x}", (super::X86_64_HEAP_MAGIC_HI32 << 32) | 1)); // heap kind 1 = indexed-int array with x86 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the heap header kind
    emitter.instruction(&format!("mov QWORD PTR [rax], {}", n));                // store array length at +0
    emitter.instruction(&format!("mov QWORD PTR [rax + 8], {}", n));            // store array capacity at +8
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // zero the reserved third header word
    for (i, v) in values.iter().enumerate() {
        let off = 24 + i * 8;
        emitter.instruction(&format!("mov QWORD PTR [rax + {}], {}", off, v));  // store element into the array body
    }
    emitter.instruction("mov rdi, rax");                                        // rdi = lo = array pointer
    emitter.instruction("xor rsi, rsi");                                        // rsi = hi unused
    emitter.instruction("mov rax, 4");                                          // rax = tag (4 = indexed array)
    emitter.instruction("call __rt_mixed_from_value");                          // rax = boxed Mixed pointer
}

/// Boxes the yield key into a Mixed cell for the generator frame's `last_key` slot.
/// When `key` is `None`, generates an auto-incrementing integer key: loads the
/// counter from the frame, increments it atomically, boxes the old value, and
/// stores the result in `x0`/`rax`. ARM64 uses `x19`-relative addressing; x86_64
/// uses `r12`-relative addressing.
pub(super) fn emit_compute_key(emitter: &mut Emitter, key: Option<&MixedSource>) {
    match key {
        Some(src) => emit_box_mixed_source(emitter, src),
        None => {
            // Auto-key: load + increment the counter, then box the read value.
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("ldr x1, [x19, #{}]", gen_frame::OFF_AUTO_KEY_COUNTER)); // x1 = current auto-key
                    emitter.instruction("add x9, x1, #1");                      // x9 = next auto-key
                    emitter.instruction(&format!("str x9, [x19, #{}]", gen_frame::OFF_AUTO_KEY_COUNTER)); // store the incremented counter
                    emitter.instruction("mov x2, xzr");                         // x2 = unused hi for an int
                    emitter.instruction("mov x0, #0");                          // x0 = int tag
                    emitter.instruction("bl __rt_mixed_from_value");            // x0 = boxed auto-key Mixed pointer
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("mov rdi, QWORD PTR [r12 + {}]", gen_frame::OFF_AUTO_KEY_COUNTER)); // rdi = current auto-key
                    emitter.instruction("lea r10, [rdi + 1]");                  // r10 = next auto-key
                    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r10", gen_frame::OFF_AUTO_KEY_COUNTER)); // store the incremented counter
                    emitter.instruction("xor rsi, rsi");                        // rsi = unused hi for an int
                    emitter.instruction("xor rax, rax");                        // rax = int tag
                    emitter.instruction("call __rt_mixed_from_value");          // rax = boxed auto-key Mixed pointer
                }
            }
        }
    }
}

/// Helper for the boxed-pointer overwrite pattern: park the previous
/// pointer in x20, run `produce_new` (which leaves the new boxed Mixed
/// pointer in x0), store it into the slot at `slot_off`, then decref the
/// previous pointer.
pub(super) fn emit_replace_mixed_slot(
    emitter: &mut Emitter,
    slot_off: usize,
    produce_new: impl FnOnce(&mut Emitter),
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x20, [x19, #{}]", slot_off));     // remember the previous boxed pointer
            produce_new(emitter);
            emitter.instruction(&format!("str x0, [x19, #{}]", slot_off));      // store the freshly boxed pointer
            emitter.instruction("mov x0, x20");                                 // x0 = previous boxed pointer (or NULL)
            emitter.instruction("bl __rt_decref_mixed");                        // release the previous boxed pointer (NULL is safe)
        }
        Arch::X86_64 => {
            let old_reg = preserved_scratch_reg(emitter);
            emitter.instruction(&format!("mov {}, QWORD PTR [r12 + {}]", old_reg, slot_off)); // remember the previous boxed pointer
            produce_new(emitter);
            emitter.instruction(&format!("mov QWORD PTR [r12 + {}], rax", slot_off)); // store the freshly boxed pointer
            emitter.instruction(&format!("mov rax, {}", old_reg));              // rax = previous boxed pointer (or NULL)
            emitter.instruction("call __rt_decref_mixed");                      // release the previous boxed pointer (NULL is safe)
        }
    }
}

/// Emits a conditional branch to `false_label` when the comparison `cond` evaluates
/// to false. Evaluates both operands using `emit_load_int_source` / `emit_load_int_source_x86_64`,
/// preserving them across evaluation, then emits the appropriate compare and branch
/// using inverted condition codes. The branch is a no-op when the condition is true.
pub(super) fn emit_branch_if_false(emitter: &mut Emitter, cond: &BoolExpr, false_label: &str) {
    if let BoolExpr::MixedSlotNull { slot_idx, is_equal } = cond {
        emit_branch_if_mixed_slot_null_condition_false(emitter, *slot_idx, *is_equal, false_label);
        return;
    }

    let BoolExpr::IntCompare { left, op, right } = cond else {
        unreachable!("all non-mixed-null generator bool expressions are int comparisons");
    };
    if emitter.target.arch == Arch::X86_64 {
        emit_load_int_source_x86_64(emitter, "r10", left);
        emitter.instruction("sub rsp, 16");                                     // reserve aligned temporary storage for the left comparison value
        emitter.instruction("mov QWORD PTR [rsp], r10");                        // preserve the left comparison value while evaluating the right side
        emit_load_int_source_x86_64(emitter, "r11", right);
        emitter.instruction("mov r10, QWORD PTR [rsp]");                        // restore the left comparison value after right-side evaluation
        emitter.instruction("add rsp, 16");                                     // release the temporary comparison storage
        emitter.instruction("cmp r10, r11");                                    // compare the two computed integers
        let inverse_cc = match op {
            CmpOp::Lt => "jge",
            CmpOp::Le => "jg",
            CmpOp::Gt => "jle",
            CmpOp::Ge => "jl",
            CmpOp::Eq => "jne",
            CmpOp::Ne => "je",
        };
        emitter.instruction(&format!("{} {}", inverse_cc, false_label));        // branch if the condition is false
        return;
    }

    emit_load_int_source(emitter, "x1", left);
    emitter.instruction("sub sp, sp, #16");                                     // reserve aligned temporary storage for the left comparison value
    emitter.instruction("str x1, [sp, #0]");                                    // preserve the left comparison value while evaluating the right side
    emit_load_int_source(emitter, "x2", right);
    emitter.instruction("ldr x1, [sp, #0]");                                    // restore the left comparison value after right-side evaluation
    emitter.instruction("add sp, sp, #16");                                     // release the temporary comparison storage
    emitter.instruction("cmp x1, x2");                                          // compare the two computed integers
    let inverse_cc = match op {
        CmpOp::Lt => "ge",
        CmpOp::Le => "gt",
        CmpOp::Gt => "le",
        CmpOp::Ge => "lt",
        CmpOp::Eq => "ne",
        CmpOp::Ne => "eq",
    };
    emitter.instruction(&format!("b.{} {}", inverse_cc, false_label));          // branch if the condition is false
}

/// Emits a false branch for `$mixed_slot === null` or `$mixed_slot !== null`.
///
/// A generator Mixed slot may be a null pointer when no value was sent, or a
/// boxed Mixed null cell. Both forms compare as PHP null.
fn emit_branch_if_mixed_slot_null_condition_false(
    emitter: &mut Emitter,
    slot_idx: usize,
    is_equal: bool,
    false_label: &str,
) {
    let boxed_null = format!("{}_mixed_null", false_label);
    let done = format!("{}_mixed_null_done", false_label);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", slot_offset(slot_idx))); // load the boxed Mixed slot pointer for null comparison
        emitter.instruction("test rax, rax");                                   // null pointer slots are PHP null
        emitter.instruction(&format!("jz {}", boxed_null));                     // skip unboxing when the slot has no box
        emitter.instruction("call __rt_mixed_unbox");                           // rax = Mixed tag for the boxed slot
        emitter.instruction("cmp rax, 8");                                      // tag 8 is PHP null
        emitter.instruction(&format!("{} {}", if is_equal { "jne" } else { "je" }, false_label)); // branch when the null predicate is false
        emitter.instruction(&format!("jmp {}", done));                          // skip the raw-null branch
        emitter.label(&boxed_null);
        if !is_equal {
            emitter.instruction(&format!("jmp {}", false_label));               // raw null makes !== null false
        }
        emitter.label(&done);
        return;
    }

    emitter.instruction(&format!("ldr x0, [x19, #{}]", slot_offset(slot_idx))); // load the boxed Mixed slot pointer for null comparison
    emitter.instruction(&format!("cbz x0, {}", boxed_null));                    // null pointer slots are PHP null
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0 = Mixed tag for the boxed slot
    emitter.instruction("cmp x0, #8");                                          // tag 8 is PHP null
    emitter.instruction(&format!("b.{} {}", if is_equal { "ne" } else { "eq" }, false_label)); // branch when the null predicate is false
    emitter.instruction(&format!("b {}", done));                                // skip the raw-null branch
    emitter.label(&boxed_null);
    if !is_equal {
        emitter.instruction(&format!("b {}", false_label));                     // raw null makes !== null false
    }
    emitter.label(&done);
}
