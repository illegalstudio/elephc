//! Purpose:
//! Regression coverage for native methods first reached from eval-owned code.
//!
//! Called from:
//! - `tests/codegen/mod.rs` through the codegen integration suite.

use crate::support::*;

/// A public method called solely by an eval fragment remains executable in the
/// compiled class instead of being metadata-only.
#[test]
fn test_eval_can_invoke_a_native_method_without_a_static_call_site() {
    let out = compile_and_run(
        r#"<?php
class EvalNativeReachability {
    public function dispatch(object $event): object {
        $event->answer = "ok";
        return $event;
    }
}

$dispatcher = new EvalNativeReachability();
echo eval('$event = $dispatcher->dispatch(new stdClass()); return $event->answer;');
"#,
    );

    assert_eq!(out, "ok");
}

/// A closure declared by eval retains its declaring class scope after another eval class invokes
/// it. Without that scope, the protected read is checked against the invoker rather than the
/// factory class that declared the closure.
#[test]
fn test_eval_closure_keeps_declaring_scope_for_protected_property_access() {
    let out = compile_and_run(
        r#"<?php
$source = <<<'PHP'
class ProtectedScopeBase {
    protected string $value = 'ok';
}

class ProtectedScopeFactory extends ProtectedScopeBase {
    public function callback() {
        $owner = $this;

        return static function () use ($owner): string {
            return $owner->value;
        };
    }
}

class ProtectedScopeInvoker {
    public function invoke($callback): string {
        return $callback();
    }
}

$factory = new ProtectedScopeFactory();
$callback = $factory->callback();

return (new ProtectedScopeInvoker())->invoke($callback);
PHP;

echo eval($source);
"#,
    );

    assert_eq!(out, "ok");
}

/// A closure retains its declaration file after a runtime include returns. `__DIR__` must name
/// the included file's directory rather than the caller that invokes the closure later.
#[test]
fn test_eval_closure_keeps_declaring_file_for_magic_dir() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$path = $argc > 99 ? 'missing.php' : __DIR__.'/nested/factory.php';
include $path;

$callback = (new ClosureSourceFactory())->callback();
echo $callback();
"#,
            ),
            (
                "nested/factory.php",
                r#"<?php
class ClosureSourceFactory {
    public function callback() {
        return static fn (): string => basename(__DIR__);
    }
}
"#,
            ),
        ],
        "main.php",
    );

    assert_eq!(out, "nested");
}

/// Nested by-reference foreach loops keep their element targets while a closure is appended by
/// reference to another property-backed array. This is the minimal dynamic form of a listener
/// registry that compacts callbacks after priority sorting.
#[test]
fn test_eval_nested_foreach_references_survive_property_append_closure() {
    let out = compile_and_run(
        r#"<?php
$source = <<<'PHP'
class DynamicListenerRegistry {
    private array $listeners = [];
    private array $optimized = [];

    public function add(string $event, int $priority, mixed $listener): void {
        $this->listeners[$event][$priority][] = $listener;
    }

    public function optimize(string $event): int {
        krsort($this->listeners[$event]);
        $this->optimized[$event] = [];

        foreach ($this->listeners[$event] as &$listeners) {
            foreach ($listeners as &$listener) {
                $closure = &$this->optimized[$event][];
                $closure = static function () use (&$listener, &$closure): void {};
            }
        }

        return count($this->optimized[$event]);
    }
}

$registry = new DynamicListenerRegistry();
$registry->add('event', 0, 1);
$registry->add('event', 100, 2);

return $registry->optimize('event');
PHP;

echo eval($source);
"#,
    );

    assert_eq!(out, "2");
}

/// An eval-declared class extending a native exception class (`\LogicException`) must declare
/// successfully and behave like PHP: the constructor accepts message/code, the subclass is
/// catchable by its native parent's type, and the inherited native accessors work.
///
/// Nothing in the AOT-compiled program calls `LogicException::getMessage()`/`getCode()`
/// directly -- only the eval fragment does -- so their EIR bodies are pruned as unreachable by
/// the same dead-code pass `test_eval_can_invoke_a_native_method_without_a_static_call_site`
/// above exercises for a user class. Reachability metadata used to double as the eval
/// reflection bridge's ABSTRACT bit for AOT class methods, so a native ancestor method missing
/// only its emitted symbol was reported abstract, and every native-based eval subclass failed
/// `class X could not be declared` at declaration time.
///
/// `php -n` 8.5.6 (`scratchpad/verify_native_ext.php`, this session): `caught:boom:42`.
#[test]
fn test_eval_class_can_extend_a_native_exception_and_be_caught_by_its_parent_type() {
    let out = compile_and_run(
        r#"<?php
$source = <<<'PHP'
class EvalLogicSub extends \LogicException {}

try {
    throw new EvalLogicSub("boom", 42);
} catch (\LogicException $e) {
    echo "caught:" . $e->getMessage() . ":" . $e->getCode();
}
PHP;
echo eval($source);
"#,
    );

    assert_eq!(out, "caught:boom:42");
}

/// An eval-declared class extending a native exception class and defining its OWN constructor
/// (constructor-promoted property included) must be able to call `parent::__construct(...)` on
/// the native ancestor. This is the exact shape of Symfony's
/// `Dotenv\Exception\FormatException extends \LogicException`, which gated the whole `--web`
/// campaign: `final class FormatException extends \LogicException implements ExceptionInterface`
/// with a promoted `FormatExceptionContext $context` parameter and a `parent::__construct(...)`
/// call built from `sprintf(...)`.
///
/// `parent::__construct()` static-call syntax has no entry in the eval reflection method index
/// when the resolved parent is native -- `collect_eval_native_instance_methods` (codegen) skips
/// `__construct` on purpose, since allocation-time `new X()` dispatch uses a separate constructor
/// registration table keyed for that purpose. Every generic static-dispatch branch therefore fell
/// through to "undefined method" for a native `parent::__construct()` call specifically.
///
/// `php -n` 8.5.6 (`scratchpad/verify_parent_construct.php`, this session): `bad in
/// "/tmp/x":0:is`.
#[test]
fn test_eval_class_extending_a_native_exception_can_call_native_parent_construct() {
    let out = compile_and_run(
        r#"<?php
$source = <<<'PHP'
class EvalCtx {
    public function p(): string { return '/tmp/x'; }
}

final class EvalFormatLike extends \LogicException {
    public function __construct(string $message, private EvalCtx $context, int $code = 0, ?\Throwable $previous = null) {
        parent::__construct(sprintf('%s in "%s"', $message, $context->p()), $code, $previous);
    }
}

$e = new EvalFormatLike('bad', new EvalCtx());
echo $e->getMessage(), ':', $e->getCode(), ':', ($e instanceof \LogicException ? 'is' : 'not'), "\n";
PHP;
echo eval($source);
"#,
    );

    assert_eq!(out, "bad in \"/tmp/x\":0:is\n");
}

/// The exact shape of Symfony's `Dotenv\Exception\FormatException`: an eval-declared class
/// extends a native exception AND implements an interface that itself extends `\Throwable`
/// (`Symfony\Component\Dotenv\Exception\ExceptionInterface extends \Throwable`).
///
/// PHP only rejects `implements Throwable` ("Class X cannot implement interface Throwable,
/// extend Exception or Error instead", `php -n` 8.5.6) when the class does not already inherit
/// Throwable from its parent. `validate_eval_class_does_not_implement_throwable_interfaces`
/// rejected this UNCONDITIONALLY whenever any implemented interface transitively reached
/// `Throwable`, without checking whether the class's own parent already supplied it -- so every
/// Symfony-shaped exception subclass (parent native, own interface re-stating Throwable) failed
/// to declare, one stage past the abstract-parent-requirements gate this file's other tests fix.
///
/// `php -n` 8.5.6 (`scratchpad/verify_full_symfony_shape.php`, this session):
/// `bad in "/tmp/x":/tmp/x:is:is`.
#[test]
fn test_eval_class_extending_native_and_implementing_a_throwable_interface_declares() {
    let out = compile_and_run(
        r#"<?php
$source = <<<'PHP'
interface MyDotenvExceptionInterface extends \Throwable {}

final class MyFormatException extends \LogicException implements MyDotenvExceptionInterface
{
    public function __construct(string $message, private string $context, int $code = 0, ?\Throwable $previous = null)
    {
        parent::__construct(sprintf('%s in "%s"', $message, $context), $code, $previous);
    }

    public function getContext(): string
    {
        return $this->context;
    }
}

$e = new MyFormatException('bad', '/tmp/x');
echo $e->getMessage(), ':', $e->getContext(), ':', ($e instanceof MyDotenvExceptionInterface ? 'is' : 'not'), ':', ($e instanceof \Throwable ? 'is' : 'not'), "\n";
PHP;
echo eval($source);
"#,
    );

    assert_eq!(out, "bad in \"/tmp/x\":/tmp/x:is:is\n");
}

/// A STRING-KEYED array forwarded through a compiled method's declared `array` parameter keeps
/// its keys when the callee is an eval-declared override.
///
/// The compiled caller passes such a parameter by reference marker: a frame address plus the tag
/// its STATIC type spells, and `array` spells the packed tag whatever the slot actually holds.
/// The interpreter rebuilt the value from that tag, so a hash arrived as a packed array -- every
/// key gone, element 0 reading back as the raw slot word. The heap kind on the payload is the
/// only honest source for the shape.
///
/// `php -n` 8.5.6 (`scratchpad/hasharg2.php`, this session) answers `[seed=G zero=no0 n=1
/// keys=seed]` for all three rows.
#[test]
fn test_a_string_keyed_array_keeps_its_keys_through_a_declared_array_parameter() {
    let out = compile_and_run(
        r#"<?php
abstract class HashArgBase
{
    public function viaTyped(array $ctx): string { return $this->show($ctx); }
    public function viaUntyped($ctx): string { return $this->show($ctx); }

    abstract public function show(array $ctx): string;
}

eval('class HashArgImpl extends HashArgBase {
    public function show(array $ctx): string {
        return "[seed=" . ($ctx["seed"] ?? "MISSING")
            . " zero=" . ($ctx[0] ?? "no0")
            . " n=" . count($ctx)
            . " keys=" . implode(",", array_keys($ctx)) . "]";
    }
}');

$impl = new HashArgImpl();
echo $impl->show(['seed' => 'G']), "\n";
echo $impl->viaTyped(['seed' => 'G']), "\n";
echo $impl->viaUntyped(['seed' => 'G']), "\n";
"#,
    );

    assert_eq!(
        out,
        "[seed=G zero=no0 n=1 keys=seed]\n\
         [seed=G zero=no0 n=1 keys=seed]\n\
         [seed=G zero=no0 n=1 keys=seed]\n"
    );
}

/// An argument a COMPILED caller owns survives until an eval-declared GENERATOR method runs.
///
/// The generator's body does not execute during the call that creates it, so its scope must own
/// its by-value parameters. `retain_generator_scope_args` retained only the arguments the
/// INTERPRETER had marked owned, and a compiled caller marks every argument it packs unowned --
/// it releases the boxed cells itself the moment the bridge returns. The generator then read a
/// freed cell on its first resume, which came back as an empty array.
///
/// `php -n` 8.5.6 (`scratchpad/gencross8.php`, this session) answers `[./1]` for all three rows.
#[test]
fn test_a_compiled_argument_outlives_the_call_that_creates_an_eval_generator() {
    let out = compile_and_run(
        r#"<?php
abstract class GenArgBase
{
    public function literal(): string { return $this->drain($this->emit(['seed' => '.'])); }

    public function merged(array $context): string
    {
        $context += ['seed' => '.'];

        return $this->drain($this->emit($context));
    }

    public function mergedLocal(array $context): string
    {
        $merged = $context + ['seed' => '.'];

        return $this->drain($this->emit($merged));
    }

    private function drain(iterable $it): string
    {
        $out = '';
        $n = 0;
        foreach ($it as $chunk) {
            $out .= $chunk;
            if (++$n >= 4) { $out .= '...CUT'; break; }
        }

        return $out;
    }

    abstract public function emit(array $context): iterable;
}

eval('class GenArgLeaf extends GenArgBase {
    public function emit(array $context): iterable {
        yield "[" . ($context["seed"] ?? "MISSING") . "/" . count($context) . "]";
    }
}');

$leaf = new GenArgLeaf();
echo $leaf->literal(), "\n";
echo $leaf->merged([]), "\n";
echo $leaf->mergedLocal([]), "\n";
"#,
    );

    assert_eq!(out, "[./1]\n[./1]\n[./1]\n");
}

/// Verifies a Throwable from an eval-declared GENERATOR reaches the compiled `foreach` driving it.
///
/// A compiled loop drives such a generator through the `__rt_gen_*` helpers, which probe for an
/// eval owner and land in the generator-protocol bridge. That bridge answered "handled, null" for
/// every failed step on the assumption that a bridge frame above would report the Throwable it
/// left on the eval context -- but the caller here is compiled code, and there is no such frame.
/// The exception vanished and the loop spun forever on a generator that could neither advance nor
/// say why. It now hands the Throwable back for native unwinding, as the dynamic-callable invoker
/// already did.
///
/// `php -n` 8.5.6 (`scratchpad/throwcross.php`, this session) answers all four rows below.
#[test]
fn test_a_throw_from_an_eval_generator_reaches_the_compiled_foreach() {
    let out = compile_and_run(
        r#"<?php
abstract class ThrowGenBase
{
    public function drain(): string
    {
        $out = '';
        foreach ($this->gen() as $chunk) {
            $out .= $chunk;
        }

        return $out;
    }

    public function drainGuarded(): string
    {
        try {
            return $this->drain();
        } catch (Throwable $e) {
            throw new RuntimeException('wrapped: ' . $e->getMessage(), 0, $e);
        }
    }

    abstract public function gen(): iterable;
}

eval('class ThrowGenLeaf extends ThrowGenBase { public function gen(): iterable { yield "a"; throw new LogicException("late"); } }');

$leaf = new ThrowGenLeaf();

try {
    echo 'gen     : ', $leaf->drain(), "\n";
} catch (Throwable $e) {
    echo 'gen     : caught ', get_class($e), ': ', $e->getMessage(), "\n";
}

try {
    echo 'genwrap : ', $leaf->drainGuarded(), "\n";
} catch (Throwable $e) {
    echo 'genwrap : caught ', get_class($e), ': ', $e->getMessage(), "\n";
}
"#,
    );

    // The label is printed twice on purpose: `echo 'gen : ', $leaf->drain()` emits the first
    // operand before `drain()` throws, and the catch prints its own. `php -n` does the same.
    assert_eq!(
        out,
        "gen     : gen     : caught LogicException: late\n\
         genwrap : genwrap : caught RuntimeException: wrapped: late\n"
    );
}

/// Verifies a PROTECTED method declared by a compiled ancestor is reachable between SIBLINGS.
///
/// php checks a protected method against the topmost ancestor whose declaration the override
/// replaces (`zend_get_function_root_class`), not against the class carrying the concrete body.
/// elephc checked the concrete class, so one eval subclass calling the abstract protected method
/// on ANOTHER eval subclass of the same compiled parent was refused with
/// `Call to protected method BaseTpl::genDisplay() from scope ChildTpl`. Twig does exactly this:
/// `Twig\Template::yield()` calls `$this->doDisplay()` on a template whose class is a sibling of
/// the caller's, and every render died on it.
///
/// `php -n` 8.5.6 (`scratchpad/protscope2.php`, this session): `direct  : base-gen` /
/// `nested  : child(base-gen)`.
#[test]
fn test_a_protected_method_of_a_compiled_parent_is_reachable_between_eval_siblings() {
    let out = compile_and_run(
        r#"<?php
abstract class SiblingBase
{
    public function driveGen(array $context): iterable
    {
        yield from $this->genDisplay($context);
    }

    public function drainGen(array $context): string
    {
        $out = '';
        $n = 0;
        foreach ($this->driveGen($context) as $chunk) {
            $out .= $chunk;
            if (++$n >= 8) { $out .= '...CUT'; break; }
        }

        return $out;
    }

    abstract protected function genDisplay(array $context): iterable;
}

eval('class SiblingLeaf extends SiblingBase {
    protected function genDisplay(array $context): iterable { yield "base-gen"; yield from []; }
}');

eval('class SiblingOuter extends SiblingBase {
    protected function genDisplay(array $context): iterable {
        $parent = new SiblingLeaf();
        yield "child(";
        yield from $parent->driveGen($context);
        yield ")";
        yield from [];
    }
}');

echo 'direct : ', (new SiblingLeaf())->drainGen([]), "\n";
echo 'nested : ', (new SiblingOuter())->drainGen([]), "\n";
"#,
    );

    assert_eq!(out, "direct : base-gen\nnested : child(base-gen)\n");
}

/// Verifies a STRING-KEYED array survives a compiled `array` parameter into a DYNAMIC-name call.
///
/// `$target->$method($ctx)` is lowered as a descriptor-invoker call, which passes each local as a
/// by-reference marker carrying the tag its STATIC type spells. `array` spells the PACKED tag
/// whatever the slot holds, so a hash arrived with no keys and its element 0 reading back as the
/// raw slot word. `__rt_array_kind_tag` asks the payload instead. Twig's `yieldBlock` reaches
/// every template block through exactly this call shape, so the render context arrived empty.
///
/// `php -n` 8.5.6 (`scratchpad/dyngen2.php`, this session) answers `[G/1/seed]` for every row.
#[test]
fn test_a_string_keyed_array_survives_a_dynamic_name_call_through_an_array_parameter() {
    let out = compile_and_run(
        r#"<?php
class DynCaller
{
    public function drive(object $target, string $method, array $ctx): string
    {
        return $target->$method($ctx);
    }

    public function driveGen(object $target, string $method, array $ctx): iterable
    {
        yield $target->$method($ctx);
    }

    public function drivePair(array $pair, array $ctx): string
    {
        $target = $pair[0];
        $method = $pair[1];

        return $target->$method($ctx);
    }
}

eval('class DynLeaf {
    public function show(array $ctx): string {
        return "[" . ($ctx["seed"] ?? "MISSING") . "/" . count($ctx) . "/" . implode(",", array_keys($ctx)) . "]";
    }
}');

$caller = new DynCaller();
$leaf = new DynLeaf();

echo 'plain : ', $caller->drive($leaf, 'show', ['seed' => 'G']), "\n";
foreach ($caller->driveGen($leaf, 'show', ['seed' => 'G']) as $chunk) {
    echo 'gen   : ', $chunk, "\n";
}
echo 'pair  : ', $caller->drivePair([$leaf, 'show'], ['seed' => 'G']), "\n";
"#,
    );

    assert_eq!(
        out,
        "plain : [G/1/seed]\ngen   : [G/1/seed]\npair  : [G/1/seed]\n"
    );
}
