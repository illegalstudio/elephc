//! Purpose:
//! Verifies that Fiber state errors and runtime switches respect published operand owners.
//!
//! Called from:
//! - The AST-to-EIR and target assembly unit test module.
//!
//! Key details:
//! - Caller-rooted arguments and receivers must not be consumed again by Fiber helpers.
//! - The shared transfer predicate applies to every supported architecture.

/// Emits Fiber operand-owner regression assembly for one supported target.
fn assembly(source: &str, target: &str) -> String {
    use crate::ir::{Immediate, Op};

    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let main = module.functions.iter().find(|function| function.name == "main").unwrap();
    for call in main.instructions.iter().filter(|inst| inst.op == Op::MethodCall) {
        for argument in call.operands.iter().skip(1) {
            let rooted = main.instructions.iter().any(|inst| {
                if inst.op == Op::StoreLocal && inst.operands == [*argument] {
                    if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
                        return [Op::PushCallOperandOwner, Op::ReleaseLocalSlot].into_iter().all(|op| {
                            main.instructions.iter().any(|owner| {
                                owner.op == op && owner.immediate == Some(Immediate::LocalSlot(slot))
                            })
                        });
                    }
                }
                false
            });
            assert!(rooted, "{target}: fixture must store, publish, and retire its argument slot: {argument:?}");
        }
    }
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap()
}

/// Resume state guards defer published argument leases to caller unwinding on all targets.
#[test]
fn fiber_resume_guard_does_not_retire_published_argument_owners_on_all_targets() {
    let source = r#"<?php
class GuardedFiberPayload {}
$fiber = new Fiber(static function (): void {});
try { $fiber->resume(new GuardedFiberPayload()); } catch (FiberError $error) {}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let asm = assembly(source, target);
        let (_, guarded) = asm.split_once("fiber_resume_suspended").expect("Fiber lifecycle branch");
        let (rejected, _) = guarded.split_once("__rt_fiber_throw_state_error")
            .expect("rejected state error");
        assert!(!rejected.contains("__rt_decref_object"), "{target}: {rejected}");
        assert!(!rejected.contains("__rt_decref_mixed"), "{target}: {rejected}");
    }
}

/// Throw preserves a caller's local receiver and a Throwable held by a published operand root.
#[test]
fn fiber_throw_does_not_transfer_published_receiver_or_throwable_on_all_targets() {
    let source = r#"<?php
$fiber = new Fiber(static function (): void {});
try { $fiber->throw(new RuntimeException("owned")); }
catch (Throwable $error) {}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let asm = assembly(source, target);
        let (before, _) = asm.split_once("__rt_fiber_throw\n").expect("Fiber throw helper");
        let tail = before.lines().rev().take(4).collect::<Vec<_>>();
        let (receiver, throwable) = if target == "linux-x86_64" {
            ("mov rdx, 0", "mov rcx, 0")
        } else {
            ("mov x2, #0", "mov x3, #0")
        };
        assert!(tail.iter().any(|line| line.trim() == receiver), "{target}: {tail:?}");
        assert!(tail.iter().any(|line| line.trim() == throwable), "{target}: {tail:?}");
    }
}
