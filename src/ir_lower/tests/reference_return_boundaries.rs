//! Purpose:
//! Pins which reference assignments and by-reference returns EIR lowering accepts, and proves
//! the refused shapes fail compilation instead of lowering an ordinary value as a cell pointer.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - A reference binding is accepted only when the SELECTED lowering staged a transferred cell.
//! - An ordinary addressable local is promoted in place so its identity survives the return.
//! - Every supported target lowers the accepted shapes to the same reference-cell structure.

use crate::codegen::platform::Target;
use crate::ir::{LocalKind, Op, Terminator};
use crate::ir_lower::LoweringError;
use std::path::Path;

/// The five first-class targets every reference-cell lowering path must agree on.
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

/// A reference assignment from a runtime-selected callable is refused on every target.
///
/// The descriptor invoker copies a by-reference pointee into an owned `Mixed` and retires the
/// cell, so the caller receives a value. Binding it would dereference a payload word.
#[test]
fn dynamic_callable_reference_assignment_is_refused_on_every_target() {
    let source = r#"<?php
class DynamicReferenceHolder { public array $items = [1]; }
function &dynamicReferenceSource(DynamicReferenceHolder $holder): array { return $holder->items; }
function &otherDynamicReferenceSource(DynamicReferenceHolder $holder): array { return $holder->items; }
function bindDynamicReference(int $choice): void {
    $holder = new DynamicReferenceHolder();
    $callback = $choice > 0 ? 'dynamicReferenceSource' : 'otherDynamicReferenceSource';
    $alias = &$callback($holder);
    echo count($alias);
}
bindDynamicReference($argc);
"#;
    for name in TARGETS {
        let error = match super::try_lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        ) {
            Ok(_) => panic!("{name}: dynamic callable reference assignment must be refused"),
            Err(LoweringError::Unsupported(error)) => error,
            Err(other) => panic!("{name}: expected a refusal, got {other:?}"),
        };
        assert!(
            error.message.contains("Unsupported reference assignment"),
            "{name}: {}",
            error.message
        );
        assert!(error.span.line > 0, "{name}: the refusal names a source position");
    }
}

/// A reference assignment from a plain by-value call is refused rather than bound.
#[test]
fn by_value_call_reference_assignment_is_refused() {
    let message = refusal(
        r#"<?php
class ValueOnlyHolder { public array $items = [1]; }
function valueOnlySource(ValueOnlyHolder $holder): array { return $holder->items; }
$holder = new ValueOnlyHolder();
$alias = &valueOnlySource($holder);
echo count($alias);
"#,
    );
    assert!(message.contains("Unsupported reference assignment"), "{message}");
}

/// Resolved functions, methods, static methods and bound closures all stage a transferred cell.
///
/// Each accepted binding adopts the staged owner, so the count of `AdoptRefCellPtr` operations
/// is the count of reference assignments in the caller on every target.
#[test]
fn resolved_reference_assignments_adopt_staged_cells_on_every_target() {
    let source = r#"<?php
class ResolvedReferenceHolder {
    public array $items = [1];
    public function &reference(): array { return $this->items; }
    public static function &staticReference(ResolvedReferenceHolder $holder): array { return $holder->items; }
}
function &resolvedReferenceSource(ResolvedReferenceHolder $holder): array { return $holder->items; }
function bindResolvedReferences(): void {
    $holder = new ResolvedReferenceHolder();
    $fromFunction = &resolvedReferenceSource($holder);
    $fromMethod = &$holder->reference();
    $fromStatic = &ResolvedReferenceHolder::staticReference($holder);
    $closure = function &() use ($holder): array { return $holder->items; };
    $fromClosure = &$closure();
    echo count($fromFunction), count($fromMethod), count($fromStatic), count($fromClosure);
}
bindResolvedReferences();
"#;
    for name in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let caller = module
            .functions
            .iter()
            .find(|function| function.name.eq_ignore_ascii_case("bindResolvedReferences"))
            .expect("the reference-binding caller is lowered");
        let adopted = caller
            .instructions
            .iter()
            .filter(|inst| inst.op == Op::AdoptRefCellPtr)
            .count();
        assert_eq!(adopted, 4, "{name}: every resolved reference assignment adopts its cell");
        assert!(
            !caller.instructions.iter().any(|inst| inst.op == Op::BindRefCellPtr),
            "{name}: a returned cell is never bound without adopting its owner"
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// An ordinary local returned by reference is promoted in place on every target.
///
/// Promotion keeps the variable's identity: the callee writes through the same cell the caller
/// aliases, and the cell outlives the frame because its owner is transferred, not borrowed.
#[test]
fn ordinary_local_reference_returns_promote_before_transfer_on_every_target() {
    let source = r#"<?php
function &promotedLocalReference(): string {
    $value = 'promoted';
    return $value;
}
$alias = &promotedLocalReference();
echo $alias;
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
            .find(|function| function.name.eq_ignore_ascii_case("promotedLocalReference"))
            .expect("the promoting callee is lowered");
        let promotion = callee
            .instructions
            .iter()
            .position(|inst| inst.op == Op::PromoteLocalRefCell)
            .expect("the returned local is promoted to a managed cell");
        let acquisition = callee
            .instructions
            .iter()
            .position(|inst| inst.op == Op::AcquireRefCell)
            .expect("the promoted cell is leased to the caller");
        assert!(promotion < acquisition, "{name}: promote before leasing the cell");
        let owner = callee
            .locals
            .iter()
            .filter(|local| local.kind == LocalKind::ReturnRefCell)
            .count();
        assert_eq!(owner, 1, "{name}: exactly one reference-return lease slot per frame");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// Returning an alias of an indexed-array element by reference is refused.
///
/// `__rt_reference_cell_owner` answers zero for that interior address, so nothing can be
/// transferred and the array could be released while the caller still holds the alias.
#[test]
fn borrowed_array_element_alias_reference_return_is_refused() {
    let message = refusal(
        r#"<?php
function &escapingElementAlias(): int {
    $numbers = [1, 2];
    $slot = &$numbers[0];
    return $slot;
}
$alias = &escapingElementAlias();
echo $alias;
"#,
    );
    assert!(message.contains("alias of an array element"), "{message}");
}

/// A transitive alias of an array element inherits the interior marker and is refused too.
#[test]
fn transitive_array_element_alias_reference_return_is_refused() {
    let message = refusal(
        r#"<?php
function &relayedElementAlias(): int {
    $numbers = [1, 2];
    $first = &$numbers[0];
    $second = &$first;
    return $second;
}
$alias = &relayedElementAlias();
echo $alias;
"#,
    );
    assert!(message.contains("alias of an array element"), "{message}");
}

/// Returning a value expression by reference is refused instead of lowered as a scalar.
#[test]
fn value_shaped_reference_return_is_refused() {
    let message = refusal(
        r#"<?php
function &valueShapedReference(): int {
    $numbers = [1, 2];
    return $numbers[0];
}
$alias = &valueShapedReference();
echo $alias;
"#,
    );
    assert!(
        message.contains("from a variable or a property"),
        "{message}"
    );
}

/// A fallthrough `finally` that rebinds the returned variable keeps ONE leased cell.
///
/// The lease slot is written once per return, before the finally body runs, and the return
/// terminator reads that same slot. The structural proof is that the frame holds exactly one
/// lease slot and every value-carrying return is preceded by its own acquisition.
#[test]
fn fallthrough_finally_rebinding_keeps_one_reference_return_lease_on_every_target() {
    let source = r#"<?php
class FallthroughReferenceHolder { public string $text = ''; }
function &fallthroughReference(): string {
    $holder = new FallthroughReferenceHolder();
    $holder->text = 'first';
    $slot = &$holder->text;
    try {
        return $slot;
    } finally {
        $other = new FallthroughReferenceHolder();
        $other->text = 'second';
        $slot = &$other->text;
    }
}
$alias = &fallthroughReference();
echo $alias;
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
            .find(|function| function.name.eq_ignore_ascii_case("fallthroughReference"))
            .expect("the finally-rebinding callee is lowered");
        let leases = callee
            .locals
            .iter()
            .filter(|local| local.kind == LocalKind::ReturnRefCell)
            .count();
        assert_eq!(leases, 1, "{name}: one lease slot carries the returned cell");
        let acquisitions = callee
            .instructions
            .iter()
            .filter(|inst| inst.op == Op::AcquireRefCell)
            .count();
        let value_returns = callee
            .blocks
            .iter()
            .filter(|block| {
                matches!(&block.terminator, Some(Terminator::Return { value: Some(_) }))
            })
            .count();
        assert_eq!(
            acquisitions, value_returns,
            "{name}: every returned cell is leased exactly once before its return"
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// Owned by-value arguments of a by-reference-returning callee are rooted before the call.
///
/// Without a root, a callee that throws leaves the caller's owned temporary unreachable from
/// frame cleanup. The root is published before the call and retired after it.
#[test]
fn by_reference_return_calls_root_their_value_arguments_on_every_target() {
    let source = r#"<?php
class RootedArgumentHolder { public array $items = [1]; }
function &rootedArgumentReference(array $first, array $second, RootedArgumentHolder $holder): array {
    return $holder->items;
}
function bindRootedArguments(int $size): void {
    $holder = new RootedArgumentHolder();
    $alias = &rootedArgumentReference(array_fill(0, $size, 'a'), array_fill(0, $size, 'b'), $holder);
    echo count($alias);
}
bindRootedArguments($argc);
"#;
    for name in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let caller = module
            .functions
            .iter()
            .find(|function| function.name.eq_ignore_ascii_case("bindRootedArguments"))
            .expect("the rooted-argument caller is lowered");
        let call = caller
            .instructions
            .iter()
            .position(|inst| inst.op == Op::Call && inst.operands.len() == 3)
            .expect("the three-argument reference call is lowered");
        let published = caller.instructions[..call]
            .iter()
            .filter(|inst| inst.op == Op::PushCallOperandOwner)
            .count();
        assert!(
            published >= 2,
            "{name}: both owned array arguments are rooted before the callee can throw"
        );
        let adopt = caller
            .instructions
            .iter()
            .position(|inst| inst.op == Op::AdoptRefCellPtr)
            .expect("the returned cell is staged");
        let retire = caller
            .instructions
            .iter()
            .position(|inst| inst.op == Op::ReleaseLocalSlot)
            .expect("rooted arguments are retired after the call");
        assert!(
            adopt < retire,
            "{name}: the transferred lease is published before caller cleanup can throw"
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// The interior-alias marker survives whichever branch arm is lowered last.
///
/// Branch lowering sequences the arms, so a managed rebinding in the arm lowered second would
/// otherwise clear a marker the first arm established and let a borrowed element address escape.
#[test]
fn array_element_alias_reference_return_is_refused_in_both_branch_orders() {
    for (first, second) in [
        ("$slot = &$numbers[0];", "$slot = &$holder->value;"),
        ("$slot = &$holder->value;", "$slot = &$numbers[0];"),
    ] {
        let source = format!(
            r#"<?php
class BranchReferenceHolder {{ public int $value = 7; }}
function &branchElementAlias(int $choice): int {{
    $numbers = [1, 2];
    $holder = new BranchReferenceHolder();
    if ($choice > 0) {{
        {first}
    }} else {{
        {second}
    }}
    return $slot;
}}
$alias = &branchElementAlias($argc);
echo $alias;
"#
        );
        let message = refusal(&source);
        assert!(
            message.contains("alias of an array element"),
            "{first} / {second}: {message}"
        );
    }
}

/// A terminated borrowed arm cannot taint a managed reference that reaches the return.
#[test]
fn terminated_borrowed_reference_branches_do_not_taint_managed_returns() {
    for (first, second) in [
        ("$slot = &$numbers[0]; throw new RuntimeException('stop');", "$slot = &$holder->value;"),
        ("$slot = &$holder->value;", "$slot = &$numbers[0]; throw new RuntimeException('stop');"),
    ] {
        let source = format!(r#"<?php
class ReachableReferenceHolder {{ public int $value = 7; }}
function &reachableReference(int $choice): int {{
    $numbers = [1, 2];
    $holder = new ReachableReferenceHolder();
    if ($choice > 0) {{ {first} }} else {{ {second} }}
    return $slot;
}}
$alias = &reachableReference($argc);
echo $alias;
"#);
        for name in TARGETS {
            let module = super::lower_source_at_for_target(
                &source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
            );
            crate::codegen::generate_user_asm_from_ir(&module, false, false)
                .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        }
    }
}

/// An explicit managed rebinding clears the conservative borrowed marker after a merge.
#[test]
fn managed_reference_rebinding_clears_merged_borrowed_provenance() {
    let source = r#"<?php
class RecoveredReferenceHolder { public int $value = 9; }
function &recoveredReference(int $choice): int {
    $numbers = [1, 2];
    $holder = new RecoveredReferenceHolder();
    if ($choice > 0) { $slot = &$numbers[0]; } else { $slot = &$holder->value; }
    $slot = &$holder->value;
    return $slot;
}
$alias = &recoveredReference($argc);
echo $alias;
"#;
    for name in TARGETS {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// A reference relayed through a by-reference parameter is guarded at run time on every target.
///
/// Nothing in the callee's frame can tell whether the caller bound an array element or a managed
/// cell, so the accepted lowering carries the owner-zero guard that fails closed instead of
/// handing back an interior address.
#[test]
fn relayed_reference_parameter_returns_guard_owner_lookup_on_every_target() {
    let source = r#"<?php
function &relayReferenceParameter(mixed &$slot): mixed {
    return $slot;
}
function relayThroughElementAlias(): void {
    $numbers = [1, 2];
    $borrowed = &$numbers[0];
    $alias = &relayReferenceParameter($borrowed);
    echo $alias;
}
relayThroughElementAlias();
"#;
    for name in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let relay = module
            .functions
            .iter()
            .find(|function| function.name.eq_ignore_ascii_case("relayReferenceParameter"))
            .expect("the relaying callee is lowered");
        assert!(
            relay.instructions.iter().any(|inst| inst.op == Op::AcquireRefCell),
            "{name}: the relay still leases the cell it was handed"
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let lookup = assembly
            .find("__rt_reference_cell_owner")
            .expect("the lease asks for the cell's owner");
        let guard = assembly
            .find("__rt_borrowed_reference_return_error")
            .expect("a zero owner fails closed instead of publishing an interior address");
        assert!(lookup < guard, "{name}: the guard follows the owner lookup");
    }
}

/// A reference assignment's staging record is published before the call and detached after it.
///
/// This is the structural half of same-frame catch safety: the adopted lease lives in a ref-cell
/// owner slot that a REAL cleanup record covers, published before the arguments are evaluated so
/// the record nests outside every argument root, and detached only once the alias owns the cell.
#[test]
fn reference_assignment_staging_is_covered_by_a_cleanup_record_on_every_target() {
    let source = r#"<?php
class StagedReferenceHolder { public array $items = [1]; }
function &stagedReference(array $payload, StagedReferenceHolder $holder): array {
    return $holder->items;
}
function bindStagedReference(int $size): void {
    $holder = new StagedReferenceHolder();
    $alias = &stagedReference(array_fill(0, $size, 'a'), $holder);
    echo count($alias);
}
bindStagedReference($argc);
"#;
    for name in TARGETS {
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            Target::parse(name).unwrap(),
        );
        let caller = module
            .functions
            .iter()
            .find(|function| function.name.eq_ignore_ascii_case("bindStagedReference"))
            .expect("the staging caller is lowered");
        let (adopt, adopted) = caller
            .instructions
            .iter()
            .enumerate()
            .find_map(|(index, inst)| match inst.immediate {
                Some(crate::ir::Immediate::LocalSlotPair { second, .. })
                    if inst.op == Op::AdoptRefCellPtr =>
                {
                    Some((index, second))
                }
                _ => None,
            })
            .expect("the transferred cell is adopted into a hidden owner slot");
        assert_eq!(
            caller.locals[adopted.as_raw() as usize].kind,
            LocalKind::RefCell,
            "{name}: the staged lease lives in a reference-cell owner slot"
        );
        let covers = |op: Op| {
            caller.instructions.iter().position(|inst| {
                inst.op == op && inst.immediate == Some(crate::ir::Immediate::LocalSlot(adopted))
            })
        };
        let publish = covers(Op::PushCallOperandOwner)
            .expect("a cleanup record covers the staged reference-cell owner");
        let detach =
            covers(Op::PopCallOperandOwner).expect("the staging record is detached, not leaked");
        let call = caller
            .instructions
            .iter()
            .position(|inst| inst.op == Op::Call && inst.operands.len() == 2)
            .expect("the two-argument reference call is lowered");
        assert!(
            publish < call && call < adopt && adopt < detach,
            "{name}: publish {publish} < call {call} < adopt {adopt} < detach {detach}"
        );
        // Source-evaluation records can be popped and republished before the call. Walk the
        // actual slot stack instead of comparing pre-call pushes with only post-call pops.
        let mut active = Vec::new();
        for (index, inst) in caller.instructions.iter().enumerate().take(detach + 1).skip(publish) {
            if index == call || index == adopt {
                assert_eq!(active.first(), Some(&adopted), "{name}: staging covers {index}");
            }
            let Some(crate::ir::Immediate::LocalSlot(slot)) = inst.immediate else { continue; };
            match inst.op {
                Op::PushCallOperandOwner => active.push(slot),
                Op::PopCallOperandOwner => {
                    assert_eq!(active.pop(), Some(slot), "{name}: LIFO retirement at {index}");
                }
                _ => {}
            }
        }
        assert!(active.is_empty(), "{name}: staging and argument records all retire");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// A refusal inside a statement the representation fixed point re-lowers stays a single record.
///
/// `stmt::repr_fixpoint` lowers such a statement, discards that lowering, and lowers it again.
/// The discarded pass's refusal rolls back with it (see the `ir_lower::diagnostics` unit tests),
/// and successive lowering runs on the same thread must not see each other's records.
#[test]
fn refusals_survive_re_lowering_and_do_not_leak_between_runs() {
    let source = r#"<?php
function &refusedInsideConvertedStatement(int $choice): mixed {
    $numbers = [1, 2];
    $slot = &$numbers[0];
    $slot = 'text';
    if ($choice > 0) {
        return $slot;
    }
    return $slot;
}
$alias = &refusedInsideConvertedStatement($argc);
echo $alias;
"#;
    let first = refusal(source);
    assert!(first.contains("alias of an array element"), "{first}");
    let second = refusal(source);
    assert_eq!(first, second, "successive runs report the same refusal");

    // A program with no refused shape must lower cleanly right after one that refused, which is
    // what proves the thread-local sink is cleared at the start of every run.
    super::lower_source_at_for_target(
        "<?php $values = [1, 2]; $alias = &$values[0]; $alias = 5; echo $values[0];",
        Path::new("main.php"),
        Path::new("."),
        Target::detect_host(),
    );
}
