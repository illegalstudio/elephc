//! Purpose:
//! Interpreter tests for ReflectionClass capabilities and construction metadata.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Readonly, instantiable, cloneable, iterable, and origin predicates are covered.

use super::super::super::*;
use super::super::support::*;

/// Verifies ReflectionClass exposes eval readonly class metadata.
#[test]
fn execute_program_reflects_eval_class_readonly_predicate() {
    let program = parse_fragment(
        br#"class EvalReadonlyPlain {}
readonly class EvalReadonlyReflect {}
final readonly class EvalReadonlyFinalReflect {}
enum EvalReadonlyEnumReflect { case Ready; }
interface EvalReadonlyIface {}
trait EvalReadonlyTrait {}
echo (new ReflectionClass("EvalReadonlyPlain"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyReflect"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyFinalReflect"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyEnumReflect"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyIface"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyTrait"))->isReadOnly() ? "R" : "r";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "rRRrrr");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass exposes eval class instantiability metadata.
#[test]
fn execute_program_reflects_eval_class_instantiable_predicate() {
    let program = parse_fragment(
        br#"abstract class EvalInstAbstract {}
class EvalInstPublic {}
final class EvalInstFinal {}
class EvalInstPrivate { private function __construct() {} }
class EvalInstProtected { protected function __construct() {} }
interface EvalInstIface {}
trait EvalInstTrait {}
enum EvalInstEnum { case Ready; }
echo (new ReflectionClass("EvalInstAbstract"))->isInstantiable() ? "A" : "a";
echo (new ReflectionClass("EvalInstPublic"))->isInstantiable() ? "B" : "b";
echo (new ReflectionClass("EvalInstFinal"))->isInstantiable() ? "C" : "c";
echo (new ReflectionClass("EvalInstPrivate"))->isInstantiable() ? "P" : "p";
echo (new ReflectionClass("EvalInstProtected"))->isInstantiable() ? "R" : "r";
echo (new ReflectionClass("EvalInstIface"))->isInstantiable() ? "I" : "i";
echo (new ReflectionClass("EvalInstTrait"))->isInstantiable() ? "T" : "t";
echo (new ReflectionClass("EvalInstEnum"))->isInstantiable() ? "E" : "e";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "aBCprite");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::isAnonymous reports false for eval-declared named class-like symbols.
#[test]
fn execute_program_reflection_class_reports_named_classes_not_anonymous() {
    let program = parse_fragment(
        br#"class EvalNamedAnonymousReflect {}
interface EvalNamedAnonymousIface {}
trait EvalNamedAnonymousTrait {}
enum EvalNamedAnonymousEnum { case Ready; }
echo (new ReflectionClass("EvalNamedAnonymousReflect"))->isAnonymous() ? "C" : "c";
echo (new ReflectionClass("EvalNamedAnonymousIface"))->isAnonymous() ? "I" : "i";
echo (new ReflectionClass("EvalNamedAnonymousTrait"))->isAnonymous() ? "T" : "t";
echo (new ReflectionClass("EvalNamedAnonymousEnum"))->isAnonymous() ? "E" : "e";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "cite");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::isCloneable reports eval class clone metadata.
#[test]
fn execute_program_reflects_eval_class_cloneable_predicate() {
    let program = parse_fragment(
        br#"abstract class EvalCloneAbstract {}
class EvalClonePlain {}
final class EvalCloneFinal {}
class EvalClonePrivate { private function __clone() {} }
class EvalCloneProtected { protected function __clone() {} }
class EvalClonePublic { public function __clone() {} }
interface EvalCloneIface {}
trait EvalCloneTrait {}
enum EvalCloneEnum { case Ready; }
echo (new ReflectionClass("EvalCloneAbstract"))->isCloneable() ? "A" : "a";
echo (new ReflectionClass("EvalClonePlain"))->isCloneable() ? "P" : "p";
echo (new ReflectionClass("EvalCloneFinal"))->isCloneable() ? "F" : "f";
echo (new ReflectionClass("EvalClonePrivate"))->isCloneable() ? "V" : "v";
echo (new ReflectionClass("EvalCloneProtected"))->isCloneable() ? "R" : "r";
echo (new ReflectionClass("EvalClonePublic"))->isCloneable() ? "U" : "u";
echo (new ReflectionClass("EvalCloneIface"))->isCloneable() ? "I" : "i";
echo (new ReflectionClass("EvalCloneTrait"))->isCloneable() ? "T" : "t";
echo (new ReflectionClass("EvalCloneEnum"))->isCloneable() ? "E" : "e";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "aPFvrUite");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::isIterable reports eval Traversable-compatible class metadata.
#[test]
fn execute_program_reflects_eval_class_iterable_predicate() {
    let program = parse_fragment(
        br#"class EvalIterablePlain {}
abstract class EvalIterableAbstract implements Iterator {}
interface EvalIterableIface extends Iterator {}
trait EvalIterableTrait {}
enum EvalIterableEnum { case Ready; }
class EvalIterableIterator implements Iterator {
    public function current() { return null; }
    public function key() { return null; }
    public function next() {}
    public function valid() { return false; }
    public function rewind() {}
}
class EvalIterableAggregate implements IteratorAggregate {
    public function getIterator() { return $this; }
}
echo (new ReflectionClass("EvalIterablePlain"))->isIterable() ? "P" : "p";
$iter = new ReflectionClass("EvalIterableIterator");
echo $iter->isIterable() ? "I" : "i";
echo $iter->isIterateable() ? "A" : "a";
echo (new ReflectionClass("EvalIterableAggregate"))->isIterable() ? "G" : "g";
echo (new ReflectionClass("EvalIterableAbstract"))->isIterable() ? "B" : "b";
echo (new ReflectionClass("EvalIterableIface"))->isIterable() ? "F" : "f";
echo (new ReflectionClass("EvalIterableEnum"))->isIterable() ? "E" : "e";
echo (new ReflectionClass("EvalIterableTrait"))->isIterable() ? "H" : "h";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "pIAGbfeh");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass origin predicates report eval class-like symbols as user-defined.
#[test]
fn execute_program_reflects_eval_class_origin_predicates() {
    let program = parse_fragment(
        br#"class EvalOriginClass {}
interface EvalOriginIface {}
trait EvalOriginTrait {}
enum EvalOriginEnum { case Ready; }
function eval_reflect_origin($name) {
    $r = new ReflectionClass($name);
    echo $r->isInternal() ? "I" : "i";
    echo $r->isUserDefined() ? "U" : "u";
    echo ":";
}
eval_reflect_origin("EvalOriginClass");
eval_reflect_origin("EvalOriginIface");
eval_reflect_origin("EvalOriginTrait");
eval_reflect_origin("EvalOriginEnum");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "iU:iU:iU:iU:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass::getConstructor exposes eval constructor metadata.
#[test]
fn execute_program_reflection_class_get_constructor() {
    let program = parse_fragment(
        br#"class EvalCtorBase {
    public function __construct($required, $optional = 2) {}
}
class EvalCtorChild extends EvalCtorBase {}
class EvalCtorPlain {}
interface EvalCtorInterface {
    public function __construct($required);
}
trait EvalCtorTrait {
    public function __construct($required, $optional = null, ...$rest) {}
}
$base = (new ReflectionClass("EvalCtorBase"))->getConstructor();
echo $base->getName(); echo "/";
echo $base->getNumberOfParameters(); echo "/";
echo $base->getNumberOfRequiredParameters(); echo ":";
$child = (new ReflectionClass("EvalCtorChild"))->getConstructor();
echo $child->getName(); echo "/";
echo $child->getNumberOfParameters(); echo "/";
echo $child->getNumberOfRequiredParameters(); echo ":";
$plain = (new ReflectionClass("EvalCtorPlain"))->getConstructor();
echo $plain === null ? "null" : "bad"; echo ":";
$interface = (new ReflectionClass("EvalCtorInterface"))->getConstructor();
echo $interface->getName(); echo "/";
echo $interface->getNumberOfParameters(); echo "/";
echo $interface->getNumberOfRequiredParameters(); echo ":";
$trait = (new ReflectionClass("EvalCtorTrait"))->getConstructor();
echo $trait->getName(); echo "/";
echo $trait->getNumberOfParameters(); echo "/";
echo $trait->getNumberOfRequiredParameters();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "__construct/2/1:__construct/2/1:null:__construct/1/1:__construct/3/1"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
