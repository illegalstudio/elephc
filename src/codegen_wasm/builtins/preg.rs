//! Purpose:
//! Lowering and shape predicate for `preg_match` on the WASM regex engine.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.

use super::*;

/// Validates `preg_match($pattern, $subject)` and COMPILES the pattern.
///
/// This gate does more than check shapes: it runs the pattern through the compiler and
/// reports whatever that refuses. The whole engine rests on it. `codegen_wasm::regex`
/// implements a subset of PCRE, and a subset is only honest if the constructs outside it are
/// refused at COMPILE time with their names — a runtime that quietly mismatched a
/// backreference would be worse than no engine at all.
///
/// `$matches` is refused rather than ignored, for the same reason. Every other unimplemented
/// feature announces itself in the pattern; an untouched `$matches` array would not.
pub(super) fn preg_match_shape_issue(
    function: &Function,
    module: &Module,
    call: &Instruction,
) -> Option<String> {
    if call.operands.len() > 2 {
        return Some(
            "preg_match's $matches is not filled by this target. Every other unsupported \
             construct is visible in the pattern; a capture that silently never arrives is \
             not, so the call is refused rather than answered without it"
                .to_string(),
        );
    }
    let [pattern, subject] = call.operands.as_slice() else {
        return Some(format!(
            "preg_match takes a pattern and a subject, got {} operands",
            call.operands.len()
        ));
    };
    for (value, what) in [(pattern, "pattern"), (subject, "subject")] {
        let Some(value) = function.value(*value) else {
            return Some(format!("the {what} is missing from the value table"));
        };
        if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
            return Some(format!(
                "preg_match's {what} is {:?}/{:?}, not a string",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    let Some(bytes) = sprintf_literal_format(function, module, *pattern) else {
        return Some(
            "only a LITERAL pattern is compiled; a computed one would need the whole PCRE \
             parser at runtime, which this target does not carry"
                .to_string(),
        );
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return Some("the pattern is not valid UTF-8".to_string());
    };
    if let Err(why) = crate::codegen_wasm::regex::compile(&text) {
        return Some(format!("preg_match pattern {text:?}: {why}"));
    }
    if call.result_type != IrType::I64 {
        return Some(format!(
            "preg_match answers an int, not {:?}",
            call.result_type
        ));
    }
    None
}

/// Lowers `preg_match()` onto the pattern VM.
///
/// The pattern itself never reaches the module: what the planner laid out is the compiled
/// PROGRAM, and this passes its address. Two call sites with the same pattern share it.
pub(super) fn lower_preg_match(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let pattern = operand(inst, 0)?;
    let bytes = sprintf_literal_format(ctx.function, ctx.module, pattern)
        .ok_or_else(|| WasmError::Unsupported("preg_match pattern is not a literal".into()))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| WasmError::Unsupported("preg_match pattern is not valid UTF-8".into()))?;
    let &(program, classes) = ctx.regex_programs.get(&text).ok_or_else(|| {
        WasmError::Unsupported(format!("preg_match pattern {text:?} was never laid out"))
    })?;
    ctx.fb
        .ins(&format!("i32.const {program}"), "the compiled program");
    ctx.fb
        .ins(&format!("i32.const {classes}"), "its class bitmaps");
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb
        .ins("call $__rt_preg_match", "the backtracking VM");
    store_result(ctx, inst)
}
