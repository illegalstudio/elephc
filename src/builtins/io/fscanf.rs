//! Purpose:
//! Home of the PHP `fscanf` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` calls `ensure_stream_resource` on the stream argument for validation and returns
//!   php's 2-argument result type, `array|false|null`: an entry per non-suppressed conversion,
//!   `false` once the stream is at end of file, and `null` for a line that ran out of input
//!   before assigning anything — an EMPTY line reaches `null`, not `false`. `returns: Mixed`
//!   stays in the contract because the macro's scalar `returns:` field cannot express it.
//! - The scan itself is `__elephc_fscanf` in `crate::scanf_prelude`, which reads ONE LINE with
//!   `fgets()` — newline included, as php's `php_stream_get_line` does — and hands it to the
//!   shared engine. `sscanf()` lowers to the same engine, so the two cannot drift.
//! - The by-ref `$vars` output form answers an `int|false` instead: php assigns each field
//!   through the reference and returns the number of conversions that consumed input, or `false`
//!   for a stream already at end of file. It lowers to one prelude wrapper per arity, and the
//!   contract's `variadic_writes` is what lets a caller pass variables that do not exist yet —
//!   `fscanf($h, '%s %d', $name, $age)` is the manual's own idiom.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport,
    BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

/// The elephc-PHP prelude function that reads one line and scans it.
const FSCANF_ENGINE_FUNCTION: &str = "__elephc_fscanf";

builtin! {
    contract: "fscanf",
    check: check,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::CheckerHook { check, lazy: false },
        result_type: BuiltinResultType::Checked,
        effects: BuiltinEffects::Shared(crate::builtins::string::sscanf::engine_call_effects),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "fscanf is scanned by an injected prelude function, which a runtime-selected callable cannot reach",
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Validates the stream argument and returns php's `array|false|null` result type.
///
/// The `$vars` form changes the shape: php answers the conversion COUNT rather than the array,
/// and `false` only for a stream already at end of file.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    crate::builtins::string::sscanf::check_vars_arity(cx.name, cx.args.len(), cx.span)?;
    Ok(fscanf_result_type(cx.args.len()))
}

/// Returns php's `fscanf` result type for the given argument count.
///
/// `false` is in both shapes — a stream already at end of file answers it either way. What the
/// arity changes is the rest: without `$vars` php answers the matched-field array, or `null` for
/// a line that ran out before assigning; with them it fills each one and answers the number of
/// conversions that consumed input.
///
/// Kept out of the check hook for the reason its `sscanf()` counterpart is: the generated-docs
/// extractor reads a hook's body for a return type more precise than the contract's `mixed`.
fn fscanf_result_type(arg_count: usize) -> PhpType {
    if arg_count > 2 {
        return PhpType::Union(vec![PhpType::Int, PhpType::False]);
    }
    PhpType::Union(vec![
        PhpType::Array(Box::new(PhpType::Mixed)),
        PhpType::False,
        PhpType::Void,
    ])
}

/// Lowers `fscanf(stream, format)` to a direct call into the injected scanf prelude.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    crate::builtins::string::sscanf::lower_scanf_engine_call(ctx, call, FSCANF_ENGINE_FUNCTION)
}
