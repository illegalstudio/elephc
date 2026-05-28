//! Purpose:
//! End-to-end codegen tests for SPL helpers and AOT autoload behavior.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise composer.json autoload sections and compile-time SPL rule extraction.

use crate::support::*;

/// Verifies PSR-4 single namespace autoload.
#[test]
fn test_psr4_single_namespace_autoload() {
    // PSR-4 single namespace maps "App\" → "src/" and class is autoloaded.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Greeter.php",
                "<?php\nnamespace App;\nclass Greeter {\n    public function hello(): string { return \"hi\"; }\n}\n",
            ),
            (
                "main.php",
                "<?php\n$g = new App\\Greeter();\necho $g->hello();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "hi");
}

/// Verifies PSR-4 nested namespace autoload.
#[test]
fn test_psr4_nested_namespace_autoload() {
    // PSR-4 with a two-segment namespace App\Models maps src/ and resolves correctly.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Models/User.php",
                "<?php\nnamespace App\\Models;\nclass User {\n    public function __construct(public string $name) {}\n    public function greet(): string { return \"hello \" . $this->name; }\n}\n",
            ),
            (
                "main.php",
                "<?php\n$u = new App\\Models\\User(\"World\");\necho $u->greet();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "hello World");
}

/// Verifies PSR-4 transitive autoload.
#[test]
fn test_psr4_transitive_autoload() {
    // Greeter uses User via use statement; both classes must be autoloaded transitively.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Models/User.php",
                "<?php\nnamespace App\\Models;\nclass User {\n    public function __construct(public string $name) {}\n}\n",
            ),
            (
                "src/Service/Greeter.php",
                "<?php\nnamespace App\\Service;\nuse App\\Models\\User;\nclass Greeter {\n    public function greet(User $u): string { return \"hi \" . $u->name; }\n}\n",
            ),
            (
                "main.php",
                "<?php\n$u = new App\\Models\\User(\"Ada\");\n$g = new App\\Service\\Greeter();\necho $g->greet($u);\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "hi Ada");
}

/// Verifies PSR-4 static property assignment triggers autoload.
#[test]
fn test_psr4_static_property_assignment_triggers_autoload() {
    // Static property write on App\State\::$count triggers PSR-4 autoload.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/State.php",
                "<?php\nnamespace App;\nclass State { public static int $count = 0; }\n",
            ),
            (
                "main.php",
                "<?php\nApp\\State::$count = 1;\necho \"ok\";\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies PSR-4 scoped constant access triggers autoload.
#[test]
fn test_psr4_scoped_constant_access_triggers_autoload() {
    // Class constant access App\Config::NAME triggers PSR-4 autoload.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Config.php",
                "<?php\nnamespace App;\nclass Config { public const NAME = \"cfg\"; }\n",
            ),
            ("main.php", "<?php\necho App\\Config::NAME;\n"),
        ],
        "main.php",
    );
    assert_eq!(out, "cfg");
}

/// Verifies PSR-4 pipe value triggers autoload.
#[test]
fn test_psr4_pipe_value_triggers_autoload() {
    // First-class callable pipe syntax `|>` with class argument triggers PSR-4 autoload.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/PipeClass.php",
                "<?php\nnamespace App;\nclass PipeClass { public function tag(): string { return \"pipe\"; } }\n",
            ),
            (
                "main.php",
                "<?php\nfunction id(App\\PipeClass $p): string { return $p->tag(); }\necho (new App\\PipeClass()) |> id(...);\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "pipe");
}

/// Verifies PSR-4 vendor autoload.
#[test]
fn test_psr4_vendor_autoload() {
    // Nested vendor PSR-4: Acme\Widgets\ maps vendor/acme/widgets/src/.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "vendor/acme/widgets/composer.json",
                r#"{"autoload":{"psr-4":{"Acme\\Widgets\\":"src/"}}}"#,
            ),
            (
                "vendor/acme/widgets/src/Widget.php",
                "<?php\nnamespace Acme\\Widgets;\nclass Widget {\n    public function render(): string { return \"WIDGET\"; }\n}\n",
            ),
            (
                "main.php",
                "<?php\n$w = new Acme\\Widgets\\Widget();\necho $w->render();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "WIDGET");
}

/// Verifies no composer JSON compiles normally.
#[test]
fn test_no_composer_json_compiles_normally() {
    // Program without composer.json must still compile; autoload index is empty and class loads via include path.
    let out = compile_and_run_files(
        &[(
            "main.php",
            "<?php\nclass Local {\n    public function hi(): string { return \"local\"; }\n}\n$l = new Local();\necho $l->hi();\n",
        )],
        "main.php",
    );
    assert_eq!(out, "local");
}

/// Verifies SPL autoload register returns true.
#[test]
fn test_spl_autoload_register_returns_true() {
    // spl_autoload_register with a closure returns true on success.
    let out = compile_and_run(
        r#"<?php
$ok = spl_autoload_register(function($name) {});
echo $ok ? "true" : "false";
"#,
    );
    assert_eq!(out, "true");
}

/// Verifies SPL autoload unregister returns true.
#[test]
fn test_spl_autoload_unregister_returns_true() {
    // spl_autoload_unregister returns true after removing a registered autoloader.
    let out = compile_and_run(
        r#"<?php
$ok = spl_autoload_unregister(function($name) {});
echo $ok ? "true" : "false";
"#,
    );
    assert_eq!(out, "true");
}

/// Verifies SPL autoload functions returns empty array.
#[test]
fn test_spl_autoload_functions_returns_empty_array() {
    // spl_autoload_functions() returns empty array when no autoloaders are registered.
    let out = compile_and_run(
        r#"<?php
$fns = spl_autoload_functions();
echo count($fns);
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies SPL autoload extensions returns default.
#[test]
fn test_spl_autoload_extensions_returns_default() {
    // spl_autoload_extensions() returns the default ".inc,.php" when called with no argument.
    let out = compile_and_run(
        r#"<?php
echo spl_autoload_extensions();
"#,
    );
    assert_eq!(out, ".inc,.php");
}

/// Verifies SPL autoload call compiles as noop.
#[test]
fn test_spl_autoload_call_compiles_as_noop() {
    // spl_autoload_call with a literal class name compiles as a no-op (no registered autoloaders).
    let out = compile_and_run(
        r#"<?php
spl_autoload_call("Foo");
echo "after";
"#,
    );
    assert_eq!(out, "after");
}

/// Verifies SPL autoload compiles as noop.
#[test]
fn test_spl_autoload_compiles_as_noop() {
    // spl_autoload (deprecated no-op) must compile and execute without error.
    let out = compile_and_run(
        r#"<?php
spl_autoload("Bar");
echo "after";
"#,
    );
    assert_eq!(out, "after");
}

// --- closure-aware spl_autoload_register ---

/// Verifies register with concat closure loads class.
#[test]
fn test_register_with_concat_closure_loads_class() {
    // Closure with direct concatenation __DIR__ . '/lib/' . $name . '.php' loads class from lib/.
    let out = compile_and_run_files(
        &[
            (
                "lib/Widget.php",
                "<?php\nclass Widget {\n    public function tag(): string { return \"widget\"; }\n}\n",
            ),
            (
                "main.php",
                "<?php\nspl_autoload_register(function ($name) {\n    require_once __DIR__ . '/lib/' . $name . '.php';\n});\n$w = new Widget();\necho $w->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "widget");
}

/// Verifies register name is case insensitive before name resolver.
#[test]
fn test_register_name_is_case_insensitive_before_name_resolver() {
    // SPL_AUTOLOAD_REGISTER case-insensitive alias compiles and loads class.
    let out = compile_and_run_files(
        &[
            (
                "lib/MixedCase.php",
                "<?php\nclass MixedCase { public function tag(): string { return \"case\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
SPL_AUTOLOAD_REGISTER(function ($name) {
    require_once __DIR__ . '/lib/' . $name . '.php';
});
$m = new MixedCase();
echo $m->tag();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "case");
}

/// Verifies namespaced local SPL autoload register is not collected.
#[test]
fn test_namespaced_local_spl_autoload_register_is_not_collected() {
    // Namespaced local spl_autoload_register shadows the builtin and is called directly.
    let out = compile_and_run(
        r#"<?php
namespace App;
function spl_autoload_register($loader) { echo "local"; }
spl_autoload_register(function ($name) {});
"#,
    );
    assert_eq!(out, "local");
}

/// Verifies unregister name is case insensitive before name resolver.
#[test]
fn test_unregister_name_is_case_insensitive_before_name_resolver() {
    // sPl_AuToLoAd_UnReGiStEr case-insensitive alias removes the autoloader from the stack.
    let out = compile_and_run(
        r#"<?php
spl_autoload_register(function ($name) {
    require_once __DIR__ . '/missing/' . $name . '.php';
});
sPl_AuToLoAd_UnReGiStEr(function ($name) {
    require_once __DIR__ . '/missing/' . $name . '.php';
});
echo count(spl_autoload_functions());
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies register with str replace closure.
#[test]
fn test_register_with_str_replace_closure() {
    // PSR-0-style autoloader: str_replace('\', '_', $name) maps class name to file path.
    let out = compile_and_run_files(
        &[
            (
                "lib/App_User.php",
                "<?php\nclass App_User {\n    public function __construct(public string $name) {}\n}\n",
            ),
            (
                "main.php",
                "<?php\nspl_autoload_register(function ($name) {\n    require_once __DIR__ . '/lib/' . str_replace('\\\\', '_', $name) . '.php';\n});\n$u = new App_User(\"Ada\");\necho $u->name;\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "Ada");
}

/// Verifies register with intermediate variable.
#[test]
fn test_register_with_intermediate_variable() {
    // Closure captures $path in local variable before require_once; variable threading must be correct.
    let out = compile_and_run_files(
        &[
            (
                "lib/Box.php",
                "<?php\nclass Box {\n    public function tag(): string { return \"box\"; }\n}\n",
            ),
            (
                "main.php",
                "<?php\nspl_autoload_register(function ($name) {\n    $path = __DIR__ . '/lib/' . $name . '.php';\n    require_once $path;\n});\n$b = new Box();\necho $b->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "box");
}

/// Verifies register with file exists positive branch.
#[test]
fn test_register_with_file_exists_positive_branch() {
    // Closure guards require_once with file_exists($path); file is present so then-branch loads class.
    let out = compile_and_run_files(
        &[
            (
                "lib/Present.php",
                "<?php\nclass Present {\n    public function tag(): string { return \"present\"; }\n}\n",
            ),
            (
                "main.php",
                "<?php\nspl_autoload_register(function ($name) {\n    $path = __DIR__ . '/lib/' . $name . '.php';\n    if (file_exists($path)) {\n        require_once $path;\n    }\n});\n$p = new Present();\necho $p->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "present");
}

/// Verifies register file exists directory guard loads file.
#[test]
fn test_register_file_exists_directory_guard_loads_file() {
    // file_exists() returns true for directories; base-dir guard passes and class file is loaded.
    let out = compile_and_run_files(
        &[
            (
                "lib/DirectoryGuard.php",
                "<?php\nclass DirectoryGuard {\n    public function tag(): string { return \"dir\"; }\n}\n",
            ),
            (
                "main.php",
                "<?php\nspl_autoload_register(function ($name) {\n    $base = __DIR__ . '/lib';\n    if (file_exists($base)) {\n        require_once $base . '/' . $name . '.php';\n    }\n});\n$d = new DirectoryGuard();\necho $d->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "dir");
}

/// Verifies register is readable directory guard loads file.
#[test]
fn test_register_is_readable_directory_guard_loads_file() {
    // is_readable() on a directory returns true; class file is loaded through the guard.
    let out = compile_and_run_files(
        &[
            (
                "lib/ReadableGuard.php",
                "<?php\nclass ReadableGuard {\n    public function tag(): string { return \"readable\"; }\n}\n",
            ),
            (
                "main.php",
                "<?php\nspl_autoload_register(function ($name) {\n    $base = __DIR__ . '/lib';\n    if (is_readable($base)) {\n        require_once $base . '/' . $name . '.php';\n    }\n});\n$r = new ReadableGuard();\necho $r->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "readable");
}

/// Verifies register chain first misses second loads.
#[test]
fn test_register_chain_first_misses_second_loads() {
    // Two registered closures; first misses (file not in lib/missing/), second hits (file in lib/).
    let out = compile_and_run_files(
        &[
            (
                "lib/Chained.php",
                "<?php\nclass Chained {\n    public function tag(): string { return \"chained\"; }\n}\n",
            ),
            (
                "main.php",
                "<?php\nspl_autoload_register(function ($name) {\n    $path = __DIR__ . '/lib/missing/' . $name . '.php';\n    if (file_exists($path)) {\n        require_once $path;\n    }\n});\nspl_autoload_register(function ($name) {\n    $path = __DIR__ . '/lib/' . $name . '.php';\n    if (file_exists($path)) {\n        require_once $path;\n    }\n});\n$c = new Chained();\necho $c->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "chained");
}

/// Verifies register unregister round trip.
#[test]
fn test_register_unregister_round_trip() {
    // Register two closures, unregister the first; second closure still loads class.
    let out = compile_and_run_files(
        &[
            (
                "lib/Survives.php",
                "<?php\nclass Survives {\n    public function tag(): string { return \"alive\"; }\n}\n",
            ),
            (
                "main.php",
                "<?php\nspl_autoload_register(function ($name) {\n    require_once __DIR__ . '/missing/' . $name . '.php';\n});\nspl_autoload_register(function ($name) {\n    require_once __DIR__ . '/lib/' . $name . '.php';\n});\nspl_autoload_unregister(function ($name) {\n    require_once __DIR__ . '/missing/' . $name . '.php';\n});\n$s = new Survives();\necho $s->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "alive");
}

/// Verifies register with use capture falls back to PSR-4.
#[test]
fn test_register_with_use_capture_falls_back_to_psr4() {
    // Closure with `use ($base)` capture is rejected by collector; PSR-4 from composer.json takes over.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Fallback.php",
                "<?php\nnamespace App;\nclass Fallback {\n    public function tag(): string { return \"psr4\"; }\n}\n",
            ),
            (
                "main.php",
                "<?php\n$base = __DIR__ . '/elsewhere';\nspl_autoload_register(function ($name) use ($base) {\n    require_once $base . '/' . $name . '.php';\n});\n$f = new App\\Fallback();\necho $f->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "psr4");
}

/// Verifies SPL autoload extensions round trip.
#[test]
fn test_spl_autoload_extensions_round_trip() {
    // Read default, write new value (returns old), read again returns the new value.
    let out = compile_and_run(
        r#"<?php
echo spl_autoload_extensions();
echo "\n";
$old = spl_autoload_extensions(".php,.inc");
echo $old;
echo "\n";
echo spl_autoload_extensions();
"#,
    );
    assert_eq!(out, ".inc,.php\n.inc,.php\n.php,.inc");
}

/// Verifies SPL autoload extensions null arg is readonly.
#[test]
fn test_spl_autoload_extensions_null_arg_is_readonly() {
    // spl_autoload_extensions(null) is explicit read-only; global is unchanged afterward.
    let out = compile_and_run(
        r#"<?php
spl_autoload_extensions(".custom");
$current = spl_autoload_extensions(null);
echo $current;
echo "\n";
echo spl_autoload_extensions();
"#,
    );
    assert_eq!(out, ".custom\n.custom");
}

/// Verifies SPL autoload functions size reflects register count.
#[test]
fn test_spl_autoload_functions_size_reflects_register_count() {
    // Two registered closures; spl_autoload_functions() count is 2 and class loads via second rule.
    let out = compile_and_run_files(
        &[
            (
                "lib/Alpha.php",
                "<?php\nclass Alpha { public function tag(): string { return \"a\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function ($name) {
    require_once __DIR__ . '/lib/' . $name . '.php';
});
spl_autoload_register(function ($name) {
    require_once __DIR__ . '/missing/' . $name . '.php';
});
$fns = spl_autoload_functions();
echo count($fns);
$a = new Alpha();
echo $a->tag();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "2a");
}

/// Verifies SPL autoload functions iterable.
#[test]
fn test_spl_autoload_functions_iterable() {
    // foreach over spl_autoload_functions() iterates one entry per registered rule.
    let out = compile_and_run_files(
        &[
            (
                "lib/Beta.php",
                "<?php\nclass Beta { public function tag(): string { return \"b\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function ($name) {
    require_once __DIR__ . '/lib/' . $name . '.php';
});
$count = 0;
foreach (spl_autoload_functions() as $entry) {
    $count = $count + 1;
}
echo $count;
$b = new Beta();
echo $b->tag();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "1b");
}

// --- composer.json autoload sections ---

/// Verifies autoload files section always inlines.
#[test]
fn test_autoload_files_section_always_inlines() {
    // Files listed under autoload.files are inlined unconditionally; no class autoload needed.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"files":["src/helpers.php"]}}"#,
            ),
            (
                "src/helpers.php",
                "<?php\nfunction shout(string $s): string { return strtoupper($s); }\n",
            ),
            (
                "main.php",
                "<?php\necho shout(\"hello\");\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "HELLO");
}

/// Verifies autoload files section executes before main in composer order.
#[test]
fn test_autoload_files_section_executes_before_main_in_composer_order() {
    // Files autoload sections execute in composer.json order before main.php.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"files":["src/a.php","src/b.php"]}}"#,
            ),
            ("src/a.php", "<?php\necho \"a\";\n"),
            ("src/b.php", "<?php\necho \"b\";\n"),
            ("main.php", "<?php\necho \"m\";\n"),
        ],
        "main.php",
    );
    assert_eq!(out, "abm");
}

/// Verifies class triggered autoload executes before first use.
#[test]
fn test_class_triggered_autoload_executes_before_first_use() {
    // Class file echoes "load" when parsed; must execute before first class instantiation in main.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Foo.php",
                "<?php\nnamespace App;\necho \"load\";\nclass Foo {}\n",
            ),
            (
                "main.php",
                "<?php\n$f = new App\\Foo();\necho \"main\";\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loadmain");
}

/// Verifies autoload classmap explicit file.
#[test]
fn test_autoload_classmap_explicit_file() {
    // classmap entry with explicit .php file scans it for class declarations and indexes by FQN.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"classmap":["lib/AnyName.php"]}}"#,
            ),
            (
                "lib/AnyName.php",
                "<?php\nnamespace App;\nclass Mapped { public function tag(): string { return \"mapped\"; } }\n",
            ),
            (
                "main.php",
                "<?php\n$m = new App\\Mapped();\necho $m->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "mapped");
}

/// Verifies autoload classmap directory scan.
#[test]
fn test_autoload_classmap_directory_scan() {
    // classmap entry with directory path recursively walks it and indexes all class FQNs.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"classmap":["legacy/"]}}"#,
            ),
            (
                "legacy/whatever.php",
                "<?php\nnamespace Vendor\\Legacy;\nclass Tool { public function tag(): string { return \"legacy-tool\"; } }\n",
            ),
            (
                "main.php",
                "<?php\n$t = new Vendor\\Legacy\\Tool();\necho $t->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "legacy-tool");
}

/// Verifies autoload dev PSR-4 section.
#[test]
fn test_autoload_dev_psr4_section() {
    // autoload-dev PSR-4 is merged into the same AOT index as autoload; both sections contribute to one binary.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}},"autoload-dev":{"psr-4":{"App\\Tests\\":"tests/"}}}"#,
            ),
            (
                "src/Service.php",
                "<?php\nnamespace App;\nclass Service { public function tag(): string { return \"svc\"; } }\n",
            ),
            (
                "tests/ServiceTest.php",
                "<?php\nnamespace App\\Tests;\nclass ServiceTest { public function tag(): string { return \"test\"; } }\n",
            ),
            (
                "main.php",
                "<?php\n$s = new App\\Service();\n$t = new App\\Tests\\ServiceTest();\necho $s->tag() . \":\" . $t->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "svc:test");
}

/// Verifies PSR-0 namespaced prefix.
#[test]
fn test_psr0_namespaced_prefix() {
    // Legacy PSR-0 with namespaced prefix Vendor\Pkg maps to vendor-src/ directory.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-0":{"Vendor\\Pkg":"vendor-src/"}}}"#,
            ),
            (
                "vendor-src/Vendor/Pkg/Sub/Item.php",
                "<?php\nnamespace Vendor\\Pkg\\Sub;\nclass Item { public function tag(): string { return \"psr0\"; } }\n",
            ),
            (
                "main.php",
                "<?php\n$i = new Vendor\\Pkg\\Sub\\Item();\necho $i->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "psr0");
}

/// Verifies PSR-0 underscore class convention.
#[test]
fn test_psr0_underscore_class_convention() {
    // Twig_Loader_Filesystem-style underscore-as-directory convention: Twig_ prefix maps to lib/.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-0":{"Twig_":"lib/"}}}"#,
            ),
            (
                "lib/Twig/Loader/Filesystem.php",
                "<?php\nclass Twig_Loader_Filesystem {\n    public function tag(): string { return \"twig\"; }\n}\n",
            ),
            (
                "main.php",
                "<?php\n$t = new Twig_Loader_Filesystem();\necho $t->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "twig");
}

/// Verifies PSR-4 longest prefix wins.
#[test]
fn test_psr4_longest_prefix_wins() {
    // Two PSR-4 prefixes both matching App\Models\User; the longer prefix wins (App\Models\ > App\).
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/","App\\Models\\":"models/"}}}"#,
            ),
            (
                "src/Models/User.php",
                "<?php\nnamespace App\\Models;\nclass User { public function tag(): string { return \"src\"; } }\n",
            ),
            (
                "models/User.php",
                "<?php\nnamespace App\\Models;\nclass User { public function tag(): string { return \"models\"; } }\n",
            ),
            (
                "main.php",
                "<?php\n$u = new App\\Models\\User();\necho $u->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "models");
}

/// Verifies class exists literal triggers autoload.
#[test]
fn test_class_exists_literal_triggers_autoload() {
    // class_exists with a literal class name and default autoload=true loads class via PSR-4.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Probe.php",
                "<?php\nnamespace App;\nclass Probe { public function tag(): string { return \"probed\"; } }\n",
            ),
            (
                "main.php",
                "<?php\nif (class_exists(\"App\\\\Probe\")) {\n    $p = new App\\Probe();\n    echo $p->tag();\n}\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "probed");
}

/// Verifies class exists with explicit true triggers autoload.
#[test]
fn test_class_exists_with_explicit_true_triggers_autoload() {
    // class_exists with explicit autoload=true (second arg = true) triggers PSR-4 autoload.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Forced.php",
                "<?php\nnamespace App;\nclass Forced { public function tag(): string { return \"f\"; } }\n",
            ),
            (
                "main.php",
                "<?php\nclass_exists(\"App\\\\Forced\", true);\n$f = new App\\Forced();\necho $f->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "f");
}

/// Verifies class exists with int nonzero triggers autoload.
#[test]
fn test_class_exists_with_int_nonzero_triggers_autoload() {
    // class_exists($name, 1) with integer non-zero second arg behaves like autoload=true.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/IntFlag.php",
                "<?php\nnamespace App;\nclass IntFlag { public function tag(): string { return \"int\"; } }\n",
            ),
            (
                "main.php",
                "<?php\nclass_exists(\"App\\\\IntFlag\", 1);\n$i = new App\\IntFlag();\necho $i->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "int");
}

/// Verifies class exists dynamic autoload arg does not trigger aot autoload.
#[test]
fn test_class_exists_dynamic_autoload_arg_does_not_trigger_aot_autoload() {
    // class_exists with a variable (non-literal) second arg must not trigger AOT autoload; panics.
    let result = std::panic::catch_unwind(|| {
        compile_and_run_files(
            &[
                (
                    "composer.json",
                    r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
                ),
                (
                    "src/DynamicFlag.php",
                    "<?php\nnamespace App;\nclass DynamicFlag {}\n",
                ),
                (
                    "main.php",
                    "<?php\n$autoload = true;\nclass_exists(\"App\\\\DynamicFlag\", $autoload);\n$d = new App\\DynamicFlag();\necho \"loaded\";\n",
                ),
            ],
            "main.php",
        )
    });
    assert!(result.is_err());
}

/// Verifies interface exists literal triggers autoload.
#[test]
fn test_interface_exists_literal_triggers_autoload() {
    // interface_exists with literal name triggers PSR-4 autoload and class implementing it is available.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Renderable.php",
                "<?php\nnamespace App;\ninterface Renderable { public function render(): string; }\n",
            ),
            (
                "src/Widget.php",
                "<?php\nnamespace App;\nclass Widget implements Renderable { public function render(): string { return \"w\"; } }\n",
            ),
            (
                "main.php",
                "<?php\ninterface_exists(\"App\\\\Renderable\");\n$w = new App\\Widget();\necho $w->render();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "w");
}

/// Verifies class like exists literals are case insensitive.
#[test]
fn test_class_like_exists_literals_are_case_insensitive() {
    // class_exists, interface_exists, enum_exists with lowercase names are case-insensitive.
    let out = compile_and_run(
        r#"<?php
class Foo {}
interface Paintable {}
enum Status { case Ready; }
echo class_exists("foo", false) ? "c" : "n";
echo interface_exists("paintable", false) ? "i" : "n";
echo enum_exists("status", false) ? "e" : "n";
"#,
    );
    assert_eq!(out, "cie");
}

/// Verifies trait exists reports declared traits.
#[test]
fn test_trait_exists_reports_declared_traits() {
    // trait_exists reports declared traits; both canonical and lowercase names return true.
    let out = compile_and_run(
        r#"<?php
trait VisibleTrait {}
echo trait_exists("VisibleTrait", false) ? "y" : "n";
echo trait_exists("visibletrait", false) ? "y" : "n";
"#,
    );
    assert_eq!(out, "yy");
}

// --- alternative register call shapes ---

/// Verifies register with variable stored closure.
#[test]
fn test_register_with_variable_stored_closure() {
    // Closure stored in $loader variable then registered with spl_autoload_register($loader) loads class.
    let out = compile_and_run_files(
        &[
            (
                "lib/Stored.php",
                "<?php\nclass Stored { public function tag(): string { return \"stored\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
$loader = function ($name) {
    require_once __DIR__ . '/lib/' . $name . '.php';
};
spl_autoload_register($loader);
$s = new Stored();
echo $s->tag();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "stored");
}

/// Verifies register with function name string.
#[test]
fn test_register_with_function_name_string() {
    // spl_autoload_register('myAutoloader') with function name string loads class via named autoloader.
    let out = compile_and_run_files(
        &[
            (
                "lib/Named.php",
                "<?php\nclass Named { public function tag(): string { return \"named\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
function myAutoloader($name) {
    require_once __DIR__ . '/lib/' . $name . '.php';
}
spl_autoload_register('myAutoloader');
$n = new Named();
echo $n->tag();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "named");
}

/// Verifies register inside if true block.
#[test]
fn test_register_inside_if_true_block() {
    // if (true) { spl_autoload_register(...); } folds at compile time; closure is collected.
    let out = compile_and_run_files(
        &[
            (
                "lib/Guarded.php",
                "<?php\nclass Guarded { public function tag(): string { return \"in\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
if (true) {
    spl_autoload_register(function ($name) {
        require_once __DIR__ . '/lib/' . $name . '.php';
    });
}
$g = new Guarded();
echo $g->tag();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "in");
}

/// Verifies register inside if false else block.
#[test]
fn test_register_inside_if_false_else_block() {
    // if (false) { ... } else { register(...); } else branch taken at compile time; closure is collected.
    let out = compile_and_run_files(
        &[
            (
                "lib/Else_.php",
                "<?php\nclass Else_ { public function tag(): string { return \"else\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
if (false) {
    spl_autoload_register(function ($name) {
        require_once __DIR__ . '/missing/' . $name . '.php';
    });
} else {
    spl_autoload_register(function ($name) {
        require_once __DIR__ . '/lib/' . $name . '.php';
    });
}
$e = new Else_();
echo $e->tag();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "else");
}

/// Verifies register with sprintf in closure.
#[test]
fn test_register_with_sprintf_in_closure() {
    // Closure uses sprintf to construct the require_once path; class file is found and loaded.
    let out = compile_and_run_files(
        &[
            (
                "src/Formatted.php",
                "<?php\nclass Formatted { public function tag(): string { return \"fmt\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function ($name) {
    require_once sprintf("%s/src/%s.php", __DIR__, $name);
});
$f = new Formatted();
echo $f->tag();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "fmt");
}

/// Verifies register with dirname in closure.
#[test]
fn test_register_with_dirname_in_closure() {
    // Closure uses dirname(__DIR__) to navigate to sibling lib/ directory; class is loaded.
    let out = compile_and_run_files(
        &[
            (
                "lib/Above.php",
                "<?php\nclass Above { public function tag(): string { return \"above\"; } }\n",
            ),
            (
                "sub/main.php",
                r#"<?php
spl_autoload_register(function ($name) {
    require_once dirname(__DIR__) . '/lib/' . $name . '.php';
});
$a = new Above();
echo $a->tag();
"#,
            ),
        ],
        "sub/main.php",
    );
    assert_eq!(out, "above");
}

// --- introspection + alias + extra autoload sections ---

/// Verifies SPL object ID unique and stable.
#[test]
fn test_spl_object_id_unique_and_stable() {
    // spl_object_id returns same value for same object (stable) and different values for distinct objects (unique).
    let out = compile_and_run(
        r#"<?php
class Box {}
$a = new Box();
$b = new Box();
echo (spl_object_id($a) === spl_object_id($a)) ? "stable" : "drift";
echo ":";
echo (spl_object_id($a) !== spl_object_id($b)) ? "unique" : "same";
"#,
    );
    assert_eq!(out, "stable:unique");
}

/// Verifies SPL object hash distinct.
#[test]
fn test_spl_object_hash_distinct() {
    // spl_object_hash returns same value for same object (stable) and different values for distinct objects (unique).
    let out = compile_and_run(
        r#"<?php
class Box {}
$a = new Box();
$b = new Box();
echo (spl_object_hash($a) === spl_object_hash($a)) ? "stable" : "drift";
echo ":";
echo (spl_object_hash($a) !== spl_object_hash($b)) ? "unique" : "same";
"#,
    );
    assert_eq!(out, "stable:unique");
}

/// Verifies SPL classes returns known set.
#[test]
fn test_spl_classes_returns_known_set() {
    // spl_classes() includes Exception, Error, and LogicException in the returned array.
    let out = compile_and_run(
        r#"<?php
$names = spl_classes();
$found_exception = false;
$found_error = false;
$found_logic = false;
foreach ($names as $n) {
    if ($n === "Exception") $found_exception = true;
    if ($n === "Error") $found_error = true;
    if ($n === "LogicException") $found_logic = true;
}
echo $found_exception ? "e" : "-";
echo $found_error ? "r" : "-";
echo $found_logic ? "l" : "-";
"#,
    );
    assert_eq!(out, "erl");
}

/// Verifies get class returns static type.
#[test]
fn test_get_class_returns_static_type() {
    // get_class($dog) returns "Dog"; get_parent_class returns "Animal".
    let out = compile_and_run(
        r#"<?php
class Animal {}
class Dog extends Animal {}
$d = new Dog();
echo get_class($d);
echo ":";
echo get_parent_class($d);
"#,
    );
    assert_eq!(out, "Dog:Animal");
}

/// Verifies is a walks parent chain.
#[test]
fn test_is_a_walks_parent_chain() {
    // is_a and is_subclass_of walk the parent chain: Dog → Animal.
    let out = compile_and_run(
        r#"<?php
class Animal {}
class Dog extends Animal {}
$d = new Dog();
echo is_a($d, "Dog") ? "y" : "n";
echo is_a($d, "Animal") ? "y" : "n";
echo is_a($d, "Cat") ? "y" : "n";
echo is_subclass_of($d, "Dog") ? "y" : "n";
echo is_subclass_of($d, "Animal") ? "y" : "n";
"#,
    );
    assert_eq!(out, "yynny");
}

/// Verifies is a target string is case insensitive.
#[test]
fn test_is_a_target_string_is_case_insensitive() {
    // is_a and is_subclass_of target string is case-insensitive (lowercase "dog", "animal", "pettable").
    let out = compile_and_run(
        r#"<?php
interface Pettable {}
class Animal {}
class Dog extends Animal implements Pettable {}
$dog = new Dog();
echo is_a($dog, "dog") ? "d" : "n";
echo is_a($dog, "animal") ? "a" : "n";
echo is_a($dog, "pettable") ? "p" : "n";
echo is_subclass_of($dog, "animal") ? "s" : "n";
"#,
    );
    assert_eq!(out, "daps");
}

/// Verifies is a parent chain normalizes namespaced parent.
#[test]
fn test_is_a_parent_chain_normalizes_namespaced_parent() {
    // is_a with backslash-prefixed uppercase namespaced parent class normalizes correctly.
    let out = compile_and_run(
        r#"<?php
namespace App;
class Animal {}
class Dog extends \App\Animal {}
$dog = new Dog();
echo is_a($dog, "app\\animal") ? "a" : "n";
echo is_subclass_of($dog, "\\APP\\ANIMAL") ? "s" : "n";
"#,
    );
    assert_eq!(out, "as");
}

/// Verifies is a walks implemented interface.
#[test]
fn test_is_a_walks_implemented_interface() {
    // is_a and is_subclass_of walk implemented interfaces: Cat implements Pettable.
    let out = compile_and_run(
        r#"<?php
interface Pettable {}
class Cat implements Pettable {}
$c = new Cat();
echo is_a($c, "Pettable") ? "y" : "n";
echo is_a($c, "Cat") ? "y" : "n";
echo is_subclass_of($c, "Pettable") ? "y" : "n";
"#,
    );
    assert_eq!(out, "yyy");
}

/// Verifies register with use capture warns.
#[test]
fn test_register_with_use_capture_warns() {
    // Closure with `use ($base)` capture is rejected; count(spl_autoload_functions) is 0 afterward.
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$base = __DIR__;
spl_autoload_register(function ($name) use ($base) {
    require_once $base . '/x.php';
});
echo count(spl_autoload_functions());
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "0");
}

/// Verifies get declared classes includes user classes.
#[test]
fn test_get_declared_classes_includes_user_classes() {
    // get_declared_classes() includes user-declared Alpha and Beta.
    let out = compile_and_run(
        r#"<?php
class Alpha {}
class Beta {}
$classes = get_declared_classes();
$found_alpha = false;
$found_beta = false;
foreach ($classes as $c) {
    if ($c === "Alpha") $found_alpha = true;
    if ($c === "Beta") $found_beta = true;
}
echo $found_alpha ? "a" : "-";
echo $found_beta ? "b" : "-";
"#,
    );
    assert_eq!(out, "ab");
}

/// Verifies get declared classes preserves user declaration order.
#[test]
fn test_get_declared_classes_preserves_user_declaration_order() {
    // get_declared_classes() preserves declaration order: Zebra appears before Alpha.
    let out = compile_and_run(
        r#"<?php
class Zebra {}
class Alpha {}
$classes = get_declared_classes();
$idx = 0;
$zebra = -1;
$alpha = -1;
foreach ($classes as $c) {
    if ($c === "Zebra") $zebra = $idx;
    if ($c === "Alpha") $alpha = $idx;
    $idx = $idx + 1;
}
echo ($zebra >= 0 && $alpha >= 0 && $zebra < $alpha) ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies get declared interfaces includes user interfaces.
#[test]
fn test_get_declared_interfaces_includes_user_interfaces() {
    // get_declared_interfaces() includes user-declared MyContract.
    let out = compile_and_run(
        r#"<?php
interface MyContract {}
$ifaces = get_declared_interfaces();
$found = false;
foreach ($ifaces as $i) {
    if ($i === "MyContract") $found = true;
}
echo $found ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies get declared interfaces preserves user declaration order.
#[test]
fn test_get_declared_interfaces_preserves_user_declaration_order() {
    // get_declared_interfaces() preserves declaration order: ZebraContract appears before AlphaContract.
    let out = compile_and_run(
        r#"<?php
interface ZebraContract {}
interface AlphaContract {}
$ifaces = get_declared_interfaces();
$idx = 0;
$zebra = -1;
$alpha = -1;
foreach ($ifaces as $i) {
    if ($i === "ZebraContract") $zebra = $idx;
    if ($i === "AlphaContract") $alpha = $idx;
    $idx = $idx + 1;
}
echo ($zebra >= 0 && $alpha >= 0 && $zebra < $alpha) ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies get declared traits preserves user declaration order.
#[test]
fn test_get_declared_traits_preserves_user_declaration_order() {
    // get_declared_traits() preserves declaration order: ZebraTrait appears before AlphaTrait.
    let out = compile_and_run(
        r#"<?php
trait ZebraTrait {}
trait AlphaTrait {}
$traits = get_declared_traits();
$idx = 0;
$zebra = -1;
$alpha = -1;
foreach ($traits as $t) {
    if ($t === "ZebraTrait") $zebra = $idx;
    if ($t === "AlphaTrait") $alpha = $idx;
    $idx = $idx + 1;
}
echo ($zebra >= 0 && $alpha >= 0 && $zebra < $alpha) ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies class alias creates subclass.
#[test]
fn test_class_alias_creates_subclass() {
    // class_alias("Original", "Alias") creates Alias as subclass of Original; instanceof works both ways.
    let out = compile_and_run(
        r#"<?php
class Original {
    public function tag(): string { return "orig"; }
}
class_alias("Original", "Alias");
$a = new Alias();
echo $a->tag();
echo ":";
echo ($a instanceof Original) ? "yes" : "no";
echo ":";
echo ($a instanceof Alias) ? "yes" : "no";
"#,
    );
    assert_eq!(out, "orig:yes:yes");
}

/// Verifies class alias with namespace.
#[test]
fn test_class_alias_with_namespace() {
    // class_alias with namespaced original App\Original and namespaced alias App\Alias creates valid alias.
    let out = compile_and_run(
        r#"<?php
namespace App;
class Original {
    public function tag(): string { return "ns-orig"; }
}
\class_alias("App\\Original", "App\\Alias");
$a = new Alias();
echo $a->tag();
"#,
    );
    assert_eq!(out, "ns-orig");
}

/// Verifies class alias name is case insensitive before name resolver.
#[test]
fn test_class_alias_name_is_case_insensitive_before_name_resolver() {
    // CLASS_ALIAS case-insensitive alias compiles and creates alias; new alias class is usable.
    let out = compile_and_run(
        r#"<?php
class Original {
    public function tag(): string { return "alias-case"; }
}
CLASS_ALIAS("Original", "AliasCase");
$a = new AliasCase();
echo $a->tag();
"#,
    );
    assert_eq!(out, "alias-case");
}

/// Verifies PSR-4 empty prefix root namespace.
#[test]
fn test_psr4_empty_prefix_root_namespace() {
    // PSR-4 with empty prefix "" maps root namespace to src/; Plain class in src/ is found.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"":"src/"}}}"#,
            ),
            (
                "src/Plain.php",
                "<?php\nclass Plain { public function tag(): string { return \"root\"; } }\n",
            ),
            (
                "main.php",
                "<?php\n$p = new Plain();\necho $p->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "root");
}

/// Verifies autoload does not shadow builtin exception.
#[test]
fn test_autoload_does_not_shadow_builtin_exception() {
    // User-defined Exception class in autoload must not shadow the builtin SPL Exception.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"":"src/"}}}"#,
            ),
            (
                "src/Exception.php",
                "<?php\nclass Exception { public function broken(): string { return \"wrong\"; } }\n",
            ),
            (
                "main.php",
                "<?php\ntry {\n    throw new Exception(\"core\");\n} catch (Exception $e) {\n    echo $e->getMessage();\n}\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "core");
}

/// Verifies classmap exclude from classmap.
#[test]
fn test_classmap_exclude_from_classmap() {
    // lib/ contains Real class plus tests/ subdirectory excluded via exclude-from-classmap; Excluded is missing.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"classmap":["lib/"],"exclude-from-classmap":["lib/tests/"]}}"#,
            ),
            (
                "lib/Real.php",
                "<?php\nclass Real { public function tag(): string { return \"real\"; } }\n",
            ),
            (
                "lib/tests/Excluded.php",
                "<?php\nclass Excluded { public function tag(): string { return \"excluded\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
$r = new Real();
echo $r->tag();
echo ":";
echo class_exists("Excluded", false) ? "exists" : "missing";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "real:missing");
}

/// Verifies classmap exclude with double star.
#[test]
fn test_classmap_exclude_with_double_star() {
    // **/internal/** glob pattern excludes lib/sub/internal/Hidden.php from classmap; Hidden is missing.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"classmap":["lib/"],"exclude-from-classmap":["**/internal/**"]}}"#,
            ),
            (
                "lib/Real.php",
                "<?php\nclass Real { public function tag(): string { return \"real\"; } }\n",
            ),
            (
                "lib/sub/internal/Hidden.php",
                "<?php\nclass Hidden { public function tag(): string { return \"hidden\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
$r = new Real();
echo $r->tag();
echo ":";
echo class_exists("Hidden", false) ? "exists" : "missing";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "real:missing");
}

/// Verifies classmap exclude with filename glob.
#[test]
fn test_classmap_exclude_with_filename_glob() {
    // lib/*.test.php glob excludes lib/Foo.test.php from classmap; FooTest is missing.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"classmap":["lib/"],"exclude-from-classmap":["lib/*.test.php"]}}"#,
            ),
            (
                "lib/Real.php",
                "<?php\nclass Real { public function tag(): string { return \"real\"; } }\n",
            ),
            (
                "lib/Foo.test.php",
                "<?php\nclass FooTest { public function tag(): string { return \"test\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
$r = new Real();
echo $r->tag();
echo ":";
echo class_exists("FooTest", false) ? "exists" : "missing";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "real:missing");
}

/// Verifies SPL autoload call with literal loads class.
#[test]
fn test_spl_autoload_call_with_literal_loads_class() {
    // spl_autoload_call("App\\Forced") forces autoload resolution even when program doesn't reference the class.
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Forced.php",
                "<?php\nnamespace App;\nclass Forced {\n    public function tag(): string { return \"forced\"; }\n}\n",
            ),
            (
                "main.php",
                "<?php\nspl_autoload_call(\"App\\\\Forced\");\n$f = new App\\Forced();\necho $f->tag();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "forced");
}
