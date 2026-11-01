//! Purpose:
//! End-to-end codegen tests for SPL helpers and AOT autoload behavior.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise manifest autoload sections and compile-time SPL rule extraction.

use crate::support::*;

/// Verifies PSR-4 single namespace autoload.
#[test]
fn test_psr4_single_namespace_autoload() {
    // PSR-4 single namespace maps "App\" → "src/" and class is autoloaded.
    let out = compile_and_run_files(
        &[
            (
                "module.json",
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

/// Verifies a definite class-string fallback seeds PSR-4 for a later dynamic `new` target.
#[test]
fn test_psr4_dynamic_new_from_literal_array_default() {
    let out = compile_and_run_files(
        &[
            (
                "public/composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "public/src/Runtime.php",
                "<?php\nnamespace App;\nclass Runtime { public function run(): string { return \"dynamic\"; } }\n",
            ),
            ("public/autoload.php", "<?php\nreturn 1;\n"),
            (
                "public/bootstrap.php",
                "<?php\nif ((require_once __DIR__ . '/autoload.php') !== 1) {\n    return;\n}\n$options = [];\n$options['runtime'] ??= 'App\\\\Runtime';\n$runtime = new $options['runtime']();\necho $runtime->run();\n",
            ),
            (
                "public/main.php",
                "<?php\nrequire_once dirname(__DIR__) . '/public/bootstrap.php';\n",
            ),
        ],
        "public/main.php",
    );
    assert_eq!(out, "dynamic");
}

/// Verifies a guarded dynamic factory evaluates its inherited class-string method without loading
/// unrelated implementations that merely share the factory return contract.
#[test]
fn test_psr4_guarded_dynamic_factory_evaluates_concrete_class_string_only() {
    let out = compile_and_run_files(
        &[
            (
                "module.json",
                r#"{"autoload":{"psr-4":{"Demo\\":"src/"}}}"#,
            ),
            (
                "src/Base.php",
                r#"<?php
namespace Demo;
interface Contract { public function value(): string; }
abstract class Base {
    public function create(): ?Contract {
        return class_exists($class = $this->extensionClass()) ? new $class() : null;
    }
    protected function extensionClass(): string {
        $name = static::class;
        $end = strrpos($name, '\\');
        $namespace = substr($name, 0, $end + 1);
        $short = substr($name, $end + 1);
        return $namespace . 'DependencyInjection\\' . preg_replace('/Bundle$/', '', $short) . 'Extension';
    }
}
"#,
            ),
            (
                "src/AppBundle.php",
                "<?php\nnamespace Demo;\nclass AppBundle extends Base {}\n",
            ),
            (
                "src/DependencyInjection/AppExtension.php",
                "<?php\nnamespace Demo\\DependencyInjection;\nclass AppExtension implements \\Demo\\Contract { public function value(): string { return 'ok'; } }\n",
            ),
            (
                "src/UnusedExtension.php",
                "<?php\nnamespace Demo;\necho 'unexpected';\nclass UnusedExtension implements Contract { public function value(): string { return 'wrong'; } }\n",
            ),
            (
                "main.php",
                "<?php\n$bundle = new Demo\\AppBundle();\necho $bundle->create()->value();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies a directly constructed factory can discover literal class strings stored in a
/// constructor property map before a later `new $class` resolves one map entry at runtime.
#[test]
fn test_psr4_dynamic_new_from_constructor_class_string_map() {
    let out = compile_and_run_files(
        &[
            (
                "module.json",
                r#"{"autoload":{"psr-4":{"Demo\\":"src/"}}}"#,
            ),
            (
                "src/Factory.php",
                r#"<?php
namespace Demo;
class Factory {
    private array $classes;
    public function __construct() {
        $this->classes = ['worker' => Worker::class];
    }
    public function run(): string {
        $class = $this->classes['worker'];
        return (new $class())->value();
    }
}
"#,
            ),
            (
                "src/Worker.php",
                "<?php\nnamespace Demo;\nclass Worker { public function value(): string { return 'mapped'; } }\n",
            ),
            (
                "main.php",
                "<?php\necho (new Demo\\Factory())->run();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "mapped");
}

/// Verifies an assignment-expression class default seeds a later dynamic static call.
#[test]
fn test_psr4_dynamic_static_call_from_assignment_expression_default() {
    let out = compile_and_run_files(
        &[
            (
                "module.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Handler.php",
                "<?php\nnamespace App;\nclass Handler { public static function register(bool $enabled): void { echo $enabled ? 'ok' : 'bad'; } }\n",
            ),
            (
                "main.php",
                "<?php\n$options = [];\nif (false !== $handler = ($options['handler'] ?? App\\Handler::class)) {\n    $handler::register(true);\n}\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies a class-string default assigned by a subclass reaches a dynamic static call in its
/// parent even when autoload dependency insertion places the parent declaration first.
#[test]
fn test_psr4_dynamic_static_call_from_subclass_array_default_reaches_parent() {
    let out = compile_and_run_files(
        &[
            (
                "module.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Handler.php",
                "<?php\nnamespace App;\nclass Handler { public static function register(bool $enabled): void { echo $enabled ? 'ok' : 'bad'; } }\n",
            ),
            (
                "src/BaseRuntime.php",
                "<?php\nnamespace App;\nclass BaseRuntime { public function __construct(array $options) { $handler = $options['handler']; $handler::register(true); } }\n",
            ),
            (
                "src/Runtime.php",
                "<?php\nnamespace App;\nclass Runtime extends BaseRuntime { public function __construct(array $options = []) { $options['handler'] ??= Handler::class; parent::__construct($options); } }\n",
            ),
            ("main.php", "<?php\nnew App\\Runtime();\n"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies a front controller below the manifest root discovers the parent autoload index and
/// uses a class-string default from an included runtime bootstrap to seed dynamic `new`.
#[test]
fn test_psr4_dynamic_runtime_from_parent_manifest_root() {
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Runtime.php",
                "<?php\nnamespace App;\nclass Runtime { public function run(): string { return \"parent-root\"; } }\n",
            ),
            (
                "vendor/autoload_runtime.php",
                "<?php\n$_SERVER = [];\n$_ENV = [];\n$_SERVER['APP_RUNTIME'] ??= $_ENV['APP_RUNTIME'] ?? 'App\\\\Runtime';\n$runtime = new $_SERVER['APP_RUNTIME']();\necho $runtime->run();\n",
            ),
            (
                "public/index.php",
                "<?php\nrequire_once dirname(__DIR__) . '/vendor/autoload_runtime.php';\n",
            ),
        ],
        "public/index.php",
    );
    assert_eq!(out, "parent-root");
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

/// Verifies a class a just-autoloaded file declared through `class_alias()` is not autoloaded
/// again from its own PSR-4 file in the same walker pass.
///
/// This is `nikic/php-parser`'s deprecation shape: `Int_.php` ends with
/// `class_alias(Int_::class, LNumber::class)`, and `LNumber.php` exists only for classmap tools —
/// it `require`s `Int_.php` (plainly, not `_once`) and declares `LNumber` under `if (false)`.
/// php never loads `LNumber.php` once `Int_` is loaded, because the alias already answers.
/// The walker compared each reference against the set of classes declared BEFORE the pass, so it
/// loaded `LNumber.php` too, its `require` spliced `Int_.php` a second time, and the build failed
/// with "Duplicate class declaration: P\Item" — which is what refused Laravel's closed world.
///
/// Reference value captured from php 8.5.10.
#[test]
fn test_psr4_alias_declared_by_an_autoloaded_file_is_not_autoloaded_again() {
    let out = compile_and_run_files(
        &[
            ("composer.json", r#"{"autoload":{"psr-4":{"P\\":"src/"}}}"#),
            (
                "src/Item.php",
                "<?php\nnamespace P;\nclass Item {\n    public function hi(): string { return \"item\"; }\n}\nclass_alias(Item::class, Old::class);\n",
            ),
            (
                "src/Old.php",
                "<?php\nnamespace P;\nrequire __DIR__ . '/Item.php';\nif (false) {\n    class Old extends Item {}\n}\n",
            ),
            (
                "main.php",
                "<?php\n$item = new \\P\\Item();\necho $item->hi(), \"|\";\necho class_exists(\\P\\Old::class) ? \"alias\" : \"missing\";\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "item|alias");
}

/// Verifies a class file that `require`s the file declaring its own dependency does not load
/// that file a second time as a dependency of its bundle.
///
/// The same `nikic/php-parser` shape reached the other way round: here the ALIAS is referenced
/// first, so `Sub/Old.php` loads, its `require __DIR__.'/../Item.php'` splices `Item` in, and its
/// `if (false) { class Old extends Item {} }` names `Item` as a dependency. The bundle walked that
/// dependency against the classes declared before the file, loaded `Item.php` again, and the build
/// failed "Duplicate class declaration: P\Item" (`Stmt/UseUse.php` and `UseItem.php` in Laravel's
/// closed world).
///
/// Reference value captured from php 8.5.10.
#[test]
fn test_psr4_dependency_a_file_requires_itself_is_not_autoloaded_again() {
    let out = compile_and_run_files(
        &[
            ("composer.json", r#"{"autoload":{"psr-4":{"P\\":"src/"}}}"#),
            (
                "src/Item.php",
                "<?php\nnamespace P;\nclass Base {}\nclass Item extends Base {\n    public function hi(): string { return \"item\"; }\n}\nclass_alias(Item::class, Sub\\Old::class);\n",
            ),
            (
                "src/Sub/Old.php",
                "<?php\nnamespace P\\Sub;\nuse P\\Item;\nrequire __DIR__ . '/../Item.php';\nif (false) {\n    class Old extends Item {}\n}\n",
            ),
            (
                "main.php",
                "<?php\necho class_exists(\\P\\Sub\\Old::class) ? \"alias\" : \"missing\", \"|\";\n$item = new \\P\\Item();\necho $item->hi();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "alias|item");
}

/// Verifies an autoloaded trait retains its own class imports when composed into a class.
#[test]
fn test_psr4_autoloaded_trait_method_resolves_imported_constructor() {
    let out = compile_and_run_files(
        &[
            (
                "module.json",
                r#"{"autoload":{"psr-4":{"Demo\\":"src/"}}}"#,
            ),
            (
                "src/Product.php",
                "<?php\nnamespace Demo;\nclass Product { public function name(): string { return 'ok'; } }\n",
            ),
            (
                "src/Traits/FactoryTrait.php",
                "<?php\nnamespace Demo\\Traits;\nuse Demo\\Product as ImportedProduct;\ntrait FactoryTrait {\n    public function make(): iterable { yield new ImportedProduct(); }\n    public function makeFrom(): iterable { yield from [new ImportedProduct()]; }\n}\n",
            ),
            (
                "src/Factory.php",
                "<?php\nnamespace Demo;\nuse Demo\\Traits\\FactoryTrait;\nclass Factory { use FactoryTrait; }\n",
            ),
            (
                "main.php",
                "<?php\n$factory = new Demo\\Factory();\nforeach ($factory->make() as $product) { echo $product->name(); }\nforeach ($factory->makeFrom() as $product) { echo $product->name(); }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "okok");
}

/// Verifies transitive class discovery reaches a fixpoint beyond an arbitrary depth cap.
#[test]
fn test_autoload_transitive_fixpoint_has_no_depth_cap() {
    const DEPTH: usize = 70;
    let mut owned_files = vec![(
        "composer.json".to_string(),
        r#"{"autoload":{"psr-4":{"Chain\\":"src/"}}}"#.to_string(),
    )];
    for index in 0..DEPTH {
        let next = if index + 1 == DEPTH {
            "public function value(): string { return 'done'; }".to_string()
        } else {
            format!(
                "public function value(): string {{ return (new Node{}())->value(); }}",
                index + 1
            )
        };
        owned_files.push((
            format!("src/Node{index}.php"),
            format!("<?php\nnamespace Chain;\nclass Node{index} {{ {next} }}\n"),
        ));
    }
    owned_files.push((
        "main.php".to_string(),
        "<?php\necho (new Chain\\Node0())->value();\n".to_string(),
    ));
    let files = owned_files
        .iter()
        .map(|(path, source)| (path.as_str(), source.as_str()))
        .collect::<Vec<_>>();

    let out = compile_and_run_files(&files, "main.php");
    assert_eq!(out, "done");
}

/// Verifies dormant object annotations do not demand or execute an autoloaded source.
#[test]
fn test_object_type_declarations_do_not_trigger_autoload() {
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"Optional\\":"src/"}}}"#,
            ),
            (
                "src/Dependency.php",
                "<?php\nnamespace Optional;\necho 'loaded';\nclass Dependency {}\n",
            ),
            (
                "main.php",
                "<?php\nfunction accept(Optional\\Dependency $value): Optional\\Dependency { return $value; }\nclass Holder { public Optional\\Dependency $value; }\necho 'done';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "done");
}

/// Verifies an optional class named only by a dormant closure remains a runtime concern even
/// when the closure body accesses one of its properties.
#[test]
fn test_dormant_closure_property_access_on_absent_declared_class_is_gradual() {
    let out = compile_and_run(
        r#"<?php
function register_optional_callback(): void {
    $callback = static function (UnavailableExtension $extension): void {
        echo $extension->option;
    };
}
echo 'ok';
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies a constructed class can retain a dormant closure whose absent nominal parameter is
/// accessed, without requiring a static property layout for that optional class.
#[test]
fn test_constructed_class_dormant_closure_property_access_on_absent_class_is_gradual() {
    let out = compile_and_run(
        r#"<?php
class Registrar {
    public function register(): void {
        $callback = static function (UnavailableExtension $extension): void {
            echo $extension->option;
        };
    }
}
new Registrar();
echo 'ok';
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies an autoload candidate with an unavailable direct parent stays absent instead of
/// poisoning the closed-world class schema when the requested class is only probed.
#[test]
fn test_autoload_candidate_with_missing_parent_remains_absent() {
    let out = compile_and_run_files(
        &[
            (
                "module.json",
                r#"{"autoload":{"psr-4":{"Optional\\":"src/"}}}"#,
            ),
            (
                "src/Feature.php",
                "<?php\nnamespace Optional;\nclass Feature extends MissingParent {}\n",
            ),
            (
                "main.php",
                "<?php\necho class_exists(Optional\\Feature::class) ? 'loaded' : 'absent';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "absent");
}

/// Verifies a candidate extending a supported built-in class is retained and loaded normally.
#[test]
fn test_autoload_candidate_with_builtin_parent_loads() {
    let out = compile_and_run_files(
        &[
            (
                "module.json",
                r#"{"autoload":{"psr-4":{"Local\\":"src/"}}}"#,
            ),
            (
                "src/File.php",
                "<?php\nnamespace Local;\nclass File extends \\SplFileInfo {}\n",
            ),
            (
                "main.php",
                "<?php\n$file = new Local\\File(__FILE__);\necho 'loaded';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded");
}

/// Verifies a missing-parent candidate referenced only from a dormant function body remains a
/// runtime concern and does not make an otherwise reachable program fail schema construction.
#[test]
fn test_dormant_function_can_reference_unbindable_autoload_candidate() {
    let out = compile_and_run_files(
        &[
            (
                "module.json",
                r#"{"autoload":{"psr-4":{"Optional\\":"src/"}}}"#,
            ),
            (
                "src/Feature.php",
                "<?php\nnamespace Optional;\nclass Feature extends MissingParent {}\n",
            ),
            (
                "main.php",
                "<?php\nfunction dormant() { return new Optional\\Feature(); }\necho 'ok';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies that incremental-hash usage found only after PSR-4 expansion receives the late hash
/// prelude and that bare namespaced `hash_*` calls fall back to its global declarations.
#[test]
fn test_psr4_autoloaded_hash_context_usage_injects_late_prelude() {
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Digester.php",
                "<?php\nnamespace App;\nclass Digester {\n    public static function md5(string $value): string {\n        $context = hash_init('md5');\n        hash_update($context, $value);\n        return hash_final($context);\n    }\n}\n",
            ),
            (
                "main.php",
                "<?php\necho App\\Digester::md5('abc');\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "900150983cd24fb0d6963f7d28e17f72");
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

/// Verifies a project without an autoload manifest compiles normally.
#[test]
fn test_no_autoload_manifest_compiles_normally() {
    // A manifest-free program must still compile; the autoload index is empty and the class loads via include path.
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

/// Verifies SPL autoload unregister answers like php: true only for a registered loader.
#[test]
fn test_spl_autoload_unregister_returns_true() {
    // php 8.5: true when the loader was registered, false once it is gone, false for a loader
    // that was never registered (the old expectation of `true` there was a stub's answer).
    let out = compile_and_run(
        r#"<?php
$loader = function($name) {};
spl_autoload_register($loader);
$ok = spl_autoload_unregister($loader);
$again = spl_autoload_unregister($loader);
$never = spl_autoload_unregister(function($name) {});
echo $ok ? "true" : "false", ",", $again ? "true" : "false", ",", $never ? "true" : "false";
"#,
    );
    assert_eq!(out, "true,false,false");
}

/// Verifies registering one array loader twice, from compiled code, keeps ONE registration.
///
/// PHP deduplicates by value. Each compiled `spl_autoload_register()` reaches the bridge with a
/// context of its own, so the check that only looked in that context never found the earlier
/// registration: the loader ran three times per miss and survived its own unregistration.
#[test]
fn test_spl_autoload_register_dedupes_an_array_loader_across_calls() {
    let out = compile_and_run(
        r#"<?php
class K { static function load($c) { echo "load $c\n"; } }
$a = ['K', 'load'];
spl_autoload_register($a);
spl_autoload_register($a);
spl_autoload_register(['K', 'load']);
var_dump(class_exists('Nope1'));
var_dump(spl_autoload_unregister($a));
var_dump(class_exists('Nope2'));
"#,
    );
    assert_eq!(out, "load Nope1\nbool(false)\nbool(true)\nbool(false)\n");
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

/// Verifies the canonical hand-written PSR-4 leaf expression resolves a namespaced class.
#[test]
fn test_register_with_substr_strrpos_leaf_closure() {
    // `substr($class, strrpos($class, '\\') + 1)` is the ordinary way to take a class's leaf
    // name; both builtins and the integer `+` between them must fold.
    let out = compile_and_run_files(
        &[
            (
                "src/Probe.php",
                "<?php\nnamespace App;\nclass Probe { public function run(): string { return \"reached\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function (string $class): void {
    $leaf = substr($class, strrpos($class, '\\') + 1);
    require __DIR__ . '/src/' . $leaf . '.php';
});
$p = new App\Probe();
echo $p->run();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "reached");
}

/// Verifies the leaf expression takes the LAST segment of a nested namespace.
#[test]
fn test_register_substr_strrpos_leaf_takes_last_namespace_segment() {
    // `App\Sub\Thing` has two separators; `strrpos` must find the rightmost one.
    let out = compile_and_run_files(
        &[
            (
                "src/Thing.php",
                "<?php\nnamespace App\\Sub;\nclass Thing { public function run(): string { return \"nested\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function (string $class): void {
    $leaf = substr($class, strrpos($class, '\\') + 1);
    require __DIR__ . '/src/' . $leaf . '.php';
});
$t = new App\Sub\Thing();
echo $t->run();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "nested");
}

/// Verifies a global-namespace class reproduces PHP's `false + 1`, not a "sensible" 0.
#[test]
fn test_register_substr_strrpos_global_class_drops_first_character_like_php() {
    // php 8.5.10:
    //   php -r '$c="Gadget"; var_dump(strrpos($c,"\\")+1, substr($c, strrpos($c,"\\")+1));'
    //   int(1)  string(5) "adget"
    // `strrpos` misses, `false + 1` is `1`, and the loader therefore looks for the class
    // MINUS its first character. The fixture file is named accordingly: making the
    // compiler "fix" this would silently resolve a global class to the wrong file.
    let out = compile_and_run_files(
        &[
            (
                "src/adget.php",
                "<?php\nclass Gadget { public function run(): string { return \"chopped\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function (string $class): void {
    $leaf = substr($class, strrpos($class, '\\') + 1);
    require __DIR__ . '/src/' . $leaf . '.php';
});
$g = new Gadget();
echo $g->run();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "chopped");
}

/// Verifies `strrpos` yields `false` (not `0`) when the needle is absent.
#[test]
fn test_register_substr_strrpos_miss_is_false_not_zero() {
    // The defensive form of the same loader branches on `$pos === false`. If a miss folded
    // to `0` instead, the global-namespace class would take the `else` branch and look for
    // `src/olo.php`; the class would not be found and this test would fail.
    let out = compile_and_run_files(
        &[
            (
                "src/Solo.php",
                "<?php\nclass Solo { public function run(): string { return \"solo\"; } }\n",
            ),
            (
                "src/Leaf.php",
                "<?php\nnamespace App\\Nested;\nclass Leaf { public function run(): string { return \"leaf\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function (string $class): void {
    $pos = strrpos($class, '\\');
    if ($pos === false) {
        require __DIR__ . '/src/' . $class . '.php';
    } else {
        require __DIR__ . '/src/' . substr($class, $pos + 1) . '.php';
    }
});
$s = new Solo();
echo $s->run();
$l = new App\Nested\Leaf();
echo $l->run();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "sololeaf");
}

/// Verifies the three-argument `substr` form, including an explicit and a negative length.
#[test]
fn test_register_substr_with_explicit_and_negative_length() {
    // `substr($class, 0, $pos)` takes the namespace prefix and `substr($leaf, 0, -6)` strips
    // a fixed suffix — both are ordinary PHP and exercise `substr`'s length clamping.
    let out = compile_and_run_files(
        &[
            (
                "src/App/Sub/Thing.php",
                "<?php\nnamespace App\\Sub;\nclass ThingBundle { public function run(): string { return \"sliced\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function (string $class): void {
    $pos = strrpos($class, '\\');
    $prefix = substr($class, 0, $pos);
    $leaf = substr($class, $pos + 1);
    $stem = substr($leaf, 0, -6);
    require __DIR__ . '/src/' . str_replace('\\', '/', $prefix) . '/' . $stem . '.php';
});
$t = new App\Sub\ThingBundle();
echo $t->run();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "sliced");
}

/// Verifies a body outside the supported subset is still rejected and falls through.
#[test]
fn test_register_expression_outside_subset_is_rejected_and_falls_through() {
    // The first rule computes its leaf with a ternary and `strrev` — neither is in the
    // subset, so the rule must yield nothing rather than being evaluated approximately.
    // Resolution falls through to the second, supported rule. `src/eborP.php` exists but
    // declares something else, so if the first rule ever started folding it would be
    // selected, `App\Probe` would stay undeclared, and this test would fail loudly.
    // php agrees on the output: it requires the decoy, gets no `App\Probe` from it, and
    // goes on to the second loader.
    let out = compile_and_run_files(
        &[
            (
                "src/Probe.php",
                "<?php\nnamespace App;\nclass Probe { public function run(): string { return \"fellthrough\"; } }\n",
            ),
            (
                "src/eborP.php",
                "<?php\nnamespace App;\nclass Decoy { public function run(): string { return \"wrong\"; } }\n",
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function (string $class): void {
    $pos = strrpos($class, '\\');
    $leaf = $pos === false ? $class : substr($class, $pos + 1);
    require __DIR__ . '/src/' . strrev($leaf) . '.php';
});
spl_autoload_register(function (string $class): void {
    $leaf = substr($class, strrpos($class, '\\') + 1);
    require __DIR__ . '/src/' . $leaf . '.php';
});
$p = new App\Probe();
echo $p->run();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "fellthrough");
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
    // Closure capture is rejected by the collector; the manifest PSR-4 mapping takes over.
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

// --- supported manifest autoload sections ---

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

/// Verifies the eager-file section executes before main in declaration order.
#[test]
fn test_autoload_files_section_executes_before_main_in_declaration_order() {
    // Eager-file entries execute in manifest order before main.php.
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

/// Verifies a referenced attribute class is available to reflection-backed instantiation.
#[test]
fn test_attribute_class_triggers_autoload() {
    let out = compile_and_run_files(
        &[
            (
                "module.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Marker.php",
                "<?php\nnamespace App;\n#[\\Attribute]\nclass Marker { public function __construct(public string $label) {} }\n",
            ),
            (
                "main.php",
                r#"<?php
namespace App;
#[Marker("loaded")]
class Target {}
$attribute = (new \ReflectionClass(Target::class))->getAttributes()[0]->newInstance();
echo $attribute->label;
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded");
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
    // class_exists with a variable second arg must not be guessed as an AOT
    // autoload demand.
    let out = compile_and_run_files(
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
                r#"<?php
$autoload = true;
echo class_exists("App\\DynamicFlag", $autoload) ? "exists" : "missing";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "missing");
}

/// Verifies a literal eval result preserves an indexed class-string for autoload discovery.
///
/// The class name reaches `class_exists()` through `$classes[0]`, so the autoload walk must
/// preserve the literal eval return shape rather than treating the dynamic lookup as opaque.
#[test]
fn test_class_exists_autoloads_literal_eval_class_array_element() {
    let out = compile_and_run_files(
        &[
            (
                "library/Deferred.php",
                "<?php\nnamespace Fixture;\nclass Deferred {}\n",
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function (string $class): void {
    if ($class === 'Fixture\\Deferred') {
        require_once __DIR__ . '/library/Deferred.php';
    }
});

$source = 'return ["Fixture\\Deferred"];';
$classes = eval($source);
echo class_exists($classes[0]) ? 'exists' : 'missing';
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "exists");
}

/// Verifies a literal class probe after eval invokes registered autoload callbacks.
///
/// The class name is intentionally absent from the AOT declaration set: the callback creates it
/// through `eval`, so this exercises the post-eval `class_exists()` bridge rather than static
/// project discovery or any framework-specific loader convention.
#[test]
fn test_class_exists_after_eval_triggers_registered_autoload() {
    let out = compile_and_run(
        r#"<?php
class DeferredProbeLoader {
    public function load(string $class): void {
        if ($class === 'Fixture\AbsentProbe') {
            echo 'called';
        }
    }
}

function classExistsFromAotFunction(bool $autoload): bool {
    return class_exists('Fixture\\AbsentProbe', $autoload);
}

$loader = new DeferredProbeLoader();
spl_autoload_register([$loader, 'load']);
eval('$barrier = true;');
echo classExistsFromAotFunction(false) ? 'unexpected' : 'missing';
echo classExistsFromAotFunction(true) ? 'loaded' : 'missing';
"#,
    );
    assert_eq!(out, "missingcalledmissing");
}

/// Verifies nominal construction invokes registered autoload after an eval barrier.
///
/// The source names the class directly, while the callback creates its declaration only at
/// runtime.  This prevents the test from relying on a project layout or a framework-specific
/// autoloader and exercises the bridge's normal `new ClassName` resolution semantics.
#[test]
fn test_nominal_new_after_eval_triggers_registered_autoload() {
    let out = compile_and_run_files(
        &[
            (
                "library/RuntimeConstructed.php",
                "<?php\nclass RuntimeConstructed { public function label(): void { echo 'loaded'; } }\n",
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function (string $class): void {
    if ($class === 'RuntimeConstructed') {
        require_once __DIR__ . '/library/RuntimeConstructed.php';
    }
});

eval('$barrier = true;');
$instance = new RuntimeConstructed();
$instance->label();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded");
}

/// Verifies autoloaded nominal construction materializes a separately loaded trait.
///
/// The class body is intentionally unavailable to AOT lowering.  Its trait is resolved only
/// while the runtime include executes, matching ordinary library autoload without assuming a
/// loader implementation or a framework directory layout.
#[test]
fn test_nominal_new_after_eval_autoloads_class_trait_dependencies() {
    let out = compile_and_run_files(
        &[
            (
                "library/RuntimeConstructedContract.php",
                "<?php\ninterface RuntimeConstructedContract {}\n",
            ),
            (
                "library/RuntimeConstructionTrait.php",
                "<?php\ntrait RuntimeConstructionTrait { public function label(): void { echo 'trait-loaded'; } }\n",
            ),
            (
                "library/RuntimeConstructed.php",
                "<?php\nfinal class RuntimeConstructed implements RuntimeConstructedContract { use RuntimeConstructionTrait; }\n",
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function (string $class): void {
    if ($class === 'RuntimeConstructed') {
        require_once __DIR__ . '/library/RuntimeConstructed.php';
    }
    if ($class === 'RuntimeConstructedContract') {
        require_once __DIR__ . '/library/RuntimeConstructedContract.php';
    }
    if ($class === 'RuntimeConstructionTrait') {
        require_once __DIR__ . '/library/RuntimeConstructionTrait.php';
    }
});

eval('$barrier = true;');
$instance = new RuntimeConstructed();
$instance->label();
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "trait-loaded");
}

/// Verifies an AOT function resolves `new $class` through request-global autoload state.
///
/// The function itself contains no eval expression and therefore has no persistent eval local.
/// Its dynamic class probe and construction must still use the callback-owning context registered
/// by an earlier frame, as PHP's process-wide SPL table requires.
#[test]
fn test_aot_dynamic_new_after_eval_uses_global_autoload_context() {
    let out = compile_and_run_files(
        &[
            (
                "library/RuntimeConstructed.php",
                "<?php\nclass RuntimeConstructed implements RuntimeConstructedContract { use RuntimeConstructedTrait; }\n",
            ),
            (
                "library/RuntimeConstructedTrait.php",
                "<?php\ntrait RuntimeConstructedTrait { public function label(): void { echo 'global-loaded'; } }\n",
            ),
            (
                "main.php",
                r#"<?php
function createLoadedClass(string $class): mixed {
    return class_exists($class) ? new $class() : null;
}

interface RuntimeConstructedContract {
    public function label(): void;
}

function useLoadedClass(RuntimeConstructedContract $instance): void {
    echo 'global-loaded';
}

class RuntimeAutoloadLoader {
    public function load(string $class): void {
        if ($class === 'RuntimeConstructed') {
            $path = str_replace('/library/', '/library/', __DIR__ . '/library/' . $class . '.php');
            require_once $path;
        }
        if ($class === 'RuntimeConstructedTrait') {
            $path = str_replace('/library/', '/library/', __DIR__ . '/library/' . $class . '.php');
            require_once $path;
        }
    }
}

$loader = new RuntimeAutoloadLoader();
spl_autoload_register([$loader, 'load']);
eval('$barrier = true;');
$instance = createLoadedClass('RuntimeConstructed');
useLoadedClass($instance);
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "global-loaded");
}

/// Verifies a dynamic child can pass an inline native-parent result by value and a PHP local by reference.
///
/// The child is loaded only at runtime, so both calls cross the eval bridge. The first argument is
/// a one-shot EIR merge temporary and must not be represented as a writable local cell after its
/// source slot has been moved. The second remains a user variable and must preserve PHP by-reference
/// mutation through the same bridge.
#[test]
fn test_dynamic_child_bridge_preserves_inline_values_and_php_references() {
    let out = compile_and_run_files(
        &[
            (
                "library/NativeParent.php",
                r#"<?php
class NativeParent {
    public function all(): array {
        return ['answer' => 42];
    }

    public function resolveValue(mixed $value): mixed {
        return $value;
    }
}
"#,
            ),
            (
                "library/RuntimeChild.php",
                r#"<?php
class RuntimeChild extends NativeParent {
    public function overwrite(mixed &$value): void {
        $value = 'updated';
    }
}
"#,
            ),
            (
                "main.php",
                r#"<?php
require __DIR__ . '/library/NativeParent.php';

$path = str_replace('/library/', '/library/', __DIR__ . '/library/RuntimeChild.php');
require_once $path;
$class = 'RuntimeChild';
$child = new $class();

$resolved = $child->resolveValue($child->all());
echo $resolved['answer'] . ':';
$value = 'before';
$child->overwrite($value);
echo $value;
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "42:updated");
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
    // spl_classes() lists exactly the `ext/spl` class-likes, as PHP does: LogicException and
    // SplStack are SPL, while Exception and Error belong to Core and must NOT appear.
    let out = compile_and_run(
        r#"<?php
$names = spl_classes();
$found_exception = false;
$found_stack = false;
$found_logic = false;
foreach ($names as $n) {
    if ($n === "Exception") $found_exception = true;
    if ($n === "SplStack") $found_stack = true;
    if ($n === "LogicException") $found_logic = true;
}
echo $found_exception ? "e" : "-";
echo $found_stack ? "s" : "-";
echo $found_logic ? "l" : "-";
"#,
    );
    assert_eq!(out, "-sl");
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

/// Verifies an eval static call resolves an AOT method through a runtime class alias.
#[test]
fn test_eval_static_method_call_through_class_alias() {
    let out = compile_and_run(
        r#"<?php
class OriginalReference {
    public static function config(array $config): array { return $config; }
}
class_alias(OriginalReference::class, AliasReference::class);
echo eval('return AliasReference::config(["result" => "ok"])["result"];');
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies a runtime-included alias keeps an otherwise unreachable AOT static method callable.
#[test]
fn test_dynamic_include_static_method_call_through_class_alias() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
class OriginalReference {
    public static function config(array $config): array { return $config; }
}
function loadRuntimeFile(string $path): void { include $path; }
loadRuntimeFile("runtime.php");
"#,
            ),
            (
                "runtime.php",
                r#"<?php
class_alias(OriginalReference::class, AliasReference::class);
echo AliasReference::config(["result" => "ok"])["result"];
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies a runtime-included class keeps static methods visible through its class alias.
#[test]
fn test_dynamic_include_declared_class_static_method_call_through_class_alias() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function loadRuntimeFile(string $path): void { include $path; }
loadRuntimeFile("runtime.php");
"#,
            ),
            (
                "runtime.php",
                r#"<?php
class OriginalReference {
    public static function config(array $config): array { return $config; }
}
class_alias(OriginalReference::class, AliasReference::class);
echo AliasReference::config(["result" => "ok"])["result"];
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies class_alias autoloads a PSR-4 target named only through `::class`.
#[test]
fn test_class_alias_autoloads_psr4_target_static_method() {
    let out = compile_and_run_files(
        &[
            (
                "module.json",
                r#"{"autoload":{"psr-4":{"Bridge\\Alias\\":"src/"}}}"#,
            ),
            (
                "main.php",
                r#"<?php
class_alias(\Bridge\Alias\OriginalReference::class, \Bridge\Alias\AliasReference::class);
echo \Bridge\Alias\AliasReference::config(["result" => "ok"])["result"];
"#,
            ),
            (
                "src/OriginalReference.php",
                r#"<?php
namespace Bridge\Alias;

class OriginalReference
{
    public static function config(array $config): array { return $config; }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies a conditional method-local class alias autoloads its PSR-4 target.
#[test]
fn test_method_local_class_alias_autoloads_psr4_target_static_method() {
    let out = compile_and_run_files(
        &[
            (
                "module.json",
                r#"{"autoload":{"psr-4":{"Bridge\\Alias\\":"src/"}}}"#,
            ),
            (
                "main.php",
                r#"<?php
use Bridge\Alias\OriginalReference;

class RuntimeLoader {
    public function load(): void {
        if (!class_exists(\Bridge\Alias\AliasReference::class)) {
            class_alias(OriginalReference::class, \Bridge\Alias\AliasReference::class);
        }
    }
}

(new RuntimeLoader())->load();
echo \Bridge\Alias\AliasReference::config(["result" => "ok"])["result"];
"#,
            ),
            (
                "src/OriginalReference.php",
                r#"<?php
namespace Bridge\Alias;

class OriginalReference
{
    public static function config(array $config): array { return $config; }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies ReflectionClass autoloads a PSR-4 class named only by a class constant.
#[test]
fn test_reflection_class_autoloads_psr4_class_constant_target() {
    let out = compile_cli_files_and_run(
        &[
            (
                "module.json",
                r#"{"autoload":{"psr-4":{"Bridge\\Reflect\\":"src/"}}}"#,
            ),
            (
                "main.php",
                r#"<?php
$reflection = new ReflectionClass(\Bridge\Reflect\OriginalReference::class);
echo $reflection->getMethod("config")->getName();
"#,
            ),
            (
                "src/OriginalReference.php",
                r#"<?php
namespace Bridge\Reflect;

/** Reflection metadata must retain static methods. */
class OriginalReference
{
    public static function config(array $config): array { return $config; }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "config");
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

/// Verifies resolved class constants create aliases from a nested method body.
#[test]
fn test_class_alias_from_resolved_class_constants_in_method() {
    let out = compile_and_run(
        r#"<?php
namespace Model;
class Original {
    public function tag(): string { return "resolved"; }
}
class Bootstrap {
    public static function register(): void {
        if (!class_exists(AliasName::class)) {
            class_alias(Original::class, AliasName::class);
        }
    }
}
Bootstrap::register();
$value = new AliasName();
echo $value->tag();
"#,
    );
    assert_eq!(out, "resolved");
}

/// Verifies `class_alias()` accepts a `Name::class` constant as the original class (issue #849).
///
/// `Original::class` is a compile-time constant string, so PHP accepts it wherever a literal
/// class name is accepted; it used to be refused as a "runtime" call shape.
#[test]
fn test_class_alias_accepts_class_name_constants() {
    let out = compile_and_run(
        r#"<?php
class Original {
    public function tag(): string { return "orig"; }
}
class_alias(Original::class, 'AliasName');
$a = new AliasName();
echo $a->tag(), ":", ($a instanceof Original) ? "yes" : "no", ":", ($a instanceof AliasName) ? "yes" : "no";
"#,
    );
    assert_eq!(out, "orig:yes:yes");
}

/// Verifies `Name::class` arguments of `class_alias()` resolve exactly as PHP compiles them:
/// through a `use` import, through a namespace alias prefix, against the current namespace, and
/// fully qualified, with a `__NAMESPACE__ .` concatenation for the alias name.
///
/// Also pins that a global alias name declared from a file using the statement form of
/// `namespace` is created in the global namespace (a string class name is always fully
/// qualified), where it used to be declared inside the surrounding namespace instead.
/// Expected output is PHP 8.5.10's.
#[test]
fn test_class_alias_class_constants_resolve_namespace_and_imports() {
    let out = compile_and_run(
        r#"<?php
namespace Lib\Log;
class Logger { public function name(): string { return "logger"; } }

namespace App;
use Lib\Log\Logger;
use Lib\Log as L;
class Local { public function name(): string { return "local"; } }
class_alias(Logger::class, 'GlobalLogger');
class_alias(L\Logger::class, 'App\ViaQualified');
class_alias(Local::class, __NAMESPACE__ . '\LocalAlias');
class_alias(\Lib\Log\Logger::class, \App\FullyAlias::class);
echo (new \GlobalLogger())->name(), ":";
echo (new ViaQualified())->name(), ":";
echo (new LocalAlias())->name(), ":";
echo (new FullyAlias())->name();
"#,
    );
    assert_eq!(out, "logger:logger:local:logger");
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

/// Verifies member probes resolve a class-string through a runtime autoloader.
///
/// An autoloader registered from inside a class method is not one of the shapes the compile-time
/// rule collector consumes, so it survives into the binary and the class exists only once the
/// chain has run. PHP resolves the class-string argument of `method_exists()` with
/// `zend_lookup_class`, which runs that chain, so the probe sees the class and its inherited
/// methods; a name nothing can load still answers `false`, and reflecting on it still throws.
/// Value-checked against `php -n`, which prints the same line.
#[test]
fn test_member_probes_autoload_a_runtime_registered_class_string() {
    let out = compile_and_run_files(
        &[
            (
                "loader.php",
                "<?php\nclass RuntimeLoader {\n    private string $dir;\n    public function __construct(string $dir) { $this->dir = $dir; }\n    public function loadClass(string $class): void {\n        $path = $this->dir . \"/\" . $class . \".php\";\n        if (file_exists($path)) {\n            require $path;\n        }\n    }\n    public static function register(string $dir): void {\n        $loader = new RuntimeLoader($dir);\n        spl_autoload_register([$loader, \"loadClass\"], true, true);\n    }\n}\n",
            ),
            (
                "lib/RuntimeBase.php",
                "<?php\nclass RuntimeBase {\n    public function BaseWork(): string { return \"base\"; }\n}\n",
            ),
            (
                "lib/RuntimeChild.php",
                "<?php\nclass RuntimeChild extends RuntimeBase {\n    public function ChildWork(): string { return \"child\"; }\n}\n",
            ),
            (
                "main.php",
                "<?php\nrequire __DIR__ . \"/loader.php\";\nRuntimeLoader::register(__DIR__ . \"/lib\");\n\n$cls = implode(\"\", [\"Runtime\", \"Child\"]);\necho (method_exists($cls, \"ChildWork\") ? \"C\" : \"c\");\necho (method_exists($cls, \"BaseWork\") ? \"B\" : \"b\");\necho (method_exists($cls, \"basework\") ? \"I\" : \"i\");\necho (method_exists($cls, \"missing\") ? \"M\" : \"m\");\necho \":\";\n$unknown = implode(\"\", [\"Runtime\", \"Absent\"]);\necho (method_exists($unknown, \"any\") ? \"U\" : \"u\");\necho \":\";\n$class = new ReflectionClass($cls);\necho $class->getName() . \":\";\necho ($class->hasMethod(\"basework\") ? \"H\" : \"h\") . \":\";\necho $class->getMethod(\"childwork\")->getName() . \":\";\ntry {\n    new ReflectionClass($unknown);\n    echo \"noexception\";\n} catch (ReflectionException $e) {\n    echo \"ReflectionException\";\n}\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "CBIm:u:RuntimeChild:H:ChildWork:ReflectionException");
}

/// Verifies an autoloaded class file whose FILE-SCOPE guard asks `interface_exists()` for the
/// interface it implements sees that interface declared.
///
/// `symfony/string`'s `AsciiSlugger.php` is written exactly this way, and the compiled Symfony
/// binary threw its "the symfony/translation-contracts package is not installed" LogicException
/// before the entry file's first statement ran. Two things were wrong. The autoload pass appended
/// each discovered file at its own first reference point, which is breadth-first, so the interface
/// landed AFTER the class file that demanded it; PHP's loader is depth-first and would have run it
/// first. And an autoloaded interface never emitted the activation event that `interface_exists()`
/// reads, so its overlay cell stayed zero no matter where the declaration sat.
///
/// Value-checked against `php -n`, which prints the same line.
#[test]
fn test_an_autoloaded_file_scope_guard_sees_the_interface_it_implements() {
    let out = compile_and_run_files(
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"Guarded\\":"src/"}}}"#,
            ),
            (
                "src/Marker.php",
                "<?php\nnamespace Guarded;\ninterface Marker { public function name(): string; }\n",
            ),
            (
                "src/Widget.php",
                "<?php\nnamespace Guarded;\n\nif (!interface_exists(Marker::class)) {\n    throw new \\LogicException('Marker is missing');\n}\n\nclass Widget implements Marker {\n    public function name(): string { return \"widget\"; }\n}\n",
            ),
            (
                "main.php",
                "<?php\n$w = new Guarded\\Widget();\necho $w->name(), \":\", (int) interface_exists('Guarded\\\\Marker');\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "widget:1");
}

/// Verifies an autoloaded class file that declares its class twice under
/// `if (method_exists(Base::class, 'm')) { ... } else { ... }` is compiled with the branch php
/// takes, the method being found on an ancestor interface or missing altogether.
///
/// Symfony's `TwigExtraBundle` asks `method_exists(KernelInterface::class, 'getShareDir')`; left
/// undecided, the file went to the runtime autoloader with a "declares it inside a run-time
/// condition" warning. Value-checked against `php -n`, which prints `share|old`.
#[test]
fn test_an_autoloaded_method_exists_guard_picks_the_declaration_php_takes() {
    let guarded = |class: &str, probe: &str| {
        format!(
            "<?php\nnamespace App;\n\nif (method_exists(Kernel::class, '{probe}')) {{\n    class {class} {{ public function v(): string {{ return 'share'; }} }}\n}} else {{\n    class {class} {{ public function v(): string {{ return 'old'; }} }}\n}}\n"
        )
    };
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[
            ("composer.json", r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#),
            (
                "src/BaseKernel.php",
                "<?php\nnamespace App;\ninterface BaseKernel { public function getShareDir(): string; }\n",
            ),
            ("src/Kernel.php", "<?php\nnamespace App;\ninterface Kernel extends BaseKernel {}\n"),
            ("src/Bundle.php", &guarded("Bundle", "getShareDir")),
            ("src/Legacy.php", &guarded("Legacy", "getNothing")),
            (
                "main.php",
                "<?php\necho (new App\\Bundle())->v(), '|', (new App\\Legacy())->v(), \"\\n\";\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "share|old\n");
    assert!(!stderr.contains("run-time condition"), "{stderr}");
}

/// Verifies Symfony's `OrderedHashMap` shape stays compiled and keeps php's references: the map
/// builds its iterator with `new It($this->elements, $this->keys, $this->cursors)` for
/// by-reference promoted parameters, and the iterator shares a typed `int` property with an
/// element (`$this->cursors[$id] = &$this->cursor`). Both files used to be handed to the runtime
/// autoloader ("a native call cannot keep linked"), which a closed world cannot do.
/// Value-checked against `php -n`.
#[test]
fn test_by_ref_promoted_constructor_over_properties_stays_compiled() {
    let iterator = "<?php\nnamespace Map;\nclass It implements \\Iterator {\n    private int $cursor = 0;\n    private int $id;\n    public function __construct(private array &$elements, private array &$keys, private array &$cursors) {\n        $this->id = \\count($cursors);\n        $this->cursors[$this->id] = &$this->cursor;\n    }\n    public function __destruct() { array_splice($this->cursors, $this->id, 1); }\n    public function current(): mixed { return $this->elements[$this->keys[$this->cursor]]; }\n    public function key(): mixed { return $this->keys[$this->cursor]; }\n    public function next(): void { ++$this->cursor; }\n    public function rewind(): void { $this->cursor = 0; }\n    public function valid(): bool { return isset($this->keys[$this->cursor]); }\n}\n";
    let map = "<?php\nnamespace Map;\nclass Ordered implements \\IteratorAggregate {\n    private array $elements = [];\n    private array $keys = [];\n    private array $cursors = [];\n    public function set(string $k, int $v): void { if (!isset($this->elements[$k])) { $this->keys[] = $k; } $this->elements[$k] = $v; }\n    public function remove(string $k): void {\n        $pos = array_search($k, $this->keys, true);\n        unset($this->elements[$k]);\n        array_splice($this->keys, $pos, 1);\n        foreach ($this->cursors as $i => $cursor) { if ($cursor >= $pos) { --$this->cursors[$i]; } }\n    }\n    public function getIterator(): \\Traversable { return new It($this->elements, $this->keys, $this->cursors); }\n}\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[
            ("composer.json", r#"{"autoload":{"psr-4":{"Map\\":"src/"}}}"#),
            ("src/It.php", iterator),
            ("src/Ordered.php", map),
            (
                "main.php",
                "<?php\n$m = new Map\\Ordered();\n$m->set('a', 1); $m->set('b', 2); $m->set('c', 3);\nforeach ($m as $k => $v) { echo \"$k=$v,\"; if ($k === 'a') { $m->set('d', 4); $m->remove('b'); } }\necho \"\\n\";\nforeach ($m as $k => $v) { foreach ($m as $k2 => $v2) { echo $k, $k2, ' '; } }\necho \"\\n\";\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "a=1,c=3,d=4,\naa ac ad ca cc cd da dc dd \n");
    assert!(!stderr.contains("is not compiled"), "{stderr}");
}

/// Verifies PDO reached only from an autoloaded class (Doctrine DBAL's `PDO::connect()` under
/// symfony/demo) gets the PDO classes: the early prelude gate sees the entry program alone, and
/// the static factory became a call to a class the closed world does not define.
/// Value-checked against `php`, which prints `Pdo\Sqlite|2`.
#[test]
fn test_pdo_named_only_by_an_autoloaded_class_is_declared() {
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[
            ("composer.json", r#"{"autoload":{"psr-4":{"Db\\":"src/"}}}"#),
            (
                "src/Driver.php",
                "<?php\nnamespace Db;\nfinal class Driver {\n    public static function connect(): \\PDO { return \\PDO::connect('sqlite::memory:'); }\n}\n",
            ),
            (
                "main.php",
                "<?php\n$pdo = Db\\Driver::connect();\necho get_class($pdo), '|', $pdo->query('select 1+1')->fetchColumn(), \"\\n\";\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "Pdo\\Sqlite|2\n", "{stderr}");
}

/// Verifies a PSR-4 autoloaded class keeps its PHP attributes and its file's `strict_types`.
///
/// The autoload pass rebuilt every statement of a loaded file and the rebuild dropped the
/// statement's metadata: a class's `#[...]` attributes live on its declaring statement, so
/// `ReflectionClass::getAttributes()` answered nothing for every Composer-loaded class (every
/// Symfony console command lost its `#[AsCommand]` name), and the file's `declare(strict_types=1)`
/// was replaced by the entry file's mode. Expected output from php 8.5.
#[test]
fn test_psr4_autoloaded_class_keeps_attributes_and_strict_types() {
    let out = compile_and_run_files(
        &[
            (
                "module.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/AsCommand.php",
                "<?php\nnamespace App;\n#[\\Attribute(\\Attribute::TARGET_CLASS)]\nclass AsCommand { public function __construct(public string $name = '') {} }\n",
            ),
            (
                "src/AboutCommand.php",
                "<?php\ndeclare(strict_types=1);\nnamespace App;\n/** Doc. */\n#[AsCommand(name: 'about')]\nclass AboutCommand {\n    public static function half(int $n): int { return intdiv($n, 2); }\n    public static function strict(): string { try { return (string) self::half(\"8\"); } catch (\\TypeError $e) { return 'TypeError'; } }\n}\n",
            ),
            (
                "main.php",
                "<?php\n$r = new ReflectionClass(App\\AboutCommand::class);\n$a = $r->getAttributes(App\\AsCommand::class);\necho count($a), ':', $a[0]->newInstance()->name, ':', App\\AboutCommand::strict();\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "1:about:TypeError");
}


/// Files of the two file-scope-effect fixtures below: `Window.php` echoes when it is loaded,
/// `Plain.php` only aliases, and `autoload.php` is the runtime loader a Composer project
/// registers through `vendor/autoload.php`.
fn file_scope_effect_fixture(main: &'static str) -> Vec<(&'static str, &'static str)> {
    vec![
        ("composer.json", r#"{"autoload":{"psr-4":{"P\\":"src/"}}}"#),
        (
            "src/Window.php",
            "<?php\nnamespace P;\nclass Window {\n    public static function size(): int { return 3; }\n}\necho \"window-init|\";\n",
        ),
        (
            "src/Plain.php",
            "<?php\nnamespace P;\nclass Plain {\n    public function name(): string { return 'plain'; }\n}\nif (!\\class_exists(Legacy::class, false)) {\n    \\class_alias(Plain::class, Legacy::class);\n}\n",
        ),
        (
            "autoload.php",
            "<?php\nspl_autoload_register(function (string $class): void {\n    if (str_starts_with($class, 'P\\\\')) {\n        require __DIR__ . '/src/' . substr($class, 2) . '.php';\n    }\n});\n",
        ),
        ("main.php", main),
    ]
}

/// A class file whose file-scope code has an effect, reached only from a function body, runs
/// that code when the class is first used -- here after `start|plain|` -- and not at boot.
///
/// Spliced among the declarations, `Window.php`'s echo ran before the entry's first statement;
/// psysh's `Hoa/ConsoleWindow.php` (`Console::advancedInteraction();`, which shells out to `stty`)
/// killed every Laravel artisan command that way. Reference values captured from php 8.5.10.
#[test]
fn test_effectful_class_file_reached_from_a_body_runs_its_file_scope_code_on_first_use() {
    let files = file_scope_effect_fixture(
        "<?php\nrequire __DIR__ . '/autoload.php';\nfunction size(): int { return P\\Window::size(); }\necho \"start|\";\necho (new P\\Plain())->name(), \"|\";\necho size(), size();\necho \"end\";\n",
    );
    let out = compile_cli_files_and_run(&files, "main.php");
    assert_eq!(out, "start|plain|window-init|33end");
}

/// The same file named by executable file-scope code keeps its compiled splice point, which is
/// where php's autoloader would run it.
#[test]
fn test_effectful_class_file_named_at_file_scope_stays_compiled_in_place() {
    let files = file_scope_effect_fixture(
        "<?php\nrequire __DIR__ . '/autoload.php';\necho \"start|\";\necho (new P\\Plain())->name(), \"|\";\necho P\\Window::size();\necho \"|end\";\n",
    );
    let out = compile_cli_files_and_run(&files, "main.php");
    assert_eq!(out, "start|plain|window-init|3|end");
}

/// A function declared after the class in a file an `spl_autoload_register` callback requires is
/// callable once the class is used: statically, through a string callable named by the class,
/// and from eval'd code. The resolver leaves the function's binding inside the callback, which
/// the compiled program never runs for a class it already declared.
#[test]
fn test_function_declared_in_an_autoloaded_class_file_is_callable() {
    let files = [
        (
            "src/Ext.php",
            "<?php\nnamespace Lib\\Ext;\nfinal class Ext {\n    public function tests(): array { return ['root' => 'Lib\\Ext\\is_root']; }\n}\nfunction is_root(int $x): bool { return $x === 0; }\n",
        ),
        (
            "main.php",
            "<?php\nspl_autoload_register(function ($c) {\n    if ($c === 'Lib\\Ext\\Ext') { require __DIR__ . '/src/Ext.php'; }\n});\n$t = (new Lib\\Ext\\Ext())->tests();\necho \\Lib\\Ext\\is_root(0) ? 'yes' : 'no', \"|\";\necho call_user_func($t['root'], 1) ? 'yes' : 'no', \"|\";\n$code = 'return \\Lib\\Ext\\is_root(0) ? \"yes\" : \"no\";';\necho eval($argc > 0 ? $code : 'return \"\";');\n",
        ),
    ];
    let out = compile_cli_files_and_run(&files, "main.php");
    assert_eq!(out, "yes|no|yes");
}

/// Verifies `spl_autoload_functions()` reports the request's runtime registrations.
///
/// PHP keeps ONE autoload queue per request. A compiled registration goes through the shared
/// bridge and an interpreted one through the interpreter's context, so the compiled call must ask
/// the interpreter instead of answering a static placeholder list: Symfony's `PhpDumper`
/// `getAutoloadFile()` walks `spl_autoload_functions()` looking for the Composer loader, and an
/// empty answer suppressed the generated `App_KernelProdContainer.preload.php` on every
/// `cache:warmup`.
#[test]
fn test_spl_autoload_functions_reports_runtime_registrations() {
    let out = compile_and_run(
        r#"<?php
final class ProbeLoader { public function load(string $class): void {} }
$loader = new ProbeLoader();
var_export(spl_autoload_register([$loader, 'load']));
echo "|", count(spl_autoload_functions()), "|";
$fns = spl_autoload_functions();
echo get_debug_type($fns[0]), "|", $fns[0][1], "|";
$code = 'spl_autoload_register(function (string $c): void {}); echo count(spl_autoload_functions());';
echo eval($argc > 0 ? $code : 'echo 0;');
"#,
    );
    assert_eq!(out, "true|1|array|load|2");
}
