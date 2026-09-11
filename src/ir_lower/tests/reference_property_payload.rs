//! Purpose:
//! Pins the payload contract of a by-reference return whose source is an object property: the
//! transferred cell must be readable with the callee's declared result representation, and the
//! decision is made statically when the receiver's class is known and per candidate class when
//! it is not.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - A statically decided mismatch is a source diagnostic; a `Mixed` receiver is never refused
//!   wholesale, because the same function is reached by compatible and incompatible classes.
//! - The guarded load is a distinct opcode from the plain `$x = &$obj->prop` alias, which carries
//!   no payload claim and must keep its unguarded lowering.
//! - A `Closure::bind` by-reference specialization learns the bound property's type BEFORE the
//!   closure body is lowered, so the body, the descriptor signature and the call site agree.
//! - It also materializes the bound descriptor as a second `closure_new` over the same compiled
//!   function whose only capture is the boxed receiver, rather than through the runtime
//!   `closure_bind`, so exactly one receiver box exists and the descriptor owns it.

use crate::codegen::platform::Target;
use crate::ir::{Op, Terminator};
use crate::ir_lower::LoweringError;
use crate::types::PhpType;
use std::path::Path;

/// The five first-class targets every reference-payload decision must agree on.
const TARGETS: [&str; 5] = [
    "macos-aarch64",
    "ios-arm64",
    "ios-sim-arm64",
    "linux-aarch64",
    "linux-x86_64",
];

/// Lowers `source` for the host target and returns the refusal message it must produce.
fn refusal(source: &str) -> String {
    match super::try_lower_source_at_for_target(
        source,
        Path::new("main.php"),
        Path::new("."),
        Target::detect_host(),
    ) {
        Ok(_) => panic!("expected EIR lowering to refuse this program, but it lowered"),
        Err(LoweringError::Unsupported(error)) => error.message,
        Err(other) => panic!("expected an unsupported-shape refusal, got {other:?}"),
    }
}

/// A statically known property whose slot cannot be read as the declared result is refused.
///
/// `public int $value` stores a raw integer, while a `mixed` by-reference result makes the caller
/// dereference the transferred cell as a boxed pointer. The property is shared storage, so
/// widening it behind the other aliases is not an option and the program is refused instead.
#[test]
fn typed_property_payload_mismatch_is_refused() {
    let message = refusal(
        r#"<?php
class PayloadHolder { public int $value = 7; }
function &payloadSlot(PayloadHolder $holder): mixed { return $holder->value; }
$holder = new PayloadHolder();
$alias = &payloadSlot($holder);
echo $alias;
"#,
    );
    assert!(
        message.contains("Unsupported by-reference return")
            && message.contains("payload representation"),
        "{message}"
    );
}

/// A property holding a SUBCLASS instance is still transferable through a base-typed result.
///
/// Object storage is one pointer whatever the class is, and the class relationship is a semantic
/// question the checker already settled. Refusing it here would reject ordinary covariant code
/// for a representation difference that does not exist.
#[test]
fn compatible_object_property_lowers_a_checked_cell_on_every_target() {
    let source = r#"<?php
class PayloadNode { public int $id = 4; }
class PayloadLeaf extends PayloadNode { }
class NodeHolder {
    public PayloadLeaf $node;
    public function __construct() { $this->node = new PayloadLeaf(); }
}
function &nodeSlot(NodeHolder $holder): PayloadNode { return $holder->node; }
function readNodeSlot(): void {
    $holder = new NodeHolder();
    $alias = &nodeSlot($holder);
    echo $alias->id;
}
readNodeSlot();
"#;
    for name in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let callee = module
            .functions
            .iter()
            .find(|function| function.name.eq_ignore_ascii_case("nodeSlot"))
            .expect("the by-reference callee is lowered");
        assert_eq!(
            callee
                .instructions
                .iter()
                .filter(|inst| inst.op == Op::LoadPropRefCellChecked)
                .count(),
            1,
            "{name}: the by-reference property return carries a payload claim"
        );
        for block in &callee.blocks {
            let Some(Terminator::Return { value: Some(value) }) = &block.terminator else {
                continue;
            };
            let producer = callee
                .instructions
                .iter()
                .find(|inst| inst.result == Some(*value))
                .unwrap_or_else(|| panic!("{name}: a returned address has a producing instruction"));
            assert_eq!(
                producer.op,
                Op::AcquireRefCell,
                "{name}: only an acquired cell address is returned by reference"
            );
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// An ordinary `$x = &$obj->prop` alias keeps the unguarded opcode and its bare pointer result.
///
/// The alias makes no claim about the payload (the local is bound at the property's own type),
/// so comparing its `Pointer` result against the slot would reject every alias in the language.
#[test]
fn plain_property_alias_keeps_the_unchecked_load() {
    let source = r#"<?php
class AliasHolder { public int $value = 3; }
function aliasValue(): void {
    $holder = new AliasHolder();
    $alias = &$holder->value;
    $alias = 5;
    echo $holder->value;
}
aliasValue();
"#;
    let module = super::lower_source_at_for_target(
        source,
        Path::new("main.php"),
        Path::new("."),
        Target::detect_host(),
    );
    let caller = module
        .functions
        .iter()
        .find(|function| function.name.eq_ignore_ascii_case("aliasValue"))
        .expect("the aliasing function is lowered");
    let alias_loads = caller
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::LoadPropRefCell)
        .collect::<Vec<_>>();
    assert!(!alias_loads.is_empty(), "the alias loads the property's cell");
    for load in &alias_loads {
        assert_eq!(
            load.result_php_type,
            PhpType::Pointer(None),
            "an alias publishes a bare cell pointer, not a payload claim"
        );
    }
    assert!(
        !caller
            .instructions
            .iter()
            .any(|inst| inst.op == Op::LoadPropRefCellChecked),
        "an alias carries no declared-result payload claim to check"
    );
}

/// A `Mixed` receiver is never refused wholesale, however many candidate classes disagree.
///
/// One `mixed` parameter reaches both a compatible and an incompatible class in the same program,
/// and the compatible call must keep working. The decision therefore belongs to the backend's per
/// candidate guard, which is why lowering accepts the function on every target.
#[test]
fn mixed_receiver_by_reference_return_is_accepted_on_every_target() {
    let source = r#"<?php
class GoodPayloadHolder { public mixed $value = 7; }
class BadPayloadHolder { public int $value = 9; }
function &dynamicPropertyReference(mixed $holder): mixed { return $holder->value; }
function readDynamicReferences(): void {
    $good = new GoodPayloadHolder();
    $bad = new BadPayloadHolder();
    $goodAlias = &$good->value;
    $badAlias = &$bad->value;
    $reference = &dynamicPropertyReference($good);
    echo $reference, '|', $goodAlias, '|', $badAlias;
}
readDynamicReferences();
"#;
    for name in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let callee = module
            .functions
            .iter()
            .find(|function| function.name.eq_ignore_ascii_case("dynamicPropertyReference"))
            .expect("the dynamic by-reference callee is lowered");
        let checked = callee
            .instructions
            .iter()
            .filter(|inst| inst.op == Op::LoadPropRefCellChecked)
            .collect::<Vec<_>>();
        assert_eq!(checked.len(), 1, "{name}: the dynamic return is guarded once");
        assert!(
            checked[0].effects.contains(crate::ir::Effects::MAY_THROW),
            "{name}: the guard can raise a catchable Error"
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// A by-reference `Closure::bind` specialization types the closure BODY from the bound property.
///
/// The closure's own `$this` is the untyped `Mixed` capture a top-level closure gets, so without
/// the contextual result the body would publish a `Mixed` payload claim for a `string` slot and
/// the caller would read the string cell as a boxed pointer. Retyping only the binding afterwards
/// left the compiled closure and the descriptor disagreeing, so the type is threaded in first.
#[test]
fn bound_closure_reference_return_types_its_body_from_the_bound_property() {
    let source = r#"<?php
class BoundStringHolder { public string $text = 'init'; }
$holder = new BoundStringHolder();
$bound = \Closure::bind(fn &() => $this->text, $holder, $holder);
$alias = &$bound();
$alias = 'changed';
echo $holder->text;
"#;
    for name in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let closure = module
            .closures
            .iter()
            .find(|function| function.flags.is_closure)
            .expect("the bound closure is lowered");
        assert_eq!(
            closure.return_php_type,
            PhpType::Str,
            "{name}: the closure body is lowered against the bound property's type"
        );
        assert_eq!(
            closure.params.last().map(|param| param.php_type.clone()),
            Some(PhpType::Mixed),
            "{name}: the bound receiver arrives as the boxed Mixed capture"
        );
        let checked = closure
            .instructions
            .iter()
            .find(|inst| inst.op == Op::LoadPropRefCellChecked)
            .unwrap_or_else(|| panic!("{name}: the closure returns a guarded property cell"));
        assert_eq!(
            checked.result_php_type,
            PhpType::Str,
            "{name}: the published payload claim is the bound property's representation"
        );
        assert_bound_descriptor_shape(&module, name, 1);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// Asserts the top-level body materializes `bind_count` bound descriptors the supported way.
///
/// The specialization does not call the runtime `closure_bind` at all. Each bind emits the
/// closure literal's own `closure_new` and a SECOND `closure_new` over the same compiled
/// function whose single capture is the receiver boxed as `Mixed`. That second descriptor is the
/// one that escapes, and owning its capture outright is what keeps exactly one receiver
/// reference alive instead of the descriptor and the call site each holding their own box.
fn assert_bound_descriptor_shape(module: &crate::ir::Module, target: &str, bind_count: usize) {
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("the top-level body is lowered");
    assert!(
        !main.instructions.iter().any(|inst| inst.op == Op::ClosureBind),
        "{target}: the specialization binds without the runtime closure_bind helper"
    );
    let descriptors = main
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::ClosureNew)
        .collect::<Vec<_>>();
    assert_eq!(
        descriptors.len(),
        bind_count * 2,
        "{target}: each bind emits the literal's descriptor and the bound one"
    );
    let bound = descriptors
        .iter()
        .filter(|inst| {
            inst.operands.len() == 1
                && main
                    .instructions
                    .iter()
                    .any(|producer| {
                        producer.result == inst.operands.first().copied()
                            && producer.op == Op::MixedBox
                    })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        bound.len(),
        bind_count,
        "{target}: the bound descriptor captures the boxed receiver"
    );
    for (index, descriptor) in bound.iter().enumerate() {
        assert_eq!(
            descriptor.immediate,
            descriptors[index * 2].immediate,
            "{target}: the bound descriptor reuses the literal's compiled function"
        );
    }
}

/// A bind written INSIDE a method still compiles its body against the bound receiver.
///
/// The enclosing `$this` is an ordinary typed object there, and capturing it at that class would
/// compile `$this->count` against the ENCLOSING class's slot even though the bind hands the
/// closure an instance of another class. Both classes declare a compatible `int` slot, so the
/// per-candidate guard accepts the bound receiver and the capture must be the boxed `Mixed` form.
#[test]
fn in_method_bind_to_another_class_captures_a_mixed_receiver() {
    let source = r#"<?php
class OtherCounter { public int $count = 3; }
class CounterBinder {
    public int $count = 99;
    public function bumpOther(OtherCounter $other): int {
        $bound = \Closure::bind(fn &() => $this->count, $other, OtherCounter::class);
        $alias = &$bound();
        $alias = 11;
        return $this->count;
    }
}
$binder = new CounterBinder();
echo $binder->bumpOther(new OtherCounter());
"#;
    for name in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let closure = module
            .closures
            .iter()
            .find(|function| function.flags.is_closure)
            .expect("the bound closure is lowered");
        assert_eq!(
            closure.params.last().map(|param| param.php_type.clone()),
            Some(PhpType::Mixed),
            "{name}: an in-method bind captures its receiver as Mixed, not as the lexical class"
        );
        assert_eq!(
            closure.return_php_type,
            PhpType::Int,
            "{name}: the body is typed from the BOUND class's property"
        );
        assert!(
            closure
                .instructions
                .iter()
                .any(|inst| inst.op == Op::LoadPropRefCellChecked),
            "{name}: the runtime-dispatched receiver goes through the payload guard"
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// The immediate-invoke form materializes the same bound descriptor the stored form does.
///
/// Both forms have to own the receiver box through the descriptor: it is what the direct call
/// borrows as `$this`, and it is what has to be freed when the temporary `Closure` dies. Keeping
/// the two paths structurally identical is what stops the immediate form from regrowing a second
/// unowned box.
#[test]
fn immediate_and_stored_binds_materialize_the_same_descriptor_shape() {
    let source = r#"<?php
class TwinHolder { public string $text = 'init'; }
$holder = new TwinHolder();
$first = &\Closure::bind(fn &() => $this->text, $holder, $holder)();
$bound = \Closure::bind(fn &() => $this->text, $holder, $holder);
$second = &$bound();
echo $first, $second;
"#;
    for name in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        assert_bound_descriptor_shape(&module, name, 2);
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}
