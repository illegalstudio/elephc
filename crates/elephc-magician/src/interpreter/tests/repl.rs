//! Purpose:
//! Checks the submission recovery boundary without native runtime assembly.
//!
//! Called from:
//! - The Magician interpreter test harness on REPL host platforms.
//!
//! Key details:
//! - Unsafe ABI transitions remain fatal and recovery never leaves a pending Throwable.

use super::super::*;
use super::support::*;

/// ABI guards cannot be converted to success and leak unsafe values into native writeback.
#[test]
fn repl_recovery_keeps_boundary_guards_fatal() {
    for status in [EvalStatus::AbiMismatch, EvalStatus::EscapingPcntlCallable, EvalStatus::UncaughtThrowable] {
        let mut context = ElephcEvalContext::new();
        let mut values = FakeOps::default();
        assert!(matches!(repl::finish_submission(Err(status), false, &mut context, &mut values),
            Err(actual) if actual == status));
        assert!(values.values.is_empty());
    }
}

/// Repeated recovery transfers each owned Throwable to the caller, with no pending exception.
#[test]
fn repl_recovery_transfers_throwables_without_retaining_pending_state() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    for status in [EvalStatus::RuntimeFatal, EvalStatus::ParseError, EvalStatus::UnsupportedConstruct] {
        let outcome = repl::finish_submission(Err(status), false, &mut context, &mut values).unwrap();
        let EvalOutcome::Throwable(error) = outcome else { panic!("missing throwable for {status:?}"); };
        assert!(context.take_pending_throw().is_none());
        assert_eq!(values.cell_owners[&(error.as_ptr() as usize)], 1);
        values.release(error).unwrap();
        assert!(values.cell_owners.values().all(|owners| *owners == 0));
    }
}

/// A runtime diagnostic cannot overwrite a pending exception and lose its owned cell.
#[test]
fn repl_recovery_preserves_an_existing_pending_throwable() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let pending = values.new_object("Error").unwrap();
    context.set_pending_throw(pending);
    let outcome = repl::finish_submission(Err(EvalStatus::RuntimeFatal), false, &mut context, &mut values).unwrap();
    assert!(matches!(outcome, EvalOutcome::Throwable(error) if error == pending));
    assert!(context.take_pending_throw().is_none());
    values.release(pending).unwrap();
    assert!(values.cell_owners.values().all(|owners| *owners == 0));
}
