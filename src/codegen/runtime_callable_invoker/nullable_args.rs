//! Purpose:
//! Converts runtime callable arguments to the payload/tag ABI of nullable integers.
//!
//! Called from:
//! - The runtime callable invoker's scalar coercion dispatcher.
//!
//! Key details:
//! - Missing and explicit nulls keep tag 8; present scalar values become tagged integers.
//! - Temporary boxes introduced for string conversion are released without losing the tag.

use super::*;

/// Produces the two result words required by an int|null entry parameter.
pub(super) fn coerce_to_tagged_scalar(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
    source: &PhpType,
) {
    match source.codegen_repr() {
        PhpType::TaggedScalar => {}
        PhpType::Void | PhpType::Never => crate::codegen::sentinels::emit_tagged_scalar_null(emitter),
        PhpType::Mixed | PhpType::Union(_) => mixed_to_tagged_scalar(emitter, ctx),
        PhpType::Str => {
            emit_box_current_value_as_mixed(emitter, source);
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
            mixed_to_tagged_scalar(emitter, ctx);
            release_preserved_mixed_after_arg_coercion(emitter, &PhpType::TaggedScalar);
        }
        _ => {
            coerce_result_to_type(emitter, ctx, data, source, &PhpType::Int);
            crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
        }
    }
}

/// Reads null before integer coercion, preserving the borrowed input cell across helper calls.
fn mixed_to_tagged_scalar(emitter: &mut Emitter, ctx: &mut InvokerEmitContext) {
    let null = ctx.next_label("nullable_integer.null");
    let done = ctx.next_label("nullable_integer.done");
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    let compare = match emitter.target.arch {
        Arch::AArch64 => format!("cmp {result}, #8"),
        Arch::X86_64 => format!("cmp {result}, 8"),
    };
    let branch = match emitter.target.arch {
        Arch::AArch64 => format!("b.eq {null}"),
        Arch::X86_64 => format!("je {null}"),
    };
    emitter.instruction(&compare);                                              // test the dereferenced PHP tag before null can be cast to zero
    emitter.instruction(&branch);                                               // keep explicit null distinct from an integer zero argument
    abi::emit_load_temporary_stack_slot(emitter, result, 0);
    abi::emit_call_label(emitter, "__rt_mixed_cast_int");
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
    abi::emit_jump(emitter, &done);
    emitter.label(&null);
    crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
    emitter.label(&done);
    abi::emit_release_temporary_stack(emitter, 16);
}
