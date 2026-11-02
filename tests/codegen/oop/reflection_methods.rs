//! Purpose:
//! End-to-end codegen tests for ReflectionMethod invocation paths over AOT
//! class metadata.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `ReflectionMethod::invoke()` and `invokeArgs()` are lowered only for statically-known
//!   reflectors whose target method has declared parameter types.
//! - Tests cover inline constructors, ReflectionClass lookup, local tracking,
//!   case-insensitive method names, default values, and named arguments.

use super::*;

/// Verifies AOT `ReflectionMethod` exposes function-abstract predicate metadata.
#[test]
fn test_reflection_method_reports_aot_function_abstract_predicates() {
    let out = compile_and_run(
        r#"<?php
class ReflectMethodAbstractPredicateTarget {
    #[Deprecated]
    public function deprecated(): void {}
    public function generator() { yield 1; }
    public function plain(): void {}
}

$deprecated = new ReflectionMethod(ReflectMethodAbstractPredicateTarget::class, "deprecated");
$generator = new ReflectionMethod(ReflectMethodAbstractPredicateTarget::class, "generator");
$plain = new ReflectionMethod(ReflectMethodAbstractPredicateTarget::class, "plain");
$listed = (new ReflectionClass(ReflectMethodAbstractPredicateTarget::class))->getMethod("generator");
echo ($deprecated->isDeprecated() ? "D" : "d") . ":";
echo ($plain->isDeprecated() ? "D" : "d") . ":";
echo ($generator->isGenerator() ? "G" : "g") . ":";
echo ($listed->isGenerator() ? "L" : "l") . ":";
echo ($plain->isGenerator() ? "G" : "g") . ":";
echo ($plain->isClosure() ? "C" : "c") . ":";
echo ($plain->returnsReference() ? "R" : "r") . ":";
echo ($plain->hasTentativeReturnType() ? "H" : "h") . ":";
echo $plain->getTentativeReturnType() === null ? "Q" : "q";
"#,
    );
    assert_eq!(out, "D:d:G:L:g:c:r:h:Q");
}

/// Verifies `ReflectionMethod` exposes declared AOT return type metadata.
#[test]
fn test_reflection_method_reports_aot_return_type_metadata() {
    let out = compile_and_run(
        r#"<?php
class ReflectMethodReturnDep {}
class ReflectMethodReturnTarget {
    public function named(string $value): ?string { return $value; }
    public static function factory(): ReflectMethodReturnDep { return new ReflectMethodReturnDep(); }
    public function plain() {}
}

$namedRef = new ReflectionMethod(ReflectMethodReturnTarget::class, "named");
$named = $namedRef->getReturnType();
echo ($namedRef->hasReturnType() ? "T" : "t") . ":";
echo $named->getName() . ":";
echo ($named->allowsNull() ? "N" : "n") . ":";
echo ($named->isBuiltin() ? "B" : "b") . ":";
$declaring = $namedRef->getParameters()[0]->getDeclaringFunction()->getReturnType();
echo $declaring->getName() . ":";
$static = (new ReflectionClass(ReflectMethodReturnTarget::class))->getMethod("factory")->getReturnType();
echo $static->getName() . ":";
echo ($static->allowsNull() ? "N" : "n") . ":";
echo ($static->isBuiltin() ? "B" : "b") . ":";
$plain = new ReflectionMethod(ReflectMethodReturnTarget::class, "plain");
echo ($plain->hasReturnType() ? "P" : "p") . ":";
echo $plain->getReturnType() === null ? "Q" : "q";
"#,
    );
    assert_eq!(out, "T:string:N:B:string:ReflectMethodReturnDep:n:b:p:Q");
}

/// Verifies `ReflectionMethod::isVariadic()` reports the method-level variadic flag.
#[test]
fn test_reflection_method_reports_aot_variadic_flag() {
    let out = compile_and_run(
        r#"<?php
class ReflectMethodVariadicTarget {
    public function variadic(string $head, string ...$tail): void {}
    public function fixed(string $head): void {}
}

$variadic = new ReflectionMethod(ReflectMethodVariadicTarget::class, "variadic");
$fixed = new ReflectionMethod(ReflectMethodVariadicTarget::class, "fixed");
echo ($variadic->isVariadic() ? "V" : "v") . ":";
echo $variadic->getNumberOfParameters() . ":";
echo ($fixed->isVariadic() ? "V" : "v");
"#,
    );
    assert_eq!(out, "V:2:v");
}

/// Verifies `ReflectionMethod` exposes AOT method name and origin metadata.
#[test]
fn test_reflection_method_reports_aot_name_origin_predicates() {
    let out = compile_and_run(
        r#"<?php
namespace ReflectMethodMetaNs;

class Target {
    public function run(): void {}
}

$method = new \ReflectionMethod(Target::class, "run");
echo $method->getName() . ":";
echo $method->getShortName() . ":";
echo $method->getNamespaceName() . ":";
echo ($method->inNamespace() ? "Y" : "N") . ":";
echo ($method->isInternal() ? "I" : "i") . ":";
echo $method->isUserDefined() ? "U" : "u";
"#,
    );
    assert_eq!(out, "run:run::N:i:U");
}

/// Verifies AOT `ReflectionMethod::hasPrototype()` and `getPrototype()` follow PHP inheritance rules.
#[test]
fn test_reflection_method_reports_aot_prototypes() {
    let out = compile_and_run(
        r#"<?php
interface ReflectMethodProtoParentIface {
    public function parented(): void;
}
interface ReflectMethodProtoChildIface extends ReflectMethodProtoParentIface {}
interface ReflectMethodProtoIface {
    public function iface(): void;
}
class ReflectMethodProtoBase {
    public function run(): void {}
    public function inherited(): void {}
}
class ReflectMethodProtoChild extends ReflectMethodProtoBase implements ReflectMethodProtoIface, ReflectMethodProtoChildIface {
    public function run(): void {}
    public function iface(): void {}
    public function parented(): void {}
    public function own(): void {}
}

$override = new ReflectionMethod(ReflectMethodProtoChild::class, "run");
$overrideProto = $override->getPrototype();
echo ($override->hasPrototype() ? "Y" : "N") . ":";
echo $overrideProto->getDeclaringClass()->getName() . "::";
echo $overrideProto->getName() . ":";
$iface = (new ReflectionClass(ReflectMethodProtoChild::class))->getMethod("iface");
$ifaceProto = $iface->getPrototype();
echo ($iface->hasPrototype() ? "Y" : "N") . ":";
echo $ifaceProto->getDeclaringClass()->getName() . "::";
echo $ifaceProto->getName() . ":";
$parentIface = new ReflectionMethod(ReflectMethodProtoChild::class, "parented");
$parentIfaceProto = $parentIface->getPrototype();
echo $parentIfaceProto->getDeclaringClass()->getName() . "::";
echo $parentIfaceProto->getName() . ":";
$own = new ReflectionMethod(ReflectMethodProtoChild::class, "own");
echo ($own->hasPrototype() ? "Y" : "N") . ":";
try {
    $own->getPrototype();
} catch (ReflectionException $e) {
    echo "E";
}
echo ":";
$inherited = new ReflectionMethod(ReflectMethodProtoChild::class, "inherited");
echo $inherited->hasPrototype() ? "Y" : "N";
"#,
    );
    assert_eq!(
        out,
        "Y:ReflectMethodProtoBase::run:Y:ReflectMethodProtoIface::iface:ReflectMethodProtoParentIface::parented:N:E:N"
    );
}

/// Verifies `ReflectionMethod::invoke()` calls declared AOT instance and static methods.
#[test]
fn test_reflection_method_invoke_calls_declared_aot_methods() {
    let out = compile_and_run(
        r#"<?php
class ReflectInvokeTarget {
    public function join(string $a, string $b = "B"): string { return $a . $b; }
    public function zero(): string { return "Z"; }
    public static function make(string $left, string $right = "S"): string { return $left . $right; }
    public static function staticZero(): string { return "T"; }
}

$object = new ReflectInvokeTarget();
echo (new ReflectionMethod(ReflectInvokeTarget::class, "join"))->invoke($object, "A", "C");
echo ":";
echo (new ReflectionMethod(ReflectInvokeTarget::class, "JOIN"))->invoke($object, "D");
echo ":";
echo (new ReflectionClass(ReflectInvokeTarget::class))->getMethod("join")->invoke($object, "E", "F");
echo ":";
echo (new ReflectionMethod(ReflectInvokeTarget::class, "make"))->invoke(null, right: "Y", left: "X");
echo ":";
$method = new ReflectionMethod(ReflectInvokeTarget::class, "join");
echo $method->invoke($object, "L", "M");
echo ":";
echo (new ReflectionMethod(ReflectInvokeTarget::class, "zero"))->invoke($object);
echo ":";
echo (new ReflectionMethod(ReflectInvokeTarget::class, "staticZero"))->invoke(null);
"#,
    );
    assert_eq!(out, "AC:DB:EF:XY:LM:Z:T");
}

/// Verifies `ReflectionMethod::invokeArgs()` forwards static argument arrays.
#[test]
fn test_reflection_method_invoke_args_calls_declared_aot_methods() {
    let out = compile_and_run(
        r#"<?php
class ReflectInvokeArgsTarget {
    public function join(string $left, string $right = "B"): string { return $left . $right; }
    public static function make(string $left, string $right = "S"): string { return $left . $right; }
}

$object = new ReflectInvokeArgsTarget();
echo (new ReflectionMethod(ReflectInvokeArgsTarget::class, "join"))->invokeArgs($object, ["right" => "Y", "left" => "X"]);
echo ":";
echo (new ReflectionMethod(ReflectInvokeArgsTarget::class, "JOIN"))->invokeArgs($object, ["Q"]);
echo ":";
echo (new ReflectionMethod(ReflectInvokeArgsTarget::class, "make"))->invokeArgs(null, ["right" => "N", "left" => "M"]);
echo ":";
echo (new ReflectionClass(ReflectInvokeArgsTarget::class))->getMethod("join")->invokeArgs(object: $object, args: ["left" => "L"]);
echo ":";
$method = new ReflectionMethod(ReflectInvokeArgsTarget::class, "join");
echo $method->invokeArgs(...[$object, ["A", "C"]]);
echo ":";
$localArgs = ["right" => "P", "left" => "O"];
echo (new ReflectionMethod(ReflectInvokeArgsTarget::class, "join"))->invokeArgs($object, $localArgs);
"#,
    );
    assert_eq!(out, "XY:QB:MN:LB:AC:OP");
}

/// Verifies constructors returned by `ReflectionClass::getConstructor()` can be invoked.
#[test]
fn test_reflection_method_invoke_calls_aot_constructor_from_reflection_class() {
    let out = compile_and_run(
        r#"<?php
class ReflectInvokeCtorTarget {
    public string $label = "";
    public function __construct(string $left, string $right = "B") {
        $this->label = $left . $right;
    }
    public function label(): string {
        return $this->label;
    }
}

$object = new ReflectInvokeCtorTarget("A", "A");
$result = (new ReflectionClass(ReflectInvokeCtorTarget::class))->getConstructor()->invoke($object, "X", "Y");
echo ($result === null ? "null" : "value") . ":" . $object->label();
echo ":";
$ctor = (new ReflectionClass(ReflectInvokeCtorTarget::class))->getConstructor();
$ctor->invokeArgs($object, ["right" => "N", "left" => "M"]);
echo $object->label();
"#,
    );
    assert_eq!(out, "null:XY:MN");
}

/// Verifies `ReflectionMethod::setAccessible()` is a no-op for AOT reflectors.
#[test]
fn test_reflection_method_set_accessible_is_noop_for_aot_methods() {
    let out = compile_and_run(
        r#"<?php
class ReflectMethodAccessTarget {
    private function hidden(): string {
        return "secret";
    }
}

$object = new ReflectMethodAccessTarget();
$method = new ReflectionMethod(ReflectMethodAccessTarget::class, "hidden");
echo is_null($method->setAccessible(false)) ? "M" : "m";
echo ":" . $method->invoke($object);
echo ":";
$listed = (new ReflectionClass(ReflectMethodAccessTarget::class))->getMethod("hidden");
echo is_null($listed->setAccessible(accessible: true)) ? "L" : "l";
echo ":" . $listed->invoke($object);
"#,
    );
    assert_eq!(out, "M:secret:L:secret");
}

/// Verifies `ReflectionMethod::invoke()` supports inferred AOT signatures.
#[test]
fn test_reflection_method_invoke_calls_inferred_aot_signature() {
    let out = compile_and_run(
        r#"<?php
class ReflectInvokeInferredTarget {
    public function join($a, $b) { return $a . $b; }
}
$object = new ReflectInvokeInferredTarget();
echo (new ReflectionMethod(ReflectInvokeInferredTarget::class, "join"))->invoke($object, "A", "B");
"#,
    );
    assert_eq!(out, "AB");
}

/// Verifies reflected method names keep the case they were declared with.
///
/// Method tables are keyed case-insensitively, so a lookup by any spelling still finds the
/// method, but `php -n` reports the declared spelling from every name-returning entry point:
/// `getMethods()`, `getMethod()->getName()` and `ReflectionMethod::getName()` alike.
#[test]
fn test_reflection_method_names_keep_declared_case() {
    let out = compile_and_run(
        r#"<?php
class ReflectCasePreservedTarget {
    public function CamelCaseOne(): string { return "one"; }
    public static function StaticTwo(): string { return "two"; }
    public function lower(): string { return "l"; }
}

$class = new ReflectionClass(ReflectCasePreservedTarget::class);
$names = [];
foreach ($class->getMethods() as $method) {
    $names[] = $method->getName();
}
sort($names);
echo implode(",", $names) . ":";
echo $class->getMethod("camelcaseone")->getName() . ":";
echo $class->getMethod("CamelCaseOne")->getName() . ":";
echo $class->getMethod("statictwo")->getName() . ":";
echo (new ReflectionMethod(ReflectCasePreservedTarget::class, "camelcaseone"))->getName() . ":";
echo (new ReflectionMethod(ReflectCasePreservedTarget::class, "CamelCaseOne"))->getName() . ":";
echo $class->hasMethod("CAMELCASEONE") ? "H" : "h";
"#,
    );
    assert_eq!(
        out,
        "CamelCaseOne,StaticTwo,lower:CamelCaseOne:CamelCaseOne:StaticTwo:CamelCaseOne:CamelCaseOne:H"
    );
}

/// Verifies an inherited method reports the case its declaring class wrote it with.
#[test]
fn test_reflection_inherited_method_name_keeps_declaring_case() {
    let out = compile_and_run(
        r#"<?php
class ReflectCaseParent {
    public function InheritedOne(): string { return "p"; }
}
class ReflectCaseChild extends ReflectCaseParent {
    public function OwnTwo(): string { return "c"; }
}

$class = new ReflectionClass(ReflectCaseChild::class);
$names = [];
foreach ($class->getMethods() as $method) {
    $names[] = $method->getName();
}
sort($names);
echo implode(",", $names) . ":";
echo $class->getMethod("inheritedone")->getName() . ":";
echo (new ReflectionMethod(ReflectCaseChild::class, "inheritedone"))->getDeclaringClass()->getName();
"#,
    );
    assert_eq!(out, "InheritedOne,OwnTwo:InheritedOne:ReflectCaseParent");
}

/// Verifies an interface's reflected method names keep their declared case.
#[test]
fn test_reflection_interface_method_names_keep_declared_case() {
    let out = compile_and_run(
        r#"<?php
interface ReflectCaseContract {
    public function DoTheThing(): string;
}

$names = [];
foreach ((new ReflectionClass(ReflectCaseContract::class))->getMethods() as $method) {
    $names[] = $method->getName();
}
echo implode(",", $names) . ":";
echo (new ReflectionClass(ReflectCaseContract::class))->getMethod("dothething")->getName();
"#,
    );
    assert_eq!(out, "DoTheThing:DoTheThing");
}

/// Verifies a method a class gained from a trait keeps the trait's declared case.
#[test]
fn test_reflection_trait_method_names_keep_declared_case() {
    let out = compile_and_run(
        r#"<?php
trait ReflectCaseHelper {
    public function HelpMe(): string { return "h"; }
}
class ReflectCaseHelperUser {
    use ReflectCaseHelper;
}

$class = new ReflectionClass(ReflectCaseHelperUser::class);
$names = [];
foreach ($class->getMethods() as $method) {
    $names[] = $method->getName();
}
sort($names);
echo implode(",", $names) . ":";
echo $class->getMethod("helpme")->getName() . ":";
echo (new ReflectionMethod(ReflectCaseHelperUser::class, "HELPME"))->getName();
"#,
    );
    assert_eq!(out, "HelpMe:HelpMe:HelpMe");
}

/// Verifies `hasMethod()` stays case-insensitive when the reflector reaches it as a union.
///
/// A receiver the checker cannot narrow to `ReflectionClass` — `ReflectionClass|false`, the shape
/// `ContainerBuilder::getReflectionClass()` returns — is dispatched as a boxed `Mixed` value and
/// therefore runs the synthesized method body instead of the eval bridge that a typed receiver
/// reaches. That body compares against the private name index, whose two producers disagree on
/// case: the compile-time materializer stores folded keys while the eval bridge stores the
/// declared spelling. Folding only the argument answered `false` for every method not already
/// written in lower case, so `php -n` said `true` and the binary said `false`.
///
/// `hasProperty()` sits beside it because PHP's property probe *is* case-sensitive: a fix that
/// folded both probes would report `I` here instead of `i`, and a name nothing declares still
/// answers `g`, so neither direction can be traded for the other.
#[test]
fn test_reflection_has_method_is_case_insensitive_through_a_union_receiver() {
    let out = compile_and_run(
        r#"<?php
class ReflectMixedProbeBase {
    public function InheritedMixedCase(): string { return "i"; }
}
class ReflectMixedProbeTarget extends ReflectMixedProbeBase {
    public $CamelProp = 1;
    public function CamelCaseOne(): string { return "one"; }
    public static function StaticTwo(): string { return "two"; }
    public function lower(): string { return "l"; }
}

function reflect_mixed_probe(string $class, bool $miss): ReflectionClass|false {
    return $miss ? false : new ReflectionClass($class);
}

$r = reflect_mixed_probe(ReflectMixedProbeTarget::class, false);
echo $r->hasMethod("CamelCaseOne") ? "A" : "a";
echo $r->hasMethod("camelcaseone") ? "B" : "b";
echo $r->hasMethod("CAMELCASEONE") ? "C" : "c";
echo $r->hasMethod("StaticTwo") ? "D" : "d";
echo $r->hasMethod("InheritedMixedCase") ? "E" : "e";
echo $r->hasMethod("lower") ? "F" : "f";
echo $r->hasMethod("NotDeclared") ? "G" : "g";
echo ":";
echo $r->hasProperty("CamelProp") ? "H" : "h";
echo $r->hasProperty("camelprop") ? "I" : "i";
"#,
    );
    assert_eq!(out, "ABCDEFg:Hi");
}


/// Regression for #571: reflected method names are the DECLARED spelling, not the lookup text.
///
/// Method lookup is case-insensitive, so every reflection entry point finds a method declared
/// `Match` from `"mAtCh"`. What it then reports was wrong in two different ways, which is why
/// the issue shows two lines of output for one method:
///
/// | | before | PHP |
/// |---|---|---|
/// | `new ReflectionMethod(Box::class, "mAtCh")` | `mAtCh` — the caller's own lookup text | `Match` |
/// | `(new ReflectionClass(Box::class))->getMethod("MATCH")` | `match` — the lowercase key | `Match` |
///
/// The declaration is the only answer that is a property of the program rather than of how it
/// was queried, and `method_decls` retains it, so every path now reads it back from there.
///
/// Each row is a different retention path, not a repetition: a directly declared method, a
/// static one, an inherited one, one flattened in from a trait, one declared on an interface,
/// and the trait itself reflected directly (which reads `TraitMethodInfo::declared_name`,
/// because a trait's own declarations are not reachable from a using class). The listing and
/// the prototype/declaring-function rows are included because they take separate code paths
/// from the constructor.
///
/// Every expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_reflected_method_names_report_the_declared_spelling() {
    let out = compile_and_run(
        r#"<?php
interface ReflectCasingContract {
    public function RunIt(int $Times): void;
}

trait ReflectCasingHelper {
    public function HelpMe(): void {}
}

class ReflectCasingBase {
    public function BaseThing(): void {}
}

class ReflectCasingBox extends ReflectCasingBase implements ReflectCasingContract {
    use ReflectCasingHelper;

    public function Match(): void {}
    public static function StaticOne(): void {}
    public function RunIt(int $Times): void {}
}

echo (new ReflectionMethod(ReflectCasingBox::class, "mAtCh"))->getName(), "|";
echo (new ReflectionClass(ReflectCasingBox::class))->getMethod("MATCH")->getName(), "|";
echo (new ReflectionMethod(ReflectCasingBox::class, "STATICONE"))->getName(), "|";
echo (new ReflectionMethod(ReflectCasingBox::class, "basething"))->getName(), "|";
echo (new ReflectionMethod(ReflectCasingBox::class, "helpme"))->getName(), "|";
echo (new ReflectionMethod(ReflectCasingContract::class, "runit"))->getName(), "|";
echo (new ReflectionMethod(ReflectCasingHelper::class, "HELPME"))->getName(), "|";
echo (new ReflectionMethod("ReflectCasingBox::mAtCh"))->getName(), "|";

$names = [];
foreach ((new ReflectionClass(ReflectCasingBox::class))->getMethods() as $method) {
    $names[] = $method->getName();
}
sort($names);
echo implode(",", $names), "|";

$run = new ReflectionMethod(ReflectCasingBox::class, "runit");
echo $run->getPrototype()->getName(), "|";
echo $run->getParameters()[0]->getDeclaringFunction()->getName();
"#,
    );
    assert_eq!(
        out,
        "Match|Match|StaticOne|BaseThing|HelpMe|RunIt|HelpMe|Match|\
         BaseThing,HelpMe,Match,RunIt,StaticOne|RunIt|RunIt"
    );
}

/// Regression for #571's AOT/eval consistency requirement: reflecting an AOT class from inside
/// `eval()` reports the same declared spelling the AOT path reports.
///
/// The eval bridge reads its AOT metadata from a generated table that stored the lowercase
/// lookup key, and the interpreter then lowercased the caller's text on top of that — so the
/// same program answered `Match` in compiled code and `match` one line later inside `eval()`.
/// Every reader of that table compares with `__rt_strcasecmp`, so recording the declaration
/// instead of the key changes only what is reported, never which row is found.
///
/// The class declared INSIDE `eval()` is the control: eval's own metadata always kept its
/// spelling, so it is what the AOT rows now have to match rather than a second thing to fix.
///
/// The fixture deliberately does NOT list the AOT class's methods from compiled code first —
/// that combination hangs, for a reason unrelated to naming (#1031).
///
/// Every expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_eval_reflected_method_names_report_the_declared_spelling() {
    let out = compile_and_run(
        r#"<?php
class ReflectCasingEvalBox {
    public function Match(): void {}
    public static function StaticOne(): void {}
}

eval('echo (new ReflectionMethod("ReflectCasingEvalBox", "mAtCh"))->getName(), "|";
$c = new ReflectionClass("ReflectCasingEvalBox");
echo $c->getMethod("match")->getName(), "|";
$n = []; foreach ($c->getMethods() as $m) { $n[] = $m->getName(); } sort($n);
echo implode(",", $n), "|";
$g = get_class_methods("ReflectCasingEvalBox"); sort($g);
echo implode(",", $g), "|";');
eval('class ReflectCasingEvalDeclared { public function EvalMatch(): void {} }
echo (new ReflectionMethod("ReflectCasingEvalDeclared", "evalmatch"))->getName();');
"#,
    );
    assert_eq!(out, "Match|Match|Match,StaticOne|Match,StaticOne|EvalMatch");
}

/// A private method declared on an ancestor, reflected through that ancestor and invoked on a
/// descendant receiver, runs the ancestor's body: php binds a reflected method by its declaring
/// class. The lowering used to resolve `secret` on the receiver's own class and refused the
/// program ("method call to unknown method AppK::secret").
///
/// Expected output is php 8.5's for the same fixture.
#[test]
fn test_reflection_method_invoke_private_ancestor_method_on_descendant_receiver() {
    let out = compile_and_run(
        r#"<?php
class K {
    private int $n = 5;
    protected function build(): string { return "built" . $this->n; }
    private function secret(int $x): int { return $x + $this->n; }
}
class AppK extends K {}
$k = new AppK();
$m = new \ReflectionMethod($k, 'build');
echo $m->invoke($k), "\n";
$s = new \ReflectionMethod(K::class, 'secret');
echo $s->invoke($k, 2), "\n";
echo $s->invokeArgs($k, [3]), "\n";
$own = new K();
echo $s->invoke($own, 10), "\n";
"#,
    );
    assert_eq!(out, "built5\n7\n8\n15\n");
}

/// A protected method that comes from a trait used by an abstract parent, reflected from a
/// receiver typed as an interface or as `mixed` and invoked on that same receiver, keeps its
/// receiver (the shape of a console command reflecting a kernel's `buildContainer()`).
///
/// Expected output is php 8.5's for the same fixture.
#[test]
fn test_reflection_method_invoke_trait_method_on_interface_and_mixed_receivers() {
    let out = compile_and_run(
        r#"<?php
interface KernelInterface { public function boot(): void; }
trait MicroKernelTrait {
    protected function buildContainer(): string { return "container:" . $this->name; }
    protected function withArg(int $a): string { return "arg" . $a . ":" . $this->name; }
    public function boot(): void {}
}
abstract class BaseKernel implements KernelInterface {
    use MicroKernelTrait;
    protected string $name = "base";
}
class AppKernel extends BaseKernel {}
class Holder {
    public function __construct(private KernelInterface $k) {}
    public function getKernel(): KernelInterface { return $this->k; }
}
function untyped(): mixed { return new AppKernel(); }
$kernel = clone (new Holder(new AppKernel()))->getKernel();
$kernel->boot();
$method = new \ReflectionMethod($kernel, 'buildContainer');
echo $method->invoke($kernel), "\n";
echo $method->invokeArgs($kernel, []), "\n";
$with = new \ReflectionMethod($kernel, 'withArg');
echo $with->invoke($kernel, 4), "\n";
$loose = untyped();
$m2 = new ReflectionMethod($loose, 'buildContainer');
echo $m2->invoke($loose), "\n";
"#,
    );
    assert_eq!(
        out,
        "container:base\ncontainer:base\narg4:base\ncontainer:base\n"
    );
}

/// `ReflectionMethod::invoke()` runs the REFLECTED class's method, not the receiver's override:
/// `(new ReflectionMethod(P::class, 'get'))->invoke(new C())` is `P::get`. The call was a virtual
/// `MethodCall`, so `C::get` ran. Verified against php 8.5.
#[test]
fn test_reflection_method_invoke_runs_the_reflected_class_not_the_override() {
    let out = compile_and_run(
        r#"<?php
class P { public function get(string $n): string { return "P:$n"; } }
class C extends P { public function get(string $n): string { return "C:$n"; } }
class D extends C {}
$m = new \ReflectionMethod(P::class, 'get');
echo $m->invoke(new C(), 'r'), "\n";
echo $m->invoke(new D(), 'd'), "\n";
echo $m->invoke(new P(), 'p'), "\n";
echo $m->invokeArgs(new C(), ['q']), "\n";
$c = new \ReflectionMethod(C::class, 'get');
echo $c->invoke(new D(), 's'), "\n";
echo (new C())->get('v'), "\n";
"#,
    );
    assert_eq!(out, "P:r\nP:d\nP:p\nP:q\nC:s\nC:v\n");
}

/// `ReflectionMethod::getClosure()` and `invoke()` reach a PRIVATE method by design (php ignores
/// visibility for reflection since 8.1). Built from `$this` inside a trait -- Symfony's
/// `MicroKernelTrait::loadRoutes()` calling the kernel's private `configureRoutes()` -- the target
/// is not tracked at compile time and the synthetic body calls through `call_user_func_array()`
/// with `ReflectionMethod` as lexical scope; the per-scope callable table left the private method
/// unreachable from there and every console command that builds the router failed.
#[test]
fn test_reflection_method_closure_reaches_a_private_method_from_a_trait() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
trait MicroKernel {
    public function loadRoutes(): string {
        $configureRoutes = new \ReflectionMethod($this, 'configureRoutes');
        return $configureRoutes->getClosure($this)('routes');
    }
    public function loadViaInvoke(): string {
        return (new \ReflectionMethod($this, 'configureRoutes'))->invoke($this, 'invoke');
    }
}
class Kernel {
    use MicroKernel;
    private function configureRoutes(string $what): string { return "configured $what"; }
}
$k = new Kernel();
echo $k->loadRoutes(), "|", $k->loadViaInvoke(), "|";
$name = $argc > 5 ? 'x' : 'configureRoutes';
echo (new ReflectionMethod($k, $name))->getClosure($k)('dynamic');
if ($argc > 5) { eval('echo 1;'); }
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "configured routes|configured invoke|configured dynamic");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
