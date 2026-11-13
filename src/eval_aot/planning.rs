//! Purpose:
//! Parses literal eval fragments and builds shared AOT plans.
//!
//! Called from:
//! - The eval AOT facade and sibling analysis modules.
//!
//! Key details:
//! - Source profiles, deterministic names, and parsed-program decisions remain centralized.

use super::*;

/// Parses a literal eval fragment with the builtin visibility of its physical call site.
pub(crate) fn parse_literal_fragment(fragment: &str, profile: EvalSourceProfile) -> Option<Program> {
    let source = format!("<?php {}", fragment);
    // Each parsed eval fragment carries its own source identity, so a span in the fragment can
    // never alias the ROOT file's equal coordinates in span-keyed maps such as
    // `builtin_call_types` and the binding keys (issue #1291). The id comes from a synthetic
    // counter that `reset_source_ids` does not rewind and that starts above the include range, so
    // it cannot alias an INCLUDED or autoloaded file's id either: the include counter restarts at
    // the start of every compile, which is why sharing it left a narrower alias gap.
    let source_id = crate::span::Span::fresh_synthetic_source_id();
    let tokens = crate::lexer::tokenize_with_semantics(
        &source,
        crate::source::SourceMode::Php,
        source_id,
        profile.semantics(),
    )
    .ok()?;
    let source_mode = if profile.php_only() {
        crate::source::SourceMode::Php
    } else {
        crate::source::SourceMode::Lfc
    };
    crate::parser::parse_with_mode(&tokens, source_mode).ok()
}

/// Parses a literal eval fragment and applies call-site magic-constant metadata when available.
pub(crate) fn parse_literal_fragment_with_source_path(
    fragment: &str,
    source_path: Option<&str>,
    profile: EvalSourceProfile,
) -> Option<Program> {
    let program = parse_literal_fragment(fragment, profile)?;
    Some(match source_path {
        Some(source_path) => crate::magic_constants::substitute_file_and_scope_constants(
            program,
            Path::new(source_path),
        ),
        None => program,
    })
}
/// Returns a deterministic internal function name for a literal eval fragment and source profile.
pub(crate) fn eir_function_name(fragment: &str, profile: EvalSourceProfile) -> String {
    format!(
        "{}_{}_v{}_{:016x}_{:016x}",
        EIR_AOT_FUNCTION_PREFIX,
        if profile.php_only() { "php" } else { "elephc" },
        elephc_semantics::SEMANTICS_ABI_VERSION,
        profile.semantics().bits(),
        stable_fragment_hash(fragment)
    )
}

/// Returns a deterministic internal function name for a profiled scope-aware eval fragment.
pub(crate) fn eir_scope_function_name(fragment: &str, profile: EvalSourceProfile) -> String {
    format!(
        "{}_scope_{}_v{}_{:016x}_{:016x}",
        EIR_AOT_FUNCTION_PREFIX,
        if profile.php_only() { "php" } else { "elephc" },
        elephc_semantics::SEMANTICS_ABI_VERSION,
        profile.semantics().bits(),
        stable_fragment_hash(fragment)
    )
}

/// Builds the shared literal eval AOT plan for scan, lowering, and codegen decisions.
pub(crate) fn plan_literal_fragment_with_static_calls<F>(
    fragment: &str,
    profile: EvalSourceProfile,
    static_call_supported: F,
) -> EvalAotPlan
where
    F: Fn(&str, &[Expr]) -> bool,
{
    plan_literal_fragment_with_static_and_method_calls(
        fragment,
        profile,
        static_call_supported,
        |_, _, _| false,
    )
}

/// Builds the shared literal eval AOT plan with function and static-method support.
pub(crate) fn plan_literal_fragment_with_static_and_method_calls<F, M>(
    fragment: &str,
    profile: EvalSourceProfile,
    static_call_supported: F,
    static_method_supported: M,
) -> EvalAotPlan
where
    F: Fn(&str, &[Expr]) -> bool,
    M: Fn(&StaticReceiver, &str, &[Expr]) -> bool,
{
    let Some(program) = parse_literal_fragment(fragment, profile) else {
        return parse_error_plan();
    };
    plan_parsed_literal_fragment_with_static_and_method_calls(
        fragment,
        profile,
        program,
        static_call_supported,
        static_method_supported,
    )
}

/// Builds the literal eval AOT plan with call-site source and static-method metadata.
pub(crate) fn plan_literal_fragment_with_source_path_and_static_and_method_calls<F, M>(
    fragment: &str,
    source_path: Option<&str>,
    profile: EvalSourceProfile,
    static_call_supported: F,
    static_method_supported: M,
) -> EvalAotPlan
where
    F: Fn(&str, &[Expr]) -> bool,
    M: Fn(&StaticReceiver, &str, &[Expr]) -> bool,
{
    let Some(program) =
        parse_literal_fragment_with_source_path(fragment, source_path, profile)
    else {
        return parse_error_plan();
    };
    plan_parsed_literal_fragment_with_static_and_method_calls(
        fragment,
        profile,
        program,
        static_call_supported,
        static_method_supported,
    )
}

/// Returns a conservative plan for fragments that cannot be parsed statically.
pub(super) fn parse_error_plan() -> EvalAotPlan {
    EvalAotPlan {
        function_name: None,
        eir_program: None,
        scope_function_name: None,
        scope_eir_program: None,
        reads: BTreeSet::new(),
        quiet_reads: BTreeSet::new(),
        array_read_constraints: BTreeSet::new(),
        assoc_array_read_constraints: BTreeSet::new(),
        float_predicate_read_constraints: BTreeSet::new(),
        writes: BTreeSet::new(),
        scope_direct_writes: BTreeSet::new(),
        scope_flush_writes: BTreeSet::new(),
        creates_unknown_vars: true,
        needs_eval_context: true,
        needs_global_scope: true,
        fallback_reason: Some(EvalAotFallbackReason::ParseError),
    }
}

/// Builds the shared literal eval AOT plan from an already parsed fragment program.
pub(super) fn plan_parsed_literal_fragment_with_static_and_method_calls<F, M>(
    fragment: &str,
    profile: EvalSourceProfile,
    program: Program,
    static_call_supported: F,
    static_method_supported: M,
) -> EvalAotPlan
where
    F: Fn(&str, &[Expr]) -> bool,
    M: Fn(&StaticReceiver, &str, &[Expr]) -> bool,
{
    let _semantics = crate::optimize::scoped_semantic_profile(profile.semantics());
    let mut scope_access = collect_scope_accesses(&program);
    scope_access.reads = collect_scope_reads_before_writes(&program);
    let quiet_reads = scope_access
        .reads
        .difference(&scope_access.warning_reads)
        .cloned()
        .collect();
    let folded_program = fold_static_builtin_calls_in_program(program.clone(), profile.php_only());
    let support = EirStaticCallPredicates {
        function: &static_call_supported,
        static_method: &static_method_supported,
        profile,
    };
    let eir_program =
        program_is_eir_function_safe(&folded_program, &support).then_some(folded_program.clone());
    let scope_names = scope_access
        .reads
        .union(&scope_access.writes)
        .cloned()
        .collect::<BTreeSet<_>>();
    let scope_eir_safe = eir_program.is_none()
        && !scope_names.is_empty()
        && !scope_access.creates_unknown_vars
        && program_is_eir_scope_function_safe(&folded_program, &support, &scope_names);
    let scope_flush_local =
        scope_eir_safe && scope_access.reads.is_empty() && !scope_access.writes.is_empty();
    let scope_direct = scope_eir_safe && !scope_access.reads.is_empty();
    let scope_direct_writes = if scope_direct {
        scope_access.writes.clone()
    } else {
        BTreeSet::new()
    };
    let scope_flush_writes = if scope_flush_local {
        scope_access.writes.clone()
    } else {
        BTreeSet::new()
    };
    let array_read_constraint_sets =
        collect_array_scope_read_constraint_sets(&folded_program, &scope_access.reads);
    let array_read_constraints = array_read_constraint_sets.array_like;
    let assoc_array_read_constraints = array_read_constraint_sets.assoc;
    let float_predicate_read_constraints =
        collect_float_predicate_scope_read_constraints(&folded_program, &scope_access.reads);
    let scope_eir_program = (scope_direct || scope_flush_local).then_some(folded_program);
    let is_fully_static_no_bridge = eir_program.is_some();
    let has_scope_eir = scope_eir_program.is_some();
    let needs_global_scope =
        !is_fully_static_no_bridge && !has_scope_eir && scope_access.has_scope_access();
    EvalAotPlan {
        function_name: eir_program
            .as_ref()
            .map(|_| eir_function_name(fragment, profile)),
        eir_program,
        scope_function_name: scope_eir_program
            .as_ref()
            .map(|_| eir_scope_function_name(fragment, profile)),
        scope_eir_program,
        reads: scope_access.reads,
        quiet_reads,
        array_read_constraints,
        assoc_array_read_constraints,
        float_predicate_read_constraints,
        writes: scope_access.writes,
        scope_direct_writes,
        scope_flush_writes,
        creates_unknown_vars: scope_access.creates_unknown_vars,
        needs_eval_context: !is_fully_static_no_bridge && !has_scope_eir,
        needs_global_scope,
        fallback_reason: (!is_fully_static_no_bridge && !has_scope_eir)
            .then(|| classify_fallback_reason(&program)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A literal eval fragment is tokenized with its own source identity, so its spans can never
    /// alias the root file's (or another fragment's) equal coordinates in span-keyed maps such as
    /// `builtin_call_types` (issue #1291). The coordinates stay identical; only the identity
    /// differs.
    #[test]
    fn literal_fragment_spans_carry_a_distinct_source_id() {
        let first = parse_literal_fragment("echo strlen('x');", EvalSourceProfile::permissive(false)).expect("first fragment");
        let second = parse_literal_fragment("echo strlen('x');", EvalSourceProfile::permissive(false)).expect("second fragment");
        let span_of = |program: &Program| program.first().expect("one statement").span;

        let first_span = span_of(&first);
        let second_span = span_of(&second);
        assert_ne!(first_span.source_id(), 0, "a fragment must not reuse the root source id");
        assert_ne!(
            first_span.source_id(),
            second_span.source_id(),
            "each fragment parse gets its own identity"
        );
        assert_eq!(
            (first_span.line, first_span.col),
            (second_span.line, second_span.col),
            "the same fragment keeps its coordinates; only the identity differs"
        );
        // An include-resolution reset must not rewind the synthetic counter, or a later fragment
        // could reuse an included file's identity (issue #1291).
        crate::span::Span::reset_source_ids();
        let third = parse_literal_fragment("echo strlen('x');", EvalSourceProfile::permissive(false)).expect("third fragment");
        assert_ne!(
            first_span.source_id(),
            span_of(&third).source_id(),
            "a fragment after an include reset keeps a fresh identity"
        );
    }
}

#[cfg(test)]
mod strict_profile_tests {
    use super::*;
    use elephc_semantics::{SemanticProfile, StrictFeature};

    /// Literal eval names cannot alias across syntax visibility, semantics or scope strategies.
    #[test]
    fn strict_eval_profiles_partition_aot_identities() {
        let mut names = std::collections::HashSet::new();
        for semantics in [SemanticProfile::PERMISSIVE, SemanticProfile::all(),
            SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true)] {
            for php_only in [true, false] {
                let profile = EvalSourceProfile::new(php_only, semantics);
                assert_eq!(profile.php_only(), php_only);
                assert_eq!(profile.semantics(), semantics);
                assert!(names.insert(eir_function_name("return 1;", profile)));
                assert!(names.insert(eir_scope_function_name("return 1;", profile)));
            }
        }
        assert_eq!(std::mem::size_of::<EvalSourceProfile>(), 4);
    }

    /// AOT parsing applies checked literal rules before eligibility and constant folding.
    #[test]
    fn strict_eval_profiles_preserve_numeric_literal_evidence() {
        let source = "return 9223372036854775808;";
        assert!(parse_literal_fragment(source, EvalSourceProfile::permissive(false)).is_some());
        let profile = EvalSourceProfile::new(false, SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true));
        assert!(parse_literal_fragment(source, profile).is_none());
        assert!(parse_literal_fragment("return -9223372036854775808;", profile).is_some());
        let plan = plan_literal_fragment_with_static_calls(source, profile, |_, _| false);
        assert!(matches!(plan.fallback_reason(), Some(EvalAotFallbackReason::ParseError)));
        let tokens = crate::lexer::tokenize("<?php eval('if (false) { echo 9223372036854775808; }');").unwrap();
        let program = crate::parser::parse_with_mode(&tokens, crate::source::SourceMode::Php).unwrap();
        let error = crate::semantics::validate_source(&program, profile.semantics()).unwrap_err();
        assert!(error.message.contains("strict-overflow"), "{}", error.message);
    }

    /// Lowering retains explicit profiles and the EIR boundary rejects incompatible metadata.
    #[test]
    fn strict_eval_profiles_survive_lowering_on_all_supported_targets() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen::platform::Target::parse(name).unwrap();
            let tokens = crate::lexer::tokenize("<?php eval('echo true;');").unwrap();
            let program = crate::parser::parse_with_mode(&tokens, crate::source::SourceMode::Php).unwrap();
            let checked = crate::types::check_with_target_and_options(&program, target,
                crate::types::CheckOptions { semantic_profile: profile, ..Default::default() },
                &crate::generics::GenericContext::default()).unwrap();
            let mut module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            crate::ir::validate_module(&module).unwrap();
            let inst = module.functions.iter_mut().flat_map(|function| &mut function.instructions)
                .find(|inst| inst.op == crate::ir::Op::EvalLiteralCall).expect("source-sensitive eval");
            let Some(crate::ir::Immediate::ProfiledData { profile: actual, .. }) = &mut inst.immediate else { panic!("missing profile"); };
            assert_eq!(actual.semantics(), profile, "{name}");
            *actual = EvalSourceProfile::permissive(false);
            assert!(matches!(crate::ir::validate_module(&module), Err(crate::ir::ValidationError::SemanticProfileMismatch { .. })), "{name}");
        }
    }
}
