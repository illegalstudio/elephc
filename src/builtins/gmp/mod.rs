//! Purpose:
//! Joins the shared GMP contracts to the AOT implementation: one generic EIR lowering
//! calls the matching `__elephc_gmp_*` prelude helper, and small result-kind constructors
//! give each home file the right checker-facing type and ownership.
//!
//! Called from:
//! - Each `crate::builtins::gmp::*` home file through `builtin!`.
//! - Reachability's call scan through [`prelude_helpers_for`].
//!
//! Key details:
//! - Operands are boxed to `Mixed` before the helper call so PHP 8.5's coercions and
//!   diagnostics live in one PHP implementation shared by AOT and eval.
//! - The compiler-owned strict-types flag is the helper's first argument, so weak and
//!   strict call sites keep their caller-file semantics.
//! - A helper call is a fresh result: GMP objects, strings, arrays and false do not
//!   alias the argument cells.

use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering,
    BuiltinLoweringContext, BuiltinLoweringError, BuiltinRequirement, BuiltinRequirements,
    BuiltinResultOwnership, BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemanticInput,
    BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport, BuiltinValidation,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::errors::CompileError;
use crate::ir::{Effects, Immediate, Op};
use crate::types::PhpType;
use elephc_builtin_contract::DefaultSpec;

pub mod gmp_abs;
pub mod gmp_add;
pub mod gmp_and;
pub mod gmp_binomial;
pub mod gmp_clrbit;
pub mod gmp_cmp;
pub mod gmp_com;
pub mod gmp_div;
pub mod gmp_div_q;
pub mod gmp_div_qr;
pub mod gmp_div_r;
pub mod gmp_divexact;
pub mod gmp_export;
pub mod gmp_fact;
pub mod gmp_gcd;
pub mod gmp_gcdext;
pub mod gmp_hamdist;
pub mod gmp_import;
pub mod gmp_init;
pub mod gmp_intval;
pub mod gmp_invert;
pub mod gmp_jacobi;
pub mod gmp_kronecker;
pub mod gmp_lcm;
pub mod gmp_legendre;
pub mod gmp_mod;
pub mod gmp_mul;
pub mod gmp_neg;
pub mod gmp_nextprime;
pub mod gmp_or;
pub mod gmp_perfect_power;
pub mod gmp_perfect_square;
pub mod gmp_popcount;
pub mod gmp_pow;
pub mod gmp_powm;
pub mod gmp_prob_prime;
pub mod gmp_random_bits;
pub mod gmp_random_range;
pub mod gmp_random_seed;
pub mod gmp_root;
pub mod gmp_rootrem;
pub mod gmp_scan0;
pub mod gmp_scan1;
pub mod gmp_setbit;
pub mod gmp_sign;
pub mod gmp_sqrt;
pub mod gmp_sqrtrem;
pub mod gmp_strval;
pub mod gmp_sub;
pub mod gmp_testbit;
pub mod gmp_xor;

/// The bridge required by every GMP builtin.
const GMP_REQUIREMENTS: &[BuiltinRequirement] =
    &[BuiltinRequirement::Bridge("elephc_gmp")];

/// Every prelude helper a GMP program may need.
///
/// Keeping the whole helper set for any GMP call is deliberate: the prelude is small,
/// and the helpers are called from EIR lowering rather than AST call sites, so a
/// per-name table would add a second place to forget when a function is added.
pub(crate) const HELPERS: &[&str] = &[
    "__elephc_gmp_abs",
    "__elephc_gmp_add",
    "__elephc_gmp_and",
    "__elephc_gmp_binomial",
    "__elephc_gmp_clrbit",
    "__elephc_gmp_cmp",
    "__elephc_gmp_com",
    "__elephc_gmp_div",
    "__elephc_gmp_div_q",
    "__elephc_gmp_div_qr",
    "__elephc_gmp_div_r",
    "__elephc_gmp_divexact",
    "__elephc_gmp_export",
    "__elephc_gmp_fact",
    "__elephc_gmp_gcd",
    "__elephc_gmp_gcdext",
    "__elephc_gmp_hamdist",
    "__elephc_gmp_import",
    "__elephc_gmp_init",
    "__elephc_gmp_intval",
    "__elephc_gmp_intval_cast",
    "__elephc_gmp_intval_cast_string",
    "__elephc_gmp_invert",
    "__elephc_gmp_jacobi",
    "__elephc_gmp_kronecker",
    "__elephc_gmp_lcm",
    "__elephc_gmp_legendre",
    "__elephc_gmp_mod",
    "__elephc_gmp_mul",
    "__elephc_gmp_neg",
    "__elephc_gmp_nextprime",
    "__elephc_gmp_or",
    "__elephc_gmp_perfect_power",
    "__elephc_gmp_perfect_square",
    "__elephc_gmp_popcount",
    "__elephc_gmp_pow",
    "__elephc_gmp_pow_operator",
    "__elephc_gmp_minmax_array",
    "__elephc_gmp_floatval_cast",
    "__elephc_gmp_powm",
    "__elephc_gmp_prob_prime",
    "__elephc_gmp_random_bits",
    "__elephc_gmp_random_range",
    "__elephc_gmp_random_seed",
    "__elephc_gmp_root",
    "__elephc_gmp_rootrem",
    "__elephc_gmp_scan0",
    "__elephc_gmp_scan1",
    "__elephc_gmp_setbit",
    "__elephc_gmp_sign",
    "__elephc_gmp_sqrt",
    "__elephc_gmp_sqrtrem",
    "__elephc_gmp_swallow_error",
    "__elephc_gmp_strval",
    "__elephc_gmp_sub",
    "__elephc_gmp_testbit",
    "__elephc_gmp_unserialize",
    "__elephc_gmp_xor",
];

/// Returns every prelude helper to root when `name` is a GMP builtin.
pub(crate) fn prelude_helpers_for(name: &str) -> impl Iterator<Item = &'static str> {
    let is_gmp = elephc_builtin_contract::lookup(name)
        .is_some_and(|contract| contract.module == elephc_builtin_contract::PhpModule::Gmp);
    HELPERS.iter().copied().filter(move |_| is_gmp)
}

/// Builds the shared descriptor for one result family.
const fn semantics_for(
    result_type: crate::builtins::semantics::ResultTypeFn,
    ownership: BuiltinResultOwnership,
) -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::Shared(accept_any_arguments),
        result_type: BuiltinResultType::Shared(result_type),
        effects: BuiltinEffects::Static(Effects::all()),
        result_ownership: ownership,
        requirements: BuiltinRequirements::Static(GMP_REQUIREMENTS),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::PreserveUncoerced,
        callable: BuiltinCallablePolicy::Dynamic(
            crate::builtins::semantics::callable_accepts_any_source,
        ),
        lowering: BuiltinLowering::Eir(lower),
    }
}

/// Defers every operand diagnostic to the PHP helper, which implements php-src's rules.
fn accept_any_arguments(_input: &BuiltinSemanticInput<'_>) -> Result<(), CompileError> {
    Ok(())
}

/// Resolves a fresh GMP object.
fn object_result(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Object("GMP".to_string())
}

/// Resolves a GMP-or-false result.
fn invert_result(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Union(vec![PhpType::Object("GMP".to_string()), PhpType::False])
}

/// Resolves an indexed array returned by the generic prelude helper.
///
/// The helper's declared PHP type is `array`, so its element representation is the
/// boxed `Mixed` storage even though every element happens to be a `GMP` object.
/// Narrowing the EIR result to `array<GMP>` would tell codegen to read raw object
/// pointers out of boxed cells.
fn indexed_gmp_array_result(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Array(Box::new(PhpType::Mixed))
}

/// Resolves `gmp_gcdext()`'s string-keyed generic array.
fn keyed_gmp_array_result(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    }
}

/// Resolves a scalar integer result.
fn int_result(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Int
}

/// Resolves a scalar boolean result.
fn bool_result(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Bool
}

/// Resolves a string result.
fn string_result(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Str
}

/// Resolves a void result.
fn void_result(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Void
}

/// Semantics for a fresh GMP object.
pub(crate) const fn object_semantics() -> BuiltinSemantics {
    semantics_for(object_result, BuiltinResultOwnership::Fresh)
}

/// Semantics for the GMP-or-false invert result.
pub(crate) const fn invert_semantics() -> BuiltinSemantics {
    semantics_for(invert_result, BuiltinResultOwnership::Fresh)
}

/// Semantics for an indexed array of fresh GMP objects.
pub(crate) const fn indexed_array_semantics() -> BuiltinSemantics {
    semantics_for(indexed_gmp_array_result, BuiltinResultOwnership::Fresh)
}

/// Semantics for `gmp_gcdext()`'s keyed array.
pub(crate) const fn keyed_array_semantics() -> BuiltinSemantics {
    semantics_for(keyed_gmp_array_result, BuiltinResultOwnership::Fresh)
}

/// Semantics for a non-heap integer result.
pub(crate) const fn int_semantics() -> BuiltinSemantics {
    semantics_for(int_result, BuiltinResultOwnership::NonHeap)
}

/// Semantics for a non-heap boolean result.
pub(crate) const fn bool_semantics() -> BuiltinSemantics {
    semantics_for(bool_result, BuiltinResultOwnership::NonHeap)
}

/// Semantics for a fresh string result.
pub(crate) const fn string_semantics() -> BuiltinSemantics {
    semantics_for(string_result, BuiltinResultOwnership::Fresh)
}

/// Semantics for a void result.
pub(crate) const fn void_semantics() -> BuiltinSemantics {
    semantics_for(void_result, BuiltinResultOwnership::NonHeap)
}

/// Lowers one public GMP builtin by calling its prelude helper with boxed raw operands.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let strict = ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(crate::source::current_strict_types())),
        PhpType::Bool,
        Op::ConstBool.default_effects(),
        Some(call.span),
    );
    let contract = elephc_builtin_contract::lookup(call.name)
        .ok_or_else(|| BuiltinLoweringError::new(format!(
            "{} has no shared GMP contract", call.name
        )))?;
    let mut supplied = call.operands.to_vec();
    // `emit_user_call` requires the callee's full ABI. Materialize every omitted
    // trailing default ourselves while keeping the supplied values uncoerced.
    for index in supplied.len()..contract.params.len() {
        let default = contract.params[index].default.ok_or_else(|| {
            BuiltinLoweringError::new(format!(
                "{} lowering is missing required operand {index}", call.name
            ))
        })?;
        supplied.push(emit_default(ctx, default, call.span)?);
    }
    let mut operands = Vec::with_capacity(supplied.len() + 1);
    operands.push(strict.value);
    let mut boxes = Vec::new();
    for operand in supplied {
        let representation = ctx.value_php_type(operand).codegen_repr();
        if matches!(representation, PhpType::Mixed | PhpType::Union(_)) {
            operands.push(operand);
            continue;
        }
        let boxed = ctx.emit_value(
            Op::MixedBox,
            vec![operand],
            None,
            PhpType::Mixed,
            Op::MixedBox.default_effects(),
            Some(call.span),
        );
        boxes.push(boxed.value);
        operands.push(boxed.value);
    }
    let helper = format!("__elephc_gmp_{}", call.name.trim_start_matches("gmp_"));
    let result = ctx.emit_user_call(&helper, operands, call.result_type.clone(), Some(call.span));
    for boxed in boxes {
        ctx.emit_void(
            Op::Release,
            vec![boxed],
            None,
            Op::Release.default_effects(),
            Some(call.span),
        );
    }
    Ok(result)
}

/// Emits one omitted optional argument's default value for the helper ABI.
fn emit_default(
    ctx: &mut dyn BuiltinLoweringContext,
    default: DefaultSpec,
    span: crate::span::Span,
) -> Result<crate::ir::ValueId, BuiltinLoweringError> {
    let (op, immediate, php_type) = match default {
        DefaultSpec::Null => (Op::ConstNull, None, PhpType::Void),
        DefaultSpec::Int(value) => (Op::ConstI64, Some(Immediate::I64(value)), PhpType::Int),
        DefaultSpec::IntMax => (
            Op::ConstI64,
            Some(Immediate::I64(i64::MAX)),
            PhpType::Int,
        ),
        DefaultSpec::Bool(value) => (Op::ConstBool, Some(Immediate::Bool(value)), PhpType::Bool),
        DefaultSpec::Float(value) => (Op::ConstF64, Some(Immediate::F64(value)), PhpType::Float),
        DefaultSpec::Str(value) => {
            let data = ctx.intern_string(value);
            (Op::ConstStr, Some(Immediate::Data(data)), PhpType::Str)
        }
        other => {
            return Err(BuiltinLoweringError::new(format!(
                "unsupported GMP helper default {other:?}"
            )))
        }
    };
    Ok(ctx.emit_value(
        op,
        Vec::new(),
        immediate,
        php_type,
        op.default_effects(),
        Some(span),
    ).value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every contract has exactly one reachability helper.
    #[test]
    fn every_gmp_contract_has_a_helper_root() {
        for name in elephc_builtin_contract::GMP_FUNCTIONS {
            let helpers: Vec<_> = prelude_helpers_for(name).collect();
            assert_eq!(helpers.len(), HELPERS.len(), "{name} must root every helper");
        }
        assert!(prelude_helpers_for("strlen").next().is_none());
    }

    /// Verifies result families stay distinct, especially invert and the keyed array.
    #[test]
    fn result_type_families_are_distinct() {
        let empty = BuiltinSemanticInput {
            name: "gmp_add",
            args: &[],
            arg_types: &[],
            span: crate::span::Span::dummy(),
        };
        assert_eq!(object_result(&empty), PhpType::Object("GMP".into()));
        assert_eq!(int_result(&empty), PhpType::Int);
        assert_eq!(bool_result(&empty), PhpType::Bool);
        assert_eq!(string_result(&empty), PhpType::Str);
        assert_eq!(void_result(&empty), PhpType::Void);
        assert!(matches!(invert_result(&empty), PhpType::Union(_)));
        assert!(matches!(indexed_gmp_array_result(&empty), PhpType::Array(_)));
        assert!(matches!(keyed_gmp_array_result(&empty), PhpType::AssocArray { .. }));
    }
}
