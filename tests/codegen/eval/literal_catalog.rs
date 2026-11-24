//! Purpose:
//! End-to-end coverage for the literal-eval catalog narrowing: a program whose only `eval`
//! has literal source carries just the declarations that source names, runs them correctly,
//! and still keeps the whole surface when the source is known only at runtime.
//!
//! Called from:
//! - `cargo test --test codegen_tests_eval codegen::eval::literal_catalog` through Rust's test harness.
//!
//! Key details:
//! - Every fixture needs the magician bridge (class declarations and `throw` inside the
//!   fragment keep the EIR AOT planner out), so the assembly reflects reachability, not AOT.
//! - The assembly assertions look for lowered DateTime bodies, which only a date-shaped name
//!   or an opaque eval can pull in.

use crate::support::*;

/// Compiles one fixture to assembly and returns the user assembly text.
fn user_asm(source: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "elephc_literal_catalog_{}_{}",
        std::process::id(),
        TEST_ID.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).unwrap();
    let (asm, _, _) = compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let _ = fs::remove_dir_all(&dir);
    asm
}

/// A literal fragment that names nothing date-shaped compiles without the DateTime surface,
/// while the same program with a runtime-sourced eval carries it.
#[test]
fn test_literal_eval_leaves_unnamed_builtin_surfaces_out_of_the_binary() {
    let literal = user_asm(
        "<?php eval('class LiteralCatalogPlain { public int $n = 2; } echo (new LiteralCatalogPlain())->n;');",
    );
    assert!(
        !literal.contains("DateTime_createfromformat"),
        "literal eval must not lower the DateTime alias surface"
    );
    let opaque = user_asm("<?php $code = $argv[1] ?? 'echo 1;'; eval($code);");
    assert!(
        opaque.contains("DateTime_createfromformat"),
        "runtime-sourced eval must keep the DateTime alias surface"
    );
}

/// A literal fragment still reaches everything it names: a user function, a builtin, a user
/// class it extends, a Reflection class, a date alias, and a builtin throwable it throws.
#[test]
fn test_literal_eval_reaches_every_declaration_its_source_names() {
    let out = compile_and_run(
        r#"<?php
function literal_catalog_hidden(int $n): int { return $n * 3; }
function literal_catalog_unused(): int { return 99; }
class LiteralCatalogBase { public function tag(): string { return "base"; } }
eval('
class LiteralCatalogChild extends LiteralCatalogBase { public function tag(): string { return "child:" . parent::tag(); } }
echo literal_catalog_hidden(4), "|", strtoupper("abc"), "|", (new LiteralCatalogChild())->tag(), "|";
echo (new ReflectionClass("LiteralCatalogBase"))->getShortName(), "|";
echo date_create("2024-02-03 00:00:00")->format("Y-m-d"), "|";
try { throw new DomainException("boom"); } catch (DomainException $e) { echo get_class($e), ":", $e->getMessage(); }
');
"#,
    );
    assert_eq!(
        out,
        "12|ABC|child:base|LiteralCatalogBase|2024-02-03|DomainException:boom"
    );
}

/// A literal fragment declaring a function that later literal code calls still works, and a
/// nested literal eval is read through.
#[test]
fn test_nested_literal_eval_declarations_stay_callable() {
    let out = compile_and_run(
        r#"<?php
function literal_catalog_target(): string { return "target"; }
eval('function literal_catalog_declared(): string { return eval(\'return literal_catalog_target();\'); }');
eval('echo literal_catalog_declared(), "|", function_exists("literal_catalog_target") ? "yes" : "no";');
"#,
    );
    assert_eq!(out, "target|yes");
}

/// A callable the fragment reads from argv can name a class nothing else keeps; the class and
/// its method must survive pruning for `call_user_func`, `is_callable`, `usort` and
/// `Closure::fromCallable` alike.
#[test]
fn test_literal_eval_runtime_callable_keeps_the_class_it_names() {
    let out = compile_and_run_files_per_argv(
        &[(
            "main.php",
            "<?php\nclass LiteralCatalogHidden {\n    public static function ping(): string { return \"pong\"; }\n    public static function cmp(int $a, int $b): int { return $a <=> $b; }\n}\neval('echo call_user_func($argv[1]), \"|\", is_callable($argv[1]) ? \"yes\" : \"no\", \"|\";\n$items = [3, 1, 2]; usort($items, $argv[2]); echo implode(\",\", $items), \"|\";\n$c = Closure::fromCallable($argv[2]); echo $c(2, 1), \"|\";\n$f = $argv[1]; echo $f();');\n",
        )],
        "main.php",
        &[&["LiteralCatalogHidden::ping", "LiteralCatalogHidden::cmp"]],
    );
    assert_eq!(out, vec!["pong|yes|1,2,3|1|pong".to_string()]);
}

/// An array callable whose class and method both arrive at runtime resolves inside a fragment,
/// and a class probe through a computed name still answers after an inline dynamic call.
#[test]
fn test_literal_eval_runtime_array_callables_keep_their_classes() {
    let out = compile_and_run_files_per_argv(
        &[(
            "main.php",
            "<?php\nclass LiteralCatalogPair {\n    public static function ping(): string { return \"pong\"; }\n}\n$f = 'strlen'; echo $f('x'), \"|\";\neval('echo call_user_func([$argv[1], $argv[2]]), \"|\";\n$p = $argv[3]; echo $p($argv[4]) ? \"yes\" : \"no\";');\n",
        )],
        "main.php",
        &[&["LiteralCatalogPair", "ping", "class_exists", "SplStack"]],
    );
    assert_eq!(out, vec!["1|pong|yes".to_string()]);
}

/// A runtime include EXPRESSION inside a fragment keeps the declarations the loaded file calls.
#[test]
fn test_literal_eval_include_expression_keeps_the_included_files_callees() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nfunction literal_catalog_hidden(): int { return 7; }\neval('$result = require \"part.php\"; echo $result;');\n",
            ),
            ("part.php", "<?php\nreturn literal_catalog_hidden();\n"),
        ],
        "main.php",
    );
    assert_eq!(out, "7");
}

/// A fragment redeclaring a native function is refused, like PHP's "Cannot redeclare", even
/// when nothing else references the native declaration.
#[test]
fn test_literal_eval_cannot_redeclare_an_unreferenced_native_function() {
    let stderr = compile_and_run_expect_failure(
        "<?php\nfunction literal_catalog_twice(): int { return 1; }\neval('function literal_catalog_twice(): int { return 2; }');\necho \"unreachable\";\n",
    );
    assert!(stderr.contains("Fatal error"), "{stderr}");
}

/// A fragment redeclaring a builtin class is refused like PHP's "Cannot declare class", even
/// when nothing else in the program names that builtin.
#[test]
fn test_literal_eval_cannot_redeclare_an_unreferenced_builtin_class() {
    let stderr = compile_and_run_expect_failure(
        "<?php\neval('class SplStack {}');\necho \"unreachable\";\n",
    );
    assert!(stderr.contains("Fatal error"), "{stderr}");
}

/// An interface the fragment declares can extend a gated builtin interface nothing else names.
/// The probe runs inside the fragment: inline `interface_exists` does not see eval-declared
/// interfaces on any path, which is a separate, pre-existing gap.
#[test]
fn test_literal_eval_interface_can_extend_an_unreferenced_builtin_interface() {
    let out = compile_and_run(
        "<?php\neval('interface LiteralCatalogChild extends DateTimeInterface {} echo interface_exists(\"LiteralCatalogChild\") ? \"yes\" : \"no\";');\n",
    );
    assert_eq!(out, "yes");
}

/// A fragment calling a date method on an object the outer program built reaches the method,
/// whether it spells the method or picks it at runtime through a nullsafe dynamic call.
#[test]
fn test_literal_eval_reaches_date_methods_on_outer_objects() {
    let out = compile_and_run(
        "<?php\n$d = new DateTime(\"2024-01-01\");\neval('class LiteralCatalogOuterDate {} $d->setDate(2025, 1, 1); echo $d->format(\"Y-m-d\");');\n",
    );
    assert_eq!(out, "2025-01-01");
    let out = compile_and_run_files_per_argv(
        &[(
            "main.php",
            "<?php\n$d = new DateTime(\"2024-01-01\");\neval('class LiteralCatalogOuterDateDyn {} $m = $argv[1]; $d?->$m(2025, 1, 1); echo \"ok|\";');\necho $d->format(\"Y-m-d\");\n",
        )],
        "main.php",
        &[&["setDate"]],
    );
    assert_eq!(out, vec!["ok|2025-01-01".to_string()]);
}
