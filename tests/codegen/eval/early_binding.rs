//! Purpose:
//! Checks that a fragment run by the eval interpreter binds its top-level declarations before
//! its first statement, as PHP binds them when it compiles the fragment.
//!
//! Called from:
//! - The codegen eval integration test module.
//!
//! Key details:
//! - Runtime-selected source forces Magician instead of literal-eval AOT lowering.
//! - Every expected string was produced by running the same fragment under PHP 8.5.12.

use super::*;

/// Builds dynamic eval source without allowing compile-time evaluation of the string.
fn dynamic_eval(body: &str) -> String {
    format!("<?php\n$code = $argc > 1 ? $argv[1] : <<<'CODE'\n{body}\nCODE;\neval($code);\n")
}

/// Verifies functions and early-bound class-likes are usable above their declaration, while
/// the shapes PHP links at run time exist only after their statement.
#[test]
fn test_dynamic_eval_binds_declarations_before_the_first_statement() {
    let source = dynamic_eval(r#"
echo greet(), "|", (new Plain())->name(), "|", Child::KIND, "|";
echo interface_exists("Marker") ? "yes" : "no", trait_exists("Mixin") ? "yes" : "no", "|";
echo class_exists("Tagged") ? "yes" : "no", enum_exists("Suit") ? "yes" : "no", "|";
function greet() { return "hi"; }
class Plain { public function name() { return "plain"; } }
class Child extends Plain { const KIND = "child"; }
interface Marker {}
trait Mixin {}
class Tagged implements Marker {}
enum Suit { case Hearts; }
echo class_exists("Tagged") ? "yes" : "no", enum_exists("Suit") ? "yes" : "no";
"#);
    assert_eq!(compile_and_run(&source), "hi|plain|child|yesyes|nono|yesyes");
}

/// Verifies a file included at run time binds its declarations before its first statement.
#[test]
fn test_dynamic_eval_included_file_binds_declarations_before_the_first_statement() {
    let source = dynamic_eval("include 'piece.php';");
    let files = [
        ("main.php", source.as_str()),
        ("piece.php", "<?php echo piece(), class_exists('Piece') ? 'yes' : 'no'; function piece() { return 'p|'; } class Piece {}"),
    ];
    assert_eq!(compile_and_run_files(&files, "main.php"), "p|yes");
}

/// Verifies a jump over an early-bound class keeps it declared, and a jump back over it does
/// not declare it twice.
#[test]
fn test_dynamic_eval_jump_across_early_bound_class() {
    let source = dynamic_eval(r#"
$i = 0;
goto first;
class Skipped { const V = "s"; }
first:
again:
class Repeated {}
$i++;
if ($i < 2) goto again;
echo Skipped::V, $i;
"#);
    assert_eq!(compile_and_run(&source), "s2");
}

/// Verifies a class bound before the first statement reads members whose initializers depend
/// on a later statement, each evaluated at its first use.
#[test]
fn test_dynamic_eval_early_bound_class_evaluates_members_at_first_use() {
    let source = dynamic_eval(r#"
echo class_exists("Lazy") ? "yes" : "no", "|";
define("BASE", 1);
echo Lazy::K, Lazy::$count, (new Lazy())->label;
class Lazy { const K = BASE; public static $count = BASE + 1; public $label = Other::NAME; }
class Other { const NAME = "o"; }
"#);
    assert_eq!(compile_and_run(&source), "yes|12o");
}

/// Verifies when a class copies its parent's defaults: before the first statement for a class
/// bound early, which then evaluates them again, and at its statement for a class with
/// `__toString`, which copies the value its parent already evaluated.
#[test]
fn test_dynamic_eval_class_copies_parent_defaults_when_it_is_bound() {
    let handler = "<?php\nset_error_handler(function ($level, $message) { echo '[d]'; return true; });\n";
    let source = dynamic_eval(r#"
class P { public $a = DATE_RFC7231; }
echo "p"; new P;
class S extends P { function __toString() { return ""; } }
echo "s"; new S;
class C extends P {}
echo "c"; new C;
"#)
    .replacen("<?php\n", handler, 1);
    assert_eq!(compile_and_run(&source), "p[d]sc[d]");
}
