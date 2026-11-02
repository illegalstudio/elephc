//! Purpose:
//! Pins PHP object type identity when an eval-created object crosses into another eval frame.
//!
//! Called from:
//! - `tests/codegen/mod.rs` through the codegen integration suite.

use crate::support::*;

/// ReflectionClass::newInstance from evaluated code may instantiate a class compiled into the
/// surrounding binary; its native object factory and constructor bridge must agree on that class.
#[test]
fn test_eval_reflection_class_new_instance_constructs_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
class AotReflectionConstructionTarget {
    public string $value = 'ready';
    public function __construct() {}
}
echo eval('$r = new ReflectionClass("AotReflectionConstructionTarget"); '
    . '$instance = $r->newInstance(); return get_class($instance);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "AotReflectionConstructionTarget");
}

/// ReflectionClass::newInstance must also construct a class declared by an autoloaded file in a
/// previous eval frame, matching PHP's shared runtime class table across includes/eval scopes.
#[test]
fn test_eval_reflection_class_new_instance_constructs_autoloaded_class_across_frames() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
eval('require __DIR__ . "/ReflectionAutoloadedTarget.php";');
echo eval('$reflection = new ReflectionClass("ReflectionAutoloadedTarget"); '
    . '$instance = $reflection->newInstance(); return get_class($instance);');
"#,
            ),
            (
                "ReflectionAutoloadedTarget.php",
                "<?php\nclass ReflectionAutoloadedTarget { public string $value = 'ready'; }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ReflectionAutoloadedTarget");
}

/// A compiled caller may construct a dynamically autoloaded class through ReflectionClass; the
/// Reflection AOT bridge must resolve that eval declaration in the declaring context.
#[test]
fn test_aot_reflection_new_instance_constructs_runtime_autoloaded_class() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
spl_autoload_register(static function (string $class): void {
    if ($class === 'ReflectionAotAutoloadedTarget') {
        require __DIR__ . '/ReflectionAotAutoloadedTarget.php';
    }
});
function instantiate_reflected(string $class): object {
    return (new ReflectionClass($class))->newInstance();
}
echo get_class(instantiate_reflected('ReflectionAotAutoloadedTarget'));
"#,
            ),
            (
                "ReflectionAotAutoloadedTarget.php",
                "<?php\ninterface ReflectionAotAutoloadedContract { public function configuration(): string; }\nclass ReflectionAotAutoloadedTarget implements ReflectionAotAutoloadedContract { public string $value = 'ready'; public function configuration(): string { return $this->value; } }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ReflectionAotAutoloadedTarget");
}

/// ReflectionClass::newInstance from an AOT method must see a class declared by an earlier eval
/// include, not only when the Reflection call itself runs inside the declaring eval frame.
#[test]
fn test_aot_reflection_new_instance_constructs_class_declared_in_prior_eval_frame() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
eval('require __DIR__ . "/PriorEvalReflectionTarget.php";');
function instantiate_prior_eval_class(string $class): object {
    return (new ReflectionClass($class))->newInstance();
}
echo get_class(instantiate_prior_eval_class('PriorEvalReflectionTarget'));
"#,
            ),
            (
                "PriorEvalReflectionTarget.php",
                "<?php\nclass PriorEvalReflectionTarget { public string $value = 'ready'; }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "PriorEvalReflectionTarget");
}

/// Namespaced, runtime-autoloaded declarations are valid targets for ReflectionClass::newInstance
/// when the AOT caller and the autoload callback execute in different evaluation scopes.
#[test]
fn test_aot_reflection_new_instance_constructs_namespaced_runtime_class() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
spl_autoload_register(static function (string $class): void {
    if ($class === 'App\\Config\\ReflectionTarget') {
        require __DIR__ . '/ReflectionTarget.php';
    }
});
function instantiate_namespaced_reflection_target(string $class): object {
    return (new \ReflectionClass($class))->newInstance();
}
echo get_class(instantiate_namespaced_reflection_target('App\\Config\\ReflectionTarget'));
"#,
            ),
            (
                "ReflectionTarget.php",
                "<?php\nnamespace App\\Config; class ReflectionTarget { public function __construct() {} }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "App\\Config\\ReflectionTarget");
}

/// A namespaced eval-included class implementing an interface remains constructible by an AOT
/// caller through ReflectionClass, covering the cross-context shape used by extension metadata.
#[test]
fn test_aot_reflection_new_instance_constructs_namespaced_eval_interface_class() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
eval('require __DIR__ . "/NamespacedReflectionTarget.php";');
function instantiate_namespaced_eval_class(string $class): object {
    return (new ReflectionClass($class))->newInstance();
}
echo get_class(instantiate_namespaced_eval_class('App\\Config\\NamespacedReflectionTarget'));
"#,
            ),
            (
                "NamespacedReflectionTarget.php",
                "<?php\nnamespace App\\Config; interface ReflectionConfigContract { public function configuration(): string; } class NamespacedReflectionTarget implements ReflectionConfigContract { public function configuration(): string { return 'ok'; } }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "App\\Config\\NamespacedReflectionTarget");
}

/// A class known only to the process-wide eval declaration table can be instantiated by an AOT
/// ReflectionClass caller in a later scope, as PHP's global class table permits.
#[test]
fn test_aot_reflection_new_instance_constructs_eval_only_class() {
    let out = compile_and_run_capture(
        r#"<?php
$prefix = 'RuntimeOnly';
$definition = 'class ' . $prefix . 'ReflectionTarget {}';
eval($definition);
function instantiate_eval_only_class(string $class): object {
    return (new ReflectionClass($class))->newInstance();
}
echo get_class(instantiate_eval_only_class($prefix . 'ReflectionTarget'));
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "RuntimeOnlyReflectionTarget");
}

/// A ReflectionClass cached in an object property and fetched in a later method keeps its target
/// class identity through the array/property ownership boundary.
#[test]
fn test_aot_reflection_new_instance_constructs_cached_reflection_object() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectionCacheProbe {
    private array $classes = [];
    public function getReflectionClass(string $name): ReflectionClass {
        if (!isset($this->classes[$name])) {
            $this->classes[$name] = new ReflectionClass($name);
        }
        return $this->classes[$name];
    }
    public function instantiate(string $name): object {
        $reflection = $this->getReflectionClass($name);
        return $reflection->newInstance();
    }
}
class ReflectionCacheTarget { public string $value = 'ready'; }
$cache = new ReflectionCacheProbe();
echo get_class($cache->instantiate(ReflectionCacheTarget::class));
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "ReflectionCacheTarget");
}

/// ReflectionClass cached by an AOT method must preserve the identity of a class loaded later by
/// an autoloader, the same interaction used by native extension configuration discovery.
#[test]
fn test_aot_reflection_new_instance_constructs_cached_autoloaded_class() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
spl_autoload_register(static function (string $class): void {
    if ($class === 'CachedAutoloadedReflectionTarget') {
        require __DIR__ . '/CachedAutoloadedReflectionTarget.php';
    }
});
class ReflectionCacheProbe {
    private array $classes = [];
    public function getReflectionClass(string $name): ReflectionClass {
        if (!isset($this->classes[$name])) {
            $this->classes[$name] = new ReflectionClass($name);
        }
        return $this->classes[$name];
    }
    public function instantiate(string $name): object {
        return $this->getReflectionClass($name)->newInstance();
    }
}
$cache = new ReflectionCacheProbe();
echo get_class($cache->instantiate('CachedAutoloadedReflectionTarget'));
"#,
            ),
            (
                "CachedAutoloadedReflectionTarget.php",
                "<?php\nclass CachedAutoloadedReflectionTarget { public string $value = 'ready'; }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "CachedAutoloadedReflectionTarget");
}

/// A ReflectionClass object itself may cross eval frames; newInstance must still resolve the
/// class identity held by that object when a later frame invokes it.
#[test]
fn test_eval_reflection_class_object_new_instance_crosses_frames() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
$reflection = eval('require __DIR__ . "/CrossFrameReflectionTarget.php"; return new ReflectionClass("CrossFrameReflectionTarget");');
echo eval('$instance = $reflection->newInstance(); return get_class($instance);');
"#,
            ),
            (
                "CrossFrameReflectionTarget.php",
                "<?php\nclass CrossFrameReflectionTarget { public string $value = 'ready'; }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "CrossFrameReflectionTarget");
}

/// Reflection instantiates a runtime-loaded, namespaced class with an imported interface and no
/// explicit constructor, matching PHP's default constructor behavior across eval declarations.
#[test]
fn test_eval_reflection_instantiates_runtime_class_with_aliased_interface_and_no_constructor() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
echo eval('spl_autoload_register(static function (string $class): void { if ($class === "App\\\\Contracts\\\\RuntimeContract") { require __DIR__ . "/RuntimeContract.php"; } elseif ($class === "App\\\\Bundle\\\\RuntimeConfiguration") { require __DIR__ . "/RuntimeConfiguration.php"; } }); class RuntimeContainer { public function getReflectionClass(string $className): ?ReflectionClass { return new ReflectionClass($className); } } class RuntimeExtension { public function getConfiguration(string $className, RuntimeContainer $container): object { $reflection = $container->getReflectionClass($className); if (!$reflection->implementsInterface("App\\\\Contracts\\\\RuntimeContract")) { return null; } if (!($constructor = $reflection->getConstructor()) || !$constructor->getNumberOfRequiredParameters()) { return $reflection->newInstance(); } return null; } } class ConcreteRuntimeExtension extends RuntimeExtension {} return get_class((new ConcreteRuntimeExtension())->getConfiguration("App\\\\Bundle\\\\RuntimeConfiguration", new RuntimeContainer()));');
"#,
            ),
            (
                "RuntimeContract.php",
                "<?php\nnamespace App\\Contracts; interface RuntimeContract { public function configure(): string; }\n",
            ),
            (
                "RuntimeConfiguration.php",
                "<?php\nnamespace App\\Bundle; use App\\Contracts\\RuntimeContract as Contract; class RuntimeConfiguration implements Contract { public function configure(): string { return 'ready'; } }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "App\\Bundle\\RuntimeConfiguration");
}

/// An AOT method can receive a ReflectionClass for an eval/autoload class; the nullable receiver
/// guard must not turn the empty variadic ReflectionClass::newInstance() call into one array arg.
#[test]
fn test_aot_reflection_new_instance_on_nullable_eval_reflection_class_passes_no_constructor_args() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
eval('require __DIR__ . "/RuntimeContract.php"; require __DIR__ . "/RuntimeConfiguration.php";');
class RuntimeContainer {
    public function getReflectionClass(string $className): ?ReflectionClass {
        return new ReflectionClass($className);
    }
}
class RuntimeExtension {
    public function getConfiguration(string $className, RuntimeContainer $container): ?\App\Contracts\RuntimeContract {
        $reflection = $container->getReflectionClass($className);
        if (!$reflection) {
            return null;
        }
        if (!$reflection->implementsInterface('App\\Contracts\\RuntimeContract')) {
            return null;
        }
        if (!($constructor = $reflection->getConstructor()) || !$constructor->getNumberOfRequiredParameters()) {
            return $reflection->newInstance();
        }
        return null;
    }
}
$configuration = (new RuntimeExtension())->getConfiguration('App\\Bundle\\RuntimeConfiguration', new RuntimeContainer());
if (!$configuration) {
    echo 'missing';
} else {
    echo $configuration->configure();
}
"#,
            ),
            (
                "RuntimeContract.php",
                "<?php\nnamespace App\\Contracts; interface RuntimeContract { public function configure(): string; }\n",
            ),
            (
                "RuntimeConfiguration.php",
                "<?php\nnamespace App\\Bundle; use App\\Contracts\\RuntimeContract as Contract; class RuntimeConfiguration implements Contract { public function configure(): string { return 'ready'; } }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ready");
}

/// A runtime-included object's class remains available to typed argument binding after its
/// creating eval frame returns.
#[test]
fn test_eval_returned_object_keeps_its_class_for_a_later_typed_call() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
$dependency = eval('require __DIR__ . "/RuntimeDependency.php"; return new RuntimeDependency();');
echo eval('require __DIR__ . "/RuntimeConsumer.php"; new RuntimeConsumer($dependency); return "ok";');
"#,
            ),
            (
                "RuntimeDependency.php",
                "<?php\nclass RuntimeDependency {}\n",
            ),
            (
                "RuntimeConsumer.php",
                "<?php\nclass RuntimeConsumer { public function __construct(RuntimeDependency $dependency) {} }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Objects whose classes are declared by SPL autoload in one eval frame remain assignable to a
/// typed constructor in a later frame.
#[test]
fn test_eval_autoloaded_object_keeps_its_class_for_a_later_typed_call() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
spl_autoload_register(static function (string $class): void {
    if ($class === 'AutoloadedDependency') {
        require __DIR__ . '/AutoloadedDependency.php';
    } elseif ($class === 'AutoloadedConsumer') {
        require __DIR__ . '/AutoloadedConsumer.php';
    }
});
$dependency = eval('return new AutoloadedDependency();');
echo eval('new AutoloadedConsumer($dependency); return "ok";');
"#,
            ),
            (
                "AutoloadedDependency.php",
                "<?php\nclass AutoloadedDependency {}\n",
            ),
            (
                "AutoloadedConsumer.php",
                "<?php\nclass AutoloadedConsumer { public function __construct(AutoloadedDependency $dependency) {} }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Objects returned by Reflection and round-tripped through an array retain their PHP class
/// identity when a later eval call binds a typed constructor argument.
#[test]
fn test_eval_lazy_ghost_keeps_its_class_through_array_for_typed_call() {
    let out = compile_and_run_capture(
        r#"<?php
class LazyGhostDependency { public string $value = 'new'; }
class LazyGhostConsumer { public function __construct(LazyGhostDependency $dependency) {} }
$reflection = new ReflectionClass(LazyGhostDependency::class);
$services = [];
$services['dependency'] = $reflection->newLazyGhost(static function ($object) {
    $object->value = 'initialized';
});
echo eval('new LazyGhostConsumer($services["dependency"]); return "ok";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "ok");
}

/// A Reflection-created object written into an array-valued object property keeps its class when
/// read back by a later eval call that binds a typed constructor argument.
#[test]
fn test_eval_lazy_ghost_keeps_its_class_through_object_property_array() {
    let out = compile_and_run_capture(
        r#"<?php
class PropertyArrayGhostDependency { public string $value = 'new'; }
class PropertyArrayGhostConsumer { public function __construct(PropertyArrayGhostDependency $dependency) {} }
class PropertyArrayGhostContainer { public array $privates = []; }
$container = new PropertyArrayGhostContainer();
$reflection = new ReflectionClass(PropertyArrayGhostDependency::class);
$container->privates['dependency'] = $reflection->newLazyGhost(static function ($object) {
    $object->value = 'initialized';
});
echo eval('new PropertyArrayGhostConsumer($container->privates["dependency"]); return "ok";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "ok");
}

/// A captured service container used by a lazy-ghost initializer does not erase the ghost's class.
#[test]
fn test_eval_lazy_ghost_keeps_its_class_through_capturing_initializer() {
    let out = compile_and_run_capture(
        r#"<?php
class CapturedGhostDependency { public string $value = 'new'; }
class CapturedGhostConsumer { public function __construct(CapturedGhostDependency $dependency) {} }
class CapturedGhostContainer { public array $privates = []; }
class CapturedGhostFactory {
    public static function get(CapturedGhostContainer $container, $lazyLoad = true) {
        if ($lazyLoad === true) {
            return $container->privates['dependency'] = (new ReflectionClass(CapturedGhostDependency::class))
                ->newLazyGhost(static function ($proxy) use ($container) {
                    self::get($container, $proxy);
                });
        }
        $lazyLoad->value = 'initialized';
        return $lazyLoad;
    }
}
$container = new CapturedGhostContainer();
CapturedGhostFactory::get($container);
echo eval('new CapturedGhostConsumer($container->privates["dependency"]); return "ok";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "ok");
}

/// ReflectionClass::newLazyGhost called through Magician keeps the AOT class identity.
#[test]
fn test_eval_dynamic_new_lazy_ghost_keeps_its_class_for_typed_call() {
    let out = compile_and_run_capture(
        r#"<?php
class DynamicGhostDependency { public string $value = 'new'; }
class DynamicGhostConsumer { public function __construct(DynamicGhostDependency $dependency) {} }
$services = [];
$setup = '$services["dependency"] = (new ReflectionClass("DynamicGhostDependency"))'
    . '->newLazyGhost(static function ($object) { $object->value = "initialized"; });';
eval($setup);
echo eval('new DynamicGhostConsumer($services["dependency"]); return "ok";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "ok");
}

/// Autoloaded AOT classes retain their runtime class id after constructorless allocation,
/// initialization, and a round-trip through a compiled service array into a later eval call.
#[test]
fn test_eval_autoloaded_lazy_ghost_keeps_its_class_for_typed_call() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
spl_autoload_register(static function (string $class): void {
    if ($class === 'App\\Ghost\\AutoloadedLazyGhostDependency') {
        require __DIR__ . '/AutoloadedLazyGhostDependency.php';
    } elseif ($class === 'App\\Ghost\\AutoloadedLazyGhostConsumer') {
        require __DIR__ . '/AutoloadedLazyGhostConsumer.php';
    }
});
class AutoloadedLazyGhostContainer { public array $privates = []; }
$container = new AutoloadedLazyGhostContainer();
$setup = '$container->privates["dependency"] = (new ReflectionClass("App\\Ghost\\AutoloadedLazyGhostDependency"))'
    . '->newLazyGhost(static function ($proxy) use ($container) { $proxy->__construct("initialized"); });';
eval($setup);
echo eval('new \\App\\Ghost\\AutoloadedLazyGhostConsumer($container->privates["dependency"]); return "ok";');
"#,
            ),
            (
                "AutoloadedLazyGhostDependency.php",
                "<?php\nnamespace App\\Ghost;\nclass AutoloadedLazyGhostDependency { public string $value = 'new'; public function __construct(string $value) { $this->value = $value; } }\n",
            ),
            (
                "AutoloadedLazyGhostConsumer.php",
                "<?php\nnamespace App\\Ghost;\nclass AutoloadedLazyGhostConsumer { public function __construct(AutoloadedLazyGhostDependency $dependency) {} }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies that an eval-declared subclass of a compiled class may redeclare the constructor with
/// a different signature: php exempts constructors from signature compatibility unless the
/// parent's is abstract. Symfony's generated container (`__construct()` over
/// `Container::__construct(?ParameterBagInterface $p = null)`) is this shape.
#[test]
fn test_eval_subclass_redeclares_compiled_constructor_with_another_signature() {
    let out = compile_and_run(
        r#"<?php
class CompiledBase {
    public string $tag = "base";
    public function __construct(?array $options = null) { $this->tag = "base:" . count($options ?? []); }
}
eval('class EvalChild extends CompiledBase {
    public function __construct(string $name, int $n) { $this->tag = $name . $n; }
}');
$c = new EvalChild("child", 2);
echo $c->tag;
"#,
    );
    assert_eq!(out, "child2");
}

/// Verifies that eval can call a compiled constructor as an ordinary method. The constructor is
/// published only as the class's constructor, and the method binder must still find its physical
/// signature (with the hidden argument collector an eval program gives every body), both on a
/// constructed object and inside a lazy ghost's initializer.
#[test]
fn test_eval_calls_compiled_constructor_as_a_method() {
    let out = compile_and_run(
        r#"<?php
class Dep { public string $value = 'new'; public function __construct(string $value) { $this->value = $value; } }
$o = new Dep("a");
eval('$o->__construct("b");');
echo $o->value, "|";
eval('$h = (new ReflectionClass("Dep"))->newLazyGhost(static function ($proxy) { $proxy->__construct("init"); });');
echo $h->value;
"#,
    );
    assert_eq!(out, "b|init");
}

/// Discarding a dynamic method's return of its borrowed object parameter must not free the
/// constructorless object retained by the caller.
#[test]
fn test_eval_discarded_dynamic_object_return_preserves_caller_object() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
require __DIR__ . '/DynamicReturnDependency.php';
require __DIR__ . '/DynamicReturnConsumer.php';
class DynamicReturnContainer { protected array $privates = []; }
eval('class DynamicReturnFactory extends DynamicReturnContainer {
    public static function get($container, $lazyLoad = true) {
        if ($lazyLoad === true) {
            return $container->privates["dependency"] = (new ReflectionClass("DynamicReturnDependency"))
                ->newLazyGhost(static function ($proxy) use ($container) {
                    self::get($container, $proxy);
                });
        }
        return $lazyLoad;
    }
    public static function makeConsumer($container) {
        return new DynamicReturnConsumer($container->privates["dependency"]);
    }
}');
eval('class DynamicReturnAppContainer extends DynamicReturnContainer {
    public function load($id) { return DynamicReturnFactory::get($this); }
}');
$container = eval('return new DynamicReturnAppContainer();');
eval('$container->load("dependency");');
echo eval('DynamicReturnFactory::makeConsumer($container); return "ok";');
"#,
            ),
            (
                "DynamicReturnDependency.php",
                "<?php\nclass DynamicReturnDependency { public function __construct() {} }\n",
            ),
            (
                "DynamicReturnConsumer.php",
                "<?php\nclass DynamicReturnConsumer { public function __construct(DynamicReturnDependency $dependency) {} }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// A lazily initialized object declared only by runtime autoload remains type-identifiable after
/// storage and is accepted by a constructor in a later eval context.
#[test]
fn test_eval_runtime_autoloaded_ghost_keeps_class_identity_across_contexts() {
    let out = compile_cli_files_and_run(
        &[(
            "main.php",
            r#"<?php
$prefix = __DIR__;
spl_autoload_register(static function (string $class) use ($prefix): void {
    if ($class === 'RuntimeGhostDependency') {
        require $prefix . '/RuntimeGhostDependency.php';
    } elseif ($class === 'RuntimeGhostConsumer') {
        require $prefix . '/RuntimeGhostConsumer.php';
    }
});
class RuntimeGhostContainer { public array $privates = []; }
$container = new RuntimeGhostContainer();
$setup = '$container->privates["dependency"] = (new ReflectionClass("RuntimeGhostDependency"))'
    . '->newLazyGhost(static function ($proxy) { $proxy->__construct("secret"); });';
eval($setup);
echo eval('new RuntimeGhostConsumer($container->privates["dependency"]); return "ok";');
"#,
        ),
        (
            "RuntimeGhostDependency.php",
            "<?php\nclass RuntimeGhostDependency { public string $value; public function __construct(string $value) { $this->value = $value; } }\n",
        ),
        (
            "RuntimeGhostConsumer.php",
            "<?php\nclass RuntimeGhostConsumer { public function __construct(RuntimeGhostDependency $dependency) {} }\n",
        ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// ReflectionClass::newLazyGhost retains an interpreter-owned instance across callback argument
/// cleanup and a later typed constructor call.
#[test]
fn test_eval_dynamic_class_lazy_ghost_survives_initializer_argument_cleanup() {
    let out = compile_cli_files_and_run(
        &[
            (
                "main.php",
                r#"<?php
$declarations = file_get_contents(__DIR__ . '/runtime-classes.txt');
eval($declarations);
$container = eval('return new InterpreterGhostContainer();');
eval('InterpreterGhostFactory::get($container);');
echo eval('InterpreterGhostFactory::makeConsumer($container); return "ok";');
"#,
            ),
            (
                "runtime-classes.txt",
                "class InterpreterGhostDependency { private string $secret; public function __construct(string $secret) { $this->secret = $secret; } }\nclass InterpreterGhostContainer { public array $privates = []; }\nclass InterpreterGhostFactory { public static function get($container, $lazyLoad = true) { if ($lazyLoad === true) { return $container->privates['dependency'] = (new ReflectionClass('InterpreterGhostDependency'))->newLazyGhost(static function ($proxy) use ($container) { self::get($container, $proxy); }); } return ($lazyLoad->__construct('secret') && false ?: $lazyLoad); } public static function makeConsumer($container) { return new InterpreterGhostConsumer($container->privates['dependency']); } }\nclass InterpreterGhostConsumer { public function __construct(InterpreterGhostDependency $dependency) {} }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// A compiled parameter typed with a class only the interpreter declares accepts that class's
/// instances (and an interface only the interpreted subclass implements), and still throws
/// `TypeError` for an object that is not one.
#[test]
fn test_compiled_parameter_typed_with_interpreted_class_accepts_its_instances() {
    let out = compile_and_run(
        r#"<?php
interface Marker {}
class Base { public function kind() { return 'base'; } }
eval('class Req extends Base implements Marker { public function kind() { return "req"; } }');
function handle(Req $r) { return $r->kind(); }
function marked(Marker $m) { return 'marked'; }
function make(): Base { $c = $GLOBALS['argc'] > 5 ? 'Base' : 'Req'; return new $c(); }
echo handle(make()), "\n";
echo marked(make()), "\n";
try { echo handle(new Base()), "\n"; } catch (TypeError $e) { echo "TypeError\n"; }
try { echo marked(new Base()), "\n"; } catch (TypeError $e) { echo "TypeError\n"; }
"#,
    );
    assert_eq!(out, "req\nmarked\nTypeError\nTypeError\n");
}

/// An interpreted class implements a compiled interface whose OTHER implementation calls
/// `func_get_args()`: the hidden variadic that call adds to the whole method family is ABI only
/// and is not part of the contract the interpreter checks.
#[test]
fn test_eval_class_implements_interface_whose_compiled_implementation_reads_func_get_args() {
    let out = compile_and_run(
        r#"<?php
interface Dispatcher { public function dispatch($event, $payload = [], $halt = false); }
class Fake implements Dispatcher {
    public function dispatch($event, $payload = [], $halt = false) { return 'fake:' . count(func_get_args()); }
}
eval('class Real implements Dispatcher { public function dispatch($event, $payload = [], $halt = false) { return "real:" . $event; } }');
$cls = 'Real';
$d = new $cls();
echo $d->dispatch('boot'), "|", $d instanceof Dispatcher ? 'yes' : 'no', "\n";
function fire(Dispatcher $d) { return $d->dispatch('x', [], true); }
echo fire($d), "|", fire(new Fake()), "\n";
"#,
    );
    assert_eq!(out, "real:boot|yes\nreal:x|fake:3\n");
}

/// A property that an eval-declared subclass of a COMPILED class declares must be readable from
/// compiled code through an untyped receiver.
///
/// The object is a native object of its compiled ancestor; the subclass's own properties live in
/// the eval context that declared it. A compiled frame with no eval context of its own read the
/// name through the native paths only (the declared-slot dispatch, then the stdClass helper) and
/// got null, and one with a context of its own asked THAT context, which does not know the class.
/// Laravel's `Application::register()` reads `$provider->singletons` exactly so on a service
/// provider loaded at run time, and warned "foreach() argument must be of type array|object,
/// null given" on every boot. The read now goes to the object's owning context.
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_compiled_code_reads_properties_declared_by_an_eval_subclass() {
    let out = compile_and_run(
        r#"<?php
abstract class Provider {
    public $app = "app";
    public function register() {}
}
class Aggregate extends Provider {
    protected $providers = [];
}
function register_it($provider) {
    $provider->register();
    if (property_exists($provider, 'singletons')) {
        echo gettype($provider->singletons), ":";
        foreach ($provider->singletons as $key => $value) {
            echo $key, "=", $value, ",";
        }
    }
    echo $provider->app, "\n";
}
function with_context($provider) {
    eval(chr(36) . 'unused = 1;');
    echo count($provider->singletons), "\n";
}
eval('class Foundation extends Aggregate { public $singletons = ["a" => "A", "b" => "B"]; }');
$name = "Found" . "ation";
$provider = new $name();
register_it($provider);
with_context($provider);
"#,
    );
    assert_eq!(out, "array:a=A,b=B,app\n2\n");
}

/// A compiled object's `__get()` answers a property read made through the eval bridge, which a
/// frame owning an eval context uses for every read it cannot resolve to a declared slot.
///
/// The bridge only knew declared slots and read every other name as an undeclared null, and in
/// quiet mode (`isset`/`empty`/`??`) it answered null for ANY name the object lacked. Laravel's
/// `loadViewsFrom()` closure reads `$this->app->config['view']['paths']`, where the container
/// answers `config` from `__get()`: the read came back null and every view namespace was lost.
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_bridge_property_read_calls_a_compiled_magic_get() {
    let out = compile_and_run(
        r#"<?php
class Settings implements ArrayAccess {
    public function __construct(private array $items) {}
    public function offsetExists($key): bool { return isset($this->items[$key]); }
    public function offsetGet($key): mixed { return $this->items[$key] ?? null; }
    public function offsetSet($key, $value): void { $this->items[$key] = $value; }
    public function offsetUnset($key): void { unset($this->items[$key]); }
}
class Container implements ArrayAccess {
    protected $instances = [];
    public function instance($key, $value) { $this->instances[$key] = $value; }
    public function offsetExists($key): bool { return isset($this->instances[$key]); }
    public function offsetGet($key): mixed { return $this->instances[$key]; }
    public function offsetSet($key, $value): void { $this->instances[$key] = $value; }
    public function offsetUnset($key): void { unset($this->instances[$key]); }
    public function __get($key) { return $this[$key]; }
}
class App extends Container {}
class Guarded {
    public function __isset($key) { return $key === 'open'; }
    public function __get($key) { return ['k' => $key]; }
}
class Provider {
    protected $app;
    protected $guarded;
    public function __construct($app, $guarded) { $this->app = $app; $this->guarded = $guarded; }
    public function load($namespace) {
        $show = function ($view) use ($namespace) {
            echo get_class($this->app->config), "|";
            echo isset($this->app->config['view']['paths']) ? "y" : "n", "|";
            echo implode(",", $this->app->config['view']['paths']), "|";
            echo isset($this->guarded->open['k']) ? "y" : "n";
            echo isset($this->guarded->shut['k']) ? "y" : "n", "|", $namespace, $view, "\n";
            eval(chr(36) . 'unused = 1;');
        };
        $show("!");
    }
}
$app = new App();
$app->instance('config', new Settings(['view' => ['paths' => ['/a', '/b']]]));
(new Provider($app, new Guarded()))->load("ns");
"#,
    );
    assert_eq!(out, "Settings|y|/a,/b|yn|ns!\n");
}

/// Compiled `$object[$key]` reads an eval-declared `ArrayAccess` class through its own
/// `offsetGet()`, and an exception that method throws reaches the compiled `catch`.
///
/// The runtime finds `offsetGet` through the object's class id, and an object the interpreter
/// built carries `stdClass`'s id whatever class declared it, so every such read raised "Cannot
/// use object of type stdClass as array" -- Laravel's config repository, loaded at run time, is
/// read that way by every `$app['config']['view']`. The runtime now asks the eval bridge before
/// raising.
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_compiled_index_reads_an_eval_declared_array_access_object() {
    let out = compile_and_run(
        r#"<?php
eval('class Repo implements ArrayAccess {
    private $items = ["view" => ["paths" => ["/a", "/b"]], 7 => "seven"];
    public function offsetExists($key): bool { return isset($this->items[$key]); }
    public function offsetGet($key): mixed {
        if ($key === "boom") { throw new RuntimeException("no " . $key); }
        return $this->items[$key] ?? null;
    }
    public function offsetSet($key, $value): void {}
    public function offsetUnset($key): void {}
}');
function read($repo) { return implode(",", $repo['view']['paths']); }
function read_int($repo) { return $repo[7]; }
function boom($repo) { return $repo['boom']; }
$name = 'Re' . 'po';
$repo = new $name();
echo read($repo), "|", read_int($repo), "|", $repo['missing'] ?? 'dflt', "\n";
try {
    boom($repo);
} catch (RuntimeException $e) {
    echo "caught ", $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(out, "/a,/b|seven|dflt\ncaught no boom\n");
}

/// A property written by interpreted code running in ANOTHER eval context than the one that
/// built the object is the object's property, visible to every later reader.
///
/// The write went into the writing context's overlay, where only that context looked, while the
/// owner kept its old value. Laravel's cached routes file is included from a closure with a
/// context of its own and calls `app('router')->setCompiledRoutes(...)`; the router kept
/// answering the empty collection it was built with and every route 404ed.
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_eval_object_property_written_from_another_context_is_seen_by_its_owner() {
    let out = compile_and_run(
        r#"<?php
eval('class Router {
    protected $routes = "old";
    public $open;
    public function set($value) { $this->routes = $value; }
    public function get() { return $this->routes; }
    public function drop() { unset($this->open); }
}');
$name = 'Rou' . 'ter';
$router = new $name();
$router->open = "o";
function elsewhere($router) {
    $code = 'return function ($r) { $r->set("new"); $r->open = "p"; };';
    $setter = eval($code);
    $setter($router);
}
elsewhere($router);
echo $router->get(), "|", $router->open, "\n";
function drop_elsewhere($router) {
    $code = 'return function ($r) { $r->drop(); };';
    $dropper = eval($code);
    $dropper($router);
}
drop_elsewhere($router);
echo isset($router->open) ? "set" : "unset", "\n";
"#,
    );
    assert_eq!(out, "new|p\nunset\n");
}

/// A class declared in eval reads the constants of the COMPILED interfaces it implements --
/// directly, through a compiled parent, or through an interface's own parents.
///
/// Nothing consulted a compiled interface's constants for an eval class: Carbon's
/// `static::YEARS_PER_MILLENNIUM` (declared by the compiled `CarbonInterface`) failed as an
/// "unsupported ClassConstantFetch" inside `addUnit()`.
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_eval_class_reads_constants_of_compiled_interfaces() {
    let out = compile_and_run(
        r#"<?php
interface Units {
    public const YEARS_PER_MILLENNIUM = 1000;
    public const NAME = 'units';
}
interface MoreUnits extends Units {
    public const DAYS = 7;
}
class Base implements MoreUnits {
    public function base() { return static::YEARS_PER_MILLENNIUM; }
}
eval('class Stamp extends Base {
    public function meta() {
        return [static::YEARS_PER_MILLENNIUM, self::NAME, static::DAYS, Stamp::DAYS];
    }
}
class Loose implements MoreUnits {
    public function meta() { return [static::YEARS_PER_MILLENNIUM, self::DAYS]; }
}');
$name = 'Sta' . 'mp';
echo json_encode((new $name())->meta()), (new $name())->base(), "\n";
$loose = 'Lo' . 'ose';
echo json_encode((new $loose())->meta()), "\n";
"#,
    );
    assert_eq!(out, "[1000,\"units\",7,7]1000\n[1000,7]\n");
}

/// A compiled late-static call reaches an interpreted override of a placeholder that only
/// throws, and keeps the value that override returns.
///
/// A method whose body never returns normally infers `void`, so every compiled call through it
/// discarded what an eval subclass's override returned: every runtime-loaded Laravel facade
/// resolved an empty accessor name and `Date::now()` failed with "Target class [date] does not
/// exist".
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_compiled_late_static_call_keeps_an_eval_override_result() {
    let out = compile_and_run(
        r#"<?php
abstract class Facade {
    public static function root() {
        return static::resolve(static::accessor());
    }
    protected static function resolve($name) {
        return "base:" . $name;
    }
    protected static function accessor() {
        throw new RuntimeException("no accessor");
    }
    public static function __callStatic($method, $args) {
        return $method . "->" . static::root();
    }
}
eval('class DateFacade extends Facade {
    protected static function accessor() { return "date"; }
    protected static function resolve($name) { return "override(" . parent::resolve($name) . ")"; }
}');
$class = 'Date' . 'Facade';
echo $class::root(), "|", $class::now(), "\n";
"#,
    );
    assert_eq!(out, "override(base:date)|now->override(base:date)\n");
}

/// A compiled `clone` of an eval-declared object is an object of the same class, with the
/// properties that class keeps in its context.
///
/// The compiled clone copies the native layout, which for an eval subclass is its compiled
/// ancestor's: the copy came back as that ancestor. Symfony's `Request::duplicate()` clones
/// `$this`, and Laravel's `Request::duplicate(): static` then failed its own return type.
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_compiled_clone_of_an_eval_object_keeps_its_class() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public $a = 1;
    public function duplicate(): static {
        $copy = clone $this;
        $copy->a = 2;
        return $copy;
    }
}
eval('class Sub extends Base {
    public $extra = "x";
    public function setExtra($value) { $this->extra = $value; return $this; }
    public function extra() { return $this->extra; }
    public function duplicate(): static { return parent::duplicate(); }
}');
$name = 'Su' . 'b';
$sub = (new $name())->setExtra("y");
$copy = $sub->duplicate();
echo get_class($copy), " ", $copy->extra(), " ", $copy->a, " ", $sub->a, " ";
$copy->setExtra("z");
echo $sub->extra(), " ", $copy->extra(), " ", $copy instanceof Sub ? "sub" : "base", "\n";
"#,
    );
    assert_eq!(out, "Sub y 2 1 y z sub\n");
}

/// A clone made in another eval context than the one that built the source keeps the source's
/// typed-property initialization.
///
/// The clone copied the property values and none of the initialization marks, which the source
/// keeps in its owning context: every typed property of the clone read as uninitialized. Carbon's
/// `instance()` clones like that, and the clone failed on its own `?WrapperClock $clock = null`.
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_eval_clone_keeps_typed_property_initialization() {
    let out = compile_and_run(
        r#"<?php
eval('trait LocalFactory { private ?string $clock = null; public function getClock() { return $this->clock; } }
class Stamp extends DateTime {
    use LocalFactory;
    public static function instance(DateTimeInterface $date): static {
        if ($date instanceof static) { return clone $date; }
        return new static($date->format("Y-m-d"));
    }
    public function __clone() { }
}');
$name = 'Sta' . 'mp';
$a = new $name('2024-01-02');
var_dump($a->getClock());
$b = $name::instance($a);
var_dump($b->getClock());
$c = $name::instance(new DateTime('2024-03-04'));
var_dump($c->getClock(), $c->format('Y-m-d'));
"#,
    );
    assert_eq!(out, "NULL\nNULL\nNULL\nstring(10) \"2024-03-04\"\n");
}

/// Member probes (`method_exists`, `is_callable`, `get_class_methods`) run in one eval context
/// see a class another context autoloaded after this one last synced its class table.
///
/// The probing context never looked again, so it answered from what it already knew: Laravel's
/// pipeline closure asked `method_exists($pipe, 'handle')` of a middleware its container had just
/// autoloaded elsewhere, got false, and called the object as a closure ("Object of type stdClass
/// is not callable").
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_member_probes_see_a_class_another_context_autoloaded() {
    let out = compile_and_run_files(
        &[
            (
                "Guard.php",
                r#"<?php
class Guard
{
    public function handle($request, $next)
    {
        return 'guard(' . $next($request) . ')';
    }
}
"#,
            ),
            (
                "Pipe3.php",
                r#"<?php
class Pipe3
{
    public function run($container, $name)
    {
        $probe = function () use ($container, $name) {
            $pipe = $container->make($name);
            return [
                method_exists($pipe, 'handle'),
                is_callable([$pipe, 'handle']),
                in_array('handle', get_class_methods($pipe)),
            ];
        };
        return json_encode($probe());
    }
}
"#,
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function ($class) {
    $file = __DIR__ . '/' . $class . '.php';
    if (is_file($file)) {
        require $file;
    }
});
class Container {
    public function make($class) { return new $class(); }
}
$container = new Container();
$pipe = $container->make('Pi' . 'pe3');
echo $pipe->run($container, 'Gu' . 'ard'), "\n";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "[true,true,true]\n");
}
