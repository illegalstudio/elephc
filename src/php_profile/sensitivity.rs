//! Purpose:
//! Decides whether a program's OBSERVABLE BEHAVIOR depends on the `--php-version` profile,
//! and if so, names the symbol and source position that made it depend.
//!
//! Called from:
//! - `crate::pipeline::compile()`, which turns each reported [`Sensitivity`] into a
//!   `CompileWarning` on the existing diagnostic channel.
//!
//! Key details:
//! - elephc's version-sensitive surface is FINITE and ENUMERABLE, which is what makes this
//!   decidable rather than a heuristic. Every place the compile profile changes what a
//!   program does is reachable through one of the symbols in [`WATCHED`], so "does the
//!   profile matter here?" reduces to a name-set intersection over the AST.
//! - The table is a claim about the compiler, and claims rot. `tests/php_profile_*` proves
//!   it BIDIRECTIONALLY against real compilations: a program this module calls independent
//!   must lower identically at every profile, and a program it calls dependent must lower
//!   differently at some profile. Without the second arm, a table that simply listed every
//!   symbol in PHP would pass vacuously.
//! - A reported dependence always means the program COMPUTES something different, never that
//!   it merely warns differently. See [`Sensitivity`] for the one surface deliberately held
//!   to the second standard and excluded, and for why the proving test compares stdout
//!   rather than assembly in order to make that exclusion measurable instead of asserted.
//! - `eval()` is the one entry that is not a name lookup. Its fragment runs in the linked
//!   interpreter, which the compiler forwards the profile to, so a fragment mentioning the
//!   version surface makes the PROGRAM profile-dependent even though nothing in the AST names
//!   a watched symbol. It is matched on what its argument CONTAINS, and an argument the
//!   compiler cannot read counts — over-reporting a note is the cheap direction.
//! - An empty scan is the answer for the overwhelming majority of programs, and that silence
//!   is the feature: the compiler speaks only when the profile is a choice with consequences.

use crate::opcache_prelude::detect::{self, ArgFilter, Symbol, SymbolKind};
use crate::parser::ast::Stmt;
use crate::span::Span;

/// One reason a program's behavior depends on the compile profile.
///
/// Every reported sensitivity is a VALUE difference: the program computes something
/// different under different profiles. `PHP_VERSION_ID` is `80200` under one profile and
/// `80510` under another; `opcache_get_configuration()` returns arrays of different shape.
/// That is what makes the profile a decision the user should get to make deliberately.
///
/// # The one surface that is deliberately NOT a sensitivity
///
/// PHP 8.5's NAN-to-bool coercion warning (RFC `warnings-php-8-5`) changes what a program
/// PRINTS TO STDERR without changing anything it computes: `NAN` coerces to `true` under
/// every maintained profile, and 8.5 merely *also* emits a diagnostic — see
/// `codegen_support::runtime::arrays::nan_bool_coercion_warning`. It is excluded for two
/// independent reasons, either of which would be sufficient. It changes no computed value,
/// so nothing a program produces can depend on it; and it is not attached to a symbol at all
/// — it fires at every float-truthiness site — so detecting it would mean flagging
/// essentially every program that ever tests a float, burying the findings that matter.
///
/// That exclusion is not merely asserted: `tests/php_profile_independence_tests.rs` compares
/// STDOUT rather than assembly precisely so the claim is measured. Its
/// `nan_bool_diagnostic_only` case is a program whose assembly differs across profiles while
/// its output does not, and the table calling it independent is the correct answer.
#[derive(Clone, Debug)]
pub struct Sensitivity {
    /// The watched symbol that was referenced, spelled as PHP spells it.
    pub symbol: &'static str,
    /// Where the FIRST reference to it appears.
    pub span: Span,
    /// What actually differs across profiles, phrased to complete a diagnostic sentence.
    pub detail: &'static str,
    /// Whether the symbol is a function, so a renderer can spell it `phpversion()` rather
    /// than `phpversion`. Carried as a `bool` rather than the internal `SymbolKind` because
    /// that type is crate-private while this struct crosses the module boundary.
    pub is_function: bool,
}

/// A symbol whose meaning moves with the compile profile.
struct Watched {
    /// The symbol name as PHP spells it.
    symbol: &'static str,
    /// Whether it is a function or a global constant — they obey opposite matching rules,
    /// see [`SymbolKind`].
    symbol_kind: SymbolKind,
    /// Narrows a function entry to the calls that actually depend on the profile. `ini_get`
    /// is the motivating case: it is profile-dependent for `opcache.*` directives and
    /// profile-independent for everything else, so matching it by name alone would report a
    /// dependence most callers do not have. See [`ArgFilter`].
    args: ArgFilter<'static>,
    /// Whether this symbol only exists, or only becomes profile-dependent, under `--web`.
    web_only: bool,
    /// What differs across profiles.
    detail: &'static str,
}

/// Every symbol whose meaning moves with the `--php-version` profile.
///
/// # What is deliberately ABSENT, and why
///
/// `PHP_MAJOR_VERSION` is NOT here. It remains `8` across the maintained profile set, so
/// listing it would report a dependence that does not exist. `PHP_RELEASE_VERSION` and
/// `PHP_EXTRA_VERSION` are watched because the frozen PHP 8.5 profile reports `10` and `-dev`
/// while the other maintained profiles report `0` and `""`. The invariance test pins the only
/// remaining omission, so a future `9.0` profile forces this table to grow.
///
/// `PHP_SAPI`, `php_sapi_name()` and `ini_restore()` are also absent: the first two move
/// with `--web`, not with the version, and the third is a no-op under every profile.
/// The [`WATCHED`] names an `eval()` fragment can reach, which is what makes `eval` itself a
/// profile dependence worth reporting.
///
/// A fragment is SOURCE, not a subject, so `eval` is the one entry matched on what its
/// argument CONTAINS rather than what it names. `PHP_VERSION` covers `PHP_VERSION_ID` by
/// being a prefix of it, which is why the latter is not listed separately;
/// `eval_names_are_watched_symbols` pins every entry here to the table so this list cannot
/// drift into claiming a dependence the table does not have.
///
/// The `--web` half of the table is absent because the session surface is not part of the
/// eval interpreter at all.
const EVAL_PROFILE_DEPENDENT_NAMES: &[&str] = &[
    "PHP_VERSION",
    "PHP_MINOR_VERSION",
    "PHP_RELEASE_VERSION",
    "PHP_EXTRA_VERSION",
    "E_ALL",
    "error_reporting",
    "phpversion",
    "opcache_get_configuration",
    "opcache_get_status",
];

const WATCHED: &[Watched] = &[
    Watched {
        symbol: "PHP_VERSION",
        symbol_kind: SymbolKind::Constant,
        args: ArgFilter::Any,
        web_only: false,
        detail: "reports \"8.2.0\" through \"8.5.10-dev\" depending on the profile",
    },
    Watched {
        symbol: "PHP_VERSION_ID",
        symbol_kind: SymbolKind::Constant,
        args: ArgFilter::Any,
        web_only: false,
        detail: "reports 80200 through 80510 depending on the profile",
    },
    Watched {
        symbol: "PHP_MINOR_VERSION",
        symbol_kind: SymbolKind::Constant,
        args: ArgFilter::Any,
        web_only: false,
        detail: "reports 2 through 5 depending on the profile",
    },
    Watched {
        symbol: "PHP_RELEASE_VERSION",
        symbol_kind: SymbolKind::Constant,
        args: ArgFilter::Any,
        web_only: false,
        detail: "reports 10 for the frozen PHP 8.5 profile and 0 for the others",
    },
    Watched {
        symbol: "PHP_EXTRA_VERSION",
        symbol_kind: SymbolKind::Constant,
        args: ArgFilter::Any,
        web_only: false,
        detail: "reports \"-dev\" for the frozen PHP 8.5 profile and \"\" for the others",
    },
    Watched {
        symbol: "E_ALL",
        symbol_kind: SymbolKind::Constant,
        args: ArgFilter::Any,
        web_only: false,
        detail: "reports 32767 through PHP 8.3 and 30719 from PHP 8.4 onward",
    },
    Watched {
        symbol: "error_reporting",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Any,
        web_only: false,
        detail: "defaults to the profile-specific E_ALL mask",
    },
    Watched {
        symbol: "phpversion",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Any,
        web_only: false,
        detail: "returns the profile's version string",
    },
    Watched {
        symbol: "eval",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Substrings(EVAL_PROFILE_DEPENDENT_NAMES),
        web_only: false,
        detail: "runs a fragment that can read the version surface",
    },
    Watched {
        symbol: "zend_version",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Any,
        web_only: false,
        detail: "returns \"4.2.0\" through \"4.5.0\" depending on the profile",
    },
    Watched {
        symbol: "opcache_get_configuration",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Any,
        web_only: false,
        detail: "the reported directive set and version block follow the profile",
    },
    Watched {
        symbol: "opcache_get_status",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Any,
        web_only: false,
        detail: "the reported JIT block follows the profile",
    },
    Watched {
        symbol: "ini_get",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Prefixes(&["opcache."]),
        web_only: false,
        detail: "the set of known OPcache directives follows the profile",
    },
    Watched {
        symbol: "ini_get_all",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Any,
        web_only: false,
        detail: "the set of known OPcache directives follows the profile",
    },
    Watched {
        symbol: "session_start",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Any,
        web_only: true,
        detail: "option validation gained deprecations in 8.4 and read_and_close/CHIPS in 8.5",
    },
    Watched {
        symbol: "session_create_id",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Any,
        web_only: true,
        detail: "the 8.4 prefix-length limit applies only from 8.4",
    },
    Watched {
        symbol: "session_get_cookie_params",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Any,
        web_only: true,
        detail: "the returned array gained the CHIPS `partitioned` key in 8.5",
    },
    Watched {
        symbol: "session_set_cookie_params",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Any,
        web_only: true,
        detail: "the CHIPS `partitioned` option is honored only from 8.5",
    },
    Watched {
        symbol: "session_set_save_handler",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Any,
        web_only: true,
        detail: "argument validation changed in 8.4",
    },
    Watched {
        symbol: "session_encode",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Any,
        web_only: true,
        detail: "the 8.5 encoding rules apply only from 8.5",
    },
    Watched {
        symbol: "ini_set",
        symbol_kind: SymbolKind::Function,
        args: ArgFilter::Prefixes(&["session."]),
        web_only: true,
        detail: "the accepted session directive set follows the profile",
    },
];

/// Returns every profile dependence this program has, in source order.
///
/// `web` selects whether the `--web`-only surface (sessions, and the session half of
/// `ini_set`) is in scope: without `--web` those functions are not part of the compiled
/// program at all, so referencing their names cannot make the profile observable.
///
/// The result is ordered by source position so a caller that reports all of them reads
/// top-to-bottom through the file.
pub fn scan(program: &[Stmt], web: bool) -> Vec<Sensitivity> {
    let mut found: Vec<Sensitivity> = WATCHED
        .iter()
        .filter(|watched| web || !watched.web_only)
        .filter_map(|watched| {
            let symbol = match watched.symbol_kind {
                SymbolKind::Function => Symbol::function_with_args(watched.symbol, watched.args),
                SymbolKind::CallSite => Symbol::call_site(watched.symbol),
                SymbolKind::Constant => Symbol::constant(watched.symbol),
                // No syntactic form is profile-SENSITIVE: syntax either exists in a profile or
                // does not, which is `php_profile::floor`'s question, not this module's. The
                // arm stays exhaustive so a new form has to come here and say so explicitly.
                SymbolKind::PipeOperator
                | SymbolKind::PropertyHooks
                | SymbolKind::AsymmetricVisibility
                | SymbolKind::TypedClassConst => Symbol::syntactic(watched.symbol_kind),
            };
            detect::first_reference(program, symbol).map(|span| Sensitivity {
                symbol: watched.symbol,
                span,
                detail: watched.detail,
                is_function: watched.symbol_kind == SymbolKind::Function,
            })
        })
        .collect();
    found.sort_by_key(|sensitivity| (sensitivity.span.line, sensitivity.span.col));
    found
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the sensitivity table and its scan: detection per symbol kind, the
    //! `--web` gate, source ordering, and the invariance claim that justifies leaving three
    //! `PHP_*` constants out of the table.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - These are UNIT tests of the table's own logic. The table's correctness *as a claim
    //!   about the compiler* is proven separately and bidirectionally by
    //!   `tests/php_profile_independence_tests.rs`, which compiles real programs at every
    //!   profile and diffs the output.

    use super::*;
    use crate::web_prelude::PhpVersion;

    /// Parses source the way the pipeline sees it before name resolution.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// An ordinary program that never asks about its own version is independent.
    #[test]
    fn plain_program_is_independent() {
        let program = parse(r#"<?php $a = [3, 1, 2]; sort($a); echo implode(",", $a);"#);
        assert!(scan(&program, false).is_empty());
        assert!(scan(&program, false).is_empty());
    }

    /// A bare `PHP_VERSION_ID` reference makes the program dependent, and is reported at the
    /// line where it appears.
    #[test]
    fn version_id_reference_is_dependent() {
        let program = parse("<?php\n$x = 1;\nif (PHP_VERSION_ID >= 80400) { echo 'new'; }\n");
        assert!(!scan(&program, false).is_empty());
        let found = scan(&program, false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].symbol, "PHP_VERSION_ID");
        assert_eq!(found[0].span.line, 3);
    }

    /// A `phpversion()` call is detected through the function path.
    #[test]
    fn phpversion_call_is_dependent() {
        let program = parse(r#"<?php echo phpversion();"#);
        assert!(!scan(&program, false).is_empty());
    }

    /// Session functions do not make a CLI program dependent: without `--web` they are not
    /// part of the compiled program, so their names carry no profile-dependent behavior.
    #[test]
    fn session_surface_is_web_only() {
        let program = parse(r#"<?php session_start(); echo 'x';"#);
        assert!(scan(&program, false).is_empty());
        assert!(!scan(&program, true).is_empty());
    }

    /// Several dependences are reported in source order, so a caller printing them walks the
    /// file top-to-bottom.
    #[test]
    fn results_are_in_source_order() {
        let program = parse("<?php\necho zend_version();\necho PHP_VERSION;\necho phpversion();\n");
        let found = scan(&program, false);
        let lines: Vec<u32> = found.iter().map(|s| s.span.line).collect();
        assert_eq!(lines, vec![2, 3, 4]);
    }

    /// The word `PHP_VERSION_ID` inside a string is prose, not a dependence — the property
    /// that the constant symbol kind exists to provide.
    #[test]
    fn constant_named_in_a_string_is_not_a_dependence() {
        let program = parse(r#"<?php echo "needs PHP_VERSION_ID >= 80400";"#);
        assert!(scan(&program, false).is_empty());
    }

    /// `PHP_MAJOR_VERSION`, deliberately left out of [`WATCHED`], remains invariant.
    ///
    /// This is the guard that keeps the omission honest: a future `9.0` profile must make the
    /// constant join the table instead of silently invalidating the independence claim.
    #[test]
    fn constants_absent_from_the_table_are_invariant() {
        let profiles = PhpVersion::MAINTAINED;
        let first = profiles[0];
        for profile in profiles {
            assert_eq!(
                profile.major(),
                first.major(),
                "PHP_MAJOR_VERSION now varies across profiles and must be added to WATCHED"
            );
        }
    }

    /// Conversely, the five version constants that ARE in the table really do vary, so the
    /// table is not padded with entries that would produce noise.
    #[test]
    fn constants_present_in_the_table_really_vary() {
        let distinct_strings: std::collections::HashSet<_> =
            PhpVersion::MAINTAINED
                .iter()
                .map(|p| p.version_string())
                .collect();
        let distinct_ids: std::collections::HashSet<_> =
            PhpVersion::MAINTAINED
                .iter()
                .map(|p| p.version_id())
                .collect();
        let distinct_minors: std::collections::HashSet<_> =
            PhpVersion::MAINTAINED.iter().map(|p| p.minor()).collect();
        let distinct_releases: std::collections::HashSet<_> =
            PhpVersion::MAINTAINED.iter().map(|p| p.release()).collect();
        let distinct_extras: std::collections::HashSet<_> = PhpVersion::MAINTAINED
            .iter()
            .map(|p| p.extra_version())
            .collect();
        assert_eq!(distinct_strings.len(), PhpVersion::MAINTAINED.len());
        assert_eq!(distinct_ids.len(), PhpVersion::MAINTAINED.len());
        assert_eq!(distinct_minors.len(), PhpVersion::MAINTAINED.len());
        assert!(distinct_releases.len() > 1);
        assert!(distinct_extras.len() > 1);
    }

    /// Every name `eval` is matched on is itself a table entry.
    ///
    /// The list exists because a fragment is source rather than a subject, so it has to name
    /// its needles literally. This test is what keeps that duplication honest: a name that
    /// stopped being profile-dependent would be removed from the table and would fail here
    /// rather than keep making `eval` report a dependence that no longer exists.
    #[test]
    fn eval_names_are_watched_symbols() {
        for name in EVAL_PROFILE_DEPENDENT_NAMES {
            assert!(
                WATCHED.iter().any(|watched| watched.symbol == *name),
                "{name} is matched inside eval() but is not a WATCHED symbol"
            );
        }
    }

    /// An `eval()` fragment reading the version surface is a profile dependence, and one
    /// that does not is not.
    ///
    /// This is the eval boundary the runtime bridge closes: the fragment observes whatever
    /// profile the binary was compiled for, so the program's output depends on it.
    #[test]
    fn eval_is_reported_only_when_its_fragment_reads_the_version_surface() {
        let dependent = parse("<?php eval('echo PHP_VERSION;');");
        let found = scan(&dependent, false);
        assert_eq!(found.len(), 1, "expected exactly one dependence");
        assert_eq!(found[0].symbol, "eval");

        let independent = parse("<?php eval('echo 1 + 1;');");
        assert!(scan(&independent, false).is_empty());
    }

    /// An `eval()` whose fragment is computed is reported, because the compiler cannot know
    /// what the string will say.
    ///
    /// Over-reporting is the safe direction here: the output is a note, so a false positive
    /// costs a line of advice, while a false negative costs a silently profile-dependent
    /// binary. `floor` reads the same table the other way round and is why that asymmetry is
    /// stated rather than assumed.
    #[test]
    fn eval_of_a_computed_fragment_is_reported() {
        let program = parse("<?php $code = 'echo 1;'; eval($code);");
        let found = scan(&program, false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].symbol, "eval");
    }

    /// Every table entry names a distinct symbol, so a dependence is never reported twice.
    #[test]
    fn table_has_no_duplicate_symbols() {
        let mut seen = std::collections::HashSet::new();
        for watched in WATCHED {
            assert!(
                seen.insert(watched.symbol),
                "duplicate WATCHED entry for {}",
                watched.symbol
            );
        }
    }
}
