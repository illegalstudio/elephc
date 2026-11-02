//! Purpose:
//! Bundles several complete PHP fixtures into one compiled program so a family of tests that
//! only compare stdout pays for one compile, link and run instead of one per fixture, while a
//! failure still names the fixture that produced it.
//!
//! Called from:
//! - Codegen integration tests that group same-shaped fixtures (`assert_bundle`,
//!   `assert_bundle_with`).
//!
//! Key details:
//! - Each case is a tagged script. The bundle strips the opening tag, prefixes the body with a
//!   marker `echo`, and concatenates the bodies in order; stdout is split on the markers.
//! - Cases share one global scope and one declaration namespace, so function, class and
//!   constant names must be unique across the bundle, top-level variables carry over, and a
//!   fatal error or `exit` in one case ends every later one. `declare(strict_types=1)` applies
//!   to a whole file and is refused, as is anything else that must be the first statement.
//! - The win is proportional to the compile cost the cases share: a bundle of opaque-eval
//!   fixtures amortizes the whole lowered builtin catalog, a bundle of hello-worlds saves little.

use super::*;

/// One fixture of a bundle: a complete tagged PHP script and the exact stdout it must print.
pub(crate) struct BundleCase<'a> {
    /// Name reported when the case's output differs; use the former test function's name.
    pub name: &'a str,
    /// The fixture, starting with `<?php`.
    pub source: &'a str,
    /// The stdout the fixture prints on its own.
    pub expected: &'a str,
}

/// Separator the marker `echo` prints around a case name; a control character no fixture emits.
const MARKER_SEPARATOR: char = '\x1f';
/// Marker prefix the bundled program echoes before each case runs.
const MARKER_PREFIX: &str = "__ELEPHC_BUNDLE_CASE__:";

/// Joins the cases into one tagged program, each body preceded by its marker `echo`.
///
/// Panics when a case is not a tagged script, when it opens with a `declare` (which must stay
/// the first statement of its own file), or when two cases share a name.
pub(crate) fn bundle_source(cases: &[BundleCase<'_>]) -> String {
    let mut names = std::collections::HashSet::new();
    let mut program = String::from("<?php\n");
    for case in cases {
        assert!(
            names.insert(case.name),
            "bundle case `{}` is listed twice",
            case.name
        );
        let body = case.source.strip_prefix("<?php").unwrap_or_else(|| {
            panic!("bundle case `{}` must start with `<?php`", case.name)
        });
        assert!(
            !body.trim_start().starts_with("declare("),
            "bundle case `{}` opens with `declare(...)`, which applies to a whole file; keep it as its own test",
            case.name
        );
        assert!(
            !case.name.contains(MARKER_SEPARATOR),
            "bundle case name `{}` contains the marker separator",
            case.name
        );
        program.push_str(&format!(
            "echo \"\\x1f{MARKER_PREFIX}{}\\x1f\";\n",
            case.name
        ));
        program.push_str(body);
        program.push('\n');
    }
    program
}

/// Splits bundled stdout into one output per case, in case order.
///
/// Returns an error naming the first case whose marker is missing or out of order: a fatal
/// error or `exit` in an earlier case ends the program before the later markers print.
pub(crate) fn split_bundle_output(
    cases: &[BundleCase<'_>],
    stdout: &str,
) -> Result<Vec<String>, String> {
    let mut outputs = Vec::with_capacity(cases.len());
    let mut rest = stdout;
    for (index, case) in cases.iter().enumerate() {
        let marker = format!("{MARKER_SEPARATOR}{MARKER_PREFIX}{}{MARKER_SEPARATOR}", case.name);
        let Some(start) = rest.find(&marker) else {
            return Err(format!(
                "bundle case `{}` never started; output after the previous case was:\n{rest}",
                case.name
            ));
        };
        if index == 0 && start != 0 {
            return Err(format!(
                "bundle printed before its first case:\n{}",
                &rest[..start]
            ));
        }
        let after = &rest[start + marker.len()..];
        let end = cases
            .get(index + 1)
            .and_then(|next| {
                after.find(&format!(
                    "{MARKER_SEPARATOR}{MARKER_PREFIX}{}{MARKER_SEPARATOR}",
                    next.name
                ))
            })
            .unwrap_or(after.len());
        outputs.push(after[..end].to_string());
        rest = &after[end..];
    }
    Ok(outputs)
}

/// Compiles the bundle once through `run`, splits its stdout, and compares every case, panicking
/// with each mismatching case's name, expected and actual output.
pub(crate) fn assert_bundle_with(cases: &[BundleCase<'_>], run: impl FnOnce(&str) -> String) {
    let stdout = run(&bundle_source(cases));
    let outputs = match split_bundle_output(cases, &stdout) {
        Ok(outputs) => outputs,
        Err(error) => panic!("{error}"),
    };
    let mismatches: Vec<String> = cases
        .iter()
        .zip(&outputs)
        .filter(|(case, actual)| case.expected != actual.as_str())
        .map(|(case, actual)| {
            format!(
                "bundle case `{}` printed:\n{actual}\nexpected:\n{}",
                case.name, case.expected
            )
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} of {} bundle cases failed:\n\n{}",
        mismatches.len(),
        cases.len(),
        mismatches.join("\n\n")
    );
}

/// Compiles the bundle with the default runner and compares every case's stdout.
pub(crate) fn assert_bundle(cases: &[BundleCase<'_>]) {
    assert_bundle_with(cases, compile_and_run);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two cases with the marker echoes the bundler emits.
    fn two_cases() -> [BundleCase<'static>; 2] {
        [
            BundleCase { name: "first", source: "<?php echo 'a';", expected: "a" },
            BundleCase { name: "second", source: "<?php echo 'b';", expected: "b" },
        ]
    }

    /// The bundled program keeps one opening tag and prefixes every body with its marker echo.
    #[test]
    fn bundle_source_prefixes_each_case_with_its_marker() {
        let program = bundle_source(&two_cases());
        assert_eq!(program.matches("<?php").count(), 1);
        assert!(program.contains("echo \"\\x1f__ELEPHC_BUNDLE_CASE__:first\\x1f\";\n echo 'a';"));
        assert!(program.contains("echo \"\\x1f__ELEPHC_BUNDLE_CASE__:second\\x1f\";\n echo 'b';"));
    }

    /// Stdout splits on the markers into one output per case, in order.
    #[test]
    fn split_bundle_output_returns_one_output_per_case() {
        let stdout = "\x1f__ELEPHC_BUNDLE_CASE__:first\x1fa\x1f__ELEPHC_BUNDLE_CASE__:second\x1fb";
        assert_eq!(
            split_bundle_output(&two_cases(), stdout).unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    /// A program that ended before a later case names that case.
    #[test]
    fn split_bundle_output_names_the_case_that_never_started() {
        let stdout = "\x1f__ELEPHC_BUNDLE_CASE__:first\x1fa";
        let error = split_bundle_output(&two_cases(), stdout).unwrap_err();
        assert!(error.contains("bundle case `second` never started"), "{error}");
    }

    /// A mismatch names the failing case and leaves the passing one out of the report.
    #[test]
    #[should_panic(expected = "bundle case `second` printed")]
    fn assert_bundle_with_names_the_mismatching_case() {
        assert_bundle_with(&two_cases(), |_| {
            "\x1f__ELEPHC_BUNDLE_CASE__:first\x1fa\x1f__ELEPHC_BUNDLE_CASE__:second\x1fwrong".to_string()
        });
    }

    /// A `declare` opener is refused, since strict types apply to the whole bundled file.
    #[test]
    #[should_panic(expected = "opens with `declare(...)`")]
    fn bundle_source_refuses_file_wide_declares() {
        bundle_source(&[BundleCase {
            name: "strict",
            source: "<?php declare(strict_types=1); echo 1;",
            expected: "1",
        }]);
    }
}
