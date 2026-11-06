//! Purpose:
//! Regresses constructor-local generic specialization through the full compiler pipeline.
//! Checks concrete parameter and promoted property storage, bounds, and class template scope.
//!
//! Called from:
//! - `cargo test --test codegen_tests generics_constructors`.
//!
//! Key details:
//! - PHPDoc constructor templates have the same effect in ordinary and strict PHP modes.
//! - Constructors keep their reserved method name in each concrete class.

use crate::support::*;

/// Constructor templates cannot capture a same-named global class in another method.
#[test]
fn test_generic_constructors_preserve_sibling_class_types() {
    let source = r#"<?php
class T { public int $value = 8; }
class Box {
    public function __construct<T>(T $value) {}
    public function read(T $value): int { return $value->value; }
}
$box = new Box(7);
echo $box->read(new T());
"#;
    assert_eq!(compile_and_run(source), "8");
}

/// Reusing a class parameter's spelling in the constructor retains separate template scopes.
#[test]
fn test_generic_constructors_shadow_class_parameter_names() {
    let source = r#"<?php
class Box<T> {
    public function __construct<T>(public T $value) {}
}
$box = new Box<int>("seven");
echo strtoupper($box->value);
"#;
    assert_eq!(compile_and_run(source), "SEVEN");
}

/// Native constructor templates retain parameters and specialize promoted property storage.
#[test]
fn test_generic_constructors_native_and_promoted_properties() {
    let source = r#"<?php
namespace App;
class Box {
    public function __construct<T>(public T $value) {}
}
$number = new Box(7);
$text = new Box("seven");
echo $number->value + 1, "|", strtoupper($text->value);
"#;
    assert_eq!(compile_and_run(source), "8|SEVEN");
}

/// Portable constructor templates specialize both ordinary and promoted parameters.
#[test]
fn test_generic_constructors_docblock_in_both_php_modes() {
    let source = r#"<?php
class Box {
    /**
     * @template T
     * @param T $value
     */
    public function __construct(public $value) {}
}
class Printer {
    /**
     * @template T
     * @param T $value
     */
    public function __construct($value) { echo $value, "|"; }
}
$number = new Box(7);
$text = new Box("seven");
new Printer(8);
new Printer("eight");
echo $number->value + 1, "|", strtoupper($text->value);
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), "8|eight|8|SEVEN");
    }
}

/// A constructor binds its own type parameter after explicit or inferred class arguments.
#[test]
fn test_generic_constructors_inside_generic_classes() {
    let source = r#"<?php
class Pair<T> {
    public function __construct<U>(public T $first, public U $second) {}
}
$explicit = new Pair<int>(7, "seven");
$inferred = new Pair("eight", 8);
echo $explicit->first + 1, "|", strtoupper($explicit->second), "|",
    strtoupper($inferred->first), "|", $inferred->second + 1;
"#;
    assert_eq!(compile_and_run(source), "8|SEVEN|EIGHT|9");
}

/// Constructor inference checks declared bounds before native execution in both PHP modes.
#[test]
fn test_generic_constructors_enforce_docblock_bounds() {
    let source = r#"<?php
class Box {
    /**
     * @template T of int
     * @param T $value
     */
    public function __construct($value) {}
}
new Box("seven");
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        let error = compile_cli_file_with_flags_expect_failure(source, flags);
        assert!(error.contains("<T>") && error.contains("does not satisfy"), "{error}");
    }
}
