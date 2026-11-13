//! Purpose:
//! Verifies semantic profile boundaries in eval execution.
//!
//! Called from:
//! - Focused Magician interpreter unit tests.
//!
//! Key details:
//! - Uses the fake runtime to observe failures without loading native libraries.

use super::super::*;
use super::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Runtime source cannot bypass the strict boundary through the interpreter API.
#[test]
fn strict_profile_dynamic_code_blocks_nested_eval_and_include() {
    for source in [b"eval('return 1;');".as_slice(), b"include '/must-not-read-strict-source.php';"] {
        let program = parse_fragment(source).unwrap();
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::DynamicCode, true)).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values),
            Err(EvalStatus::UncaughtThrowable));
        assert!(values.output.is_empty());
    }
}

/// An external host cannot execute with a profile whose guarantees are still unimplemented.
#[test]
fn strict_profile_pending_guarantees_cannot_execute_through_interpreter_api() {
    let program = parse_fragment(b"echo 'must not execute';").unwrap();
    let mut context = ElephcEvalContext::new();
    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true)).unwrap();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values),
        Err(EvalStatus::UnsupportedConstruct));
    assert!(values.output.is_empty());
}
