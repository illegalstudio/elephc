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
pub(crate) fn parse_literal_fragment(fragment: &str, strict_php: bool) -> Option<Program> {
    let source = format!("<?php {}", fragment);
    let tokens = crate::lexer::tokenize(&source).ok()?;
    let source_mode = if strict_php {
        crate::source::SourceMode::Php
    } else {
        crate::source::SourceMode::Lfc
    };
    crate::parser::parse_with_mode(&tokens, source_mode).ok()
}

/// Returns the line INSIDE a literal eval fragment where the parser gave up, or `None`.
///
/// php closes an `eval()` parse error with `: eval()'d code on line N`, and the bridge cannot
/// supply that N: `__elephc_eval_execute` answers a status code and nothing else, so a line
/// would have to widen the versioned result struct. For a fragment that is a compile-time
/// literal the compiler can ask a parser itself — its OWN parser, not php's, so the two agree
/// on WHERE a fragment stops making sense far more often than on WHY.
///
/// The `<?php ` prefix carries no newline, so a line in the prefixed source is the same line in
/// the fragment.
///
/// One correction is applied to what the parser answers. It reports the line of the failing
/// TOKEN; php reports an unterminated fragment against the END OF INPUT, which is a different
/// line whenever the fragment ends with a newline. Measured on `php -n` 8.5.6,
/// `eval("$a = 1;\n$b = 2;\n1 +\n")` is `on line 4` — the empty fourth line — while the last
/// token sits on line 3. So an error at the last token, which is the shape php words as
/// `unexpected end of file`, is moved to the end of input; an error anywhere earlier is a real
/// token position and stays where the parser put it.
pub(crate) fn literal_fragment_parse_error_line(fragment: &str, strict_php: bool) -> Option<u32> {
    let source = format!("<?php {}", fragment);
    let tokens = match crate::lexer::tokenize(&source) {
        Ok(tokens) => tokens,
        Err(error) => return Some(error.span.line),
    };
    let source_mode = if strict_php {
        crate::source::SourceMode::Php
    } else {
        crate::source::SourceMode::Lfc
    };
    let line = crate::parser::parse_with_mode(&tokens, source_mode)
        .err()?
        .span
        .line;
    let last_code_line = tokens
        .iter()
        .rev()
        .find(|(token, _)| !matches!(token, crate::lexer::Token::Eof))
        .map(|(_, metadata)| metadata.span.line);
    if last_code_line == Some(line) {
        // The lexer's own `Eof` token already sits at end of input, trailing newline included.
        return tokens
            .last()
            .map(|(_, metadata)| metadata.span.line)
            .or(Some(line));
    }
    Some(line)
}

/// Parses a literal eval fragment and applies call-site magic-constant metadata when available.
pub(crate) fn parse_literal_fragment_with_source_path(
    fragment: &str,
    source_path: Option<&str>,
    strict_php: bool,
) -> Option<Program> {
    let program = parse_literal_fragment(fragment, strict_php)?;
    Some(match source_path {
        Some(source_path) => crate::magic_constants::substitute_file_and_scope_constants(
            program,
            Path::new(source_path),
        ),
        None => program,
    })
}
/// Returns a deterministic internal function name for a literal eval fragment and source profile.
pub(crate) fn eir_function_name(fragment: &str, strict_php: bool) -> String {
    format!(
        "{}_{}_{:016x}",
        EIR_AOT_FUNCTION_PREFIX,
        if strict_php { "strict" } else { "elephc" },
        stable_fragment_hash(fragment)
    )
}

/// Returns a deterministic internal function name for a profiled scope-aware eval fragment.
pub(crate) fn eir_scope_function_name(fragment: &str, strict_php: bool) -> String {
    format!(
        "{}_scope_{}_{:016x}",
        EIR_AOT_FUNCTION_PREFIX,
        if strict_php { "strict" } else { "elephc" },
        stable_fragment_hash(fragment)
    )
}

/// Builds the shared literal eval AOT plan for scan, lowering, and codegen decisions.
pub(crate) fn plan_literal_fragment_with_static_calls<F>(
    fragment: &str,
    strict_php: bool,
    static_call_supported: F,
) -> EvalAotPlan
where
    F: Fn(&str, &[Expr]) -> bool,
{
    plan_literal_fragment_with_static_and_method_calls(
        fragment,
        strict_php,
        static_call_supported,
        |_, _, _| false,
    )
}

/// Builds the shared literal eval AOT plan with function and static-method support.
pub(crate) fn plan_literal_fragment_with_static_and_method_calls<F, M>(
    fragment: &str,
    strict_php: bool,
    static_call_supported: F,
    static_method_supported: M,
) -> EvalAotPlan
where
    F: Fn(&str, &[Expr]) -> bool,
    M: Fn(&StaticReceiver, &str, &[Expr]) -> bool,
{
    let Some(program) = parse_literal_fragment(fragment, strict_php) else {
        return parse_error_plan();
    };
    plan_parsed_literal_fragment_with_static_and_method_calls(
        fragment,
        strict_php,
        program,
        static_call_supported,
        static_method_supported,
    )
}

/// Builds the literal eval AOT plan with call-site source and static-method metadata.
pub(crate) fn plan_literal_fragment_with_source_path_and_static_and_method_calls<F, M>(
    fragment: &str,
    source_path: Option<&str>,
    strict_php: bool,
    static_call_supported: F,
    static_method_supported: M,
) -> EvalAotPlan
where
    F: Fn(&str, &[Expr]) -> bool,
    M: Fn(&StaticReceiver, &str, &[Expr]) -> bool,
{
    let Some(program) =
        parse_literal_fragment_with_source_path(fragment, source_path, strict_php)
    else {
        return parse_error_plan();
    };
    plan_parsed_literal_fragment_with_static_and_method_calls(
        fragment,
        strict_php,
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
    strict_php: bool,
    program: Program,
    static_call_supported: F,
    static_method_supported: M,
) -> EvalAotPlan
where
    F: Fn(&str, &[Expr]) -> bool,
    M: Fn(&StaticReceiver, &str, &[Expr]) -> bool,
{
    let mut scope_access = collect_scope_accesses(&program);
    scope_access.reads = collect_scope_reads_before_writes(&program);
    let folded_program = fold_static_builtin_calls_in_program(program.clone());
    let support = EirStaticCallPredicates {
        function: &static_call_supported,
        static_method: &static_method_supported,
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
            .map(|_| eir_function_name(fragment, strict_php)),
        eir_program,
        scope_function_name: scope_eir_program
            .as_ref()
            .map(|_| eir_scope_function_name(fragment, strict_php)),
        scope_eir_program,
        reads: scope_access.reads,
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
