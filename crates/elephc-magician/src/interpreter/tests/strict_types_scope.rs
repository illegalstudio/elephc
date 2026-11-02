//! Purpose:
//! Interpreter tests pinning WHERE `declare(strict_types=1)` applies, which is not where the
//! declaration it governs lives.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::strict_types_scope`.
//!
//! Key details:
//! - php scopes the directive to the file containing the code doing the coercing. An argument is
//!   coerced at the CALL, so the caller's file decides; a `return` value is coerced at the
//!   `return`, so the callee's file decides. One rule, two halves that point opposite ways.
//! - The mode was a single context flag set when the directive executed and never restored, so
//!   one strict included file turned the whole rest of the program strict. Symfony's vendor tree
//!   is almost entirely `declare(strict_types=1)`, and an interpreted container includes it.
//! - Every expected string is `php -n` 8.5.6's output on the same two files.

use super::super::*;
use super::support::*;

/// Runs a caller fragment against files written to disk and returns what it echoed.
fn run_scope_fixture(tag: &str, files: &[(&str, &str)], fragment: &[u8]) -> String {
    let dir = std::env::temp_dir().join(format!(
        "elephc-magician-strict-scope-{}-{}",
        std::process::id(),
        tag
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create strict scope fixture directory");
    for (name, contents) in files {
        std::fs::write(dir.join(name), contents).expect("write strict scope fixture");
    }
    let program = parse_fragment(fragment).expect("parse strict scope caller fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site(
        dir.join("app.php").to_string_lossy().into_owned(),
        dir.to_string_lossy().into_owned(),
        1,
    );
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute strict scope caller fragment");
    let _ = std::fs::remove_dir_all(&dir);
    values.output.clone()
}

/// A strict vendor file: two functions, a class, and one call written inside it.
const STRICT_VENDOR: &str = r#"<?php declare(strict_types=1);
function sf(int $x): int { return $x + 1; }
function sret(): int { return "5"; }
function calls_out() {
    try { return "ok:" . mf("5"); } catch (TypeError $e) { return "TypeError"; }
}
class SC {
    public function take(int $x): int { return $x + 1; }
    public function bad(): int { return "9"; }
}
echo "loaded;";
"#;

/// Verifies all six ways the two files can reach each other, in one run.
///
/// `php -n` 8.5.6 prints `loaded;sf:6;mf:6;out:TypeError;sret:TypeError;nret:5;take:6;bad:TypeError;`.
///
/// Read as three pairs, each pair being one claim that only holds if the mode is scoped:
///
/// - `sf:6` and `take:6` -- a function and a method DECLARED in the strict file, called from the
///   lenient one, coerce their arguments. The call site decides, not the declaration.
/// - `out:TypeError` -- a call WRITTEN in the strict file is strict even though the callee was
///   declared in the lenient one. Same rule, other direction.
/// - `sret:TypeError` and `bad:TypeError` against `nret:5` -- a `return` obeys the file the body
///   was declared in, so the same lenient caller gets a throw from one and a coercion from the
///   other.
///
/// A single global flag can produce at most one of those three answers.
#[test]
fn a_strict_file_governs_its_own_calls_and_returns_and_no_others() {
    assert_eq!(
        run_scope_fixture(
            "both_ways",
            &[("vendor.php", STRICT_VENDOR)],
            br#"function mf(int $x): int { return $x + 1; }
function nret(): int { return "5"; }
include "vendor.php";
try { $r = sf("5"); echo "sf:", $r, ";"; } catch (\TypeError $e) { echo "sf:TypeError;"; }
try { $r = mf("5"); echo "mf:", $r, ";"; } catch (\TypeError $e) { echo "mf:TypeError;"; }
echo "out:", calls_out(), ";";
try { $r = sret(); echo "sret:", $r, ";"; } catch (\TypeError $e) { echo "sret:TypeError;"; }
try { $r = nret(); echo "nret:", $r, ";"; } catch (\TypeError $e) { echo "nret:TypeError;"; }
$o = new SC();
try { $r = $o->take("5"); echo "take:", $r, ";"; } catch (\TypeError $e) { echo "take:TypeError;"; }
try { $r = $o->bad(); echo "bad:", $r, ";"; } catch (\TypeError $e) { echo "bad:TypeError;"; }"#,
        ),
        "loaded;sf:6;mf:6;out:TypeError;sret:TypeError;nret:5;take:6;bad:TypeError;",
    );
}

/// Verifies the directive stops at the included file's edge.
///
/// `php -n` 8.5.6 prints `before:6;loaded;after:6;`. This is the failure that mattered in
/// practice: the flag was set when the directive executed and never put back, so every call the
/// program made after including one strict vendor file was checked strictly. The same call is
/// made on both sides of the include so the assertion cannot pass by the mode never changing.
#[test]
fn an_included_files_directive_does_not_leak_into_the_file_that_included_it() {
    assert_eq!(
        run_scope_fixture(
            "no_leak",
            &[("vendor.php", STRICT_VENDOR)],
            br#"function mf(int $x): int { return $x + 1; }
try { $r = mf("5"); echo "before:", $r, ";"; } catch (\TypeError $e) { echo "before:TypeError;"; }
include "vendor.php";
try { $r = mf("5"); echo "after:", $r, ";"; } catch (\TypeError $e) { echo "after:TypeError;"; }"#,
        ),
        "before:6;loaded;after:6;",
    );
}
