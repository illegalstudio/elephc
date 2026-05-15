//! Purpose:
//! End-to-end codegen tests for SPL helpers and AOT autoload behavior.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise composer.json autoload sections and compile-time SPL rule extraction.

use crate::support::*;

#[test]
fn test_psr4_single_namespace_autoload() {
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

#[test]
fn test_psr4_nested_namespace_autoload() {
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

#[test]
fn test_psr4_transitive_autoload() {
    // Greeter uses User; both must be autoloaded transitively.
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

#[test]
fn test_psr4_static_property_assignment_triggers_autoload() {
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

#[test]
fn test_psr4_scoped_constant_access_triggers_autoload() {
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

#[test]
fn test_psr4_pipe_value_triggers_autoload() {
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

#[test]
fn test_psr4_vendor_autoload() {
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

#[test]
fn test_no_composer_json_compiles_normally() {
    // Programs without composer.json must still compile; autoload is a no-op
    // when the index is empty.
    let out = compile_and_run_files(
        &[(
            "main.php",
            "<?php\nclass Local {\n    public function hi(): string { return \"local\"; }\n}\n$l = new Local();\necho $l->hi();\n",
        )],
        "main.php",
    );
    assert_eq!(out, "local");
}

#[test]
fn test_spl_autoload_register_returns_true() {
    let out = compile_and_run(
        r#"<?php
$ok = spl_autoload_register(function($name) {});
echo $ok ? "true" : "false";
"#,
    );
    assert_eq!(out, "true");
}

#[test]
fn test_spl_autoload_unregister_returns_true() {
    let out = compile_and_run(
        r#"<?php
$ok = spl_autoload_unregister(function($name) {});
echo $ok ? "true" : "false";
"#,
    );
    assert_eq!(out, "true");
}

#[test]
fn test_spl_autoload_functions_returns_empty_array() {
    let out = compile_and_run(
        r#"<?php
$fns = spl_autoload_functions();
echo count($fns);
"#,
    );
    assert_eq!(out, "0");
}

#[test]
fn test_spl_autoload_extensions_returns_default() {
    let out = compile_and_run(
        r#"<?php
echo spl_autoload_extensions();
"#,
    );
    assert_eq!(out, ".inc,.php");
}

#[test]
fn test_spl_autoload_call_compiles_as_noop() {
    let out = compile_and_run(
        r#"<?php
spl_autoload_call("Foo");
echo "after";
"#,
    );
    assert_eq!(out, "after");
}

#[test]
fn test_spl_autoload_compiles_as_noop() {
    let out = compile_and_run(
        r#"<?php
spl_autoload("Bar");
echo "after";
"#,
    );
    assert_eq!(out, "after");
}

// --- closure-aware spl_autoload_register ---

#[test]
fn test_register_with_concat_closure_loads_class() {
    // Direct concatenation pattern: __DIR__ . '/lib/' . $name . '.php'.
    // The interpreter resolves __DIR__ to the magic-constant-substituted
    // absolute path and the class is loaded.
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

#[test]
fn test_register_name_is_case_insensitive_before_name_resolver() {
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

#[test]
fn test_namespaced_local_spl_autoload_register_is_not_collected() {
    let out = compile_and_run(
        r#"<?php
namespace App;
function spl_autoload_register($loader) { echo "local"; }
spl_autoload_register(function ($name) {});
"#,
    );
    assert_eq!(out, "local");
}

#[test]
fn test_unregister_name_is_case_insensitive_before_name_resolver() {
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

#[test]
fn test_register_with_str_replace_closure() {
    // PSR-0-style autoloader: backslash → underscore translation.
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

#[test]
fn test_register_with_intermediate_variable() {
    // The closure binds $path as a local before requiring it. The
    // interpreter must thread variable assignments correctly.
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

#[test]
fn test_register_with_file_exists_positive_branch() {
    // The closure guards the include with file_exists($path). The file is
    // present, so the interpreter takes the then-branch and loads.
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

#[test]
fn test_register_file_exists_directory_guard_loads_file() {
    // PHP's file_exists() returns true for directories. Autoload rules
    // often guard on the base directory before requiring the class file.
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

#[test]
fn test_register_is_readable_directory_guard_loads_file() {
    // is_readable() checks read access, not whether the path is a regular
    // file. A readable autoload base directory must pass the guard.
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

#[test]
fn test_register_chain_first_misses_second_loads() {
    // Two registered closures. The first looks in lib/missing/ (where the
    // file isn't); the second looks in lib/ where it is. The first rule's
    // file_exists guard returns false, so we fall through to the second.
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

#[test]
fn test_register_unregister_round_trip() {
    // Register a closure, then unregister it. The class must still load
    // because of the second (still-registered) closure.
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

#[test]
fn test_register_with_use_capture_falls_back_to_psr4() {
    // The closure uses a captured variable via `use(...)` — the collector
    // rejects it, so PSR-4 from composer.json takes over.
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

#[test]
fn test_spl_autoload_extensions_round_trip() {
    // Read default, write new value (returns old), read again returns
    // the new value.
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

#[test]
fn test_spl_autoload_extensions_null_arg_is_readonly() {
    // Passing null is the explicit read-only call shape; the global is
    // unchanged afterward.
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

#[test]
fn test_spl_autoload_functions_size_reflects_register_count() {
    // Two register sites at the top level → spl_autoload_functions()
    // has size 2.
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

#[test]
fn test_spl_autoload_functions_iterable() {
    // Foreach over the introspection array yields one entry per rule.
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

#[test]
fn test_autoload_files_section_always_inlines() {
    // Files listed under autoload.files must be inlined unconditionally.
    // The helper below isn't referenced as a class — it's just a
    // function that main.php expects to be in scope.
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

#[test]
fn test_autoload_files_section_executes_before_main_in_composer_order() {
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

#[test]
fn test_class_triggered_autoload_executes_before_first_use() {
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

#[test]
fn test_autoload_classmap_explicit_file() {
    // classmap entry pointing directly at a .php file: the compiler
    // scans it for class declarations and indexes them under their FQN.
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

#[test]
fn test_autoload_classmap_directory_scan() {
    // classmap entry pointing at a directory: the compiler walks it
    // recursively and indexes every class declaration under the right
    // FQN, even when the file path doesn't follow PSR-4.
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

#[test]
fn test_autoload_dev_psr4_section() {
    // Classes declared under autoload-dev are merged into the same
    // index. In an AOT model there is no production/test split — both
    // sections contribute to one binary.
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

#[test]
fn test_psr0_namespaced_prefix() {
    // Legacy PSR-0 with a namespaced prefix. Behaves like PSR-4 for
    // namespaced classes.
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

#[test]
fn test_psr0_underscore_class_convention() {
    // Twig_Loader_Filesystem-style class names: prefix `Twig_` maps to
    // `lib/`, and the underscore-separated class lives at the path
    // formed by treating each underscore as a directory separator.
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

#[test]
fn test_psr4_longest_prefix_wins() {
    // Two PSR-4 prefixes that both could resolve App\Models\User; the
    // longer one (App\Models\) must win, picking models/User.php instead
    // of src/Models/User.php.
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

#[test]
fn test_class_exists_literal_triggers_autoload() {
    // class_exists with a literal class name and the default
    // (autoload = true) loads the class even if the rest of the program
    // doesn't reference it directly.
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

#[test]
fn test_class_exists_with_explicit_true_triggers_autoload() {
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

#[test]
fn test_class_exists_with_int_nonzero_triggers_autoload() {
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

#[test]
fn test_class_exists_dynamic_autoload_arg_does_not_trigger_aot_autoload() {
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

#[test]
fn test_interface_exists_literal_triggers_autoload() {
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

#[test]
fn test_class_like_exists_literals_are_case_insensitive() {
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

#[test]
fn test_trait_exists_reports_declared_traits() {
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

#[test]
fn test_register_with_variable_stored_closure() {
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

#[test]
fn test_register_with_function_name_string() {
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

#[test]
fn test_register_inside_if_true_block() {
    // if (true) { register(...); } — folds to true so the inner
    // register is collected like a top-level call.
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

#[test]
fn test_register_inside_if_false_else_block() {
    // if (false) { ... } else { register(...); } — the else branch is
    // taken at compile time.
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

#[test]
fn test_register_with_sprintf_in_closure() {
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

#[test]
fn test_register_with_dirname_in_closure() {
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

#[test]
fn test_spl_object_id_unique_and_stable() {
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

#[test]
fn test_spl_object_hash_distinct() {
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

#[test]
fn test_spl_classes_returns_known_set() {
    let out = compile_and_run(
        r#"<?php
$names = spl_classes();
$found_exception = false;
$found_logic = false;
foreach ($names as $n) {
    if ($n === "Exception") $found_exception = true;
    if ($n === "LogicException") $found_logic = true;
}
echo $found_exception ? "e" : "-";
echo $found_logic ? "l" : "-";
"#,
    );
    assert_eq!(out, "el");
}

#[test]
fn test_get_class_returns_static_type() {
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

#[test]
fn test_is_a_walks_parent_chain() {
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

#[test]
fn test_is_a_target_string_is_case_insensitive() {
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

#[test]
fn test_is_a_parent_chain_normalizes_namespaced_parent() {
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

#[test]
fn test_is_a_walks_implemented_interface() {
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

#[test]
fn test_register_with_use_capture_warns() {
    // Captures aren't supported; the call compiles as a no-op and
    // composer.json PSR-4 takes over. The fixture exists already (in
    // test_register_with_use_capture_falls_back_to_psr4); this one
    // narrows the contract: after rejection, count(spl_autoload_functions)
    // is 0 because no rule got registered.
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

#[test]
fn test_get_declared_classes_includes_user_classes() {
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

#[test]
fn test_get_declared_classes_preserves_user_declaration_order() {
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

#[test]
fn test_get_declared_interfaces_includes_user_interfaces() {
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

#[test]
fn test_get_declared_interfaces_preserves_user_declaration_order() {
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

#[test]
fn test_get_declared_traits_preserves_user_declaration_order() {
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

#[test]
fn test_class_alias_creates_subclass() {
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

#[test]
fn test_class_alias_with_namespace() {
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

#[test]
fn test_class_alias_name_is_case_insensitive_before_name_resolver() {
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

#[test]
fn test_psr4_empty_prefix_root_namespace() {
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

#[test]
fn test_autoload_does_not_shadow_builtin_exception() {
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

#[test]
fn test_classmap_exclude_from_classmap() {
    // A classmap directory contains both regular code and a tests/
    // subdirectory excluded via exclude-from-classmap. The test class
    // must NOT end up in the index, while the regular class does.
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

#[test]
fn test_classmap_exclude_with_double_star() {
    // **/internal/** matches the internal/ subtree at any depth. Real
    // sits at lib/Real.php and is kept; Hidden sits at lib/sub/internal/
    // Hidden.php and is dropped.
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

#[test]
fn test_classmap_exclude_with_filename_glob() {
    // The pattern lib/*.test.php matches test files in lib/ but not in
    // subdirectories. The .test.php file is excluded, the regular file
    // is kept.
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

#[test]
fn test_spl_autoload_call_with_literal_loads_class() {
    // spl_autoload_call("App\\Forced") with a literal class string forces
    // the autoload pass to resolve App\Forced even though the rest of the
    // program doesn't reference it.
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
