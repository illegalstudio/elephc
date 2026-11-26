//! Purpose:
//! Integration or regression tests for diagnostic coverage of warnings, including warning unused variable, warning byref params not flagged as unused, and warning unreachable code.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies that the compiler emits "Unused variable" warnings for function parameters
/// and local assignments that are never read.
#[test]
fn test_warning_unused_variable() {
    expect_warning("<?php function foo($x) { $y = 1; return 2; }", "Unused variable: $x");
    expect_warning("<?php function foo($x) { $y = 1; return 2; }", "Unused variable: $y");
}

/// Verifies that by-reference parameters are NOT flagged as unused even when they
/// are not explicitly read in the function body, because they are semantically
/// output parameters — the callee writes to the caller's variable.
#[test]
fn test_warning_byref_params_not_flagged_as_unused() {
    expect_no_warning(
        "<?php function fill(int &$out): void { $out = 42; }",
        "Unused variable: $out",
    );
    expect_no_warning(
        "<?php function getColor(int $index, int &$r, int &$g, int &$b): void { $r = 255; $g = 128; $b = 0; }",
        "Unused variable: $r",
    );
    expect_no_warning(
        "<?php function getColor(int $index, int &$r, int &$g, int &$b): void { $r = 255; $g = 128; $b = 0; }",
        "Unused variable: $g",
    );
    expect_no_warning(
        "<?php function getColor(int $index, int &$r, int &$g, int &$b): void { $r = 255; $g = 128; $b = 0; }",
        "Unused variable: $b",
    );
}

/// Verifies that a function with an EMPTY body never reports its parameters as
/// unused. A no-op cannot read anything, so the warning would fire on every
/// parameter of every deliberate stub, and the parameter names are part of the
/// public contract (PHP 8 named arguments bind to them) — renaming them to
/// silence the warning would break callers.
///
/// elephc's injected curl prelude depends on this: PHP 8's `curl_close()`,
/// `curl_multi_close()` and `curl_share_close()` are no-ops with a required
/// parameter, so compiling any program that calls one used to print an
/// "Unused variable" warning against an invisible prelude line.
#[test]
fn test_warning_empty_body_params_not_flagged_as_unused() {
    expect_no_warning(
        "<?php function noop($handle): void {}",
        "Unused variable: $handle",
    );
    expect_no_warning(
        "<?php class C { public function noop($handle): void {} }",
        "Unused variable: $handle",
    );
    // A non-empty body still reports, so the exemption is scoped to the no-op case.
    expect_warning(
        "<?php function almost_noop($handle): void { $y = 1; echo $y; }",
        "Unused variable: $handle",
    );
}

/// Verifies that code immediately following a `return` statement within a function
/// body is flagged as unreachable.
#[test]
fn test_warning_unreachable_code() {
    expect_warning("<?php function foo() { return 1; echo 2; }", "Unreachable code");
}

/// Verifies that code after a `switch` that covers all cases (including `default`)
/// is flagged as unreachable.
#[test]
fn test_warning_unreachable_after_exhaustive_switch() {
    expect_warning(
        "<?php function foo($flag) { switch ($flag) { case 1: return 1; default: return 2; } echo 3; }",
        "Unreachable code",
    );
}

/// Verifies that code after a `try-catch` where both branches return is flagged
/// as unreachable.
#[test]
fn test_warning_unreachable_after_exhaustive_try_catch() {
    expect_warning(
        "<?php function foo() { try { return 1; } catch (Exception $e) { return 2; } echo 3; }",
        "Unreachable code",
    );
}

/// Verifies that code after a `try-finally` where the `finally` block performs a
/// return is flagged as unreachable (finally's return takes precedence over the
/// try's return from the caller's perspective).
#[test]
fn test_warning_unreachable_after_try_finally_return() {
    expect_warning(
        "<?php function foo() { try { return 1; } finally { return 2; } echo 3; }",
        "Unreachable code",
    );
}

/// Verifies that undefined-variable compound-assignment warnings are reported once,
/// even though the checker runs multiple stabilization passes.
#[test]
fn test_warning_undefined_compound_assignment_is_deduped() {
    let result = check_source_full("<?php $x += 1; echo $x;").unwrap();
    let count = result
        .warnings
        .iter()
        .filter(|warning| warning.message.contains("Undefined variable: $x"))
        .count();
    assert_eq!(count, 1);
}

/// Verifies that code after a `try-catch` where only the `catch` branch returns
/// is NOT flagged as unreachable, because the `try` block may complete without
/// returning and fall through to the subsequent code.
#[test]
fn test_warning_no_unreachable_after_fallthrough_try() {
    expect_no_warning(
        "<?php function foo() { try { echo 1; } catch (Exception $e) { return 2; } echo 3; }",
        "Unreachable code",
    );
}

/// Verifies that a closure variable is not flagged as unused when the closure is
/// invoked (the variable holds a callable that is called, so it is used).
#[test]
fn test_warning_closure_call_marks_callable_variable_as_used() {
    expect_no_warning(
        "<?php function foo() { $f = function() { return 1; }; $f(); }",
        "Unused variable: $f",
    );
}

/// Verifies that writing through a by-reference closure capture is treated as an
/// observable caller update rather than an unused closure-local assignment.
#[test]
fn test_warning_byref_closure_capture_write_is_used() {
    expect_no_warning(
        "<?php function install(int &$target): void { $setter = function(int $value) use (&$target): void { $target = $value; }; $setter(42); }",
        "Unused variable: $target",
    );
}

/// Verifies that unused parameters of nested function declarations are analyzed
/// and produce warnings, not silently ignored.
#[test]
fn test_warning_nested_function_is_analyzed() {
    expect_warning(
        "<?php function outer() { function inner($x) { return 1; } }",
        "Unused variable: $x",
    );
}

/// Verifies that an outer variable captured by an arrow function is not flagged
/// as unused, because the arrow function implicitly returns the variable.
#[test]
fn test_warning_arrow_function_marks_outer_variable_as_used() {
    expect_no_warning(
        "<?php function outer() { $x = 1; $f = fn() => $x; }",
        "Unused variable: $x",
    );
}

/// Verifies that the span of an unused parameter warning is a real source span
/// (line > 0) rather than a dummy fallback span, ensuring the diagnostic points
/// to the correct source location.
#[test]
fn test_warning_unused_param_has_real_span() {
    let result = check_source_full("<?php function foo($x) { return 1; }").unwrap();
    let warning = result
        .warnings
        .iter()
        .find(|warning| warning.message.contains("Unused variable: $x"))
        .expect("expected unused param warning");
    assert!(warning.span.line > 0, "expected non-dummy span, got {:?}", warning.span);
}

/// Verifies that marking a private method as final produces a warning, because
/// private methods cannot be overridden by child classes, making the `final`
/// modifier redundant.
#[test]
fn test_warning_final_private_method() {
    expect_warning(
        "<?php class Box { final private function seal() { return 1; } }",
        "Private methods cannot be final as they are never overridden by other classes",
    );
}

/// Verifies that `as final` on a private trait method warns like a written `final private`
/// method, with or without an alias and through nested trait uses, while making a public trait
/// method private and final does not (#2352).
#[test]
fn test_warning_trait_alias_as_final_private_method() {
    for adaptation in ["foo as final;", "foo as final bar;", "T::foo as final;"] {
        expect_warning(
            &format!(
                "<?php trait T {{ private function foo() {{ return 1; }} }} class C {{ use T {{ {adaptation} }} }}"
            ),
            "Private methods cannot be final as they are never overridden by other classes",
        );
    }
    expect_no_warning(
        "<?php trait T { public function foo() { return 1; } } class C { use T { foo as private; foo as final; } }",
        "Private methods cannot be final",
    );
    // A nested `use` decides the visibility the outer `as final` sees: a visibility change or
    // an alias of a private method counts, the declaration it replaced does not.
    for source in [
        "<?php trait Inner { public function foo() { return 1; } } trait Outer { use Inner { foo as private; } } class C { use Outer { foo as final; } }",
        "<?php trait Inner { private function foo() { return 1; } } trait Outer { use Inner { foo as bar; } } class C { use Outer { bar as final; } }",
        "<?php trait Inner { private function foo() { return 1; } } trait Outer { use Inner; } class C { use Outer { foo as final; } }",
    ] {
        expect_warning(
            source,
            "Private methods cannot be final as they are never overridden by other classes",
        );
    }
    for source in [
        "<?php trait Inner { private function foo() { return 1; } } trait Outer { use Inner { foo as public; } } class C { use Outer { foo as final; } }",
        "<?php trait A { public function foo() { return 1; } } trait B { public function foo() { return 2; } } trait Outer { use A, B { A::foo insteadof B; B::foo as private; } } class C { use Outer { foo as final; } }",
    ] {
        expect_no_warning(source, "Private methods cannot be final");
    }
}

/// Verifies that a final private constructor does NOT produce a warning. Unlike
/// regular private methods, a final private constructor is a legitimate pattern
/// (e.g., singleton enforcement) and is not redundant.
#[test]
fn test_warning_final_private_constructor_is_allowed() {
    expect_no_warning(
        "<?php class Box { final private function __construct() {} }",
        "Private methods cannot be final",
    );
}

// --- #[\Deprecated] warnings (PHP 8.4) ---

/// Verifies that calling a function marked with #[Deprecated] without arguments
/// emits a deprecation warning.
#[test]
fn test_warning_deprecated_function_call() {
    expect_warning(
        "<?php #[\\Deprecated] function legacy(): int { return 1; } echo legacy();",
        "Call to deprecated function: legacy()",
    );
}

/// Verifies that a deprecation reason provided via #[Deprecated("use newApi()")] is
/// included in the emitted warning message after a dash separator.
#[test]
fn test_warning_deprecated_function_includes_reason() {
    expect_warning(
        "<?php #[\\Deprecated(\"use newApi()\")] function oldApi(): int { return 1; } echo oldApi();",
        "Call to deprecated function: oldApi() — use newApi()",
    );
}

/// Verifies that calling an instance method marked with #[Deprecated] emits a
/// deprecation warning mentioning the method name.
#[test]
fn test_warning_deprecated_method_call() {
    expect_warning(
        "<?php class Svc { #[\\Deprecated] public function fetch(): int { return 1; } } $s = new Svc(); echo $s->fetch();",
        "Call to deprecated method: Svc::fetch()",
    );
}

/// Verifies that calling a static method marked with #[Deprecated] emits a
/// deprecation warning including the reason string when provided.
#[test]
fn test_warning_deprecated_static_method_call() {
    expect_warning(
        "<?php class Reg { #[\\Deprecated(\"removed in v3\")] public static function lookup(): int { return 1; } } echo Reg::lookup();",
        "Call to deprecated static method: Reg::lookup() — removed in v3",
    );
}

/// Verifies PHP 8.5-style deprecated class constants retain their attribute
/// metadata through schema construction and warn when accessed.
#[test]
fn test_warning_deprecated_class_constant_access() {
    expect_warning(
        "<?php class Status { #[\\Deprecated(\"as it has no effect\")] public const OLD = 1; } echo Status::OLD;",
        "Use of deprecated class constant: Status::OLD — as it has no effect",
    );
}

/// Verifies that the short `#[Deprecated]` attribute (without backslash) on a
/// function is recognized as a deprecation marker and produces a warning when
/// the function is called.
#[test]
fn test_warning_deprecated_unqualified_form_is_recognized() {
    expect_warning(
        "<?php #[Deprecated] function legacy(): int { return 1; } echo legacy();",
        "Call to deprecated function: legacy()",
    );
}

/// Verifies that a `use Deprecated as Old;` import alias is accepted as a valid
/// deprecation attribute and produces a warning when the aliased function is called.
#[test]
fn test_warning_deprecated_import_alias_is_recognized() {
    expect_warning(
        "<?php use Deprecated as Old; #[Old] function legacy(): int { return 1; } echo legacy();",
        "Call to deprecated function: legacy()",
    );
}

/// Verifies that `#[Foo\Deprecated]` (a namespaced attribute that is not the
/// built-in `Deprecated`) does NOT produce a deprecation warning.
#[test]
fn test_warning_no_deprecation_for_qualified_lookalike() {
    expect_no_warning(
        "<?php #[Foo\\Deprecated] function legacy(): int { return 1; } echo legacy();",
        "deprecated function",
    );
}

/// Verifies that `#[Deprecated]` inside a namespace does NOT produce a deprecation
/// warning when called via an unqualified name, because in namespaced files the
/// compiler's builtin attribute fallback is not applied to local names.
#[test]
fn test_warning_no_deprecation_for_namespaced_unqualified_lookalike() {
    expect_no_warning(
        "<?php namespace N; #[Deprecated] function legacy(): int { return 1; } echo legacy();",
        "deprecated function",
    );
}

/// Verifies that a deprecated function does NOT produce a deprecation warning
/// when it is declared but never called.
#[test]
fn test_warning_no_deprecation_when_not_called() {
    expect_no_warning(
        "<?php #[\\Deprecated] function legacy(): int { return 1; } echo 1;",
        "deprecated function",
    );
}

/// Verifies that a regular (non-deprecated) function does NOT produce any
/// deprecation warning when called.
#[test]
fn test_warning_no_deprecation_for_undeprecated_function() {
    expect_no_warning(
        "<?php function fresh(): int { return 1; } echo fresh();",
        "deprecated",
    );
}

/// Builtin deprecations carry the profile that introduced them: PHP 8.6 reports the
/// non-canonical alias functions and the SPL helpers, while 8.5 keeps the plain call.
#[test]
fn test_builtin_deprecations_start_at_php_86() {
    use elephc::php_version::PhpVersion;

    let cases = [
        (
            "<?php is_double(1.0);",
            "Call to deprecated function: is_double() — use is_float() instead",
        ),
        (
            "<?php is_long(1);",
            "Call to deprecated function: is_long() — use is_int() instead",
        ),
        (
            "<?php is_integer(1);",
            "Call to deprecated function: is_integer() — use is_int() instead",
        ),
        (
            "<?php spl_classes();",
            "Call to deprecated function: spl_classes() — use ReflectionExtension::getClassNames() instead",
        ),
        (
            "<?php spl_object_hash(new stdClass());",
            "Call to deprecated function: spl_object_hash() — use spl_object_id() instead",
        ),
    ];
    for (source, expected) in cases {
        elephc::codegen::set_compile_profile(PhpVersion::Php86, false);
        let result = check_source_full(source).expect("8.6 source must type-check");
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.message.contains(expected)),
            "{source}: warnings {:?} do not contain {expected:?}",
            result
                .warnings
                .iter()
                .map(|warning| warning.message.clone())
                .collect::<Vec<_>>()
        );

        elephc::codegen::set_compile_profile(PhpVersion::Php85, false);
        let older = check_source_full(source).expect("8.5 source must type-check");
        assert!(
            !older
                .warnings
                .iter()
                .any(|warning| warning.message.contains("Call to deprecated function")),
            "{source}: PHP 8.5 must not report the 8.6 deprecation, got {:?}",
            older
                .warnings
                .iter()
                .map(|warning| warning.message.clone())
                .collect::<Vec<_>>()
        );
    }
    elephc::codegen::set_compile_profile(PhpVersion::default(), false);
}

/// PHP 8.6 deprecates `return` inside a `finally` block; earlier profiles keep it silent.
#[test]
fn test_finally_return_is_deprecated_php_86() {
    use elephc::php_version::PhpVersion;
    let source = "<?php function f(): int { try { return 1; } finally { return 2; } }";

    elephc::codegen::set_compile_profile(PhpVersion::Php86, false);
    let result = check_source_full(source).expect("8.6 source must type-check");
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.message.contains("Returning from a finally block is deprecated")),
        "warnings {:?}",
        result.warnings.iter().map(|w| w.message.clone()).collect::<Vec<_>>()
    );

    elephc::codegen::set_compile_profile(PhpVersion::Php85, false);
    let older = check_source_full(source).expect("8.5 source must type-check");
    assert!(
        !older
            .warnings
            .iter()
            .any(|warning| warning.message.contains("finally block is deprecated")),
        "PHP 8.5 must not deprecate the finally return: {:?}",
        older.warnings.iter().map(|w| w.message.clone()).collect::<Vec<_>>()
    );
    elephc::codegen::set_compile_profile(PhpVersion::default(), false);
}

/// PHP 8.6 deprecates the ignored third argument of `define()`; 8.5 stays silent.
#[test]
fn test_define_case_insensitive_argument_is_deprecated_php_86() {
    use elephc::php_version::PhpVersion;

    elephc::codegen::set_compile_profile(PhpVersion::Php86, false);
    let result = check_source_full("<?php define('X_86_PROBE', 1, false);")
        .expect("8.6 source must type-check");
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.message.contains("case_insensitive argument to define() is deprecated")),
        "warnings {:?}",
        result.warnings.iter().map(|w| w.message.clone()).collect::<Vec<_>>()
    );

    elephc::codegen::set_compile_profile(PhpVersion::Php85, false);
    let older = check_source_full("<?php define('X_85_PROBE', 1, false);")
        .expect("8.5 source must type-check");
    assert!(
        !older
            .warnings
            .iter()
            .any(|warning| warning.message.contains("case_insensitive argument to define()")),
        "PHP 8.5 must not deprecate define()'s third argument: {:?}",
        older.warnings.iter().map(|w| w.message.clone()).collect::<Vec<_>>()
    );
    elephc::codegen::set_compile_profile(PhpVersion::default(), false);
}

/// PHP 8.6 deprecates a string subject when `$allow_string` is statically false.
#[test]
fn test_is_a_and_is_subclass_of_string_subject_deprecation_php86() {
    use elephc::php_version::PhpVersion;

    elephc::codegen::set_compile_profile(PhpVersion::Php86, false);
    let result = check_source_full("<?php var_dump(is_a('Foo', 'Bar'));").expect("must type-check");
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.message.contains("Calling is_a() with a string when $allow_string is false")),
        "{:?}",
        result.warnings.iter().map(|w| w.message.clone()).collect::<Vec<_>>()
    );
    let allowed = check_source_full("<?php var_dump(is_a('Foo', 'Bar', true));").expect("must type-check");
    assert!(
        !allowed.warnings.iter().any(|warning| warning.message.contains("Calling is_a()")),
        "$allow_string = true must stay silent"
    );

    let subclass = check_source_full("<?php var_dump(is_subclass_of('Foo', 'Bar', false));")
        .expect("must type-check");
    assert!(
        subclass
            .warnings
            .iter()
            .any(|warning| warning.message.contains("Calling is_subclass_of() with a string when $allow_string is false")),
        "{:?}",
        subclass.warnings.iter().map(|w| w.message.clone()).collect::<Vec<_>>()
    );
    let default_allowed = check_source_full("<?php var_dump(is_subclass_of('Foo', 'Bar'));")
        .expect("must type-check");
    assert!(
        !default_allowed
            .warnings
            .iter()
            .any(|warning| warning.message.contains("Calling is_subclass_of()")),
        "is_subclass_of defaults $allow_string to true"
    );

    elephc::codegen::set_compile_profile(PhpVersion::Php85, false);
    let older = check_source_full("<?php var_dump(is_a('Foo', 'Bar'));").expect("must type-check");
    assert!(
        !older
            .warnings
            .iter()
            .any(|warning| warning.message.contains("Calling is_a()")),
        "PHP 8.5 must not deprecate the string subject"
    );
    elephc::codegen::set_compile_profile(PhpVersion::default(), false);
}

/// PHP 8.6 deprecates SplFileObject's object-oriented CSV helpers.
#[test]
fn test_spl_file_object_csv_methods_are_deprecated_php86() {
    use elephc::php_version::PhpVersion;

    let source = "<?php $file = new SplFileObject('x'); $file->fgetcsv(); $file->fputcsv(['a']);";
    elephc::codegen::set_compile_profile(PhpVersion::Php86, false);
    let result = check_source_full(source).expect("must type-check");
    for method in ["fgetcsv", "fputcsv"] {
        assert!(
            result.warnings.iter().any(|warning| {
                warning.message.contains(&format!("Call to deprecated method: SplFileObject::{}", method))
            }),
            "missing {method}: {:?}",
            result.warnings.iter().map(|w| w.message.clone()).collect::<Vec<_>>()
        );
    }

    elephc::codegen::set_compile_profile(PhpVersion::Php85, false);
    let older = check_source_full(source).expect("must type-check");
    assert!(
        !older
            .warnings
            .iter()
            .any(|warning| warning.message.contains("Call to deprecated method: SplFileObject::")),
        "PHP 8.5 must not deprecate the CSV helpers"
    );
    elephc::codegen::set_compile_profile(PhpVersion::default(), false);
}

/// PHP 8.6 deprecates passing an object to array_walk()/array_walk_recursive().
#[test]
fn test_array_walk_object_deprecation_php86() {
    use elephc::php_version::PhpVersion;

    let source = "<?php class C { public int $a = 1; } $c = new C(); array_walk($c, function ($v, $k): void {});";
    elephc::codegen::set_compile_profile(PhpVersion::Php86, false);
    let result = check_source_full(source).expect("must type-check");
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.message.contains("Passing an object for argument #1 $array to array_walk() is deprecated, call get_object_vars() first instead")),
        "{:?}",
        result.warnings.iter().map(|w| w.message.clone()).collect::<Vec<_>>()
    );

    let recursive = "<?php class C { public int $a = 1; } $c = new C(); array_walk_recursive($c, function ($v, $k): void {});";
    let result = check_source_full(recursive).expect("must type-check");
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.message.contains("Passing an object for argument #1 $array to array_walk_recursive() is deprecated")),
        "{:?}",
        result.warnings.iter().map(|w| w.message.clone()).collect::<Vec<_>>()
    );

    elephc::codegen::set_compile_profile(PhpVersion::Php85, false);
    let older = check_source_full(source).expect("must type-check");
    assert!(
        !older
            .warnings
            .iter()
            .any(|warning| warning.message.contains("array_walk() is deprecated")),
        "PHP 8.5 must not deprecate the object argument"
    );
    elephc::codegen::set_compile_profile(PhpVersion::default(), false);
}

/// PHP 8.6 deprecates passing an object in `mb_convert_variables()` `$vars`.
#[test]
fn test_mb_convert_variables_object_deprecation_php86() {
    use elephc::php_version::PhpVersion;

    let source = "<?php class C { public string $a = 'x'; } $c = new C(); mb_convert_variables('UTF-8', 'auto', $c);";
    elephc::codegen::set_compile_profile(PhpVersion::Php86, false);
    let result = check_source_full(source).expect("must type-check");
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.message.contains("Passing an object for argument #3 ($vars) to mb_convert_variables() is deprecated, call get_object_vars() first instead")),
        "{:?}",
        result.warnings.iter().map(|w| w.message.clone()).collect::<Vec<_>>()
    );

    elephc::codegen::set_compile_profile(PhpVersion::Php85, false);
    let older = check_source_full(source).expect("must type-check");
    assert!(
        !older
            .warnings
            .iter()
            .any(|warning| warning.message.contains("mb_convert_variables() is deprecated")),
        "PHP 8.5 must not deprecate the object argument"
    );
    elephc::codegen::set_compile_profile(PhpVersion::default(), false);
}
