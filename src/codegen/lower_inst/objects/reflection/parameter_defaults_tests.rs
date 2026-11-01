//! Purpose:
//! Pins the cost of resolving a reflected parameter default to its EIR helper.
//!
//! Called from:
//! - The reflection parameter-defaults module's unit test harness.
//!
//! Key details:
//! - Counts examined helper candidates with a test-only counter, never wall-clock time.

use super::*;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Target;
use crate::codegen::shared_state::SharedCodegenState;
use crate::ir::{Function, IrType, Module};
use crate::span::Span;

/// A default helper whose `callable_default` is the literal `index` written at line `index + 1`.
fn helper(index: u32) -> Function {
    let mut function = Function::new(format!("__default_helper_{index}"), IrType::Void, PhpType::Void);
    function.callable_default =
        Some(Expr::new(ExprKind::IntLiteral(index as i64), Span::new(index + 1, 1)));
    function
}

/// A reflected default finds its helper among only the helpers written at the same span: the
/// linear scan of every function and closure it replaced dominated Symfony's codegen, once per
/// reflected parameter. The match itself, its order and a miss are unchanged.
#[test]
fn reflected_default_examines_only_helpers_at_its_span() {
    let target = Target::parse("macos-aarch64").unwrap();
    let mut module = Module::new(target);
    for index in 0..200 {
        module.functions.push(helper(index));
    }
    module.closures.push(helper(150));
    let function = crate::codegen::shared_helper::helper_function("reflect_defaults", PhpType::Void);
    let layout = crate::codegen::frame::layout_for_function(&function, target, false, false, false);
    let mut emitter = Emitter::new(target);
    let mut data = DataSection::new();
    let mut shared = SharedCodegenState::for_module(&module);
    let ctx = FunctionContext::new(
        &module, &function, &mut emitter, &mut data, &mut shared, layout, false, false, false, None,
    );
    let wanted = module.functions[150].callable_default.clone().unwrap();
    DEFAULT_HELPER_CANDIDATES.with(|count| count.set(0));
    let found = reflection_parameter_default_value(&ctx, "", None, &wanted).unwrap();
    assert_eq!(found, Some(ReflectionParameterDefaultValue::EirHelper("__default_helper_150".into())));
    let elsewhere = Expr::new(ExprKind::IntLiteral(150), Span::new(9_999, 1));
    assert_ne!(
        reflection_parameter_default_value(&ctx, "", None, &elsewhere).unwrap(),
        Some(ReflectionParameterDefaultValue::EirHelper("__default_helper_150".into())),
        "an equal value at another span names no helper"
    );
    let examined = DEFAULT_HELPER_CANDIDATES.with(|count| count.get());
    assert!(examined <= 2, "{examined} helper candidates examined for two lookups among 201");
}
