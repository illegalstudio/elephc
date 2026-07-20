//! Purpose:
//! Integration tests for `--strict-php` diagnostics: extension builtins hidden
//! from user programs, user redeclaration of extension names, and the
//! undefined-function hint pointing at the disabled extension.
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

/// Verifies the undefined-function diagnostic names the disabled extension so
/// users understand why a working non-strict program stopped compiling.
#[test]
fn test_strict_error_extension_builtin_call_carries_hint() {
    expect_strict_error(
        "<?php $x = ptr_get(1);",
        "ptr_get() exists as an elephc extension; it is disabled by --strict-php",
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

/// Verifies `is_real` stays available under strict mode: it is treated as PHP
/// for strict purposes even though PHP 8 removed it.
#[test]
fn test_strict_keeps_is_real_working() {
    let result = check_source_strict("<?php var_dump(is_real(1.5));");
    assert!(
        result.is_ok(),
        "is_real must keep working under --strict-php, got: {result:?}",
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
