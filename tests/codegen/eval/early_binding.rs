//! Purpose:
//! Checks that a fragment run by the eval interpreter binds its top-level declarations before
//! its first statement, as PHP binds them when it compiles the fragment.
//!
//! Called from:
//! - The codegen eval integration test module.
//!
//! Key details:
//! - Runtime-selected source forces Magician instead of literal-eval AOT lowering.
//! - The stdout-only cases run as one bundle, so the builtin catalog a runtime-selected
//!   fragment lowers is compiled once. Their declarations share one namespace across the
//!   bundle, so every case uses its own names.
//! - Every expected string was produced by running the same fragment under PHP 8.5.12.

use super::*;

/// Builds dynamic eval source without allowing compile-time evaluation of the string.
fn dynamic_eval(body: &str) -> String {
    format!("<?php\n$code = $argc > 1 ? $argv[1] : <<<'CODE'\n{body}\nCODE;\neval($code);\n")
}

/// Verifies, in one compiled program, what a fragment binds before its first statement:
/// - `declarations`: functions and early-bound class-likes are usable above their declaration,
///   while the shapes PHP links at run time exist only after their statement;
/// - `jump`: a jump over an early-bound class keeps it declared, and a jump back over one does
///   not declare it twice;
/// - `lazy_members`: an early-bound class reads members whose initializers depend on a later
///   statement, each evaluated at its first use;
/// - `undecidable_override`: a child whose override names a class declared by an earlier
///   statement is linked when its own statement runs;
/// - `compiled_parent`: the same holds when the parent is a class of the compiled program;
/// - `parent_defaults`: a class copies its parent's defaults when it is bound, before the
///   first statement for an early-bound class, which then evaluates them again, and at its
///   statement for a class with `__toString`, which copies the value already evaluated. It
///   installs an error handler, so it runs last.
#[test]
fn test_eval_early_binding_bundle() {
    let declarations = dynamic_eval(r#"
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
    let jump = dynamic_eval(r#"
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
    let lazy_members = dynamic_eval(r#"
echo class_exists("Lazy") ? "yes" : "no", "|";
define("BASE", 1);
echo Lazy::K, Lazy::$count, (new Lazy())->label;
class Lazy { const K = BASE; public static $count = BASE + 1; public $label = Other::NAME; }
class Other { const NAME = "o"; }
"#);
    let undecidable_override = dynamic_eval(r#"
echo class_exists("Shelf") ? "yes" : "no", class_exists("Library") ? "yes" : "no", "|";
if (true) { class Item {} class Book extends Item {} }
class Shelf { function pick(): Item { return new Item(); } }
class Library extends Shelf { function pick(): Book { return new Book(); } }
echo class_exists("Library") ? "yes" : "no", get_class((new Library())->pick());
"#);
    let compiled_parent = dynamic_eval(r#"
echo class_exists("EvalShelf") ? "yes" : "no", "|";
class EvalShelf extends NativeShelf { function pick(): LaterBox { return new LaterBox(); } }
class LaterBox extends stdClass {}
echo class_exists("EvalShelf") ? "yes" : "no", get_class((new EvalShelf())->pick());
"#)
    .replacen("<?php\n", "<?php\nclass NativeShelf { function pick(): stdClass { return new stdClass(); } }\n", 1);
    let handler = "<?php\nset_error_handler(function ($level, $message) { echo '[d]'; return true; });\n";
    let parent_defaults = dynamic_eval(r#"
class Dated { public $a = DATE_RFC7231; }
echo "p"; new Dated;
class DatedText extends Dated { function __toString() { return ""; } }
echo "s"; new DatedText;
class DatedChild extends Dated {}
echo "c"; new DatedChild;
"#)
    .replacen("<?php\n", handler, 1);
    assert_bundle(&[
        BundleCase { name: "declarations", source: &declarations, expected: "hi|plain|child|yesyes|nono|yesyes" },
        BundleCase { name: "jump", source: &jump, expected: "s2" },
        BundleCase { name: "lazy_members", source: &lazy_members, expected: "yes|12o" },
        BundleCase { name: "undecidable_override", source: &undecidable_override, expected: "yesno|yesBook" },
        BundleCase { name: "compiled_parent", source: &compiled_parent, expected: "no|yesLaterBox" },
        BundleCase { name: "parent_defaults", source: &parent_defaults, expected: "p[d]sc[d]" },
    ]);
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
