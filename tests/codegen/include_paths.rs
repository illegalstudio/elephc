//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of include paths, including include with dunder dir concat, include with const ref, and include with define ref.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise include/require resolution, temporary project layout, and native binary output.

use crate::support::*;

// `require __DIR__ . '/...';` is the most common idiomatic include pattern
// in PHP. After magic-constant substitution, __DIR__ becomes a string literal
// and the resolver's path folder concatenates it with the trailing literal.

/// Verifies `require __DIR__ . '/lib/inner.php'` works after magic-constant
/// substitution. `__DIR__` is lowered to a string literal by the magic-constants
/// pass; the resolver then concatenates it with the trailing literal to produce
/// the resolved include path.
#[test]
fn test_include_with_dunder_dir_concat() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire __DIR__ . '/lib/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies `require LIB . '/inner.php'` where `LIB` is a top-level `const`
/// defined before the require. The resolver resolves the const reference to
/// the string `'lib'` before path resolution.
#[test]
fn test_include_with_const_ref() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nconst LIB = 'lib';\nrequire LIB . '/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies `require LIB . '/inner.php'` where `LIB` is a top-level `define()`
/// evaluated before the require. The resolver tracks defines incrementally as
/// files are inlined, allowing the require to reference a define set earlier.
#[test]
fn test_include_with_define_ref() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\ndefine('LIB', 'lib');\nrequire LIB . '/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies `require LIB . '/inner.php'` where `LIB` is set via the fully
/// qualified `\define()` call. The backslash prefix is canonical and the
/// define still feeds the include path resolution.
#[test]
fn test_include_with_fully_qualified_define_ref() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n\\define('LIB', 'lib');\nrequire LIB . '/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies `require __DIR__ . '/' . 'lib' . '/' . 'inner.php'` with multiple
/// chained concatenations. The resolver must correctly evaluate all BinaryOp
/// nodes in the path expression before resolving the include path.
#[test]
fn test_include_with_nested_concat() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire __DIR__ . '/' . 'lib' . '/' . 'inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies a constant defined in an included bootstrap file can be used in a
/// subsequent require within the main file. The resolver tracks constants
/// incrementally as files are inlined in order, so this cross-file forward
/// reference works.
#[test]
fn test_include_with_const_defined_in_included_file() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire 'bootstrap.php';\nrequire SUBDIR . '/inner.php';\necho 'after';\n",
            ),
            (
                "bootstrap.php",
                "<?php\nconst SUBDIR = 'lib';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies `const BASE = __DIR__ . '/lib'; require BASE . '/inner.php'`
/// works. `__DIR__` is lowered to a string literal by the magic-constants pass
/// before the resolver runs, so the path folder sees a plain
/// `BinaryOp(StringLiteral, Concat, StringLiteral)`.
#[test]
fn test_include_with_dunder_file_dir_const() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nconst BASE = __DIR__ . '/lib';\nrequire BASE . '/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies `require LIB . '/inner.php'` where `LIB` is declared inside a
/// namespace (`namespace App; const LIB = 'lib';`). The name resolver applies
/// the namespace scope, so the const reference resolves correctly.
#[test]
fn test_include_with_namespaced_const_ref() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nnamespace App;\nconst LIB = 'lib';\nrequire LIB . '/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies `require LIB . '/inner.php'` where `LIB` is imported via
/// `use const Config\LIB` from an included config file. The `use const`
/// directive makes the const available in the importing file's scope for
/// path resolution.
#[test]
fn test_include_with_const_import_ref() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nnamespace App;\nuse const Config\\LIB;\nrequire 'config.php';\nrequire LIB . '/inner.php';\necho 'after';\n",
            ),
            (
                "config.php",
                "<?php\nnamespace Config;\nconst LIB = 'lib';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies that a const declared in one namespace is not accessible from
/// another namespace when used in an include path. The require must fail
/// because `LIB` in namespace `B` refers to a non-existent const in that scope.
#[test]
fn test_include_does_not_use_const_from_other_namespace() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                "<?php\nnamespace A;\nconst LIB = 'lib';\nnamespace B;\nrequire LIB . '/inner.php';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    ));
}

/// Verifies that a `define()` call inside a function does not feed a
/// top-level require. Constants defined inside a function have local scope
/// and cannot be referenced by statements outside that function.
#[test]
fn test_define_inside_function_does_not_feed_top_level_include() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                "<?php\nfunction boot() {\n    define('LIB', 'lib');\n}\nrequire LIB . '/inner.php';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    ));
}

/// Verifies that a namespaced `Config\define()` call does not feed a require
/// that references `LIB` at the top level. The callable `Config\define` is not
/// the global `define`, so the const is not set and the require fails.
#[test]
fn test_qualified_define_call_does_not_feed_include() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                "<?php\nnamespace App;\nConfig\\define('LIB', 'lib');\nrequire LIB . '/inner.php';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    ));
}

/// Verifies that a `define()` inside a function can feed a `require` that is
/// also inside the same function. Function-local defines are in scope for
/// statements within that function body.
#[test]
fn test_define_inside_function_can_feed_include_in_same_function() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nfunction boot() {\n    define('LIB', 'lib');\n    require LIB . '/inner.php';\n}\nboot();\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies that when an included file defines a function with an untyped
/// parameter, calling it with a specialized array from `load_items()` produces
/// a specialized variant. The `append_item` function is called with array
/// type info, so its `count($items) > 0` guard is not folded away.
#[test]
fn test_include_function_variant_specializes_untyped_array_param_from_call() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire 'lib.php';\n$items = load_items();\n$items = append_item($items, \"c\");\necho count($items) . ':' . $items[2];\n",
            ),
            (
                "lib.php",
                "<?php\nfunction load_items() {\n    return [\"a\", \"b\"];\n}\n\nfunction append_item($items, $value) {\n    if (count($items) > 0) {\n        $items[] = $value;\n    }\n    return $items;\n}\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "3:c");
}

/// Verifies that when an included file defines a function with an untyped
/// parameter, calling it with a specialized string from `trim()` produces a
/// specialized variant. The `describe_text` function is called with string type
/// info, enabling inlining and DCE of the strlen branch.
#[test]
fn test_include_function_variant_specializes_untyped_string_param_from_call() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire 'lib.php';\necho describe_text(trim(\" hello \"));\n",
            ),
            (
                "lib.php",
                "<?php\nfunction describe_text($input) {\n    if (strlen($input) === 0) {\n        return \"empty\";\n    }\n    return \"len=\" . strlen($input);\n}\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "len=5");
}

/// Verifies that when an included file defines a function with an untyped
/// parameter, calling it with an integer argument (which cannot be passed to
/// strlen) produces a compile error. The specialized variant is not available
/// for that type, and no valid fallback exists.
#[test]
fn test_include_function_variant_keeps_error_when_call_does_not_respecialize() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                "<?php\nrequire 'lib.php';\necho describe_text(123);\n",
            ),
            (
                "lib.php",
                "<?php\nfunction describe_text($input) {\n    return strlen($input);\n}\n",
            ),
        ],
        "main.php",
    ));
}

/// Issue #476 review follow-up: an include inside a CLOSURE in a `for` clause is DEFERRED, so
/// the clause's include restriction must not reach it.
///
/// `include`/`require` cannot run as the clause itself — the resolver expands a value-include
/// by rewriting the surrounding statement LIST, which a clause is not — and the parser rejects
/// those two shapes with a named diagnostic. A closure body is different: its include runs when
/// the closure is CALLED, after the loop. The first version of the guard reused the resolver's
/// `has_includes`, which descends into closure bodies, and rejected a program PHP accepts.
///
/// The ordering in the expected output is the proof: the loop body and `compiled` print first,
/// and the included file's own output appears only at the call. Measured on PHP 8.5.10.
#[test]
fn test_for_clause_allows_a_deferred_include_inside_a_closure() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nfor ($f = function () { include 'inner.php'; }, $i = 0; $i < 1; $i++) {\n\
                 echo 'body ', $i, \"\\n\";\n}\necho \"compiled\\n\";\n$f();\necho \"|called\\n\";\n",
            ),
            ("inner.php", "<?php\necho 'inc';\n"),
        ],
        "main.php",
    );
    assert_eq!(out, "body 0\ncompiled\ninc|called\n");
}


/// An include-variant declaration is not "never called" just because its generated symbol is not
/// the name the call site wrote.
///
/// A function declared in an included file is registered under
/// `__elephc_include_variant_{hash}_{local}`, while the call site names it as the source wrote
/// it. `widen_dynamic_only_passthrough_returns` compares those two, so a variant whose body
/// returns one of its parameters looked like a function no caller had ever touched — its return
/// was widened to `mixed` while the caller still expected the specialized type, and
/// `count($items)` printed a raw pointer instead of `3`.
///
/// `append_item` is the shape that matters: an untyped parameter returned straight back, with a
/// real call site that passes it an array. `load_items` sits beside it as the control — it takes
/// no parameters at all, so the rule must not reach it for a different reason.
///
/// Every expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_include_variant_returning_its_parameter_keeps_the_call_site_type() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire 'lib.php';\n$items = load_items();\n$items = append_item($items, \"c\");\necho count($items) . ':' . $items[2];\n",
            ),
            (
                "lib.php",
                "<?php\nfunction load_items() {\n    return [\"a\", \"b\"];\n}\n\nfunction append_item($items, $value) {\n    if (count($items) > 0) {\n        $items[] = $value;\n    }\n    return $items;\n}\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "3:c");
}

/// A valid line longer than 65,535 columns in an INCLUDED file compiles, as it does in the root.
///
/// The packed included-source span asserted its end column fit in 16 bits, so this program made
/// the compiler panic ("included-source column exceeds packed span range"). MEASURED on reference
/// PHP 8.5.10: `70000`, then `after`.
#[test]
fn test_include_with_a_line_past_65535_columns() {
    let long = format!("<?php\n$x = '{}'; echo strlen($x), \"\\n\";\n", "a".repeat(70_000));
    let out = compile_and_run_files(
        &[
            ("long.php", &long),
            ("main.php", "<?php\ninclude __DIR__ . '/long.php';\necho \"after\\n\";\n"),
        ],
        "main.php",
    );
    assert_eq!(out, "70000\nafter\n");
}
