//! Purpose:
//! End-to-end regressions for closure literals executed inside runtime eval.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_closure` through Rust's test harness.
//!
//! Key details:
//! - Fixtures compile PHP to native code, enter the eval bridge, and execute
//!   closure callable paths through elephc-magician.

use crate::support::{
    compile_and_run, compile_and_run_capture, compile_and_run_files, compile_and_run_with_heap_debug,
};

/// Verifies dynamic foreach references write through to source arrays and survive nested aliases.
#[test]
fn test_dynamic_foreach_reference_preserves_array_element_aliases() {
    let out = compile_and_run(
        r#"<?php
eval('$items = [1, 2];
foreach ($items as $key => &$item) {
    $item = $item + $key + 10;
}
$aliases = [];
foreach ($items as $key => &$item) {
    $aliases["nested"][$key] =& $item;
}
$aliases["nested"][0] = 99;
echo $items[0] . ":" . $items[1] . ":" . $item;');
"#,
    );

    assert_eq!(out, "99:13:13");
}

/// Verifies dynamic conditional array destructuring preserves its RHS and null fallback.
#[test]
fn test_dynamic_array_destructure_assignment_condition() {
    let out = compile_and_run(
        r#"<?php
eval('$propertyScopes = ["property" => ["declaring", "real"]];
if ([$scope, $name] = $propertyScopes["property"] ?? null) {
    echo $scope . ":" . $name;
}
if ([$scope, $name] = $propertyScopes["missing"] ?? null) {
    echo "unexpected";
}
echo ":" . (is_null($scope) ? "null" : $scope);');
"#,
    );

    assert_eq!(out, "declaring:real:null");
}

/// Verifies a dynamic leading logical negation applies after its nested assignment.
#[test]
fn test_dynamic_negated_assignment_expression() {
    let out = compile_and_run(
        r#"<?php
eval('$sentinel = 1;
$values = [0];
$isRef = !$valueIsStatic = $values[0] !== $sentinel;
echo ($isRef ? "true" : "false") . ":" . ($valueIsStatic ? "true" : "false");');
"#,
    );

    assert_eq!(out, "false:true");
}

/// Verifies a dynamic comparison observes an assignment in its right operand.
#[test]
fn test_dynamic_comparison_right_hand_assignment() {
    let out = compile_and_run(
        r#"<?php
eval('$out = null !== $ref = 1;
echo ($out ? "true" : "false") . ":" . $ref;');
"#,
    );

    assert_eq!(out, "true:1");
}

/// Verifies a dynamic logical branch evaluates a terminal comparison assignment on its right.
#[test]
fn test_dynamic_logical_comparison_right_hand_assignment() {
    let out = compile_and_run(
        r#"<?php
eval('$enabled = true;
$out = $enabled && null !== $ref = 1;
echo ($out ? "true" : "false") . ":" . $ref;');
"#,
    );

    assert_eq!(out, "true:1");
}

/// Verifies dynamic `(array)` casts preserve public eval-object properties and scalar elements.
#[test]
fn test_dynamic_array_cast_expression() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalArrayCastBox {
    public string $name = "Ada";
}
$object = new EvalArrayCastBox();
$properties = (array) $object;
$scalar = (array) 7;
echo $properties["name"] . ":" . $scalar[0];');
"#,
    );

    assert_eq!(out, "Ada:7");
}

/// Verifies an eval array-element receiver can dispatch an instance method.
#[test]
fn test_dynamic_array_element_method_call() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalArrayMethodBox {
    public function wake(): void { echo "ok"; }
}
$objects = [new EvalArrayMethodBox()];
$objects[0]->wake();');
"#,
    );

    assert_eq!(out, "ok");
}

/// Verifies dynamic array-reference assignments preserve a nested array source lvalue.
#[test]
fn test_dynamic_array_reference_assignment_uses_array_element_source() {
    let out = compile_and_run(
        r#"<?php
eval('$refs = [7];
$values = [];
$values["item"] =& $refs[0];
$refs[0] = 9;
echo $values["item"];');
"#,
    );

    assert_eq!(out, "9");
}

/// Verifies a dynamic prefix increment returns the updated value for an array index.
#[test]
fn test_dynamic_prefix_increment_array_index_expression() {
    let out = compile_and_run(
        r#"<?php
eval('$tokens = ["zero", "one"];
$i = -1;
echo $tokens[++$i] . ":" . $i;');
"#,
    );

    assert_eq!(out, "zero:0");
}

/// Verifies a dynamic postfix increment returns the prior value while updating its variable.
#[test]
fn test_dynamic_postfix_increment_expression() {
    let out = compile_and_run(
        r#"<?php
eval('$i = 0;
$before = $i++;
echo $before . ":" . $i;');
"#,
    );

    assert_eq!(out, "0:1");
}

/// Verifies dynamic nested array append materializes and writes through its parent lvalue.
#[test]
fn test_dynamic_nested_array_append() {
    let out = compile_and_run(
        r#"<?php
eval('$index = [];
$key = "closures";
$index[$key][] = "first";
$index[$key][] = "second";
echo $index[$key][0] . ":" . $index[$key][1];');
"#,
    );

    assert_eq!(out, "first:second");
}

/// Verifies dynamic callable declarations accept a trailing parameter comma.
#[test]
fn test_dynamic_trailing_parameter_comma() {
    let out = compile_and_run(
        r#"<?php
eval('function trailingParameterComma(string $value,): string {
    return $value;
}
echo trailingParameterComma("ok");');
"#,
    );

    assert_eq!(out, "ok");
}

/// Verifies dynamic goto exits a nested loop and resumes at the function-local label.
#[test]
fn test_dynamic_goto_label_control_flow() {
    let out = compile_and_run(
        r#"<?php
eval('foreach ([1, 2] as $item) {
    if ($item) {
        goto done;
    }
}
echo "skip";
done:
echo "ok";');
"#,
    );

    assert_eq!(out, "ok");
}

/// Verifies eval closure literals dispatch through direct calls and call_user_func_array.
#[test]
fn test_eval_closure_literal_dispatches_direct_and_call_user_func_array() {
    let out = compile_and_run(
        r#"<?php
eval('$fn = function($left, $right = 2) { return $left + $right; };
echo $fn(3); echo ":";
echo call_user_func_array($fn, ["right" => 6, "left" => 5]);');
"#,
    );

    assert_eq!(out, "5:11");
}

/// Verifies eval closure literals are exposed as PHP `Closure` objects.
#[test]
fn test_eval_closure_literal_is_php_closure_object() {
    let out = compile_and_run(
        r#"<?php
eval('$fn = function() { return "ok"; };
echo is_object($fn) ? "O" : "o"; echo ":";
echo get_class($fn); echo ":";
echo $fn instanceof Closure ? "I" : "i"; echo ":";
echo class_exists("Closure") ? "K" : "k"; echo ":";
echo is_callable($fn) ? "C" : "c"; echo ":";
echo call_user_func($fn);');
"#,
    );

    assert_eq!(out, "O:Closure:I:K:C:ok");
}

/// Verifies a closure returned by a runtime `require` invokes through AOT code in its activation.
///
/// Runtime include execution owns the closure body in Magician and represents the value as a PHP
/// Closure object, whereas AOT descriptor dispatch usually sees native `__invoke` candidates.
/// The fallback must preserve the original boxed value and use the eval callable ABI only after
/// no native candidate matches.
#[test]
fn test_dynamic_require_returned_closure_invokes_from_aot_activation() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
class DynamicRequiredClosureSubject {
    public function value(): string { return 'dynamic-closure-ok'; }
}

function invoke_callback(string $path, object $subject): string {
    $callback = require $path;
    return $callback($subject);
}

echo invoke_callback('callback.php', new DynamicRequiredClosureSubject());
"#,
            ),
            (
                "callback.php",
                r#"<?php
return static function (object $subject): string {
    return $subject->value();
};
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "dynamic-closure-ok");
}

/// Verifies a dynamic include Closure survives its loader frame before crossing a callable boundary.
///
/// The returned Closure is created in an AOT loader closure, so its Magician context must remain
/// registered after that loader returns. A second AOT method accepts it as `callable`, creates a
/// first-class callable, and invokes it only after the original dynamic frame has unwound.
#[test]
fn test_dynamic_require_closure_survives_loader_return_and_callable_boundary() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
class EscapedDynamicClosureSubject {
    public function value(): string { return 'escaped-dynamic-closure-ok'; }
}

class EscapedDynamicClosureLoader {
    public function load(string $path): string {
        $load = Closure::bind(static function (string $path) {
            return include $path;
        }, null, null);
        $callback = $load($path);
        if (!is_object($callback) || !is_callable($callback)) {
            return 'not-callable';
        }

        return $this->invoke($callback, new EscapedDynamicClosureSubject());
    }

    private function invoke(callable $callback, object $subject): string {
        $callback = $callback(...);

        return $callback($subject);
    }
}

echo (new EscapedDynamicClosureLoader())->load('callback.php');
"#,
            ),
            (
                "callback.php",
                r#"<?php
return static function (object $subject): string {
    return $subject->value();
};
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "escaped-dynamic-closure-ok");
}

/// Verifies a function declared by a runtime include resolves from a separately lowered namespace.
///
/// Dynamic includes can bootstrap ordinary global PHP functions before an AOT method reaches an
/// unqualified namespaced call. The method must probe the request-local declaration owner and
/// then preserve PHP's namespace-to-global fallback, rather than freezing an undefined function.
#[test]
fn test_dynamic_include_function_resolves_from_aot_namespace() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace DynamicIncludeFunctionProbe;

class Consumer {
    public function render(string $value): string {
        return dynamic_include_function($value);
    }
}

$path = 'functions.php';
include $path;
echo (new Consumer())->render('ok');
"#,
            ),
            (
                "functions.php",
                r#"<?php
function dynamic_include_function(string $value): string {
    return 'dynamic-function:' . $value;
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "dynamic-function:ok");
}

/// Verifies a runtime include executes function declarations guarded by extension and symbol probes.
///
/// Common compatibility files define a global fallback only when an optional extension is absent
/// and no native function occupies the name. The dynamic declaration must become visible to a
/// separately lowered namespaced caller after both guards evaluate false.
#[test]
fn test_dynamic_include_registers_extension_guarded_function_for_aot_caller() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace DynamicIncludeExtensionGuardProbe;

class Consumer {
    public function render(string $value): string {
        return extension_guarded_dynamic_function($value);
    }
}

$path = 'functions.php';
include $path;
echo (new Consumer())->render('ok');
"#,
            ),
            (
                "functions.php",
                r#"<?php
if (extension_loaded('nonexistent_dynamic_probe_extension')) {
    return;
}

if (!function_exists('extension_guarded_dynamic_function')) {
    function extension_guarded_dynamic_function(string $value): string {
        return 'extension-guarded-function:' . $value;
    }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "extension-guarded-function:ok");
}

/// Verifies an optional-extension compatibility name is not predeclared without a callable body.
///
/// The compiler may know an optional extension signature for static checking while the native
/// extension is not linked. `function_exists()` must still be false in a dynamically included
/// fallback file, allowing its conditional global declaration to own the later AOT call.
#[test]
fn test_dynamic_include_extension_fallback_overrides_unlinked_optional_signature() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace DynamicIncludeOptionalExtensionProbe;

class Consumer {
    public function render(string $value): string {
        return deepclone_to_array($value, null, false)[0];
    }
}

$path = 'functions.php';
include $path;
echo (new Consumer())->render('ok');
"#,
            ),
            (
                "functions.php",
                r#"<?php
if (extension_loaded('deepclone')) {
    return;
}

if (!function_exists('deepclone_to_array')) {
    function deepclone_to_array(mixed $value, ?array $allowed_classes = null, bool $allow_named_closures = false): array {
        return ['deepclone-fallback:' . $value];
    }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "deepclone-fallback:ok");
}

/// Verifies a nested runtime require preserves global fallback function ownership.
///
/// Compatibility packages commonly load a version-specific bootstrap from a small outer guard.
/// The inner declaration has to remain registered for a later AOT method after the outer include
/// returns, exactly as any nested PHP include does.
#[test]
fn test_nested_dynamic_include_registers_extension_fallback_for_aot_caller() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace NestedDynamicIncludeExtensionProbe;

class Consumer {
    public function render(string $value): string {
        return deepclone_to_array($value, null, false)[0];
    }
}

$path = 'bootstrap.php';
include $path;
echo (new Consumer())->render('ok');
"#,
            ),
            (
                "bootstrap.php",
                r#"<?php
if (extension_loaded('deepclone')) {
    return;
}

require 'bootstrap81.php';
"#,
            ),
            (
                "bootstrap81.php",
                r#"<?php
if (!function_exists('deepclone_to_array')) {
    function deepclone_to_array(mixed $value, ?array $allowed_classes = null, bool $allow_named_closures = false): array {
        return ['nested-deepclone-fallback:' . $value];
    }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "nested-deepclone-fallback:ok");
}

/// Verifies a runtime include's global function survives the AOT loader frame that created it.
///
/// PHP function declarations are request-global, not scoped to the method that performed the
/// include. The owning eval context must therefore remain callable after that loader method
/// returns and before a separate AOT consumer uses the fallback.
#[test]
fn test_dynamic_include_function_survives_aot_loader_return() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace DynamicIncludeLoaderLifetimeProbe;

class Loader {
    public function load(): void {
        $path = 'functions.php';
        include $path;
    }
}

class Consumer {
    public function render(string $value): string {
        return loader_lifetime_dynamic_function($value);
    }
}

(new Loader())->load();
echo (new Consumer())->render('ok');
"#,
            ),
            (
                "functions.php",
                r#"<?php
if (!function_exists('loader_lifetime_dynamic_function')) {
    function loader_lifetime_dynamic_function(string $value): string {
        return 'loader-lifetime-function:' . $value;
    }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loader-lifetime-function:ok");
}

/// Verifies a retained dynamic function also owns the global scope it declared against.
///
/// The AOT loader frame frees its normal eval scopes on return. A global fallback must retain the
/// scope that holds its global aliases until the request releases the owning function context, so
/// later `global` reads never observe freed storage.
#[test]
fn test_dynamic_include_function_keeps_global_scope_after_aot_loader_return() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace DynamicIncludeGlobalScopeLifetimeProbe;

class Loader {
    public function load(): void {
        $path = 'functions.php';
        include $path;
    }
}

class Consumer {
    public function render(): int {
        return loader_lifetime_global_counter();
    }
}

(new Loader())->load();
$consumer = new Consumer();
echo $consumer->render() . ':' . $consumer->render();
"#,
            ),
            (
                "functions.php",
                r#"<?php
function loader_lifetime_global_counter(): int {
    global $counter;
    $counter = 7;

    return $counter;
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "7:7");
}

/// Verifies an SPL autoloader registered by a runtime include resolves a later static class call.
///
/// PHP invokes registered autoload callbacks when a dynamic function first references an unknown
/// class. The callback and its closure context must remain live long enough to include the class
/// declaration, after which normal dynamic static-method dispatch supplies the result.
#[test]
fn test_dynamic_include_spl_autoloads_static_class_for_dynamic_function() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace DynamicIncludeSplAutoloadProbe;

class Consumer {
    public function render(): string {
        return autoloaded_dynamic_function();
    }
}

$path = 'bootstrap.php';
include $path;
echo (new Consumer())->render();
"#,
            ),
            (
                "bootstrap.php",
                r#"<?php
spl_autoload_register(static function (string $class): void {
    if ('RuntimeAutoloadTarget' === $class) {
        include 'target.php';
    }
});

function autoloaded_dynamic_function(): string {
    return RuntimeAutoloadTarget::render('ok');
}
"#,
            ),
            (
                "target.php",
                r#"<?php
class RuntimeAutoloadTarget {
    public static function render(string $value): string {
        return 'autoloaded:' . $value;
    }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "autoloaded:ok");
}

/// Verifies an eval-declared interface autoloads its missing parent interface before registration.
#[test]
fn test_dynamic_eval_interface_autoloads_parent_interface() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$bootstrap = 'bootstrap.php';
include $bootstrap;
eval('interface DynamicAutoloadedChildContract extends DynamicAutoloadedParentContract {}');
eval('trait DynamicAutoloadedTrait {} enum DynamicAutoloadedEnum { case Ready; }');
echo interface_exists('DynamicAutoloadedChildContract') ? 'loaded' : 'missing';
echo ':';
echo trait_exists('DynamicAutoloadedTrait') ? 'trait' : 'missing';
echo ':';
echo enum_exists('DynamicAutoloadedEnum') ? 'enum' : 'missing';
"#,
            ),
            (
                "bootstrap.php",
                r#"<?php
spl_autoload_register(static function (string $class): void {
    if ('DynamicAutoloadedParentContract' === $class) {
        include 'parent.php';
    }
});
"#,
            ),
            (
                "parent.php",
                "<?php\ninterface DynamicAutoloadedParentContract {}\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded:trait:enum");
}

/// Verifies eval parses and executes a `??=` property assignment nested in concatenation.
#[test]
fn test_dynamic_eval_null_coalesce_assign_on_concat_rhs() {
    let out = compile_and_run(
        r#"<?php
class EvalConcatAssignmentProbe {
    private string $directory = "dir:";
    private ?string $tmpSuffix = null;

    public function path(): string {
        eval('$this->directory.$this->tmpSuffix ??= str_replace("/", "-", "x/y");');
        return $this->directory.$this->tmpSuffix;
    }
}

$probe = new EvalConcatAssignmentProbe();
echo $probe->path() . ":" . $probe->path();
"#,
    );
    assert_eq!(out, "dir:x-y:dir:x-y");
}

/// Verifies eval executes `!static_property ??= static_method()` with PHP association rules.
#[test]
fn test_dynamic_eval_negated_static_property_null_coalesce_assignment() {
    let out = compile_and_run(
        r#"<?php
class EvalStaticAssignmentProbe {
    public static ?bool $ready = null;

    public static function resolve(): bool {
        return true;
    }
}

eval('if (!EvalStaticAssignmentProbe::$ready ??= EvalStaticAssignmentProbe::resolve()) { echo "bad"; }');
echo EvalStaticAssignmentProbe::$ready ? "ready" : "bad";
"#,
    );
    assert_eq!(out, "ready");
}

/// Verifies an eval child interface overrides an inherited method contract before class validation.
#[test]
fn test_dynamic_eval_child_interface_method_contract_overrides_parent() {
    let out = compile_and_run(
        r#"<?php
eval('interface EvalDeferredParentContract { public function item(): EvalDeferredParentValue; }
interface EvalDeferredChildContract extends EvalDeferredParentContract { public function item(): EvalDeferredChildValue; }
abstract class EvalDeferredContractBase implements EvalDeferredChildContract {
    public function item(): EvalDeferredChildValue { throw new \\RuntimeException(); }
}');
echo "declared";
"#,
    );
    assert_eq!(out, "declared");
}

/// Verifies an eval-declared class autoloads its missing parent class before registration.
#[test]
fn test_dynamic_eval_class_autoloads_parent_class() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$bootstrap = 'bootstrap.php';
include $bootstrap;
eval('class DynamicAutoloadedChild extends DynamicAutoloadedParent {}');
echo (new DynamicAutoloadedChild())->label();
"#,
            ),
            (
                "bootstrap.php",
                r#"<?php
spl_autoload_register(static function (string $class): void {
    if ('DynamicAutoloadedParent' === $class) {
        include 'parent.php';
    }
});
"#,
            ),
            (
                "parent.php",
                "<?php\nclass DynamicAutoloadedParent { public function label(): string { return 'parent-loaded'; } }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "parent-loaded");
}

/// Verifies an SPL object-method callback can autoload a dynamic static-call target.
///
/// Real autoloaders commonly retain an AOT object and register `[object, method]`, not a closure.
/// The generic registration table must normalize that callable at invocation time and let the
/// native method bridge execute the include before the original dynamic static call is retried.
#[test]
fn test_dynamic_include_spl_autoloads_via_aot_object_method_callback() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace DynamicIncludeSplObjectAutoloadProbe;

class Loader {
    public function load(string $class): void {
        if ('RuntimeObjectAutoloadTarget' === $class) {
            include 'target.php';
        }
    }
}

class Consumer {
    public function render(): string {
        return autoloaded_object_dynamic_function();
    }
}

$path = 'bootstrap.php';
include $path;
echo (new Consumer())->render();
"#,
            ),
            (
                "bootstrap.php",
                r#"<?php
$loader = new \DynamicIncludeSplObjectAutoloadProbe\Loader();
spl_autoload_register([$loader, 'load'], true, true);

function autoloaded_object_dynamic_function(): string {
    return RuntimeObjectAutoloadTarget::render('ok');
}
"#,
            ),
            (
                "target.php",
                r#"<?php
class RuntimeObjectAutoloadTarget {
    public static function render(string $value): string {
        return 'object-autoloaded:' . $value;
    }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "object-autoloaded:ok");
}

/// Verifies an AOT SPL registration is shared with a later dynamic static class call.
///
/// Autoload setup frequently runs in compiled bootstrap code while a dynamically included
/// compatibility function first names the class. The AOT builtin must register into its persistent
/// eval context, not return success without publishing the callback.
#[test]
fn test_aot_spl_autoload_registration_serves_dynamic_static_class_call() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace AotSplAutoloadRegistrationProbe;

class Loader {
    public function load(string $class): void {
        if ('AotRegisteredAutoloadTarget' === $class) {
            include 'target.php';
        }
    }
}

class Consumer {
    public function render(): string {
        return aot_registered_autoload_function();
    }
}

$loader = new Loader();
spl_autoload_register([$loader, 'load']);
$path = 'bootstrap.php';
include $path;
echo (new Consumer())->render();
"#,
            ),
            (
                "bootstrap.php",
                r#"<?php
function aot_registered_autoload_function(): string {
    return AotRegisteredAutoloadTarget::render('ok');
}
"#,
            ),
            (
                "target.php",
                r#"<?php
class AotRegisteredAutoloadTarget {
    public static function render(string $value): string {
        return 'aot-registered-autoload:' . $value;
    }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "aot-registered-autoload:ok");
}

/// Verifies an AOT callable array falls back to a runtime eval static method when the class is
/// declared after the compilation boundary instead of being retained as an AOT descriptor case.
#[test]
fn test_aot_callable_array_dispatches_eval_declared_static_class() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function invokeRuntimeStaticCallable(string $path): string
{
    include $path;
    $class = 'RuntimeStaticCallableTarget';

    if (!class_exists($class)) {
        return 'missing';
    }

    return call_user_func([$class, 'label']);
}

$path = $argc > 0 ? __DIR__.'/dynamic.php' : __DIR__.'/missing.php';
echo invokeRuntimeStaticCallable($path);
"#,
            ),
            (
                "dynamic.php",
                r#"<?php
class RuntimeStaticCallableTarget
{
    public static function label(): string
    {
        return 'ok';
    }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies an exception from a dynamically included static callable unwinds into the AOT
/// caller's PHP catch block without corrupting its native exception-handler frame.
#[test]
fn test_aot_callable_array_catches_dynamic_static_exception() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function invokeRuntimeStaticCallable(string $path): string
{
    include $path;
    $class = 'RuntimeThrowingStaticTarget';

    try {
        call_user_func([$class, 'fail']);
    } catch (RuntimeException $exception) {
        return 'caught';
    }

    return 'not-caught';
}

$path = $argc > 0 ? __DIR__.'/dynamic.php' : __DIR__.'/missing.php';
echo invokeRuntimeStaticCallable($path);
"#,
            ),
            (
                "dynamic.php",
                r#"<?php
class RuntimeThrowingStaticTarget
{
    public static function fail(): void
    {
        throw new RuntimeException('probe');
    }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "caught");
}

/// Verifies a large AOT frame can catch an exception from a dynamically included static
/// callable without reinterpreting boxed string cells as an unrelated native descriptor.
#[test]
fn test_large_aot_callable_array_catches_dynamic_static_exception() {
    let mut main = String::from("<?php\n\nfunction largeFrameDynamicTryCatch(string $path): string\n{\n");
    for index in 0..1600 {
        main.push_str(&format!("    $slot{index} = {index};\n"));
    }
    main.push_str(
        r#"    include $path;
    $class = 'RuntimeThrowingStaticTarget';

    try {
        call_user_func([$class, 'fail']);
    } catch (RuntimeException $exception) {
        return 'caught';
    }

    return 'not-caught';
}

echo largeFrameDynamicTryCatch(__DIR__.'/dynamic.php');
"#,
    );
    let out = compile_and_run_files(
        &[
            ("main.php", &main),
            (
                "dynamic.php",
                r#"<?php
class RuntimeThrowingStaticTarget
{
    public static function fail(): void
    {
        throw new RuntimeException('probe');
    }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "caught");
}

/// Verifies an AOT static-method callback survives SPL registration for a dynamic class call.
#[test]
fn test_aot_spl_autoload_registration_accepts_static_method_callback() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace AotSplStaticAutoloadProbe;

class Loader {
    public static function load(string $class): void {
        if ('AotStaticAutoloadTarget' === $class) {
            include 'target.php';
        }
    }
}

class Consumer {
    public function render(): string {
        return aot_static_autoload_function();
    }
}

spl_autoload_register([Loader::class, 'load'], true, true);
$path = 'bootstrap.php';
include $path;
echo (new Consumer())->render();
"#,
            ),
            (
                "bootstrap.php",
                r#"<?php
function aot_static_autoload_function(): string {
    return AotStaticAutoloadTarget::render('ok');
}
"#,
            ),
            (
                "target.php",
                r#"<?php
class AotStaticAutoloadTarget {
    public static function render(string $value): string { return 'aot-static-autoload:' . $value; }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "aot-static-autoload:ok");
}

/// Verifies an AOT loader's callback remains visible after that loader frame returns.
#[test]
fn test_aot_spl_autoload_registration_crosses_loader_context_boundary() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace AotSplAutoloadBoundaryProbe;

class Loader {
    public function register(): void { spl_autoload_register([$this, 'load']); }
    public function load(string $class): void {
        if ('AotBoundaryAutoloadTarget' === $class) { include 'target.php'; }
    }
}

class Consumer {
    public function render(): string { return aot_boundary_autoload_function(); }
}

(new Loader())->register();
$path = 'bootstrap.php';
include $path;
echo (new Consumer())->render();
"#,
            ),
            (
                "bootstrap.php",
                r#"<?php
function aot_boundary_autoload_function(): string {
    return AotBoundaryAutoloadTarget::render('ok');
}
"#,
            ),
            (
                "target.php",
                r#"<?php
class AotBoundaryAutoloadTarget {
    public static function render(string $value): string { return 'boundary-autoload:' . $value; }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "boundary-autoload:ok");
}

/// Verifies a later AOT autoloader is skipped after an earlier owner loads the requested class.
#[test]
fn test_aot_spl_autoload_stops_after_foreign_owner_loads_class() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace AotSplAutoloadStopProbe;

class Loader {
    public function register(): void { spl_autoload_register([$this, 'load']); }
    public function load(string $class): void {
        if ('AotAutoloadStopTarget' === $class) { include 'target.php'; }
    }
}

class FailingLoader {
    public static function load(string $class): void {
        throw new \RuntimeException('later loader must not run');
    }
}

(new Loader())->register();
spl_autoload_register([FailingLoader::class, 'load']);
$path = 'bootstrap.php';
include $path;
echo aot_autoload_stop_function();
"#,
            ),
            (
                "bootstrap.php",
                r#"<?php
function aot_autoload_stop_function(): string {
    return AotAutoloadStopTarget::render('ok');
}
"#,
            ),
            (
                "target.php",
                r#"<?php
class AotAutoloadStopTarget {
    public static function render(string $value): string { return 'autoload-stop:' . $value; }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "autoload-stop:ok");
}

/// Verifies a dynamically included function executes foreach short-array targets.
#[test]
fn test_dynamic_include_function_supports_foreach_array_destructure_target() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
class Consumer {
    public function render(): string { return foreach_destructure_dynamic_function(); }
}
$path = 'functions.php';
include $path;
echo (new Consumer())->render();
"#,
            ),
            (
                "functions.php",
                r#"<?php
function foreach_destructure_dynamic_function(): string {
    $out = '';
    foreach ([[1, 'a'], [2, 'b']] as [$id, $class]) {
        $out = $out . $id . $class;
    }
    return $out;
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "1a2b");
}

/// Verifies a signature-known compatibility function still resolves from a dynamic include.
///
/// Some names are known to the checker for type analysis yet deliberately have no native EIR
/// lowering because a PHP compatibility file provides them. Such a name must use the dynamic
/// function owner when runtime inclusion is present instead of becoming a language-construct
/// backend refusal.
#[test]
fn test_dynamic_include_signature_known_function_resolves_from_aot_namespace() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace DynamicIncludeSignatureKnownProbe;

class Consumer {
    public function render(string $value): string {
        return trigger_deprecation($value);
    }
}

$path = 'functions.php';
include $path;
echo (new Consumer())->render('ok');
"#,
            ),
            (
                "functions.php",
                r#"<?php
function trigger_deprecation(string $value): string {
    return 'signature-known-function:' . $value;
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "signature-known-function:ok");
}

/// Verifies a dynamic positional spread invokes a signature-known include-defined function.
///
/// Runtime spreads retain the original argument container until the dynamic owner applies PHP's
/// positional rules. This is the same shape used by compatibility helpers forwarding a stored
/// variadic argument list.
#[test]
fn test_dynamic_include_signature_known_function_accepts_runtime_spread() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace DynamicIncludeSignatureSpreadProbe;

class Consumer {
    public function render(): string {
        $arguments = ['left', 'right'];

        return trigger_deprecation(...$arguments);
    }
}

$path = 'functions.php';
include $path;
echo (new Consumer())->render();
"#,
            ),
            (
                "functions.php",
                r#"<?php
function trigger_deprecation(string ...$parts): string {
    return 'spread-function:' . $parts[0] . ':' . $parts[1];
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "spread-function:left:right");
}

/// Verifies a dynamic compatibility function can raise a silenced user deprecation.
///
/// The runtime body uses the same `@trigger_error(..., E_USER_DEPRECATED)` shape as common PHP
/// compatibility files. Error suppression must hide the diagnostic while preserving source-order
/// string formatting and the function's normal return to its AOT caller.
#[test]
fn test_dynamic_include_function_supports_silenced_trigger_error() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace DynamicIncludeTriggerErrorProbe;

class Consumer {
    public function render(): string {
        trigger_deprecation('package', '1.0', 'feature %s', 'name');

        return 'after';
    }
}

$path = 'functions.php';
include $path;
echo (new Consumer())->render();
"#,
            ),
            (
                "functions.php",
                r#"<?php
function trigger_deprecation(string $package, string $version, string $message, mixed ...$args): void {
    @trigger_error(($package || $version ? "Since $package $version: " : '') . ($args ? vsprintf($message, $args) : $message), E_USER_DEPRECATED);
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "after");
}

/// Verifies a native builtin stays on its AOT lowering path after a dynamic include barrier.
///
/// Dynamic function-owner fallback is for names without an implementation in the shared builtin
/// registry. Routing a registered builtin through Magician would hide its compiled semantics and
/// make an unrelated include change its behavior.
#[test]
fn test_dynamic_include_does_not_reroute_registered_builtin() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace DynamicIncludeBuiltinProbe;

class Consumer {
    public function render(): string {
        return basename('/fixture/config/services.php');
    }
}

$path = 'bootstrap.php';
include $path;
echo (new Consumer())->render();
"#,
            ),
            ("bootstrap.php", "<?php function dynamic_include_marker(): void {}"),
        ],
        "main.php",
    );
    assert_eq!(out, "services.php");
}

/// Verifies eval closure by-value captures snapshot the defining value for each call.
#[test]
fn test_eval_closure_by_value_capture_uses_snapshot() {
    let out = compile_and_run(
        r#"<?php
eval('$x = 1;
$fn = function($add) use ($x) { $x += $add; return $x; };
$x = 9;
echo $fn(1); echo ":";
echo $fn(2); echo ":";
echo $x;');
"#,
    );

    assert_eq!(out, "2:3:9");
}

/// Verifies eval closure by-reference captures write back to the defining scope.
#[test]
fn test_eval_closure_by_ref_capture_writes_back() {
    let out = compile_and_run(
        r#"<?php
eval('$x = 1;
$fn = function() use (&$x) { $x += 4; };
$fn();
echo $x;');
"#,
    );

    assert_eq!(out, "5");
}

/// Verifies eval closure literals are visible through ReflectionFunction metadata and invocation.
#[test]
fn test_eval_closure_reflection_function_metadata_and_invoke() {
    let out = compile_and_run(
        r#"<?php
eval('$seed = 4;
$fn = function($delta = 1) use ($seed) { return $seed + $delta; };
$ref = new ReflectionFunction($fn);
$staticFn = static function() {};
$staticRef = new ReflectionFunction($staticFn);
echo $ref->isClosure() ? "C" : "c"; echo ":";
echo $ref->isAnonymous() ? "A" : "a"; echo ":";
echo $ref->isStatic() ? "S" : "s"; echo ":";
echo $staticRef->isClosure() ? "C" : "c"; echo ":";
echo $staticRef->isStatic() ? "S" : "s"; echo ":";
$vars = $ref->getClosureUsedVariables();
echo count($vars); echo ":";
echo $vars["seed"]; echo ":";
echo $ref->invoke(3); echo ":";
echo $ref->invokeArgs(["delta" => 5]);');
"#,
    );

    assert_eq!(out, "C:A:s:C:S:1:4:7:9");
}

/// Verifies eval `Closure::call()` binds `$this` and passes later args by value.
#[test]
fn test_eval_closure_call_binds_this_and_uses_by_value_args() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalClosureCallBox {
    public int $base = 10;
}
$box = new EvalClosureCallBox();
$fn = function(int &$value, int $delta): int {
    $value = $value + $this->base + $delta;
    return $value;
};
$seed = "2";
echo $fn->call($box, $seed, 3);
echo ":";
echo gettype($seed);
echo ":";
echo $seed;');
"#,
    );

    assert_eq!(out, "15:string:2");
}

/// Verifies eval `call_user_func()` invokes closures with by-value argument semantics.
#[test]
fn test_eval_closure_call_user_func_uses_by_value_args() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalClosureCallUserFuncBox {
    public int $base = 10;
}

$fn = function(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
};
$first = "2";
echo call_user_func($fn, $first, 3);
echo ":" . gettype($first) . ":" . $first . "|";

$box = new EvalClosureCallUserFuncBox();
$method = function(int &$value, int $delta): int {
    $value = $value + $this->base + $delta;
    return $value;
};
$bound = $method->bindTo($box);
$second = "4";
echo call_user_func($bound, $second, 5);
echo ":" . gettype($second) . ":" . $second;');
"#,
    );

    assert_eq!(out, "5:string:2|19:string:4");
}

/// Verifies eval `call_user_func_array()` degrades non-reference closure args by value.
#[test]
fn test_eval_closure_call_user_func_array_degrades_non_ref_args() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalClosureCallUserFuncArrayBox {
    public int $base = 10;
}

$fn = function(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
};
$first = "2";
$firstArgs = [$first, 3];
echo call_user_func_array($fn, $firstArgs);
echo ":" . gettype($first) . ":" . $first;
echo ":" . gettype($firstArgs[0]) . ":" . $firstArgs[0] . "|";

$second = "4";
$secondArgs = [&$second, 5];
echo call_user_func_array($fn, $secondArgs);
echo ":" . gettype($second) . ":" . $second;
echo ":" . gettype($secondArgs[0]) . ":" . $secondArgs[0] . "|";

$box = new EvalClosureCallUserFuncArrayBox();
$method = function(int &$value, int $delta): int {
    $value = $value + $this->base + $delta;
    return $value;
};
$bound = $method->bindTo($box);
$third = "6";
$thirdArgs = [$third, 7];
echo call_user_func_array($bound, $thirdArgs);
echo ":" . gettype($third) . ":" . $third;
echo ":" . gettype($thirdArgs[0]) . ":" . $thirdArgs[0] . "|";

$fourth = "8";
$fourthArgs = [&$fourth, 9];
echo call_user_func_array($bound, $fourthArgs);
echo ":" . gettype($fourth) . ":" . $fourth;
echo ":" . gettype($fourthArgs[0]) . ":" . $fourthArgs[0];');
"#,
    );

    assert_eq!(
        out,
        "5:string:2:string:2|9:integer:9:integer:9|23:string:6:string:6|27:integer:27:integer:27"
    );
}

/// Verifies eval `Closure::bind()` and `bindTo()` persist `$this` across later calls.
#[test]
fn test_eval_closure_bind_persists_this_and_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalClosureBindBox {
    public int $base = 10;
}
$box = new EvalClosureBindBox();
$fn = function(int &$value, int $delta): int {
    $value = $value + $this->base + $delta;
    return $value;
};

$bound = $fn->bindTo($box);
$seed = "2";
echo is_object($bound) ? "O:" : "o:";
echo $bound($seed, 3) . ":" . gettype($seed) . ":" . $seed . "|";

$other = "4";
echo call_user_func_array($bound, [&$other, 5]) . ":" . gettype($other) . ":" . $other . "|";

$staticBound = Closure::bind($fn, $box);
$third = "6";
echo $staticBound($third, 7) . ":" . gettype($third) . ":" . $third . "|";

$static = static function() { return "bad"; };
echo is_null($static->bindTo($box)) ? "N" : "n";');
"#,
    );

    assert_eq!(out, "O:15:integer:15|19:integer:19|23:integer:23|N");
}

/// Verifies eval `Closure::bind()` and `bindTo()` honor explicit private-access scope.
#[test]
fn test_eval_closure_bind_honors_explicit_scope_and_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalClosureScopeBase {
    private int $secret = 40;
}

class EvalClosureScopeChild extends EvalClosureScopeBase {}

class EvalClosureScopeFactory {
    public function make() {
        return function(int &$value, int $delta): string {
            $value += $this->secret + $delta;
            return $value . ":" . get_called_class();
        };
    }
}

$fn = (new EvalClosureScopeFactory())->make();
$child = new EvalClosureScopeChild();

$bound = $fn->bindTo($child, "EvalClosureScopeBase");
$first = "1";
echo $bound($first, 1) . ":" . gettype($first) . ":" . $first . "|";

$staticBound = Closure::bind($fn, $child, "EvalClosureScopeBase");
$second = "2";
echo call_user_func_array($staticBound, [&$second, 2]) . ":" . gettype($second) . ":" . $second;');
"#,
    );

    assert_eq!(
        out,
        "42:EvalClosureScopeChild:integer:42|44:EvalClosureScopeChild:integer:44"
    );
}

/// Verifies eval Closure binding to `null` preserves explicit class scope and by-ref args.
#[test]
fn test_eval_closure_bind_null_receiver_preserves_explicit_scope_and_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalClosureNullScopeBox {
    private static int $secret = 40;
}

$fn = function(int &$value, int $delta): int {
    $value += self::$secret + $delta;
    return $value;
};

$bound = Closure::bind($fn, null, "EvalClosureNullScopeBox");
$first = "1";
echo $bound($first, 2) . ":" . gettype($first) . ":" . $first . "|";

$boundTo = $fn->bindTo(null, "EvalClosureNullScopeBox");
$second = "3";
echo call_user_func_array($boundTo, [&$second, 4]) . ":" . gettype($second) . ":" . $second;');
"#,
    );

    assert_eq!(out, "43:integer:43|47:integer:47");
}

/// Verifies eval Closure `__invoke` works as an array callable and preserves by-ref args.
#[test]
fn test_eval_closure_invoke_array_callable_preserves_by_ref_args() {
    let out = compile_and_run_capture(
        r#"<?php
eval('$fn = function(int &$value, int $delta): int {
    $value += $delta;
    return $value;
};
$callback = [$fn, "__invoke"];
echo is_callable($callback) ? "C:" : "c:";

$first = "2";
echo $callback($first, 3) . ":" . gettype($first) . ":" . $first . "|";

$second = "4";
echo call_user_func_array($callback, [&$second, 5]) . ":" . gettype($second) . ":" . $second . "|";

$fromCallable = Closure::fromCallable($callback);
$third = "6";
echo $fromCallable($third, 7) . ":" . gettype($third) . ":" . $third . "|";

$fourth = "8";
echo $fn->__invoke($fourth, 9) . ":" . gettype($fourth) . ":" . $fourth;');
"#,
    );

    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "C:5:integer:5|9:integer:9|13:integer:13|17:integer:17"
    );
}

/// Verifies `call_user_func()` on an interpreted closure keeps the `$this` and class scope the
/// closure was declared with, exactly as a direct `$closure()` call does.
///
/// The callable path ran a closure with no binding at all when none was explicitly given, so
/// Laravel's `call_user_func($default, ...)` over `fn () => ImplicitRouteBinding::
/// resolveForRoute($this->container, $route)` warned "Undefined variable $this".
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_eval_call_user_func_keeps_a_closure_declaring_this() {
    let out = compile_and_run(
        r#"<?php
class Holder {
    public static function make($container, $route) { return "made " . $container . $route; }
}
eval('class R {
    public $c = "C";
    public $cb = null;
    public function f($route) {
        $default = fn () => Holder::make($this->c, $route);
        return call_user_func($default, 1) . "|" . call_user_func($this->cb ?? $default) . "|"
            . ($this->cb ?? $default)() . "|" . $default(1, 2);
    }
}');
$name = 'R';
$r = new $name();
echo $r->f("/x"), "\n";
"#,
    );
    assert_eq!(out, "made C/x|made C/x|made C/x|made C/x\n");
}

/// Compiled `new ReflectionFunction($closure)` reflects a closure an eval'd function created and
/// handed over, then the closure still runs.
///
/// The closure is registered only with the eval context that made it; the reflection ran in a
/// context of its own, found no closure there, and read the object as a function NAME: "eval()
/// fragment uses an unsupported construct". Laravel reaches this at the end of every request,
/// when the container reflects each `terminating()` callback a provider registered.
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_compiled_reflection_of_a_closure_another_context_created() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public $callbacks = [];
    public function describe($i) {
        $callback = $this->callbacks[$i];
        $reflector = new ReflectionFunction($callback);
        $names = [];
        foreach ($reflector->getParameters() as $parameter) {
            $names[] = $parameter->getName();
        }
        echo count($names), ':', implode(',', $names), ' => ', $callback(...array_fill(0, count($names), 3)), "\n";
    }
}
$code = 'function register_callbacks($box) {
    $box->callbacks[] = static function () { return "none"; };
    $box->callbacks[] = function ($x, $y) { return $x * $y; };
}';
eval($code);
$box = new Box();
register_callbacks($box);
$box->describe(0);
$box->describe(1);
"#,
    );
    assert_eq!(out, "0: => none\n2:x,y => 9\n");
}

/// Verifies `ReflectionFunction` over compiled closure literals reports the file and the full
/// line range of the literal: a multi-line function closure, an arrow fn, and a method's closure.
#[test]
fn test_reflection_function_of_compiled_closure_reports_file_and_lines() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class K {
    public function m() {
        return function () {
        };
    }
}
function show($label, $c) {
    $r = new ReflectionFunction($c);
    $file = (string) $r->getFileName();
    echo $label, " ", $file === __FILE__ ? "main" : "other", " ", $r->getStartLine(), "-", $r->getEndLine(), "\n";
}
$f = function ($x) {
    return $x + 1;
};
$g = fn($y) => $y * 2;
$h = (new K())->m();
show("function", $f);
show("arrow", $g);
show("method", $h);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "function main 13-15\narrow main 16-16\nmethod main 4-5\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "closure reflection leaked: {}",
        out.stderr
    );
}

/// Verifies closures created by eval reflect php's `... : eval()'d code` file and their line
/// range inside the fragment.
#[test]
fn test_reflection_function_of_eval_closure_reports_eval_file_and_lines() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function show($label, $c) {
    $r = new ReflectionFunction($c);
    $file = (string) $r->getFileName();
    echo $label, " ", str_contains($file, "eval()'d code") ? "eval" : "other", " ", $r->getStartLine(), "-", $r->getEndLine(), "\n";
}
$e = eval('return function ($a) {
    return $a;
};');
show("eval", $e);
$e2 = eval('return fn($z) => $z;');
show("eval-arrow", $e2);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "eval eval 1-3\neval-arrow eval 1-1\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "closure reflection leaked: {}",
        out.stderr
    );
}

/// Verifies a closure from a runtime-included file reflects that file and its real lines.
///
/// No leak assertion: any `include` at runtime currently ends with two live blocks (the same
/// with `return 5;` as the file's body), independent of closures or Reflection.
#[test]
fn test_reflection_function_of_included_closure_reports_included_file_and_lines() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function show($label, $c) {
    $r = new ReflectionFunction($c);
    $file = (string) $r->getFileName();
    echo $label, " ", str_contains($file, "rf_inc_") ? "inc" : "other", " ", $r->getStartLine(), "-", $r->getEndLine(), "\n";
}
$file = sys_get_temp_dir() . "/rf_inc_" . getmypid() . ".php";
file_put_contents($file, "<?php\n\n\nreturn function () {\n    return 1;\n};\n");
$i = include $file;
show("include", $i);
unlink($file);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "include inc 4-6\n");
}
