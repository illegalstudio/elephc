//! Purpose:
//! Lowering and shape predicate for the zlib builtins.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.

use super::*;

/// Validates the four zlib builtins: one string in, a string or a boxed union out.
///
/// php's optional `$level` and `$encoding` arguments are refused rather than ignored. The
/// level would ask for a compression ratio this compressor does not offer choices about — it
/// emits one fixed-Huffman block whatever it is told — and silently accepting it would promise
/// a tuning that does nothing.
pub(super) fn zlib_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [subject] = call.operands.as_slice() else {
        return Some(format!(
            "{target:?} takes one string, got {} operands — this target has no compression \
             level to set, so php's optional arguments are refused rather than ignored",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*subject) else {
        return Some("operand is missing from the value table".to_string());
    };
    if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
        return Some(format!(
            "{target:?} takes a string, not {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    // The two compressors always succeed and answer a bare string; the two decompressors can
    // fail, and php's `false` needs the boxed union to be distinguishable from `""`.
    let decompresses = matches!(target, RuntimeFnId::Gzinflate | RuntimeFnId::Gzuncompress);
    let want = if decompresses {
        IrType::Heap(IrHeapKind::Mixed)
    } else {
        IrType::Str
    };
    if call.result.is_none() || call.result_type != want {
        return Some(format!(
            "{target:?} answers {want:?}, not {:?}",
            call.result_type
        ));
    }
    None
}

/// Lowers the four zlib builtins onto the codec.
pub(super) fn lower_zlib(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(
        match target {
            RuntimeFnId::Gzdeflate => "call $__rt_gzdeflate",
            RuntimeFnId::Gzinflate => "call $__rt_gzinflate",
            RuntimeFnId::Gzcompress => "call $__rt_gzcompress",
            _ => "call $__rt_gzuncompress",
        },
        "RFC 1951, written in the module",
    );
    store_result(ctx, inst)
}
