//! Purpose:
//! Lowers `CallableReflectionMeta`: the reflection facts `ReflectionFunction` reads from a
//! Closure's runtime descriptor (`isAnonymous()` and the `getClosureScopeClass()` name).
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` for `Op::CallableReflectionMeta`.
//!
//! Key details:
//! - The descriptor's invocation record (`CALLABLE_DESC_INVOCATION_OFFSET`) stores the callable
//!   shape and, in its auxiliary name slot, the PHP scope class recorded when the descriptor was
//!   emitted (the lexical class of a closure literal, the declaring class of a method).
//! - A boxed operand must hold a callable descriptor; anything else throws `Error`.
//! - A shape that cannot answer (an eval callback adapter, a late-bound `static::m(...)` whose
//!   scope is only known per call) throws `Error` instead of reporting a guessed answer.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_support::callable_descriptor::{
    CallableDescriptorShape, CALLABLE_DESC_INVOCATION_OFFSET,
};
use crate::ir::{Immediate, Instruction};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};
use crate::codegen::{CodegenIrError, Result};

/// `CallableReflectionMeta` immediate selecting `ReflectionFunction::isAnonymous()`.
pub(crate) const CALLABLE_META_IS_ANONYMOUS: i64 = 0;
/// `CallableReflectionMeta` immediate selecting the `getClosureScopeClass()` name.
pub(crate) const CALLABLE_META_SCOPE_CLASS: i64 = 1;

/// Runtime boxed tag of a callable descriptor payload.
const MIXED_TAG_CALLABLE: i64 = 10;

/// Invocation-record word offsets (see `callable_descriptor::invocation_record`).
const INVOCATION_SHAPE_OFFSET: usize = 0;
const INVOCATION_AUX_PTR_OFFSET: usize = 40;
const INVOCATION_AUX_LEN_OFFSET: usize = 48;

const NOT_A_CLOSURE_MESSAGE: &str =
    "ReflectionFunction can only reflect a Closure value here";
const UNKNOWN_SHAPE_MESSAGE: &str =
    "ReflectionFunction cannot reflect this Closure in AOT mode (eval-created or late-bound target)";

/// Lowers one descriptor reflection read into a bool or a borrowed static string.
pub(super) fn lower_callable_reflection_meta(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let field = match inst.immediate {
        Some(Immediate::I64(field))
            if field == CALLABLE_META_IS_ANONYMOUS || field == CALLABLE_META_SCOPE_CLASS =>
        {
            field
        }
        _ => {
            return Err(CodegenIrError::invalid_module(
                "callable_reflection_meta requires a field immediate",
            ))
        }
    };
    let boxed = match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Callable => false,
        PhpType::Mixed | PhpType::Union(_) => true,
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "callable_reflection_meta requires a Closure operand, got {:?}",
                other
            )))
        }
    };
    let not_closure = ctx.next_label("callable_meta_not_closure");
    let unknown = ctx.next_label("callable_meta_unknown");
    let done = ctx.next_label("callable_meta_done");
    let (empty_label, _) = ctx.data.add_string(b"");
    let closure = CallableDescriptorShape::Closure as i64;
    let adapter = CallableDescriptorShape::CallbackAdapter as i64;
    // Shapes whose PHP scope is fixed by the shape itself when no scope name was recorded:
    // a closure literal outside any class, and named functions/builtins/externs.
    let scopeless = [
        CallableDescriptorShape::Closure as i64,
        CallableDescriptorShape::Function as i64,
        CallableDescriptorShape::Builtin as i64,
        CallableDescriptorShape::Extern as i64,
    ];

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(value, "x0")?;
            if boxed {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_CALLABLE)); // only a boxed callable descriptor can be reflected
                ctx.emitter.instruction(&format!("b.ne {}", not_closure));      // reject any other boxed payload
                ctx.emitter.instruction("mov x0, x1");                          // continue with the unboxed descriptor pointer
            }
            abi::emit_load_from_address(ctx.emitter, "x9", "x0", CALLABLE_DESC_INVOCATION_OFFSET);
            ctx.emitter.instruction(&format!("cbz x9, {}", unknown));           // a descriptor without an invocation record cannot answer
            abi::emit_load_from_address(ctx.emitter, "x10", "x9", INVOCATION_SHAPE_OFFSET);
            ctx.emitter.instruction(&format!("cmp x10, #{}", adapter));         // eval callback adapters hide their real target
            ctx.emitter.instruction(&format!("b.eq {}", unknown));              // refuse to guess an eval target's reflection facts
            if field == CALLABLE_META_IS_ANONYMOUS {
                ctx.emitter.instruction(&format!("cmp x10, #{}", closure));     // only closure literals are anonymous
                ctx.emitter.instruction("cset x0, eq");                         // materialize isAnonymous() as a bool
                ctx.emitter.instruction(&format!("b {}", done));                // finish the anonymity read
            } else {
                let no_aux = ctx.next_label("callable_meta_no_scope_name");
                let scopeless_label = ctx.next_label("callable_meta_scopeless");
                abi::emit_load_from_address(ctx.emitter, "x2", "x9", INVOCATION_AUX_LEN_OFFSET);
                ctx.emitter.instruction(&format!("cbz x2, {}", no_aux));        // no recorded scope name: decide from the shape
                abi::emit_load_from_address(ctx.emitter, "x1", "x9", INVOCATION_AUX_PTR_OFFSET);
                ctx.emitter.instruction(&format!("b {}", done));                // return the recorded scope class name
                ctx.emitter.label(&no_aux);
                for shape in scopeless {
                    ctx.emitter.instruction(&format!("cmp x10, #{}", shape));   // test a shape that has no class scope
                    ctx.emitter.instruction(&format!("b.eq {}", scopeless_label)); // report no scope for it
                }
                ctx.emitter.instruction(&format!("b {}", unknown));             // a method shape without a recorded scope cannot answer
                ctx.emitter.label(&scopeless_label);
                abi::emit_symbol_address(ctx.emitter, "x1", &empty_label);
                ctx.emitter.instruction("mov x2, #0");                          // the empty name means no closure scope
                ctx.emitter.instruction(&format!("b {}", done));                // finish the scope read
            }
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(value, "rax")?;
            if boxed {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                ctx.emitter.instruction(&format!("cmp rax, {}", MIXED_TAG_CALLABLE)); // only a boxed callable descriptor can be reflected
                ctx.emitter.instruction(&format!("jne {}", not_closure));       // reject any other boxed payload
                ctx.emitter.instruction("mov rax, rdi");                        // continue with the unboxed descriptor pointer
            }
            abi::emit_load_from_address(ctx.emitter, "r10", "rax", CALLABLE_DESC_INVOCATION_OFFSET);
            ctx.emitter.instruction("test r10, r10");                           // a descriptor without an invocation record cannot answer
            ctx.emitter.instruction(&format!("je {}", unknown));                // refuse to guess
            abi::emit_load_from_address(ctx.emitter, "r11", "r10", INVOCATION_SHAPE_OFFSET);
            ctx.emitter.instruction(&format!("cmp r11, {}", adapter));          // eval callback adapters hide their real target
            ctx.emitter.instruction(&format!("je {}", unknown));                // refuse to guess an eval target's reflection facts
            if field == CALLABLE_META_IS_ANONYMOUS {
                ctx.emitter.instruction("xor eax, eax");                        // clear the bool result before setting its low byte
                ctx.emitter.instruction(&format!("cmp r11, {}", closure));      // only closure literals are anonymous
                ctx.emitter.instruction("sete al");                             // materialize isAnonymous() as a bool
                ctx.emitter.instruction(&format!("jmp {}", done));              // finish the anonymity read
            } else {
                let no_aux = ctx.next_label("callable_meta_no_scope_name");
                let scopeless_label = ctx.next_label("callable_meta_scopeless");
                abi::emit_load_from_address(ctx.emitter, "rdx", "r10", INVOCATION_AUX_LEN_OFFSET);
                ctx.emitter.instruction("test rdx, rdx");                       // no recorded scope name: decide from the shape
                ctx.emitter.instruction(&format!("je {}", no_aux));             // branch to the shape decision
                abi::emit_load_from_address(ctx.emitter, "rax", "r10", INVOCATION_AUX_PTR_OFFSET);
                ctx.emitter.instruction(&format!("jmp {}", done));              // return the recorded scope class name
                ctx.emitter.label(&no_aux);
                for shape in scopeless {
                    ctx.emitter.instruction(&format!("cmp r11, {}", shape));    // test a shape that has no class scope
                    ctx.emitter.instruction(&format!("je {}", scopeless_label)); // report no scope for it
                }
                ctx.emitter.instruction(&format!("jmp {}", unknown));           // a method shape without a recorded scope cannot answer
                ctx.emitter.label(&scopeless_label);
                abi::emit_symbol_address(ctx.emitter, "rax", &empty_label);
                ctx.emitter.instruction("xor edx, edx");                        // the empty name means no closure scope
                ctx.emitter.instruction(&format!("jmp {}", done));              // finish the scope read
            }
        }
    }

    ctx.emitter.label(&not_closure);
    super::exceptions::emit_error(ctx, NOT_A_CLOSURE_MESSAGE);
    ctx.emitter.label(&unknown);
    super::exceptions::emit_error(ctx, UNKNOWN_SHAPE_MESSAGE);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}
