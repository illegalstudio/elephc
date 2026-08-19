//! Purpose:
//! Builds the EIR scaffolding and frame for a synthetic helper emitted once per module,
//! so a lowering sequence that would otherwise be copied at every site has a body to live in.
//!
//! Called from:
//! - `crate::codegen::shared_mixed_string` for the `__toString` dispatch ladder.
//! - `crate::codegen::shared_count_guard` for `count()`'s countable check.
//!
//! Key details:
//! - The helper takes its boxed `Mixed` in the INT RESULT register, not an argument register,
//!   because that is where the inlined sequences already had it. Nothing is shuffled, which is
//!   what makes moving a sequence out of its original frame a MOVE and not a rewrite.
//! - The frame saves the reserved nested-call register unconditionally. Only the string ladder
//!   uses it, but it is callee-saved and outside the allocator's tracking, so a helper that
//!   might reach one must save it itself: the frame layout only reserves a slot for functions
//!   containing a Mixed-receiver METHOD call, and no helper is one.
//! - A helper body may THROW. Every raise ends in a jump to `__rt_throw_current`, which unwinds
//!   through this frame to the caller's handler — pinned by
//!   `test_a_throw_inside_the_shared_string_ladder_is_still_catchable`, because the extra frame
//!   between the raise and the `catch` is exactly what sharing introduces.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::frame;
use crate::codegen::platform::Arch;
use crate::codegen::shared_state::SharedCodegenState;
use crate::ir::{
    BasicBlock, BlockId, Function, FunctionParam, IrHeapKind, IrType, Module, Ownership, Value,
    ValueDef, ValueId,
};
use crate::types::PhpType;

use super::Result;

/// The SSA value standing in for a helper's parameter.
///
/// It exists to satisfy the shared signature and is never loaded — the boxed pointer is
/// already in the result register — which is what lets a sequence written against a real
/// frame run inside a synthetic one.
pub(super) fn helper_value() -> ValueId {
    ValueId::from_raw(0)
}

/// Emits one shared helper: its frame, the caller's body, and the matching return.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_shared_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
    label: &str,
    return_php_type: PhpType,
    comment: &str,
    body: impl FnOnce(&mut FunctionContext<'_>) -> Result<()>,
) -> Result<()> {
    let function = helper_function(label, return_php_type);
    // Shared helpers own no cleanup-tracked locals, so an exception can skip
    // their synthetic frame and unwind through the caller's activation record.
    let layout = frame::layout_for_function(&function, emitter.target, regalloc_linear, false);
    let mut ctx = FunctionContext::new(
        module, &function, emitter, data, shared, layout, false, false, false, None,
    );

    ctx.emitter.blank();
    ctx.emitter.comment(comment);
    ctx.emitter.label(label);
    emit_helper_entry(&mut ctx);
    body(&mut ctx)?;
    emit_helper_exit(&mut ctx);
    Ok(())
}

/// Builds the minimal EIR function a `FunctionContext` needs to exist.
fn helper_function(label: &str, return_php_type: PhpType) -> Function {
    let return_ir_type = match return_php_type {
        PhpType::Str => IrType::Str,
        _ => IrType::Void,
    };
    let mut function = Function::new(label.to_string(), return_ir_type, return_php_type);
    function.flags.is_synthetic = true;
    function.params.push(FunctionParam {
        name: "value".to_string(),
        ir_type: IrType::Heap(IrHeapKind::Mixed),
        php_type: PhpType::Mixed,
        by_ref: false,
        variadic: false,
    });
    let entry = BlockId::from_raw(0);
    function
        .blocks
        .push(BasicBlock::new(entry, "entry".to_string(), vec![ValueId::from_raw(0)]));
    function.values.push(Value {
        ir_type: IrType::Heap(IrHeapKind::Mixed),
        php_type: PhpType::Mixed,
        def: ValueDef::BlockParam { block: entry, index: 0 },
        ownership: Ownership::Borrowed,
    });
    function.entry = entry;
    function
}

/// Saves the link register and the reserved nested-call register.
fn emit_helper_entry(ctx: &mut FunctionContext<'_>) {
    let nested = abi::nested_call_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("stp x29, x30, [sp, #-16]!");              // save frame pointer and return address
            ctx.emitter.instruction("mov x29, sp");                            // establish the helper frame pointer
            ctx.emitter.instruction(&format!("str {}, [sp, #-16]!", nested));   // preserve the caller's nested-call register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("push rbp");                                // preserve the caller frame pointer
            ctx.emitter.instruction("mov rbp, rsp");                            // establish the helper frame pointer
            ctx.emitter.instruction(&format!("push {}", nested));               // preserve the caller's nested-call register
            ctx.emitter.instruction("sub rsp, 8");                              // restore 16-byte alignment before nested calls
        }
    }
}

/// Restores what `emit_helper_entry` saved and returns, leaving the result untouched.
fn emit_helper_exit(ctx: &mut FunctionContext<'_>) {
    let nested = abi::nested_call_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr {}, [sp], #16", nested));      // restore the caller's nested-call register
            ctx.emitter.instruction("ldp x29, x30, [sp], #16");                 // restore frame pointer and return address
            ctx.emitter.instruction("ret");                                     // return whatever the body left in the result registers
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add rsp, 8");                               // release the alignment padding
            ctx.emitter.instruction(&format!("pop {}", nested));                 // restore the caller's nested-call register
            ctx.emitter.instruction("pop rbp");                                  // restore the caller frame pointer
            ctx.emitter.instruction("ret");                                      // return whatever the body left in the result registers
        }
    }
}
