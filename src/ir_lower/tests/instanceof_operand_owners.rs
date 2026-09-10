//! Purpose:
//! Regression tests for `instanceof` operand ownership in AST-to-EIR lowering.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - The backend `instanceof` entry and eval introspection adapters only borrow
//!   the EIR value/target operands, so the source lowering must retire any
//!   independently owned temporary (notably a class-name string detached from a
//!   boxed `Mixed` local) and preserve borrowed locals and the `Bool` result.
//! - An owned value operand is rooted through the call-operand-owner
//!   infrastructure across a dynamic target that can unwind.

use crate::ir::{Immediate, Op};
use crate::types::PhpType;

/// The five supported native targets exercised by every case in this module.
const TARGETS: [&str; 5] = [
    "macos-aarch64",
    "ios-arm64",
    "ios-sim-arm64",
    "linux-aarch64",
    "linux-x86_64",
];

/// Lowers `source` for `target`, panicking with a target-labelled message on failure.
fn lower_for(source: &str, target: &str) -> crate::ir::Module {
    super::lower_source_at_for_target(
        source,
        std::path::Path::new("main.php"),
        std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    )
}

/// Returns whether any instruction releases exactly `value`.
fn releases(function: &crate::ir::Function, value: crate::ir::ValueId) -> bool {
    release_count(function, value) != 0
}

/// Counts retirement instructions so duplicate releases cannot satisfy a presence assertion.
fn release_count(function: &crate::ir::Function, value: crate::ir::ValueId) -> usize {
    function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::Release && inst.operands == [value])
        .count()
}

/// Returns the original local read, validating any independently retained predicate lease.
fn original_borrow(function: &crate::ir::Function, operand: crate::ir::ValueId) -> crate::ir::ValueId {
    let producer = function.instructions.iter().find(|inst| inst.result == Some(operand)).unwrap();
    if producer.op != Op::Acquire { return operand; }
    let root = function.instructions.iter().find(|inst| {
        inst.op == Op::StoreLocal && inst.operands == [operand]
    }).expect("a retained predicate operand must have a root owner");
    for op in [Op::PushCallOperandOwner, Op::PopCallOperandOwner, Op::ReleaseLocalSlot] {
        assert_eq!(function.instructions.iter().filter(|inst| {
            inst.op == op && inst.immediate == root.immediate
        }).count(), 1, "balanced retained predicate lease: {op:?}");
    }
    assert!(!releases(function, operand), "the retained lease retires only through its owner slot");
    producer.operands[0]
}

/// All five `instanceof` target forms preserve the original borrowed `$this` value operand.
///
/// The named, `self`, and `parent` forms carry a class-name immediate and lower to
/// `Op::InstanceOf`; the `static` form (which pushes the current `$this` as a runtime target) and
/// the dynamic class-name form both lower to `Op::InstanceOfDynamic`. In every form the tested
/// value is the method's borrowed `$this`, which the predicate must not consume. A dynamic target
/// may retain a balanced provisional lease because the final slot representation can still widen.
#[test]
fn all_instanceof_target_forms_preserve_borrowed_this_on_all_targets() {
    let source = r#"<?php
class InstFormsBase {}
class InstFormsChild extends InstFormsBase {
    public function classify(string $cls): string {
        $out = "";
        $out .= $this instanceof InstFormsBase ? "n" : "-";
        $out .= $this instanceof self ? "s" : "-";
        $out .= $this instanceof parent ? "p" : "-";
        $out .= $this instanceof static ? "t" : "-";
        $out .= $this instanceof $cls ? "d" : "-";
        return $out;
    }
}
echo (new InstFormsChild())->classify("InstFormsBase");
"#;
    for target in TARGETS {
        let module = lower_for(source, target);
        let method = module
            .class_methods
            .iter()
            .find(|method| method.name == "InstFormsChild::classify")
            .unwrap_or_else(|| panic!("{target}: missing classify method"));
        let named = method
            .instructions
            .iter()
            .filter(|inst| inst.op == Op::InstanceOf)
            .count();
        let dynamic = method
            .instructions
            .iter()
            .filter(|inst| inst.op == Op::InstanceOfDynamic)
            .count();
        assert_eq!(named, 3, "{target}: named/self/parent immediate forms");
        assert_eq!(dynamic, 2, "{target}: `static` and dynamic class-name forms");
        for inst in method
            .instructions
            .iter()
            .filter(|inst| matches!(inst.op, Op::InstanceOf | Op::InstanceOfDynamic))
        {
            for operand in &inst.operands {
                let value = original_borrow(method, *operand);
                let load = method
                    .instructions
                    .iter()
                    .find(|other| other.result == Some(value))
                    .unwrap_or_else(|| panic!("{target}: value operand must be defined"));
                assert_eq!(load.op, Op::LoadLocal, "{target}: `$this` and the class parameter are local loads");
                assert!(
                    !releases(method, value),
                    "{target}: neither a borrowed value nor an implicit/explicit borrowed target may be retired"
                );
            }
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// A dynamic `instanceof` over concrete borrowed operands retires neither operand on any target.
///
/// The object and class-string parameters are concrete borrowed locals: the object load owns
/// nothing, and the concrete-string provisional release is pruned by builder finalization. Both
/// operands must therefore remain live for the caller.
#[test]
fn dynamic_instanceof_preserves_borrowed_operands_on_all_targets() {
    let source = r#"<?php
class InstBorrowBase {}
class InstBorrowChild extends InstBorrowBase {}
function probeBorrowedInstanceof(InstBorrowChild $object, string $cls): bool {
    return $object instanceof $cls;
}
echo probeBorrowedInstanceof(new InstBorrowChild(), "InstBorrowBase") ? "1" : "0";
"#;
    for target in TARGETS {
        let module = lower_for(source, target);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "probeBorrowedInstanceof")
            .unwrap_or_else(|| panic!("{target}: missing probe function"));
        let inst = function
            .instructions
            .iter()
            .find(|inst| inst.op == Op::InstanceOfDynamic)
            .unwrap_or_else(|| panic!("{target}: missing dynamic instanceof"));
        for operand in &inst.operands {
            let original = original_borrow(function, *operand);
            assert!(
                !releases(function, original),
                "{target}: borrowed dynamic-instanceof operands must survive the predicate"
            );
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// A post-eval dynamic class-name target releases its detached string operand on every target.
///
/// This is the regression: after an opaque `eval()` widens the function's locals to boxed
/// `Mixed` storage, `$target = get_parent_class($object)` is stored in a `Mixed` slot. Reading
/// it as the dynamic `instanceof` target detaches an owned `Str` copy that the backend entry and
/// the eval introspection adapter only borrow, so the lowering must retire that exact copy. One
/// such leak accrued per call in the original fixture.
#[test]
fn post_eval_dynamic_target_releases_detached_class_name_string_on_all_targets() {
    let source = r#"<?php
class InstEvalBase { public int $value = 7; }
class InstEvalChild extends InstEvalBase { public function __destruct() {} }
function probeEvalInstanceof(InstEvalChild $object, string $source): bool {
    eval($source);
    $target = get_parent_class($object);
    return $object instanceof $target;
}
$source = 'return null; // ' . $argc;
echo probeEvalInstanceof(new InstEvalChild(), $source) ? "1" : "0";
"#;
    for target in TARGETS {
        let module = lower_for(source, target);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "probeEvalInstanceof")
            .unwrap_or_else(|| panic!("{target}: missing probe function"));
        let inst = function
            .instructions
            .iter()
            .find(|inst| inst.op == Op::InstanceOfDynamic)
            .unwrap_or_else(|| panic!("{target}: missing dynamic instanceof"));
        let target_operand = inst.operands[1];
        let load = function
            .instructions
            .iter()
            .find(|other| other.result == Some(target_operand))
            .unwrap_or_else(|| panic!("{target}: dynamic target must be defined"));
        assert_eq!(load.op, Op::LoadLocal, "{target}: the target reads a local slot");
        assert_eq!(
            load.result_php_type,
            PhpType::Str,
            "{target}: the class-name target reads as a string"
        );
        let Some(Immediate::LocalSlot(slot)) = load.immediate else {
            panic!("{target}: the target load must identify its slot");
        };
        assert_eq!(
            function.locals[slot.as_raw() as usize]
                .php_type
                .codegen_repr(),
            PhpType::Mixed,
            "{target}: the eval-widened slot backs a detached string read"
        );
        assert_eq!(
            release_count(function, target_operand), 1,
            "{target}: the detached class-name string must be retired exactly once"
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// An owned value operand is unwind-rooted across a dynamic target on every target.
///
/// A freshly constructed object is an owned temporary. Because the dynamic target is an
/// arbitrary expression that could unwind while it is live, the value operand is published in a
/// frame slot through the call-operand-owner infrastructure (`PushCallOperandOwner` before the
/// predicate) and retired afterwards (`PopCallOperandOwner` + `ReleaseLocalSlot`), rather than
/// released as a raw SSA temporary.
#[test]
fn owned_value_operand_is_unwind_rooted_across_dynamic_target_on_all_targets() {
    let source = r#"<?php
class InstOwnedBase {}
class InstOwnedChild extends InstOwnedBase {}
function probeOwnedInstanceof(string $cls): bool {
    return (new InstOwnedChild()) instanceof $cls;
}
echo probeOwnedInstanceof("InstOwnedBase") ? "1" : "0";
"#;
    for target in TARGETS {
        let module = lower_for(source, target);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "probeOwnedInstanceof")
            .unwrap_or_else(|| panic!("{target}: missing probe function"));
        let predicate = function
            .instructions
            .iter()
            .position(|inst| inst.op == Op::InstanceOfDynamic)
            .unwrap_or_else(|| panic!("{target}: missing dynamic instanceof"));
        // The operand the predicate reads is the rooted (acquired) value published in the frame
        // slot, not the raw `ObjectNew` result the root retires internally.
        let rooted_operand = function.instructions[predicate].operands[0];
        let push = function
            .instructions
            .iter()
            .position(|inst| inst.op == Op::PushCallOperandOwner)
            .unwrap_or_else(|| panic!("{target}: an owned value operand must be rooted"));
        let Some(Immediate::LocalSlot(root_slot)) = function.instructions[push].immediate else {
            panic!("{target}: the root must identify its slot");
        };
        assert!(push < predicate, "{target}: the root is published before the predicate");
        let pop = function
            .instructions
            .iter()
            .position(|inst| {
                inst.op == Op::PopCallOperandOwner
                    && inst.immediate == Some(Immediate::LocalSlot(root_slot))
            })
            .unwrap_or_else(|| panic!("{target}: the root must be detached"));
        assert!(pop > predicate, "{target}: the root is detached after the predicate");
        assert_eq!(
            function.instructions[predicate + 1..].iter().filter(|inst| {
                inst.op == Op::ReleaseLocalSlot
                    && inst.immediate == Some(Immediate::LocalSlot(root_slot))
            }).count(), 1,
            "{target}: the rooted owner retires exactly once through its slot"
        );
        assert!(
            !releases(function, rooted_operand),
            "{target}: the transferred root owner is not also released as a raw SSA temporary"
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// A throwing target cannot strand a fresh object or a detached post-eval string.
#[test]
fn instanceof_value_owners_are_published_before_throwing_target_calls_on_all_targets() {
    let source = r#"<?php
class InstThrowValue {}
function throwingInstTarget(): string { throw new RuntimeException("target"); }
function freshInstValue(): bool {
    return (new InstThrowValue()) instanceof (throwingInstTarget());
}
function detachedInstValue(string $source): bool {
    eval($source);
    $value = str_repeat("value", 3);
    return $value instanceof (throwingInstTarget());
}
try { freshInstValue(); } catch (RuntimeException $error) {}
try { detachedInstValue('return null; // ' . $argc); } catch (RuntimeException $error) {}
"#;
    for target in TARGETS {
        let module = lower_for(source, target);
        for name in ["freshInstValue", "detachedInstValue"] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let predicate = function.instructions.iter().position(|inst| inst.op == Op::InstanceOfDynamic).unwrap();
            let value = function.instructions[predicate].operands[0];
            let source_value = original_borrow(function, value);
            let producer = function.instructions.iter().find(|inst| inst.result == Some(source_value)).unwrap();
            if name == "detachedInstValue" {
                assert_eq!(producer.op, Op::LoadLocal, "{target}: detached source is a local read");
                assert_eq!(producer.result_php_type, PhpType::Str, "{target}: detached string representation");
                let Some(Immediate::LocalSlot(slot)) = producer.immediate else { panic!("local read slot"); };
                assert_eq!(function.locals[slot.as_raw() as usize].php_type.codegen_repr(), PhpType::Mixed);
            }
            let store = function.instructions.iter().find(|inst| {
                inst.op == Op::StoreLocal && inst.operands == [value]
            }).expect("predicate owns a rooted value lease");
            let push = function.instructions.iter().position(|inst| {
                inst.op == Op::PushCallOperandOwner && inst.immediate == store.immediate
            }).unwrap();
            let target_call = function.instructions[..predicate].iter().rposition(|inst| inst.op == Op::Call).unwrap();
            let pop = function.instructions.iter().position(|inst| {
                inst.op == Op::PopCallOperandOwner && inst.immediate == store.immediate
            }).unwrap();
            assert!(push < target_call && target_call < predicate && predicate < pop,
                "{target}: {name} keeps its operand rooted throughout the throwing target call");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}
