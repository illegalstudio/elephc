//! Purpose:
//! End-to-end value tests for reflecting on a class that a file included at RUN TIME declares.
//! Symfony's container reflects on classes that arrive through autoload, which is exactly this
//! path, and a compiled binary reported `Fatal error: eval() runtime failed` for it.
//!
//! Called from:
//! - `cargo test --test codegen_tests include_reflection` through Rust's test harness.
//!
//! Key details:
//! - The include path in `main.php` is DELIBERATELY not a string literal. A literal path is
//!   resolved and inlined at compile time, which puts the included declarations into the program
//!   the checker walks and makes the fixture prove nothing. A computed path is the real shape:
//!   the file is chosen at run time and its text is executed by the interpreter.
//! - For the same reason `main.php` must never name a Reflection class, call `eval`, use
//!   `new $c`, or call `class_get_attributes`. Each of those independently makes
//!   `types::checker::builtin_types::reflection::gate::program_may_reference_reflection` answer
//!   true, and the fixture would then pass without exercising anything.
//! - The `eval()` twin is the control. It passes today because `eval` sets
//!   `prelude_prune::usage::introspects`, which that gate does read. The pair is what places the
//!   failure in the gate rather than in the interpreter.
//! - Every expected string is `php -n` 8.5.6's output on the same two files.

use crate::support::*;

/// The included file: it declares the class and reflects on it, naming Reflection nowhere else.
const REFLECTING_INCLUDE: &str = r#"<?php
class S1Plain { public int $id = 4; }
echo "loaded;";
$r = new ReflectionClass("S1Plain");
echo "reflected;", $r->getName(), ";";
echo $r->getProperty("id")->getType()->getName(), ";done";
"#;

/// Verifies a runtime-included file can reflect on the class it just declared.
///
/// `php -n` 8.5.6 prints `loaded;reflected;S1Plain;int;done`. Before the Reflection surface
/// gate learned about runtime includes, a compiled binary printed `loaded;` and then
/// `Fatal error: eval() runtime failed`: the checker never registered `ReflectionClass`, so its
/// metadata was never emitted, `__rt_new_by_name` answered null for it, and the interpreter can
/// only report that as a fatal.
#[test]
fn test_runtime_include_can_reflect_on_the_class_it_declares() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n$name = \"piece\" . \".php\";\ninclude $name;\n",
            ),
            ("piece.php", REFLECTING_INCLUDE),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded;reflected;S1Plain;int;done");
}

/// Verifies the caller can reflect on a class a runtime include declared.
///
/// `php -n` 8.5.6 prints `loaded;S1Plain;`. The reflection sits in the compiled file here, so
/// the name IS spelled where the checker can read it; this is the half that already worked, and
/// it is pinned so a later narrowing of the gate cannot take it away.
#[test]
fn test_caller_reflects_on_a_class_a_runtime_include_declared() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n$name = \"piece\" . \".php\";\ninclude $name;\necho (new ReflectionClass(\"S1Plain\"))->getName(), \";\";\n",
            ),
            (
                "piece.php",
                "<?php\nclass S1Plain { public int $id = 4; }\necho \"loaded;\";\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded;S1Plain;");
}

/// Verifies the same reflecting source reaches the same value through `eval()`.
///
/// This is the control for the first case: `php -n` 8.5.6 prints
/// `loaded;reflected;S1Plain;int;done` here too. `eval` sets `usage.introspects`, which the
/// Reflection gate reads, so this path registers the whole family and has always worked. If this
/// test passes while the first fails, the difference is the gate, not the interpreter.
#[test]
fn test_eval_can_reflect_on_the_class_it_declares() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class S1Plain { public int $id = 4; }
echo "loaded;";
$r = new ReflectionClass("S1Plain");
echo "reflected;", $r->getName(), ";";
echo $r->getProperty("id")->getType()->getName(), ";done";
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(out, "loaded;reflected;S1Plain;int;done");
}
