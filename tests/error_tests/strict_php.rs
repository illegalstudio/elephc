//! Purpose:
//! Integration tests for `--strict-php` diagnostics: extension builtins hidden
//! from user programs, user redeclaration of extension names, and the
//! undefined-function hint pointing at the disabled extension. Covers the syntax audit
//! and PHPDoc generic semantics after physical-file processing.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Strict mode is thread-local; each test enables it around the shared
//!   frontend helpers and disables it before asserting, so parallel tests are
//!   unaffected.
//! - Behavior contract: under strict mode elephc-only builtins act exactly as
//!   if they did not exist, matching the PHP interpreter.

use super::*;

/// A hidden extension's name is an ordinary user function, not a builtin type guard.
#[test]
fn test_strict_user_is_real_does_not_narrow_to_float() {
    let result = check_source_strict("<?php function is_real($value): bool { return true; } $value = [1, 2]; if (is_real($value)) { echo $value[0]; }");
    assert!(result.is_ok(), "{result:?}");
}

/// Callable and pipe lookup explain why hidden extension names are unavailable.
#[test]
fn test_strict_hidden_extension_callable_reports_hint() {
    for source in ["<?php $f = is_real(...);", "<?php $value = 1 |> is_real(...);"] {
        expect_strict_error(source, "is_real() exists as an elephc extension; it is disabled by --strict-php");
    }
    assert!(check_source_strict("<?php function is_real($value): bool { return true; } $f = is_real(...); echo $f(1);").is_ok());
}

/// Runs [`check_source`] with strict-PHP mode enabled for the duration.
/// The RAII guard restores the previous state even when the checked pipeline panics.
fn check_source_strict(src: &str) -> Result<(), String> {
    let _guard = elephc::strict_php::scoped_enable();
    check_source(src)
}

/// Asserts that `src` fails under strict mode with a message containing `expected_substr`.
fn expect_strict_error(src: &str, expected_substr: &str) {
    match check_source_strict(src) {
        Ok(_) => panic!(
            "Expected strict-php error containing '{}', but got Ok",
            expected_substr
        ),
        Err(msg) => {
            assert!(
                msg.contains(expected_substr),
                "Error '{}' doesn't contain '{}'",
                msg,
                expected_substr,
            );
        }
    }
}

/// Verifies a call to an extension builtin is an undefined function under strict
/// mode, exactly as it would be under the PHP interpreter.
#[test]
fn test_strict_error_extension_builtin_call_is_undefined() {
    expect_strict_error("<?php $x = ptr_get(1);", "Undefined function: ptr_get");
}

/// Verifies the four Elephc-only catalog functions are unavailable in strict PHP mode.
#[test]
fn test_strict_error_elephc_only_catalog_functions_are_undefined() {
    for (call, name) in [
        ("clamp(1, 0, 2)", "clamp"),
        ("log2(8)", "log2"),
        ("grapheme_strrev('abc')", "grapheme_strrev"),
        ("is_real(1.0)", "is_real"),
    ] {
        expect_strict_error(
            &format!("<?php {call};"),
            &format!("Undefined function: {name}"),
        );
    }
}

/// Verifies the undefined-function diagnostic names the disabled extension so
/// users understand why a working non-strict program stopped compiling.
#[test]
fn test_strict_error_extension_builtin_call_carries_hint() {
    expect_strict_error(
        "<?php $x = ptr_get(1);",
        "ptr_get() exists as an elephc extension; it is disabled by --strict-php",
    );
}

/// Hides Elephc's daemon convenience wrapper while leaving PHP's POSIX helpers available.
#[test]
fn test_strict_error_pcntl_daemon_is_undefined() {
    expect_strict_error(
        "<?php pcntl_daemon();",
        "pcntl_daemon() exists as an elephc extension; it is disabled by --strict-php",
    );
    assert!(
        check_source_strict("<?php posix_setpgid(0, 0); posix_setsid();").is_ok(),
        "PHP POSIX helpers must remain visible under --strict-php",
    );
}

/// Verifies migrated buffer builtins are hidden like every other extension.
#[test]
fn test_strict_error_buffer_len_is_undefined() {
    expect_strict_error("<?php $x = buffer_len(1);", "Undefined function: buffer_len");
}

/// Verifies zval bridge builtins are hidden under strict mode.
#[test]
fn test_strict_error_zval_pack_is_undefined() {
    expect_strict_error("<?php $x = zval_pack(1);", "Undefined function: zval_pack");
}

/// Verifies attribute-introspection extensions are hidden under strict mode.
#[test]
fn test_strict_error_class_attribute_names_is_undefined() {
    expect_strict_error(
        "<?php class A {} $x = class_attribute_names('A');",
        "Undefined function: class_attribute_names",
    );
}

/// Verifies a user program may declare its own function with an extension
/// builtin's name under strict mode — the name does not exist in PHP, so the
/// declaration is plain userland code and calls resolve to it.
#[test]
fn test_strict_allows_user_declared_ptr_get() {
    let result = check_source_strict(
        "<?php function ptr_get(int $x): int { return $x + 1; } echo ptr_get(41);",
    );
    assert!(
        result.is_ok(),
        "user-declared ptr_get must compile under --strict-php, got: {result:?}",
    );
}

/// Verifies the same user declaration stays rejected without strict mode, where
/// the extension builtin does exist and PHP redeclaration rules apply.
#[test]
fn test_non_strict_still_rejects_user_declared_ptr_get() {
    expect_error(
        "<?php function ptr_get(int $x): int { return $x + 1; } echo ptr_get(41);",
        "Cannot redeclare built-in function: ptr_get",
    );
}

/// Verifies genuine PHP builtins keep working under strict mode.
#[test]
fn test_strict_keeps_php_builtins_working() {
    let result = check_source_strict("<?php echo strlen('abc');");
    assert!(
        result.is_ok(),
        "strlen must keep working under --strict-php, got: {result:?}",
    );
}

/// Parses `src` and returns the strict-PHP audit violations as message strings.
fn strict_audit_messages(src: &str) -> Vec<String> {
    let tokens = tokenize(src).expect("audit fixtures must tokenize");
    let ast = parse(&tokens).expect("audit fixtures must parse");
    elephc::strict_php::check(&ast)
        .into_iter()
        .map(|e| e.message)
        .collect()
}

/// Asserts the audit reports exactly one violation containing `expected_substr`.
fn expect_audit_violation(src: &str, expected_substr: &str) {
    let messages = strict_audit_messages(src);
    assert!(
        messages.iter().any(|m| m.contains(expected_substr)),
        "Audit messages {messages:?} do not contain '{expected_substr}'",
    );
}

/// Verifies the audit rejects `ifdef` conditional compilation blocks.
#[test]
fn test_audit_rejects_ifdef() {
    expect_audit_violation(
        "<?php ifdef FEATURE { echo 1; }",
        "`ifdef` conditional compilation is an elephc extension",
    );
}

/// Verifies the audit rejects `packed class` declarations.
#[test]
fn test_audit_rejects_packed_class() {
    expect_audit_violation(
        "<?php packed class P { public int $x; }",
        "`packed class` is an elephc extension",
    );
}

/// Verifies the audit rejects `array<T>` type arguments.
#[test]
fn test_audit_rejects_array_type_argument() {
    expect_audit_violation(
        "<?php function f(array<int> $a): int { return $a[0]; }",
        "`array<T>` type arguments are an elephc extension",
    );
}

/// Verifies the audit rejects an `array<T>` type argument in return position too.
#[test]
fn test_audit_rejects_array_type_argument_in_return_position() {
    expect_audit_violation(
        "<?php function f(): array<string> { return []; }",
        "`array<T>` type arguments are an elephc extension",
    );
}

/// Verifies the audit rejects `array<K, V>` type arguments with their own message.
#[test]
fn test_audit_rejects_assoc_array_type_arguments() {
    expect_audit_violation(
        "<?php function f(array<string, int> $m): int { return 0; }",
        "`array<K, V>` type arguments are an elephc extension",
    );
}

/// Verifies the audit rejects a type parameter list on a function declaration.
#[test]
fn test_audit_rejects_generic_function() {
    expect_audit_violation(
        "<?php function identity<T>(T $value): T { return $value; }",
        "generic functions are an elephc extension",
    );
}

/// Native method templates are extensions in every shared class-like member list.
#[test]
fn test_audit_rejects_generic_methods() {
    for declaration in [
        "class C { public function id<T>(T $value): T { return $value; } }",
        "class C { public static function id<T>(T $value): T { return $value; } }",
        "interface C { public function id<T>(T $value): T; }",
        "trait C { public function id<T>(T $value): T { return $value; } }",
        "enum C { case One; public function id<T>(T $value): T { return $value; } }",
    ] {
        expect_audit_violation(
            &format!("<?php {declaration}"),
            "generic methods are an elephc extension",
        );
    }
}

/// Supported PHPDoc generics on functions, classes and members stay active in either strict mode.
#[test]
fn test_docblock_generics_remain_active_with_and_without_strict_php() {
    let source = r#"<?php
/**
 * @template U
 * @param U $value
 * @return U
 */
function identity($value) { return $value; }
/** @template T */
class C {
    /** @param T $value */
    public function __construct(private $value) {}
    /** @return T */
    public function get() { return $this->value; }
}
class Identity {
    /**
     * @template V
     * @param V $value
     * @return V
     */
    public function id($value) { return $value; }
    /**
     * @template V
     * @param V $value
     * @return V
     */
    public static function copy($value) { return $value; }
}
$number = identity((new C(7))->get());
$text = identity((new C("seven"))->get());
$method_number = (new Identity())->id(7);
$method_text = (new Identity())->id("seven");
$static_number = Identity::copy(9);
$static_text = Identity::copy("nine");
"#;
    for strict in [false, true] {
        let _guard = strict.then(elephc::strict_php::scoped_enable);
        let ast = parse(&tokenize(source).expect("tokenizes")).expect("parses");
        let ast = elephc::source::finalize_physical_program(
            ast,
            source,
            Path::new("generic-methods.php"),
            elephc::source::SourceMode::Php,
            &HashSet::new(),
        )
        .expect("PHPDoc must pass the physical-file audit");
        let ast = elephc::name_resolver::resolve(ast).expect("resolves names");
        let (_, result) = elephc::generics::monomorphize(ast, |program, bounds| {
            types::check_with_options_and_bounds(program, types::CheckOptions::default(), bounds)
        })
        .expect("annotated declarations must specialize");
        assert_eq!(
            result.global_env.get("number"),
            Some(&types::PhpType::Int),
            "strict={strict}"
        );
        assert_eq!(
            result.global_env.get("text"),
            Some(&types::PhpType::Str),
            "strict={strict}"
        );
        for name in ["method_number", "static_number"] {
            assert_eq!(
                result.global_env.get(name), Some(&types::PhpType::Int), "{name}, strict={strict}"
            );
        }
        for name in ["method_text", "static_text"] {
            assert_eq!(
                result.global_env.get(name), Some(&types::PhpType::Str), "{name}, strict={strict}"
            );
        }
    }
}

/// A `@template` docblock is valid PHP — the annotations are comments — so `--strict-php` must
/// accept it even though it compiles to generics.
///
/// This is the whole reason the docblock surface exists, and it only holds because the
/// annotations are applied AFTER the audit. Applying them first made the audit see the injected
/// `<T>` and `array<T>` and reject a file php-src parses happily.
#[test]
fn test_audit_accepts_a_template_docblock() {
    let messages = strict_audit_messages(
        "<?php\n\
         /**\n\
          * @template T\n\
          * @param array<T> $items\n\
          * @return T\n\
          */\n\
         function firstOf(array $items) { return $items[0]; }",
    );
    assert!(
        messages.is_empty(),
        "expected no violations for an annotated PHP file, got {messages:?}"
    );
}

/// A bare `array` hint stays valid PHP: it parses as `TypeExpr::Named("array")`, never as
/// `TypeExpr::Array`, so the audit must leave it alone.
#[test]
fn test_audit_accepts_bare_array_hint() {
    let messages = strict_audit_messages("<?php function f(array $a): array { return $a; }");
    assert!(
        messages.is_empty(),
        "expected no violations for a bare array hint, got {messages:?}"
    );
}

/// Verifies the audit rejects `extern` function declarations.
#[test]
fn test_audit_rejects_extern_block() {
    expect_audit_violation(
        "<?php extern \"System\" { function getpid(): int; }",
        "`extern` declarations are an elephc extension",
    );
}

/// Verifies the audit rejects `ptr_cast<T>(...)` expressions.
#[test]
fn test_audit_rejects_ptr_cast() {
    expect_audit_violation(
        "<?php $x = 1; $p = ptr_cast<MyStruct>($x);",
        "`ptr_cast<T>` is an elephc extension",
    );
}

/// Verifies the audit rejects `buffer_new<T>(...)` allocations.
#[test]
fn test_audit_rejects_buffer_new() {
    expect_audit_violation(
        "<?php buffer<int> $b = buffer_new<int>(4);",
        "`buffer_new<T>` is an elephc extension",
    );
}

/// Verifies the audit rejects typed local variable declarations, which PHP
/// does not support for any type.
#[test]
fn test_audit_rejects_typed_local_declaration() {
    expect_audit_violation(
        "<?php int $x = 5;",
        "typed local variable declarations are an elephc extension",
    );
}

/// Verifies the audit rejects the `ptr` type in parameter annotations.
#[test]
fn test_audit_rejects_ptr_param_type() {
    expect_audit_violation(
        "<?php function f(ptr $p): void {}",
        "`ptr` types are an elephc extension",
    );
}

/// Verifies the audit rejects `buffer<T>` types in function return positions.
#[test]
fn test_audit_rejects_buffer_return_type() {
    expect_audit_violation(
        "<?php function f(): buffer<int> { return buffer_new<int>(1); }",
        "`buffer<T>` types are an elephc extension",
    );
}

/// Verifies the audit rejects `ptr` property types nested inside classes.
#[test]
fn test_audit_rejects_ptr_property_type() {
    expect_audit_violation(
        "<?php class C { public ptr $p; }",
        "`ptr` types are an elephc extension",
    );
}

/// Verifies the audit rejects extension types on closure parameters, which
/// requires recursing through expression bodies.
#[test]
fn test_audit_rejects_ptr_type_in_closure_param() {
    expect_audit_violation(
        "<?php $f = function (ptr $p): void {};",
        "`ptr` types are an elephc extension",
    );
}

/// Verifies the audit rejects user calls to compiler-reserved `__elephc_*` names.
#[test]
fn test_audit_rejects_reserved_elephc_call() {
    expect_audit_violation(
        "<?php $x = __elephc_ptr_read_string(1, 2);",
        "reserved for the compiler",
    );
}

/// Verifies extension expressions inside PHP attribute arguments on a function
/// declaration are rejected: attribute args are ordinary expressions and must
/// not be an audit blind spot.
#[test]
fn test_audit_rejects_extension_in_function_attribute_args() {
    expect_audit_violation(
        "<?php #[Foo(buffer_new<int>(4))] function f(): void {} echo \"ok\";",
        "`buffer_new<T>` is an elephc extension",
    );
}

/// Verifies extension expressions inside parameter attribute arguments are rejected.
#[test]
fn test_audit_rejects_extension_in_param_attribute_args() {
    expect_audit_violation(
        "<?php function f(#[Foo(buffer_new<int>(2))] int $x): void {}",
        "`buffer_new<T>` is an elephc extension",
    );
}

/// Verifies extension expressions inside property attribute arguments are rejected.
#[test]
fn test_audit_rejects_extension_in_property_attribute_args() {
    expect_audit_violation(
        "<?php class C { #[Foo(buffer_new<int>(1))] public int $x = 0; }",
        "`buffer_new<T>` is an elephc extension",
    );
}

/// Verifies extension expressions inside method attribute arguments are rejected.
#[test]
fn test_audit_rejects_extension_in_method_attribute_args() {
    expect_audit_violation(
        "<?php class C { #[Foo(buffer_new<int>(1))] public function m(): void {} }",
        "`buffer_new<T>` is an elephc extension",
    );
}

/// Verifies compiler-reserved `__elephc_*` calls inside attribute arguments are rejected.
#[test]
fn test_audit_rejects_reserved_call_in_attribute_args() {
    expect_audit_violation(
        "<?php #[Foo(__elephc_ptr_read_string(1, 2))] function f(): void {}",
        "reserved for the compiler",
    );
}

/// Verifies the audit collects multiple violations in one pass instead of
/// stopping at the first, so users can fix a file in one round.
#[test]
fn test_audit_collects_multiple_violations() {
    let messages = strict_audit_messages(
        "<?php packed class P { public int $x; } int $y = 1;",
    );
    assert!(
        messages.len() >= 2,
        "expected at least 2 violations, got {messages:?}",
    );
}

/// Writes `files` to a temp project and runs the resolver (which parses
/// includes) with strict-PHP mode enabled, returning the resolver outcome.
fn resolve_files_strict(
    files: &[(&str, &str)],
    main_file: &str,
) -> Result<(), elephc::errors::CompileError> {
    let id = TEST_PROJECT_ID.fetch_add(1, Ordering::SeqCst);
    let dir =
        std::env::temp_dir().join(format!("elephc_strict_test_{}_{}", std::process::id(), id));
    fs::create_dir_all(&dir).unwrap();
    for (path, content) in files {
        let full_path = dir.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full_path, content).unwrap();
    }

    let php_path = dir.join(main_file);
    let source = fs::read_to_string(&php_path).unwrap();
    let base_dir = php_path.parent().unwrap();

    let result = {
        let _guard = elephc::strict_php::scoped_enable();
        (|| -> Result<(), elephc::errors::CompileError> {
            let tokens = tokenize(&source)?;
            let ast = parse(&tokens)?;
            let _ = elephc::resolver::resolve(ast, base_dir)?;
            Ok(())
        })()
    };

    let _ = fs::remove_dir_all(&dir);
    result
}

/// Verifies an `extern` block inside an included file is rejected under strict
/// mode: the resolver audits every included user file at parse time.
#[test]
fn test_strict_rejects_extern_inside_include() {
    let err = resolve_files_strict(
        &[
            ("main.php", "<?php require 'lib.php'; echo helper();"),
            (
                "lib.php",
                "<?php extern \"System\" { function getpid(): int; }\nfunction helper(): int { return getpid(); }",
            ),
        ],
        "main.php",
    )
    .expect_err("extern inside an include must be rejected under strict mode");
    assert!(
        err.message.contains("`extern` declarations are an elephc extension"),
        "unexpected message: {}",
        err.message,
    );
}

/// Verifies a plain-PHP include with a function declaration passes the strict
/// audit. Regression guard: the resolver synthesizes `__elephc_include_variant_*`
/// names for include-loaded functions, and auditing the post-resolve program
/// used to flag those compiler-generated names as reserved-prefix violations.
#[test]
fn test_strict_accepts_plain_php_include_with_function() {
    let result = resolve_files_strict(
        &[
            ("main.php", "<?php require 'lib.php'; echo helper();"),
            ("lib.php", "<?php function helper(): int { return 7; }"),
        ],
        "main.php",
    );
    assert!(
        result.is_ok(),
        "plain PHP include must pass the strict audit, got: {result:?}",
    );
}

/// Verifies a plain PHP program produces no audit violations.
#[test]
fn test_audit_accepts_plain_php() {
    let messages = strict_audit_messages(
        "<?php
        function fib(int $n): int { return $n < 2 ? $n : fib($n - 1) + fib($n - 2); }
        class Greeter {
            public string $name;
            public function __construct(string $name) { $this->name = $name; }
            public function greet(): string { return 'hi ' . $this->name; }
        }
        $g = new Greeter('world');
        echo $g->greet(), fib(10);
        foreach ([1, 2, 3] as $k => $v) { echo $k + $v; }
        $f = fn(int $x): int => $x * 2;
        echo $f(21), strlen('abc'), PHP_EOL;",
    );
    assert!(messages.is_empty(), "expected no violations, got {messages:?}");
}


/// A class declaring type parameters is not PHP: php-src reads the `<` as a comparison and the
/// declaration fails to parse.
#[test]
fn test_audit_rejects_generic_class_declaration() {
    expect_audit_violation(
        "<?php class Box<T> { private T $v; }",
        "Type parameters on 'Box' are an elephc extension",
    );
}

/// An interface's type parameters are rejected the same way, bound included.
#[test]
fn test_audit_rejects_generic_interface_declaration() {
    expect_audit_violation(
        "<?php interface Repository<T: Entity> { public function f(): T; }",
        "Type parameters on 'Repository' are an elephc extension",
    );
}

/// A trait's type parameters are rejected the same way.
#[test]
fn test_audit_rejects_generic_trait_declaration() {
    expect_audit_violation(
        "<?php trait Holder<T> { private T $item; }",
        "Type parameters on 'Holder' are an elephc extension",
    );
}

/// Type arguments on a used trait are their own extension, rejected on the using declaration.
#[test]
fn test_audit_rejects_type_arguments_on_a_used_trait() {
    expect_audit_violation(
        "<?php class IntBox { use Holder<int>; }",
        "Type arguments on the traits 'IntBox' uses are an elephc extension",
    );
}

/// Type arguments on an inherited interface are a SEPARATE extension from declaring type
/// parameters: a class can stop being generic and still implement a generic interface, so
/// removing one does not remove the other.
#[test]
fn test_audit_rejects_type_arguments_on_an_implemented_interface() {
    expect_audit_violation(
        "<?php class R implements Repository<User> {}",
        "Type arguments on what 'R' inherits are an elephc extension",
    );
}

/// The same for an inherited parent class.
#[test]
fn test_audit_rejects_type_arguments_on_a_parent_class() {
    expect_audit_violation(
        "<?php class Small extends Box<int> {}",
        "Type arguments on what 'Small' inherits are an elephc extension",
    );
}

/// A generic class type in an annotation is rejected wherever a type can appear.
#[test]
fn test_audit_rejects_generic_class_type_annotation() {
    expect_audit_violation(
        "<?php function f(Box<int> $b) { return $b; }",
        "Generic class type 'Box<...>' is an elephc extension",
    );
}

/// `new Box<int>()` is rejected on the TYPE rather than on the parse, so the message names the
/// extension instead of leaving the programmer with another engine's syntax error.
#[test]
fn test_audit_rejects_generic_construction() {
    expect_audit_violation(
        "<?php $b = new Box<int>(1);",
        "Generic class type 'Box<...>' is an elephc extension",
    );
}

/// An ordinary class implementing an ordinary interface stays valid PHP.
#[test]
fn test_audit_accepts_a_non_generic_class() {
    let messages =
        strict_audit_messages("<?php class Plain implements Countable { public int $x = 1; }");
    assert!(messages.is_empty(), "unexpected violations: {messages:?}");
}

/// A generic static receiver is not PHP either, and it reaches the audit through five
/// expression forms — a method call, a class constant, a static property, `::class`, and a
/// first-class callable. Auditing only the DECLARATION would accept a file php-src cannot
/// parse whenever the class is declared elsewhere.
#[test]
fn test_audit_rejects_a_generic_static_method_call() {
    expect_audit_violation(
        "<?php echo Box<int>::of(1);",
        "Generic class type 'Box<...>' is an elephc extension",
    );
}

/// The same receiver reached through a class constant.
#[test]
fn test_audit_rejects_a_generic_class_constant_receiver() {
    expect_audit_violation(
        "<?php echo Box<int>::LABEL;",
        "Generic class type 'Box<...>' is an elephc extension",
    );
}

/// And through `::class`, which is the form that looks most like plain PHP.
#[test]
fn test_audit_rejects_a_generic_class_name_receiver() {
    expect_audit_violation(
        "<?php echo Box<int>::class;",
        "Generic class type 'Box<...>' is an elephc extension",
    );
}

/// An ordinary static call stays valid PHP.
#[test]
fn test_audit_accepts_an_ordinary_static_call() {
    let messages = strict_audit_messages("<?php echo Plain::of(1), Plain::LABEL, Plain::class;");
    assert!(messages.is_empty(), "unexpected violations: {messages:?}");
}

/// `$x instanceof Box<int>` is not PHP either, and it is the fifth place a generic class type
/// can be written.
#[test]
fn test_audit_rejects_a_generic_instanceof_target() {
    expect_audit_violation(
        "<?php var_dump($x instanceof Box<int>);",
        "Generic class type 'Box<...>' is an elephc extension",
    );
}

/// A caught class is the fifth place a generic type can be written, and `--strict-php` has to
/// reject it there too — a file that only CATCHES one is still not PHP.
#[test]
fn test_audit_rejects_a_generic_catch_type() {
    expect_audit_violation(
        "<?php try { f(); } catch (Err<int> $e) {} ",
        "Generic class type 'Err<...>' is an elephc extension",
    );
}
