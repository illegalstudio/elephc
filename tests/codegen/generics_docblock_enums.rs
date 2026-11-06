//! Purpose:
//! Exercises generic PHPDoc on enum methods through specialization and native execution.
//! Covers instance/static calls, namespace resolution, bounds, and trait methods.
//!
//! Called from:
//! - `cargo test --test codegen_tests generics_docblock_enums`.
//!
//! Key details:
//! - Enum cases retain their singleton identity while their methods specialize.
//! - PHPDoc remains active in both ordinary and strict PHP mode.

use crate::support::*;

/// Enum instance, static and trait methods specialize scalar and variadic arguments.
#[test]
fn test_docblock_enum_templates_instance_static_and_trait_calls() {
    let source = r#"<?php
trait Copy {
    /**
     * @template T
     * @param T $value
     * @return T
     */
    public function copy($value) { return $value; }
}
enum Id {
    case A;
    use Copy;
    /**
     * @template T
     * @param T ...$values
     * @return T
     */
    public function first(...$values) { return $values[0]; }
    /**
     * @template U = string
     * @return U
     */
    public static function emptyValue() { return "empty"; }
}
echo Id::A->first(7, 8) + 1, "|", strtoupper(Id::A->first("seven", "eight")), "|",
    Id::emptyValue(), "|", Id::A->copy(9), "|", Id::A->name;
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), "8|SEVEN|empty|9|A");
    }
}

/// Enum templates resolve imported bounds while keeping the method parameter name local.
#[test]
fn test_docblock_enum_templates_namespaced_bounds() {
    let source = r#"<?php
namespace Model { class Entity { public function __construct(public int $id) {} } }
namespace App {
    use Model\Entity as Base;
    enum Id {
        case A;
        /**
         * @template E of Base
         * @param E $value
         * @return int
         */
        public function id($value) { return $value->id; }
    }
    echo Id::A->id(new Base(7));
}
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), "7");
    }
}

/// An enum template's bound rejects the same invalid argument as a class method template.
#[test]
fn test_docblock_enum_templates_enforce_bounds() {
    let source = r#"<?php
enum Id {
    case A;
    /**
     * @template T of int
     * @param T $value
     * @return T
     */
    public function id($value) { return $value; }
}
Id::A->id("seven");
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        let error = compile_cli_file_with_flags_expect_failure(source, flags);
        assert!(error.contains("does not satisfy its bound int"), "{error}");
    }
}

/// A concrete enum method's body must satisfy its substituted PHPDoc return type.
#[test]
fn test_docblock_enum_templates_enforce_return_types() {
    let source = r#"<?php
enum Id {
    case A;
    /**
     * @template T
     * @param T $value
     * @return array<T>
     */
    public function values($value) { return ["wrong"]; }
}
Id::A->values(7);
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        let error = compile_cli_file_with_flags_expect_failure(source, flags);
        assert!(error.contains("array<int>") && error.contains("array<string>"), "{error}");
    }
}
