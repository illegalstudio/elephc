//! Purpose:
//! Computes the LOWEST PHP profile a program could actually run on, and names the construct
//! that forced it, so a `--php-version` lower than that can be rejected instead of silently
//! producing a binary that claims a version its own source could never have run under.
//!
//! Called from:
//! - `crate::pipeline::compile()`, immediately after the profile-dependence report.
//!
//! Key details:
//!
//! - WHY THIS EXISTS. elephc's lexer, parser and type checker are entirely version-agnostic:
//!   `rg "php_version" src/parser/ src/types/` finds nothing. The full language is always
//!   accepted, whatever `--php-version` says. So `--php-version 8.2` over a file using PHP
//!   8.4 property hooks compiles happily and bakes `PHP_VERSION = "8.2.0"` into a binary
//!   whose source could not have run on 8.2. Nothing miscompiles — but the version surface
//!   lies, and a program that branches on `PHP_VERSION_ID` to avoid 8.4 features is then
//!   reasoning about a version it is not.
//!
//! - THE APPROXIMATION RUNS THE OTHER WAY FROM `sensitivity`. That module may over-report:
//!   its output is a note, and a spurious note costs a line of noise. This module's output
//!   REJECTS A COMPILE, so a false positive costs a broken build of valid code. Every
//!   judgement call here is therefore made toward UNDER-reporting: a construct whose version
//!   mapping is not certain is left out entirely. Missing a requirement preserves today's
//!   behavior; inventing one breaks a working build.
//!
//! - BLAST RADIUS. The default profile is the NEWEST maintained one, and the floor can never
//!   exceed it, so a default compile can never be rejected by this check. It fires only when
//!   `--php-version` explicitly names an older profile — that is, only on a build that was
//!   already making the false claim this check exists to catch.

use crate::opcache_prelude::detect;
use crate::parser::ast::Stmt;
use crate::span::Span;
use crate::web_prelude::PhpVersion;

/// A construct that cannot run below a given profile.
#[derive(Clone, Debug)]
pub struct Requirement {
    /// The lowest profile that can run the construct.
    pub profile: PhpVersion,
    /// How to name the construct to the user.
    pub construct: &'static str,
    /// Where it appears.
    pub span: Span,
}

/// A function that did not exist before a given profile.
///
/// Only functions elephc ACTUALLY IMPLEMENTS are listed: naming one elephc does not provide
/// would be describing a program elephc cannot compile at any profile, which the unknown
/// function error already covers, more precisely.
struct NewFunction {
    /// The function name, lowercase.
    name: &'static str,
    /// The profile that introduced it.
    profile: PhpVersion,
}

/// Functions introduced after the oldest maintained profile.
///
/// Matched at CALL POSITIONS ONLY (`detect::Symbol::call_site`), so the
/// `function_exists('json_validate')` guard — written precisely to keep code running on
/// older PHP — is not mistaken for proof that the program requires the newer one.
///
/// A user-declared function of the same name suppresses the entry: a program that ships its
/// own `json_validate()` polyfill requires nothing from 8.3.
const NEW_FUNCTIONS: &[NewFunction] = &[
    NewFunction {
        name: "json_validate",
        profile: PhpVersion::Php83,
    },
    NewFunction {
        name: "array_find",
        profile: PhpVersion::Php84,
    },
    NewFunction {
        name: "array_any",
        profile: PhpVersion::Php84,
    },
    NewFunction {
        name: "array_all",
        profile: PhpVersion::Php84,
    },
    NewFunction {
        name: "array_first",
        profile: PhpVersion::Php84,
    },
    NewFunction {
        name: "array_last",
        profile: PhpVersion::Php84,
    },
];

/// Returns the binding minimum-version requirement for `program`, or `None` when nothing in
/// it needs more than the oldest maintained profile.
///
/// "Binding" means the HIGHEST requirement found: a program using both a typed class constant
/// (8.3) and a property hook (8.4) has a floor of 8.4, and that is the one worth naming.
///
/// # What is deliberately left out
///
/// Only constructs elephc can map to a version with CERTAINTY are listed. Nesting is not a
/// limit: every syntactic form rides on `detect`'s exhaustive traversal, so a declaration
/// inside a closure body is found exactly as a top-level one is.
///
/// Constructs whose floor is the oldest maintained profile are omitted entirely rather than
/// listed and compared: `readonly class` requires 8.2, which every profile satisfies, so an
/// entry for it could never bind.
pub fn floor(program: &[Stmt]) -> Option<Requirement> {
    let mut binding: Option<Requirement> = None;
    for found in syntax_requirements(program)
        .into_iter()
        .chain(function_requirements(program))
    {
        let better = match &binding {
            Some(current) => found.profile.version_id() > current.profile.version_id(),
            None => true,
        };
        if better {
            binding = Some(found);
        }
    }
    binding
}

/// Returns whether the program feature-detects `name`, i.e. calls `function_exists` with it.
///
/// This is expressed with the existing argument-narrowing matcher rather than new machinery:
/// `function_exists('json_validate')` is simply a call to `function_exists` whose first
/// argument is that name.
///
/// # Why a guard suppresses the requirement entirely
///
/// A program that feature-detects a function has DECLARED PORTABILITY INTENT, and rejecting
/// it would break the exact idiom written to keep the code running on older PHP. That the
/// guard is inert inside elephc — these builtins are provided at every profile, so
/// `function_exists` is always true and the polyfill branch is dead — does not change what
/// the author meant, and this check exists to serve the author.
///
/// This deliberately over-suppresses: the guard is honored wherever it appears, without
/// proving it actually dominates the call. Over-suppression costs a missed requirement, which
/// leaves today's behavior in place; under-suppression costs a rejected build of correct,
/// portable code. See the module preamble on which way this check is allowed to be wrong.
fn is_feature_detected(program: &[Stmt], name: &'static str) -> bool {
    detect::first_reference(
        program,
        detect::Symbol::function_with_arg_prefixes("function_exists", std::slice::from_ref(&name)),
    )
    .is_some()
}

/// Collects requirements coming from called functions that postdate the oldest profile.
fn function_requirements(program: &[Stmt]) -> Vec<Requirement> {
    NEW_FUNCTIONS
        .iter()
        .filter(|entry| !detect::program_declares(program, entry.name))
        .filter(|entry| !is_feature_detected(program, entry.name))
        .filter_map(|entry| {
            detect::first_reference(program, detect::Symbol::call_site(entry.name)).map(|span| {
                Requirement {
                    profile: entry.profile,
                    construct: entry.name,
                    span,
                }
            })
        })
        .collect()
}

/// Collects requirements coming from SYNTAX.
///
/// Each form rides on `detect`'s single exhaustive traversal rather than a walk of its own —
/// see [`detect::Symbol::syntactic`]. That is not only less code: a statement-level walk
/// cannot reach a declaration nested inside an EXPRESSION (a class declared in a closure
/// body), and this does, for free.
fn syntax_requirements(program: &[Stmt]) -> Vec<Requirement> {
    [
        (
            detect::SymbolKind::TypedClassConst,
            PhpVersion::Php83,
            "typed class constant",
        ),
        (
            detect::SymbolKind::PropertyHooks,
            PhpVersion::Php84,
            "property hooks",
        ),
        (
            detect::SymbolKind::AsymmetricVisibility,
            PhpVersion::Php84,
            "asymmetric property visibility",
        ),
        (
            detect::SymbolKind::PipeOperator,
            PhpVersion::Php85,
            "the pipe operator",
        ),
    ]
    .into_iter()
    .filter_map(|(kind, profile, construct)| {
        detect::first_reference(program, detect::Symbol::syntactic(kind)).map(|span| Requirement {
            profile,
            construct,
            span,
        })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for minimum-version computation: each detected construct, the
    //! highest-wins rule, statement-level nesting, and the guards that keep the check from
    //! rejecting valid code.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.

    use super::*;

    /// Parses source the way the pipeline sees it before name resolution.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// An ordinary program requires nothing beyond the oldest maintained profile.
    #[test]
    fn plain_program_has_no_floor() {
        assert!(floor(&parse(r#"<?php $a = [1, 2]; echo count($a);"#)).is_none());
    }

    /// Property hooks are an 8.4 construct.
    #[test]
    fn property_hooks_require_84() {
        let found = floor(&parse(
            "<?php\nclass C {\n    public string $x { get => 'v'; }\n}\n",
        ))
        .expect("hooks must set a floor");
        assert_eq!(found.profile, PhpVersion::Php84);
        assert_eq!(found.construct, "property hooks");
    }

    /// Asymmetric visibility is an 8.4 construct.
    #[test]
    fn asymmetric_visibility_requires_84() {
        let found = floor(&parse("<?php\nclass C {\n    public private(set) int $x = 1;\n}\n"))
            .expect("asymmetric visibility must set a floor");
        assert_eq!(found.profile, PhpVersion::Php84);
    }

    /// A typed class constant is an 8.3 construct.
    #[test]
    fn typed_class_constant_requires_83() {
        let found = floor(&parse("<?php\nclass C {\n    const string N = 'v';\n}\n"))
            .expect("typed constant must set a floor");
        assert_eq!(found.profile, PhpVersion::Php83);
        assert_eq!(found.construct, "typed class constant");
    }

    /// A function introduced in 8.3 sets an 8.3 floor when actually CALLED.
    #[test]
    fn new_function_call_requires_its_profile() {
        let found = floor(&parse(r#"<?php var_dump(json_validate("{}"));"#))
            .expect("json_validate must set a floor");
        assert_eq!(found.profile, PhpVersion::Php83);
        assert_eq!(found.construct, "json_validate");
    }

    /// A `function_exists()` guard is the idiom for STAYING compatible, so it must not be
    /// read as a requirement. Rejecting it would break the very code written to be portable.
    #[test]
    fn function_exists_guard_is_not_a_requirement() {
        assert!(floor(&parse(
            r#"<?php if (function_exists('json_validate')) { echo "y"; }"#
        ))
        .is_none());
    }

    /// The GUARDED polyfill idiom — the real one — is honored even though the guard is inert
    /// inside elephc, because it states the author's portability intent.
    ///
    /// elephc provides these builtins at every profile, so `function_exists` is always true
    /// here and the inner declaration is dead code. Rejecting this program would mean
    /// rejecting the canonical way to stay compatible with older PHP.
    #[test]
    fn guarded_polyfill_suppresses_the_requirement() {
        assert!(floor(&parse(
            "<?php\nif (!function_exists('json_validate')) {\n    function json_validate(string $j): bool { return $j !== ''; }\n}\nvar_dump(json_validate('{}'));\n"
        ))
        .is_none());
    }

    /// A redeclaration at top level also suppresses it, so the user sees the compiler's more
    /// specific redeclaration error rather than a version complaint about a name they own.
    #[test]
    fn top_level_declaration_suppresses_the_requirement() {
        assert!(floor(&parse(
            r#"<?php function json_validate(string $j): bool { return true; } var_dump(json_validate("{}"));"#
        ))
        .is_none());
    }

    /// Feature detection suppresses only the function it names, not every entry.
    #[test]
    fn feature_detection_is_per_function() {
        let found = floor(&parse(
            "<?php\nif (function_exists('json_validate')) { echo 'y'; }\n$r = array_find([1], fn($v) => $v > 0);\n",
        ))
        .expect("array_find is not guarded and must still bind");
        assert_eq!(found.profile, PhpVersion::Php84);
        assert_eq!(found.construct, "array_find");
    }

    /// `array_first()` and `array_last()` are PHP 8.4 functions, so calling either sets an 8.4
    /// floor, while a `function_exists()` guard for them is still not a requirement.
    #[test]
    fn array_first_and_last_require_84() {
        for name in ["array_first", "array_last"] {
            let found = floor(&parse(&format!("<?php\n$v = {name}([1, 2]);\n")))
                .unwrap_or_else(|| panic!("{name} must set a floor"));
            assert_eq!(found.profile, PhpVersion::Php84);
            assert_eq!(found.construct, name);
            assert!(floor(&parse(&format!(
                "<?php if (function_exists('{name}')) {{ echo 'y'; }}"
            )))
            .is_none());
        }
    }

    /// The HIGHEST requirement is the binding one and the one reported.
    #[test]
    fn highest_requirement_wins() {
        let found = floor(&parse(
            "<?php\nclass C {\n    const string N = 'v';\n    public string $x { get => 'v'; }\n}\n",
        ))
        .expect("both constructs must be seen");
        assert_eq!(found.profile, PhpVersion::Php84);
        assert_eq!(found.construct, "property hooks");
    }

    /// Statement-level nesting is searched to any depth, which covers the conditional
    /// class-declaration idiom.
    #[test]
    fn nested_declaration_is_found() {
        let found = floor(&parse(
            "<?php\nif (true) {\n    class C {\n        public string $x { get => 'v'; }\n    }\n}\n",
        ))
        .expect("nested declaration must be searched");
        assert_eq!(found.profile, PhpVersion::Php84);
    }

    /// A requirement inside a method body is found, so a locally-declared class counts.
    #[test]
    fn requirement_inside_a_method_body_is_found() {
        let found = floor(&parse(
            "<?php\nclass Outer {\n    public function f(): void {\n        if (true) { class Inner { const string N = 'v'; } }\n    }\n}\n",
        ))
        .expect("method bodies must be searched");
        assert_eq!(found.profile, PhpVersion::Php83);
    }

    /// The pipe operator is an 8.5 construct, found at expression level.
    #[test]
    fn pipe_operator_requires_85() {
        let found = floor(&parse("<?php\n$r = 'x' |> strtoupper(...);\n"))
            .expect("the pipe operator must set a floor");
        assert_eq!(found.profile, PhpVersion::Php85);
        assert_eq!(found.construct, "the pipe operator");
        assert_eq!(found.span.line, 2);
    }

    /// A pipe nested inside a closure body is still found — the property that reusing
    /// `detect`'s exhaustive expression walk buys, and that a statement-level walk would miss.
    #[test]
    fn pipe_inside_a_closure_is_found() {
        let found = floor(&parse(
            "<?php\n$f = function (string $s): string {\n    return $s |> strtoupper(...);\n};\n",
        ))
        .expect("a nested pipe must set a floor");
        assert_eq!(found.profile, PhpVersion::Php85);
    }

    /// A class declared inside a CLOSURE BODY is found. A statement-level walk cannot reach
    /// it — the declaration hides behind an expression — and this is the gap that moving
    /// every syntactic form onto `detect`'s exhaustive traversal closed.
    #[test]
    fn declaration_inside_a_closure_is_found() {
        let found = floor(&parse(
            "<?php\n$f = function (): void {\n    class Inner {\n        public string $x { get => 'v'; }\n    }\n};\n",
        ))
        .expect("a declaration inside a closure must be searched");
        assert_eq!(found.profile, PhpVersion::Php84);
        assert_eq!(found.construct, "property hooks");
    }

    /// The same, for a typed class constant inside a closure.
    #[test]
    fn typed_constant_inside_a_closure_is_found() {
        let found = floor(&parse(
            "<?php\n$f = fn(): int => 1;\n$g = function (): void {\n    class Inner { const string N = 'v'; }\n};\n",
        ))
        .expect("a constant inside a closure must be searched");
        assert_eq!(found.profile, PhpVersion::Php83);
    }

    /// A bitwise-or is not a pipe, so ordinary code is untouched.
    #[test]
    fn bitwise_or_is_not_a_pipe() {
        assert!(floor(&parse(r#"<?php $a = 6 | 1; echo $a;"#)).is_none());
    }

    /// The reported span points at the construct, so the diagnostic can be acted on.
    #[test]
    fn requirement_reports_the_construct_line() {
        let found = floor(&parse(
            "<?php\n$a = 1;\n$b = 2;\nclass C {\n    public string $x { get => 'v'; }\n}\n",
        ))
        .expect("hooks must set a floor");
        assert_eq!(found.span.line, 5);
    }
}
