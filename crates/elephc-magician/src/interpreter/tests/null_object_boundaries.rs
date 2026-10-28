//! Purpose:
//! Preserves catchable null errors at declared object and iterable boundaries.
//!
//! Called from:
//! - Magician's focused interpreter tests in CI.
//!
//! Key details:
//! - These use eval-declared classes without a native XML dependency.
//! - Existing scalar mismatch behavior remains covered by method_arguments tests.

use super::super::*;
use super::support::*;

/// Typed named tails bind without the positional hash scans that make a collector pass quadratic.
#[test]
fn typed_named_variadic_binding_avoids_positional_hash_scans() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let object = values.new_object("stdClass").expect("caller object");
    let args = (0..64).map(|index| EvaluatedCallArg {
        name: Some(format!("arg{index}")), value: object, ref_target: None,
    }).collect();
    let bound = bind_evaluated_method_args_with_ref_mode(
        "typedNamedTail", &["nodes".into()],
        &[Some(EvalParameterType::new(vec![EvalParameterTypeVariant::Object], false))],
        &[None], &[false], &[true], args, EvalByRefBindingMode::RequireTarget,
        &mut context, &mut values,
    ).expect("bind named typed variadic arguments");
    assert_eq!(values.array_len(bound[0].value).unwrap(), 64);
    assert_eq!(values.array_iter_key_calls, 0, "binding must not repeatedly walk the hash by position");
    release_native_bound_args(&bound, &mut context, &mut values).expect("release collector");
    values.release(object).expect("release caller owner");
    let keys = values.values.iter().filter(|(_, value)|
        matches!(value, FakeValue::String(name) if name.starts_with("arg"))).collect::<Vec<_>>();
    assert_eq!(keys.len(), 64);
    for (id, _) in keys { assert_eq!(values.cell_owners.get(id).copied(), Some(0)); }
}

/// Executes a boundary fixture and verifies that execution continues after every caught error.
fn assert_null_boundary_output(source: &[u8], expected: &str) {
    let program = parse_fragment(source).expect("parse boundary fixture");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values)
        .expect("catch null boundary errors and continue");
    assert_eq!(values.output, expected);
    assert_eq!(values.get(result), FakeValue::Bool(true));
    for (id, value) in &values.values {
        if matches!(value, FakeValue::String(name) if name == "bad" || name == "good") {
            assert_eq!(values.cell_owners.get(id).copied(), Some(0), "temporary named variadic key leaked");
        }
    }
}

/// Rejects null before entering typed functions, methods, constructors, or closures.
#[test]
fn eval_null_object_parameter_errors_are_catchable() {
    assert_null_boundary_output(br#"
interface NullPeer {}
class NullNode implements NullPeer {}
function nullNode(NullNode $node) { echo 'entered'; }
function nullObject(object $node) { echo 'entered'; }
function nullPeer(NullPeer $node) { echo 'entered'; }
function nullUnion(NullNode|int $node) { echo 'entered'; }
function nullIterable(iterable $nodes) { echo 'entered'; }
function nullRef(NullNode &$node) { echo 'entered'; }
function nullNullable(?NullNode $node) { echo $node === null ? 'nullable|' : 'bad'; }
class NullMethods {
    public function read(self $node) { echo 'entered'; }
    public static function create(NullNode $node) { echo 'entered'; }
    public function __construct(?NullNode $node = null) {}
}
class NullConstructor { public function __construct(NullNode $node) { echo 'entered'; } }
try { nullNode(node: null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { nullObject(null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { nullPeer(null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { nullUnion(null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { nullIterable(null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$missing = null;
try { nullRef($missing); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$methods = new NullMethods();
try { $methods->read(null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { NullMethods::create(null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { new NullConstructor(null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$closure = function (object $node) { echo 'entered'; };
try { $closure(null); } catch (TypeError $e) { echo substr($e->getMessage(), strpos($e->getMessage(), 'Argument #')), "\n"; }
nullNullable(null);
return true;
"#, concat!(
        "nullNode(): Argument #1 ($node) must be of type NullNode, null given\n",
        "nullObject(): Argument #1 ($node) must be of type object, null given\n",
        "nullPeer(): Argument #1 ($node) must be of type NullPeer, null given\n",
        "nullUnion(): Argument #1 ($node) must be of type NullNode|int, null given\n",
        "nullIterable(): Argument #1 ($nodes) must be of type Traversable|array, null given\n",
        "nullRef(): Argument #1 ($node) must be of type NullNode, null given\n",
        "NullMethods::read(): Argument #1 ($node) must be of type NullMethods, null given\n",
        "NullMethods::create(): Argument #1 ($node) must be of type NullNode, null given\n",
        "NullConstructor::__construct(): Argument #1 ($node) must be of type NullNode, null given\n",
        "Argument #1 ($node) must be of type object, null given\nnullable|",
    ));
}

/// Counts positional variadic entries while named tails share the next argument number.
#[test]
fn eval_null_object_variadic_errors_preserve_numbering() {
    assert_null_boundary_output(br#"
class NullTail {}
function nullTail(int $prefix, NullTail ...$nodes) { echo 'entered'; }
function nullTailRef(NullTail &...$nodes) { echo 'entered'; }
function nullTailOrder(NullTail $first, NullTail ...$nodes) { echo 'entered'; }
$good = new NullTail();
$missing = null;
try { nullTail(1, $good, $missing); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$args = [$good, $missing];
try { nullTail(1, ...$args); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { nullTail(1, good: $good, bad: $missing); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { nullTail(1, $good, bad: $missing); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { nullTailRef($missing); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { nullTailOrder(null, null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { nullTailOrder(null, bad: null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { nullTailOrder(bad: null, first: null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { nullTailOrder(null, 'invalid'); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
return true;
"#, concat!(
        "nullTail(): Argument #3 must be of type NullTail, null given\n",
        "nullTail(): Argument #3 must be of type NullTail, null given\n",
        "nullTail(): Argument #2 must be of type NullTail, null given\n",
        "nullTail(): Argument #3 must be of type NullTail, null given\n",
        "nullTailRef(): Argument #1 must be of type NullTail, null given\n",
        "nullTailOrder(): Argument #1 ($first) must be of type NullTail, null given\n",
        "nullTailOrder(): Argument #1 ($first) must be of type NullTail, null given\n",
        "nullTailOrder(): Argument #1 ($first) must be of type NullTail, null given\n",
        "nullTailOrder(): Argument #1 ($first) must be of type NullTail, null given\n",
    ));
}

/// Resolves late static returns to the called class while lexical self retains its owner.
#[test]
fn eval_null_object_return_errors_are_catchable() {
    assert_null_boundary_output(br#"
class NullReturnBase {
    public function late(): static { return null; }
    public function lexical(): self { return null; }
    public static function lateCall(): static { return null; }
}
class NullReturnChild extends NullReturnBase {}
function nullReturnObject(): object { return null; }
function nullReturnUnion(): NullReturnBase|int { return null; }
function nullReturnIterable(): iterable { return null; }
function nullReturnNullable(): ?object { return null; }
function &nullReturnRef(): object { $missing = null; return $missing; }
$child = new NullReturnChild();
try { $child->late(); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $child->lexical(); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { NullReturnChild::lateCall(); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { nullReturnObject(); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { nullReturnUnion(); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { nullReturnIterable(); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $ref =& nullReturnRef(); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$closure = function (): object { return null; };
try { $closure(); } catch (TypeError $e) { echo substr($e->getMessage(), strpos($e->getMessage(), 'Return value')), "\n"; }
echo nullReturnNullable() === null ? 'nullable|' : 'bad';
return true;
"#, concat!(
        "NullReturnBase::late(): Return value must be of type NullReturnChild, null returned\n",
        "NullReturnBase::lexical(): Return value must be of type NullReturnBase, null returned\n",
        "NullReturnBase::lateCall(): Return value must be of type NullReturnChild, null returned\n",
        "nullReturnObject(): Return value must be of type object, null returned\n",
        "nullReturnUnion(): Return value must be of type NullReturnBase|int, null returned\n",
        "nullReturnIterable(): Return value must be of type Traversable|array, null returned\n",
        "nullReturnRef(): Return value must be of type object, null returned\n",
        "Return value must be of type object, null returned\nnullable|",
    ));
}

/// Checks plain, aliased, reflected, and hooked property writes before changing storage.
#[test]
fn eval_null_object_property_errors_preserve_previous_values() {
    assert_null_boundary_output(br#"
class NullPropertyNode {}
class NullPropertyBox {
    public NullPropertyNode $node;
    public NullPropertyNode|int $union = 1;
    public iterable $items;
    public static NullPropertyNode $shared;
    public static NullPropertyNode $plainShared;
    public ?NullPropertyNode $nullable = null;
    public NullPropertyNode $hooked { set { echo 'entered'; $this->hooked = $value; } }
}
$box = new NullPropertyBox();
$good = new NullPropertyNode();
$box->node = $good;
$box->items = [];
NullPropertyBox::$shared = $good;
NullPropertyBox::$plainShared = $good;
$alias =& NullPropertyBox::$shared;
try { $box->node = null; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$name = 'node';
try { $box->{$name} = null; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$instance = new ReflectionProperty(NullPropertyBox::class, 'node');
try { $instance->setValue($box, null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $instance->setRawValue($box, null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $box->union = null; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $box->items = null; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { NullPropertyBox::$shared = null; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$property = new ReflectionProperty(NullPropertyBox::class, 'shared');
try { $property->setValue(null, null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$class = new ReflectionClass(NullPropertyBox::class);
try { $class->setStaticPropertyValue('plainShared', null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $box->hooked = null; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$box->nullable = null;
echo ($box->node === $good && $alias === $good && NullPropertyBox::$shared === $good && $box->union === 1 && $box->nullable === null) ? 'preserved|' : 'bad';
return true;
"#, concat!(
        "Cannot assign null to property NullPropertyBox::$node of type NullPropertyNode\n",
        "Cannot assign null to property NullPropertyBox::$node of type NullPropertyNode\n",
        "Cannot assign null to property NullPropertyBox::$node of type NullPropertyNode\n",
        "Cannot assign null to property NullPropertyBox::$node of type NullPropertyNode\n",
        "Cannot assign null to property NullPropertyBox::$union of type NullPropertyNode|int\n",
        "Cannot assign null to property NullPropertyBox::$items of type Traversable|array\n",
        "Cannot assign null to property NullPropertyBox::$shared of type NullPropertyNode\n",
        "Cannot assign null to property NullPropertyBox::$shared of type NullPropertyNode\n",
        "Cannot assign null to property NullPropertyBox::$plainShared of type NullPropertyNode\n",
        "NullPropertyBox::$hooked::set(): Argument #1 ($value) must be of type NullPropertyNode, null given\n",
        "preserved|",
    ));
}

/// Failed direct and dynamic reference binds leave initialization, old values, and aliases intact.
#[test]
fn eval_null_object_property_reference_binds_preserve_storage() {
    assert_null_boundary_output(br#"
class NullRefBox { public object $node; }
$box = new NullRefBox();
$missing = null;
try { $box->node =& $missing; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$name = 'node';
try { $box->{$name} =& $missing; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$property = new ReflectionProperty(NullRefBox::class, 'node');
echo $property->isInitialized($box) ? 'bad' : 'uninitialized|';
$good = new stdClass();
$box->node = $good;
try { $box->node =& $missing; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $box->{$name} =& $missing; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo $box->node === $good ? 'preserved|' : 'bad';
$missing = new stdClass();
echo $box->node === $good ? 'detached|' : 'bad';
return true;
"#, concat!(
        "Cannot assign null to property NullRefBox::$node of type object\n",
        "Cannot assign null to property NullRefBox::$node of type object\nuninitialized|",
        "Cannot assign null to property NullRefBox::$node of type object\n",
        "Cannot assign null to property NullRefBox::$node of type object\npreserved|detached|",
    ));
}

/// Keeps ordinary methods with hook-like names distinct from actual property accessors.
#[test]
fn eval_null_object_errors_preserve_ordinary_method_names() {
    assert_null_boundary_output(br#"
class NullOrdinaryMethods {
    public function __propset_node(object $node) { echo 'entered'; }
    public function __propget_node(): object { return null; }
}
$methods = new NullOrdinaryMethods();
try { $methods->__propset_node(null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $methods->__propget_node(); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
return true;
"#, concat!(
        "NullOrdinaryMethods::__propset_node(): Argument #1 ($node) must be of type object, null given\n",
        "NullOrdinaryMethods::__propget_node(): Return value must be of type object, null returned\n",
    ));
}
