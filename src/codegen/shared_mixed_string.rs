//! Purpose:
//! Emits the shared `Mixed` string-context helpers, so the per-class `__toString`
//! dispatch ladder exists once per program instead of once per string context.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()` before the module's own functions.
//!
//! Key details:
//! - A boxed `Mixed` reaching a string context has to find `__toString` on whatever
//!   class it turns out to hold, and the ladder that does so carries one arm per
//!   candidate class. It was emitted inline at every site: measured on
//!   `examples/iterators`, 12 sites of 592 identical lines, 19.3% of the emitted
//!   assembly. The identical length is what proves it was a verbatim copy.
//! - The ladder is MOVED, not replaced. Its arms remain the same code path, so what
//!   can go wrong here is structural — a bad register or a missing symbol — and not a
//!   silently different dispatch. A dense `class_id -> __toString` table was rejected
//!   for that reason: vtable slots are numbered per class, in declaration order, which
//!   is exactly why the ladder exists.
//! - The helpers take the boxed pointer in the INT RESULT register, not an argument
//!   register, because that is where the inlined sequence already had it.

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

use super::lower_inst::{emit_mixed_string_dispatch_from_result, MixedStringContextMode};
use super::Result;

/// Label of the helper that leaves the string result of a boxed `Mixed`.
pub(super) const MIXED_TO_STRING_LABEL: &str = "_eir_shared_mixed_to_string";
/// Label of the helper that writes a boxed `Mixed` to stdout.
pub(super) const MIXED_ECHO_LABEL: &str = "_eir_shared_mixed_echo";

/// Returns whether this module shares the ladder for one string-context mode.
///
/// TWO conditions, and the second is not optional. A module with no candidate class has
/// a ladder that is only the conversion fallback plus a fatal, so routing it through a
/// call costs more than it saves. And a mode reached from a SINGLE site pays for a whole
/// helper body to remove one copy of it: measured before this condition existed,
/// `fizzbuzz` grew 13 588 -> 15 528 bytes of `__text` and `reflection` 366 984 -> 370 036,
/// while the many-site programs fell by 16-19%.
///
/// The emitter and the call sites both ask THIS function, so they cannot disagree about
/// whether a helper exists — a disagreement would either leave an unresolved label or
/// emit a body nothing calls.
pub(super) fn module_shares_mixed_string_ladder(
    module: &Module,
    mode: &MixedStringContextMode,
    shared: &mut SharedCodegenState,
) -> bool {
    let mode_index = match mode {
        MixedStringContextMode::Result => 0,
        MixedStringContextMode::Stdout => 1,
    };
    if let Some(cached) = shared.mixed_string_sharing(mode_index) {
        return cached;
    }
    let shares =
        module_has_tostring_candidate(module) && mixed_string_site_count(module, mode) >= 2;
    shared.set_mixed_string_sharing(mode_index, shares);
    shares
}

/// Returns whether any class publishes a no-argument `__toString`, the arms' source.
fn module_has_tostring_candidate(module: &Module) -> bool {
    let method_key = crate::names::php_symbol_key("__toString");
    module.class_infos.values().any(|class_info| {
        class_info
            .methods
            .get(&method_key)
            .is_some_and(|signature| signature.params.is_empty())
    })
}

/// Counts the string contexts of one mode across every body this module emits.
///
/// Over-counting only wastes a helper body; under-counting only keeps a site inline.
/// Neither can produce a call to a label that was not emitted, because the routing
/// decision reads the same count.
fn mixed_string_site_count(module: &Module, mode: &MixedStringContextMode) -> usize {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .map(|function| {
            function
                .instructions
                .iter()
                .filter(|inst| instruction_is_mixed_string_site(function, inst, mode))
                .count()
        })
        .sum()
}

/// Returns whether an instruction reaches the `__toString` ladder in either mode.
///
/// The ladder holds its receiver in the reserved nested-call register, so a function
/// containing one of these must reserve a save slot for it — see
/// `frame::function_uses_nested_call_reg`, whose own test only knows about Mixed-receiver
/// METHOD calls and therefore missed every string context.
pub(super) fn instruction_uses_mixed_string_ladder(
    function: &Function,
    inst: &crate::ir::Instruction,
) -> bool {
    instruction_is_mixed_string_site(function, inst, &MixedStringContextMode::Result)
        || instruction_is_mixed_string_site(function, inst, &MixedStringContextMode::Stdout)
}

/// Returns whether one instruction is a boxed-Mixed string context of the given mode.
fn instruction_is_mixed_string_site(
    function: &Function,
    inst: &crate::ir::Instruction,
    mode: &MixedStringContextMode,
) -> bool {
    let matches_op = match mode {
        MixedStringContextMode::Result => {
            inst.op == crate::ir::Op::Cast
                && inst.immediate == Some(crate::ir::Immediate::CastTarget(IrType::Str))
        }
        MixedStringContextMode::Stdout => inst.op == crate::ir::Op::EchoValue,
    };
    if !matches_op {
        return false;
    }
    inst.operands.first().is_some_and(|operand| {
        function.value(*operand).is_some_and(|value| {
            matches!(
                value.php_type.codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            )
        })
    })
}

/// Returns the helper label a string context in `ctx` should call, if any.
///
/// `None` inside a helper itself, which is what stops the body from calling into the
/// label it is defining.
pub(super) fn shared_ladder_label(
    ctx: &mut FunctionContext<'_>,
    mode: &MixedStringContextMode,
) -> Option<&'static str> {
    let label = match mode {
        MixedStringContextMode::Result => MIXED_TO_STRING_LABEL,
        MixedStringContextMode::Stdout => MIXED_ECHO_LABEL,
    };
    // Checked before the module scan: inside a helper there is nothing to decide, and this is
    // the one context where the answer must be "inline" whatever the module says.
    if ctx.function.name == label {
        return None;
    }
    let module = ctx.module;
    if !module_shares_mixed_string_ladder(module, mode, ctx.shared) {
        return None;
    }
    Some(label)
}

/// Emits both shared helpers when the module uses them.
pub(super) fn emit_shared_mixed_string_helpers(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    if module_shares_mixed_string_ladder(module, &MixedStringContextMode::Result, shared) {
        emit_one_helper(
            module,
            emitter,
            data,
            shared,
            regalloc_linear,
            MIXED_TO_STRING_LABEL,
            MixedStringContextMode::Result,
            PhpType::Str,
        )?;
    }
    if module_shares_mixed_string_ladder(module, &MixedStringContextMode::Stdout, shared) {
        emit_one_helper(
            module,
            emitter,
            data,
            shared,
            regalloc_linear,
            MIXED_ECHO_LABEL,
            MixedStringContextMode::Stdout,
            PhpType::Void,
        )?;
    }
    Ok(())
}

/// Emits one helper: frame, the shared dispatch, and the matching return.
fn emit_one_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
    label: &str,
    mode: MixedStringContextMode,
    return_php_type: PhpType,
) -> Result<()> {
    let function = helper_function(label, return_php_type);
    let layout = frame::layout_for_function(&function, emitter.target, regalloc_linear);
    let mut ctx = FunctionContext::new(
        module, &function, emitter, data, shared, layout, false, false, false, None,
    );

    ctx.emitter.blank();
    ctx.emitter.comment(&format!("--- shared mixed string context: {} ---", label));
    ctx.emitter.label(label);
    emit_helper_entry(&mut ctx);
    // The boxed pointer already sits in the int result register, which is where the
    // inlined sequence expected it, so the dispatch needs no argument shuffling.
    emit_mixed_string_dispatch_from_result(&mut ctx, helper_value(), mode)?;
    emit_helper_exit(&mut ctx);
    Ok(())
}

/// The SSA value standing in for the helper's parameter.
///
/// `__toString` takes no arguments, so the receiver-register materializer skips this
/// operand entirely (`operands.iter().skip(1)`, and the by-reference planner ignores a
/// non-reference parameter). It exists to satisfy the shared signature, and is never
/// loaded — which is what makes moving the ladder out of its original frame possible.
fn helper_value() -> ValueId {
    ValueId::from_raw(0)
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
///
/// The ladder holds the unboxed receiver in the nested-call register across the
/// `__toString` call. That register is callee-saved and outside the allocator's
/// tracking, and the frame layout only reserves a save slot for functions containing a
/// Mixed-receiver METHOD call — a string context is neither, so the helper saves it
/// itself rather than relying on its caller having done so.
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
            ctx.emitter.instruction("ret");                                     // return the string result in the string pair
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add rsp, 8");                               // release the alignment padding
            ctx.emitter.instruction(&format!("pop {}", nested));                 // restore the caller's nested-call register
            ctx.emitter.instruction("pop rbp");                                  // restore the caller frame pointer
            ctx.emitter.instruction("ret");                                      // return the string result in the string pair
        }
    }
}
