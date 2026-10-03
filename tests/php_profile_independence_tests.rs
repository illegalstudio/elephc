//! Purpose:
//! Proves — bidirectionally, against real compilations — that
//! `php_profile::sensitivity`'s table of version-sensitive symbols is EXACT: every program
//! it calls profile-independent really does behave identically under every maintained
//! `--php-version`, and every program it calls profile-dependent really does behave
//! differently under at least one.
//!
//! Called from:
//! - `cargo test --test php_profile_independence_tests` through Rust's test harness.
//!
//! Key details:
//!
//! - WHY BOTH ARMS ARE MANDATORY. The positive arm alone (independent => identical) is
//!   satisfied by a table that lists every symbol in PHP, because then nothing is ever
//!   called independent and the implication holds vacuously. The negative arm alone
//!   (dependent => differs) is satisfied by an EMPTY table for the same reason mirrored.
//!   Only together do they pin the table to the truth. `corpus_exercises_both_arms` fails if
//!   the corpus itself ever degenerates to one side, which is the failure mode that would
//!   silently disarm this file.
//!
//! - THE ORACLE IS STDOUT, NOT ASSEMBLY. This is deliberate and it is what makes
//!   `SensitivityKind::Diagnostic` testable rather than hand-waved. elephc's runtime
//!   warnings are written to fd 2 (`codegen_support::runtime::diagnostics`, `mov x0, #2` /
//!   `mov edi, 2`), so a surface that changes only the warning stream — the PHP 8.5
//!   NAN-to-bool coercion diagnostic being the whole of that category today — leaves stdout
//!   untouched. Comparing stdout therefore measures exactly what `SensitivityKind::Value`
//!   claims to measure: whether the program COMPUTES something different. Comparing
//!   assembly would instead conflate the two, and would report `nan_bool_diagnostic_only`
//!   below as a table error when the table is right.
//!
//! - The prediction is read from the table itself (`sensitivity::is_profile_independent`),
//!   never from a hand-written per-case expectation. A hand-written expectation would only
//!   prove that two things the author wrote agree with each other; reading the prediction
//!   from the shipping code is what makes this a test OF the shipping code.
//!
//! - THE EVAL CASES ARE THE POINT OF THE NEGATIVE ARM. `eval()` fragments run in the linked
//!   interpreter rather than through codegen, so until the compiler forwarded the profile to
//!   it, `eval('echo PHP_VERSION;')` printed `8.5.0` under every profile while the table
//!   called it dependent — an OVER-BROAD entry that only a real compilation could expose.
//!   That case now differs across profiles, and it is the measurement that the forwarding
//!   works, not a restatement that it exists.
//!
//! - Host-target only (compiles and runs a native binary), same harness style as
//!   `php_version_surface_tests`. Every case is compiled inside the managed-PCRE2 fixture,
//!   because the eval bridge pulls in regex support.

use std::fs;
#[path = "support/managed_pcre2.rs"]
mod managed_pcre2_support;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use elephc::php_profile::sensitivity;
use elephc::web_prelude::PhpVersion;

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// One corpus program, named so a failure says which construct broke the claim.
struct Case {
    /// Identifier used in assertion messages.
    name: &'static str,
    /// The PHP source, compiled once per maintained profile.
    source: &'static str,
}

/// The dedicated corpus.
///
/// Intentionally small and hand-chosen rather than harvested from `tests/codegen`: every
/// entry is here to pin one specific claim, and the cost of the test is
/// `len * PhpVersion::ALL.len()` full compile-and-link cycles, so an incidental corpus would
/// buy noise at a real price.
///
/// The independent half is not filler. `major_release_extra_are_invariant` is the negative
/// control for the three `PHP_*` constants deliberately omitted from the table;
/// `version_name_in_a_string` is the negative control for the constant/string matching
/// split; and `nan_bool_diagnostic_only` is the negative control for
/// `SensitivityKind::Diagnostic` — it is the one program here whose ASSEMBLY differs across
/// profiles while its stdout does not.
const CORPUS: &[Case] = &[
    // ---- expected profile-INDEPENDENT ----
    Case {
        name: "plain_echo",
        source: r#"<?php echo "hello";"#,
    },
    Case {
        name: "arithmetic_and_strings",
        source: r#"<?php $a = 6 * 7; echo strtoupper("v") . $a;"#,
    },
    Case {
        name: "array_pipeline",
        source: r#"<?php $a = [3, 1, 2]; sort($a); echo implode(",", $a);"#,
    },
    Case {
        name: "class_and_method",
        source: r#"<?php class C { public function f(): int { return 7; } } $c = new C(); echo $c->f();"#,
    },
    Case {
        name: "major_release_extra_are_invariant",
        source: r#"<?php echo PHP_MAJOR_VERSION, "|", PHP_RELEASE_VERSION, "|", PHP_EXTRA_VERSION;"#,
    },
    Case {
        name: "sapi_is_a_different_axis",
        source: r#"<?php echo PHP_SAPI, "|", php_sapi_name();"#,
    },
    Case {
        name: "version_name_in_a_string",
        source: r#"<?php echo "needs PHP_VERSION_ID >= 80400";"#,
    },
    Case {
        name: "nan_bool_diagnostic_only",
        source: r#"<?php $f = NAN; if ($f) { echo "truthy"; } else { echo "falsy"; }"#,
    },
    // `ini_get` is profile-dependent for OPcache directives and profile-independent for
    // everything else. Without this case the table's coarse "ini_get is dependent" entry
    // passed the suite while being wrong for the common caller — the omission that motivated
    // `Symbol::function_with_arg_prefixes`.
    Case {
        name: "ini_get_unrelated_directive",
        source: r#"<?php var_dump(ini_get("precision"));"#,
    },
    Case {
        name: "eval_fragment_without_the_version_surface",
        source: r#"<?php eval('echo 1 + 1;');"#,
    },
    // ---- expected profile-DEPENDENT ----
    // The eval boundary. The fragment runs in the linked interpreter rather than through
    // codegen, so before `__elephc_eval_set_php_version_id` existed this program printed
    // `8.5.0` under EVERY profile — observably independent while the table called it
    // dependent. That is precisely the mismatch this suite is built to catch, and it is what
    // makes this case a measurement of the bridge rather than a restatement of it.
    Case {
        name: "eval_reads_the_version_surface",
        source: r#"<?php eval('echo PHP_VERSION, "|", PHP_VERSION_ID, "|", PHP_MINOR_VERSION;');"#,
    },
    Case {
        name: "eval_calls_phpversion",
        source: r#"<?php eval('echo phpversion();');"#,
    },
    Case {
        name: "version_id_printed",
        source: r#"<?php echo PHP_VERSION_ID;"#,
    },
    Case {
        name: "version_string_printed",
        source: r#"<?php echo PHP_VERSION;"#,
    },
    Case {
        name: "minor_version_printed",
        source: r#"<?php echo PHP_MINOR_VERSION;"#,
    },
    Case {
        name: "phpversion_printed",
        source: r#"<?php echo phpversion();"#,
    },
    Case {
        name: "zend_version_printed",
        source: r#"<?php echo zend_version();"#,
    },
    Case {
        name: "version_gate_branches",
        source: r#"<?php if (PHP_VERSION_ID >= 80400) { echo "new"; } else { echo "old"; }"#,
    },
    Case {
        name: "opcache_configuration_shape",
        source: r#"<?php $c = opcache_get_configuration(); echo count($c["directives"]);"#,
    },
    Case {
        name: "ini_get_opcache_directive",
        source: r#"<?php var_dump(ini_get("opcache.jit"));"#,
    },
];

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Compiles `source` at `profile` and returns the compiled program's STDOUT.
///
/// Stderr is deliberately discarded: it carries the runtime warning stream, which is the
/// half of observable behavior that `SensitivityKind::Diagnostic` is allowed to differ in.
///
/// Every case gets the managed-PCRE2 fixture, not just the ones that need it. The corpus
/// covers `eval()`, whose fragments run in the linked interpreter rather than through
/// codegen, and that bridge pulls in regex support; configuring it per case would make the
/// fixture a property of the case rather than of the harness, and a corpus that cannot grow
/// an eval case without also editing the runner is a corpus that will not grow one.
fn compile_and_run(dir: &Path, source: &str, profile: &str, case: &str) -> String {
    let php = dir.join("prog.php");
    fs::write(&php, source).expect("failed to write corpus program");

    let mut command = Command::new(elephc_bin());
    command.arg("build");
    command.env("XDG_CACHE_HOME", dir.join("cache-root"));
    managed_pcre2_support::configure_host_managed_pcre2(&mut command, dir);
    let compile = command
        .args(["--php-version", profile, "prog.php"])
        .current_dir(dir)
        .output()
        .expect("failed to spawn elephc");
    assert!(
        compile.status.success(),
        "case `{case}` failed to compile at --php-version {profile}:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(dir.join("prog"))
        .current_dir(dir)
        .output()
        .expect("failed to run compiled program");
    assert!(
        run.status.success(),
        "case `{case}` exited non-zero at --php-version {profile}:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// Returns what the shipping table predicts for `source`: `true` when it claims the profile
/// choice is unobservable.
fn table_predicts_independent(source: &str) -> bool {
    let tokens = elephc::lexer::tokenize(source).expect("corpus program must tokenize");
    let program = elephc::parser::parse(&tokens).expect("corpus program must parse");
    sensitivity::scan(&program, false).is_empty()
}

/// THE check: for one corpus program, the table's prediction must match what the compiler
/// actually does across every maintained profile.
///
/// A failure here means one of two things, and the message says which: the table claims
/// independence for a program whose output moves (a MISSING symbol — the dangerous
/// direction, because it means the compiler stayed silent about a real choice), or it claims
/// dependence for a program whose output never moves (an OVER-BROAD entry — noisy, not
/// unsound).
///
/// One test PER CASE rather than one loop over all of them. A case is an independent claim,
/// so nextest names the one that broke instead of the loop that contained it, and each gets
/// its own timeout budget. The loop version compiled `len * 4` programs inside a single test
/// and blew CI's 60-second per-test cap — 20 seconds of real work per case does not become
/// acceptable by being summed.
fn check_case(name: &str) {
    let case = CORPUS
        .iter()
        .find(|candidate| candidate.name == name)
        .unwrap_or_else(|| panic!("no corpus case named `{name}`"));
    {
        let predicted_independent = table_predicts_independent(case.source);

        // A directory per PROFILE, not per case: the managed-PCRE2 fixture copies read-only
        // archives into the project, so preparing it twice in one directory fails on the
        // second copy. One profile, one project.
        let mut dirs: Vec<PathBuf> = Vec::new();
        let outputs: Vec<(String, String)> = PhpVersion::MAINTAINED
            .iter()
            .map(|profile| {
                let spelling = format!("{}.{}", profile.major(), profile.minor());
                let dir = make_test_dir(&format!(
                    "elephc_profile_{}_{}",
                    case.name,
                    spelling.replace('.', "_")
                ));
                let stdout = compile_and_run(&dir, case.source, &spelling, case.name);
                dirs.push(dir);
                (spelling, stdout)
            })
            .collect();

        let baseline = &outputs[0].1;
        let observed_independent = outputs.iter().all(|(_, out)| out == baseline);

        assert_eq!(
            predicted_independent,
            observed_independent,
            "case `{}`: the sensitivity table says {}, the compiler says {}.\nOutputs: {:?}\n{}",
            case.name,
            if predicted_independent { "INDEPENDENT" } else { "DEPENDENT" },
            if observed_independent { "INDEPENDENT" } else { "DEPENDENT" },
            outputs,
            if predicted_independent {
                "The table is MISSING a symbol: the compiler silently changed behavior for a \
                 program the table promised was profile-independent."
            } else {
                "The table is OVER-BROAD: it would report a dependence the compiler does not \
                 actually have, producing a diagnostic for nothing."
            },
        );

        for dir in &dirs {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

/// Generates one `#[test]` per corpus case, and records which names are covered.
macro_rules! case_tests {
    ($($name:ident,)*) => {
        /// Every case name carrying a generated test, cross-checked against [`CORPUS`] by
        /// `corpus_and_tests_agree` so neither list can quietly outgrow the other.
        const COVERED: &[&str] = &[$(stringify!($name),)*];
        $(
            #[test]
            fn $name() {
                check_case(stringify!($name));
            }
        )*
    };
}

case_tests!(
    plain_echo,
    arithmetic_and_strings,
    array_pipeline,
    class_and_method,
    major_release_extra_are_invariant,
    sapi_is_a_different_axis,
    version_name_in_a_string,
    nan_bool_diagnostic_only,
    ini_get_unrelated_directive,
    eval_fragment_without_the_version_surface,
    eval_reads_the_version_surface,
    eval_calls_phpversion,
    version_id_printed,
    version_string_printed,
    minor_version_printed,
    phpversion_printed,
    zend_version_printed,
    version_gate_branches,
    opcache_configuration_shape,
    ini_get_opcache_directive,
);

/// The corpus and the generated tests name exactly the same cases.
///
/// Splitting one loop into per-case tests bought better failure messages at the cost of a new
/// blind spot: a case added to [`CORPUS`] but not to `case_tests!` would simply never run,
/// and the file would stay green while covering less. This closes it in both directions.
#[test]
fn corpus_and_tests_agree() {
    for case in CORPUS {
        assert!(
            COVERED.contains(&case.name),
            "corpus case `{}` has no generated test — add it to `case_tests!`",
            case.name
        );
    }
    for name in COVERED {
        assert!(
            CORPUS.iter().any(|case| case.name == *name),
            "generated test `{name}` has no corpus case"
        );
    }
}

/// Guards this file against becoming vacuous.
///
/// Both implications are only meaningful together, so the corpus must keep exercising both
/// sides. If a future edit leaves every case on one side, `table_prediction_matches_\
/// observed_behavior` would still pass while proving nothing.
#[test]
fn corpus_exercises_both_arms() {
    let independent = CORPUS
        .iter()
        .filter(|case| table_predicts_independent(case.source))
        .count();
    let dependent = CORPUS.len() - independent;
    assert!(
        independent >= 4,
        "corpus must keep at least 4 profile-independent cases, has {independent}"
    );
    assert!(
        dependent >= 4,
        "corpus must keep at least 4 profile-dependent cases, has {dependent}"
    );
}
