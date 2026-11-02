//! Purpose:
//! Interpreter tests for reflecting on a class declared by a file included at run time.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::include_reflection`.
//!
//! Key details:
//! - Symfony's container reflects on classes that arrive through autoload, which is this path,
//!   and a compiled binary reported `Fatal error: eval() runtime failed` for it.
//! - These cases run the interpreter against a complete class table, so they answer whether the
//!   INTERPRETER can reflect on an include-declared class. It can; what a compiled binary lacks
//!   is the ReflectionClass metadata, which `types::checker::builtin_types::reflection::gate`
//!   decides to emit without ever consulting `prelude_prune::usage::includes_runtime_php`.
//! - Every expected string is `php -n` 8.5.6's output on the same fixture.

use super::super::*;
use super::support::*;

/// Runs one fixture set through the interpreter and returns its output.
///
/// The fragment plays the part of the compiled entry point, and the files it includes are read
/// from disk at run time exactly as an autoloader's include does.
fn run_include_fixture(tag: &str, files: &[(&str, &str)], fragment: &[u8]) -> String {
    let dir = std::env::temp_dir().join(format!(
        "elephc-magician-include-reflection-{}-{}",
        std::process::id(),
        tag
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create include reflection fixture directory");
    for (name, contents) in files {
        std::fs::write(dir.join(name), contents).expect("write include reflection fixture");
    }
    let program = parse_fragment(fragment).expect("parse include reflection fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site(
        dir.join("main.php").to_string_lossy().into_owned(),
        dir.to_string_lossy().into_owned(),
        1,
    );
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute include reflection fragment");
    let _ = std::fs::remove_dir_all(&dir);
    values.output.clone()
}

/// The included file behind every case here: one class with one typed property.
const TYPED_PROPERTY_CLASS: &str =
    "<?php class S1Plain { public int $id = 4; } echo \"loaded;\";";

/// Verifies the caller can reflect on a class an included file declared.
///
/// `php -n` 8.5.6 prints `loaded;reflected;S1Plain;`. A compiled binary printed `loaded;` and
/// then `Fatal error: eval() runtime failed`, so pinning the interpreter half green is what
/// places that failure in the metadata the binary was built with rather than in this code.
#[test]
fn reflection_class_reads_a_class_declared_by_a_runtime_include() {
    assert_eq!(
        run_include_fixture(
            "caller",
            &[("piece.php", TYPED_PROPERTY_CLASS)],
            br#"include "piece.php"; $r = new ReflectionClass("S1Plain"); echo "reflected;", $r->getName(), ";";"#,
        ),
        "loaded;reflected;S1Plain;",
    );
}

/// Verifies the included file can reflect on its own class, inside the same include frame.
///
/// `php -n` 8.5.6 prints `loaded;reflected;S1Plain;done`. The include frame pushes its own call
/// site and file magic, so this asks whether the class table is visible from inside that frame.
#[test]
fn reflection_class_works_inside_the_include_frame_that_declared_the_class() {
    assert_eq!(
        run_include_fixture(
            "inside",
            &[(
                "piece.php",
                "<?php class S1Plain { public int $id = 4; } echo \"loaded;\"; $r = new ReflectionClass(\"S1Plain\"); echo \"reflected;\", $r->getName(), \";\";",
            )],
            br#"include "piece.php"; echo "done";"#,
        ),
        "loaded;reflected;S1Plain;done",
    );
}

/// Verifies the reflected typed property is reported with its declared type and default.
///
/// `php -n` 8.5.6 prints `int;4;` for `getType()` and `getDefaultValue()` on `S1Plain::$id`.
/// The property being TYPED is what made the reduced case look class-specific.
#[test]
fn reflection_reads_a_typed_property_declared_by_a_runtime_include() {
    assert_eq!(
        run_include_fixture(
            "typed",
            &[("piece.php", TYPED_PROPERTY_CLASS)],
            br#"include "piece.php"; $p = (new ReflectionClass("S1Plain"))->getProperty("id"); echo $p->getType()->getName(), ";", $p->getDefaultValue(), ";";"#,
        ),
        "loaded;int;4;",
    );
}

/// Verifies a class declared by the caller after an include is reflected the same way.
///
/// `php -n` 8.5.6 prints `loaded;declared;S1Second;`. An include pushes and pops execution
/// state around the file, and this pins that a later caller declaration is unaffected by it.
#[test]
fn a_caller_declaration_after_an_include_is_still_reflectable() {
    assert_eq!(
        run_include_fixture(
            "both",
            &[("piece.php", TYPED_PROPERTY_CLASS)],
            br#"include "piece.php"; class S1Second { public int $n = 1; } echo "declared;"; echo (new ReflectionClass("S1Second"))->getName(), ";";"#,
        ),
        "loaded;declared;S1Second;",
    );
}
