//! Purpose:
//! Home of the registry-backed PHP `ext/intl` procedural builtins.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Each scalar-only Intl entry point has its own `builtin!` home file in this module and
//!   shares one typed EIR lowering (`intl_semantics`).
//! - Object-bearing and by-reference surfaces are synthetic AST declarations (see `crate::intl`);
//!   they do not appear here and never parse PHP source.
//! - The managed `icu` native package owns the C++ runtime and ICU archives.

use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering,
    BuiltinLoweringContext, BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership,
    BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Effects, Immediate, Op};
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::builtins::spec::BuiltinCheckCtx;
use crate::types::PhpType;

// --- Per-builtin home files -------------------------------------------------

mod grapheme_levenshtein;
mod grapheme_str_split;
mod grapheme_stripos;
mod grapheme_stristr;
mod grapheme_strlen;
mod grapheme_strpos;
mod grapheme_strripos;
mod grapheme_strrpos;
mod grapheme_strstr;
mod grapheme_substr;
mod intl_error_name;
mod intl_get_error_code;
mod intl_get_error_message;
mod intl_is_failure;
mod intlcal_get_available_locales;
mod intlcal_get_now;
mod intltz_count_equivalent_ids;
mod intltz_get_equivalent_id;
mod intltz_get_iana_id;
mod intltz_get_id_for_windows_id;
mod intltz_get_region;
mod intltz_get_tz_data_version;
mod intltz_get_windows_id;
mod locale_accept_from_http;
mod locale_add_likely_subtags;
mod locale_canonicalize;
mod locale_compose;
mod locale_filter_matches;
mod locale_get_all_variants;
mod locale_get_default;
mod locale_get_display_language;
mod locale_get_display_name;
mod locale_get_display_region;
mod locale_get_display_script;
mod locale_get_display_variant;
mod locale_get_keywords;
mod locale_get_primary_language;
mod locale_get_region;
mod locale_get_script;
mod locale_is_right_to_left;
mod locale_lookup;
mod locale_minimize_subtags;
mod locale_parse;
mod locale_set_default;
mod msgfmt_parse_message;
mod normalizer_get_raw_decomposition;
mod normalizer_is_normalized;
mod normalizer_normalize;
mod resourcebundle_locales;
mod transliterator_list_ids;

/// Rejects a literal NUL byte in a `locale_*` string argument, matching PHP's ValueError.
fn intl_check(cx: &mut BuiltinCheckCtx<'_>) -> Result<PhpType, CompileError> {
    if cx.name.starts_with("locale_") {
        for (index, arg) in cx.args.iter().enumerate() {
            let ExprKind::StringLiteral(value) = &arg.kind else {
                continue;
            };
            if !value.contains('\0') {
                continue;
            }
            let parameter = elephc_builtin_contract::lookup(cx.name)
                .and_then(|contract| contract.params.get(index))
                .map(|param| param.name)
                .unwrap_or("locale");
            return Err(CompileError::new(
                arg.span,
                &format!(
                    "{}(): Argument #{} (${}) must not contain any null bytes",
                    cx.name,
                    index + 1,
                    parameter,
                ),
            ));
        }
    }
    Ok(crate::types::signatures::builtin_call_sig(cx.name)
        .map(|signature| signature.return_type)
        .unwrap_or(PhpType::Mixed))
}

/// Returns the shared descriptor for one scalar-only Intl builtin.
pub(crate) const fn intl_semantics() -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        // ICU carries process-wide default locale and error state, and the bridge keeps a
        // mutex-protected handle table. Treat every call as effectful.
        effects: BuiltinEffects::Static(Effects::all()),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "Intl entry points lower through the managed ICU bridge",
        ),
        lowering: BuiltinLowering::Eir(lower_intl_builtin),
    }
}

/// Lowers one scalar-only Intl call through the synthetic AST bridge helper.
///
/// The helper receives the canonical family and operation names plus an indexed array of the
/// normalized operands, serializes them for the C ABI, and decodes the typed result. The helper
/// name selects the exact PHP representation (string/int/float/bool/array/void/mixed), so the
/// EIR result never has to guess a boxed representation.
fn lower_intl_builtin(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let (family, helper, result_type) = intl_family_and_helper(call.name, call.result_type);
    let family = ctx.intern_string(family);
    let family = ctx.emit_value(
        Op::ConstStr,
        Vec::new(),
        Some(Immediate::Data(family)),
        PhpType::Str,
        Op::ConstStr.default_effects(),
        Some(call.span),
    );
    let method = ctx.intern_string(call.name);
    let method = ctx.emit_value(
        Op::ConstStr,
        Vec::new(),
        Some(Immediate::Data(method)),
        PhpType::Str,
        Op::ConstStr.default_effects(),
        Some(call.span),
    );
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(call.operands.len() as u32)),
        PhpType::Array(Box::new(PhpType::Mixed)),
        Op::ArrayNew.default_effects(),
        Some(call.span),
    );
    for operand in call.operands {
        let boxed = ctx.emit_value(
            Op::MixedBox,
            vec![*operand],
            None,
            PhpType::Mixed,
            Op::MixedBox.default_effects(),
            Some(call.span),
        );
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, boxed.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(call.span),
        );
    }
    let mut arguments = vec![family.value, method.value, array.value];
    if helper == "__elephc_intl_invoke_locale" {
        let parameter_names: Vec<&'static str> = elephc_builtin_contract::lookup(call.name)
            .map(|contract| contract.params.iter().map(|param| param.name).collect())
            .unwrap_or_default();
        let mut name_values = Vec::with_capacity(parameter_names.len());
        for name in parameter_names {
            let data = ctx.intern_string(name);
            name_values.push(ctx.emit_value(
                Op::ConstStr,
                Vec::new(),
                Some(Immediate::Data(data)),
                PhpType::Str,
                Op::ConstStr.default_effects(),
                Some(call.span),
            ).value);
        }
        let names = ctx.emit_value(
            Op::ArrayNew,
            Vec::new(),
            Some(Immediate::Capacity(name_values.len() as u32)),
            PhpType::Array(Box::new(PhpType::Str)),
            Op::ArrayNew.default_effects(),
            Some(call.span),
        );
        for name in name_values {
            ctx.emit_void(
                Op::ArrayPush,
                vec![names.value, name],
                None,
                Op::ArrayPush.default_effects(),
                Some(call.span),
            );
        }
        arguments.push(names.value);
    }
    Ok(ctx.emit_user_call(
        helper,
        arguments,
        result_type,
        Some(call.span),
    ))
}

/// Maps one Intl builtin and its declared result to the runtime family and AST helper.
fn intl_family_and_helper(name: &str, result: &PhpType) -> (&'static str, &'static str, PhpType) {
    // The bridge family is the ICU component, not the PHP function prefix: `intlcal_*` and
    // `intlgregcal_*` share the calendar dispatcher, `idn_*` the IDN one, and
    // `transliterator_*` the transliteration one.
    let family = if name.starts_with("grapheme_") { "grapheme" }
        else if name.starts_with("normalizer_") { "normalizer" }
        else if name.starts_with("locale_") { "locale" }
        else if name.starts_with("intlcal_") || name.starts_with("intlgregcal_") { "intlcal" }
        else if name.starts_with("intltz_") { "timezone" }
        else if name.starts_with("idn_") { "idn" }
        else if name.starts_with("transliterator_") { "transliterator" }
        else if name.starts_with("msgfmt_") { "msgfmt" }
        else if name.starts_with("resourcebundle_") { "resourcebundle" }
        else if name.starts_with("datefmt_") { "datefmt" }
        else if name.starts_with("numfmt_") { "numfmt" }
        else if name.starts_with("collator_") { "collator" }
        else { "intl" };
    if family == "locale" {
        return ("locale", "__elephc_intl_invoke_locale", PhpType::Mixed);
    }
    if matches!(name, "grapheme_strpos" | "grapheme_stripos" | "grapheme_strrpos" | "grapheme_strripos") {
        return ("grapheme", "__elephc_intl_grapheme_pos", result.clone());
    }
    if name == "grapheme_extract" {
        return ("grapheme", "__elephc_intl_grapheme_extract", result.clone());
    }
    if name == "grapheme_levenshtein" {
        return ("grapheme", "__elephc_intl_grapheme_levenshtein", result.clone());
    }
    if name == "msgfmt_parse_message" {
        return ("msgfmt", "__elephc_intl_msgfmt_parse_message", PhpType::Mixed);
    }
    if name == "normalizer_get_raw_decomposition" {
        return ("normalizer", "__elephc_intl_normalizer_raw", PhpType::Mixed);
    }
    if matches!(name, "normalizer_normalize" | "normalizer_is_normalized") {
        // The checked helper rejects unpublished forms with PHP's ValueError before the
        // bridge can silently fall back to NFC.
        return ("normalizer", "__elephc_intl_normalizer_checked", result.clone());
    }
    let helper = match result.codegen_repr() {
        PhpType::Str => "__elephc_intl_invoke_string",
        PhpType::Int => "__elephc_intl_invoke_int",
        PhpType::Float => "__elephc_intl_invoke_float",
        PhpType::Bool => "__elephc_intl_invoke_bool",
        PhpType::Array(_) => "__elephc_intl_invoke_array",
        PhpType::Void => "__elephc_intl_invoke_void",
        _ => "__elephc_intl_invoke_mixed",
    };
    (family, helper, result.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every EIR-selected helper name is rooted by the shared prelude helper list.
    ///
    /// The usage scan only sees the source-level call, so the declaration reachability pass
    /// has to learn these helper names from `INTL_HELPERS` instead.
    #[test]
    fn eir_helpers_are_rooted_from_the_prelude_helper_list() {
        for builtin in [
            "grapheme_strpos",
            "grapheme_extract",
            "grapheme_levenshtein",
            "msgfmt_parse_message",
            "normalizer_get_raw_decomposition",
            "intlcal_get_now",
            "normalizer_normalize",
        ] {
            let (_, helper, _) = intl_family_and_helper(builtin, &PhpType::Mixed);
            assert!(
                crate::intl::prelude_helpers_for(builtin).contains(&helper),
                "{builtin} selects {helper}, which prelude_helpers_for() does not root",
            );
        }
    }
}
