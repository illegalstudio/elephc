//! Purpose:
//! Interpreter test for reflection over a descriptor whose last slot is the hidden
//! `func_get_args()` collector.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_hidden_collector`.
//!
//! Key details:
//! - The compiler lists the collector as the descriptor's variadic slot; php sees a plain
//!   function with one parameter, so `isVariadic()` must answer false.

use super::super::*;
use super::support::*;
use std::ffi::c_void;

/// Verifies the hidden collector is neither a parameter nor a variadic for reflection.
///
/// `php -n` 8.5.10: `function ($a) { return func_get_args(); }` reports one parameter and
/// `isVariadic() === false`.
#[test]
fn reflection_ignores_the_hidden_func_args_collector() {
    let program = parse_fragment(
        br#"$ref = new ReflectionFunction("hidden_probe");
echo $ref->getNumberOfParameters() . ":" . ($ref->isVariadic() ? "V" : "v");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut native = NativeFunction::new(1usize as *mut c_void, fake_native_return_descriptor, 2);
    assert!(native.set_param_name(0, "a"));
    assert!(native.set_param_name(1, "__elephc_func_args#gen"));
    assert!(native.set_variadic_index(1));
    crate::context::mark_hidden_func_args_collector(&mut native);
    let mut context = ElephcEvalContext::new();
    assert!(context.define_native_function("hidden_probe", native).is_ok());
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.output, "1:v");
}
