//! Purpose:
//! Home of the PHP `uasort` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(fixed(["array", "callback"]))`: exactly 2
//!   arguments, the `array` param is by-reference. The `ref` marker drives in-place
//!   mutation (ir_lower reads `ref_params` from the registry sig).
//! - `check` derives the comparator element type from the array value type, validates the
//!   callback with two dummy element arguments (comparator receives two values), and handles
//!   object-element arrays with typed closure hints. Returns `Void`.
//! - `lower` is a thin wrapper over the shared `arrays::lower_uasort` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "uasort",
    area: Array,
    params: [ref array: Mixed, callback: Mixed],
    returns: Void,
    check: check,
    lazy_check: true,
    lower: lower,
    summary: "Sorts an array with a user-defined comparison function and maintains index association.",
    php_manual: "https://www.php.net/manual/en/function.uasort.php",
}

/// Validates the array and comparator callback arguments for a `uasort` call.
///
/// Infers the array value element type, and validates the comparator with two dummy
/// arguments of that element type. Object-element arrays use typed closure hints so
/// an unannotated comparator body (`$a <=> $b`) is checked against the real type; a
/// Mixed element (unknown-element array) takes the same path so a TYPED comparator's
/// declared parameter contract stands (runtime-enforced) instead of a fabricated Int.
/// Arity (exactly 2) is pre-validated by the registry. Returns `Ok(PhpType::Void)`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let cmp_ty = crate::types::checker::builtins::array_element_type(&arr_ty);
    let label = format!("{}() callback", cx.name);
    if matches!(cmp_ty, PhpType::Object(_) | PhpType::Mixed) {
        if let ExprKind::Closure {
            params,
            variadic,
            variadic_by_ref,
            return_type,
            body,
            captures,
            capture_refs,
            ..
        } = &cx.args[1].kind
        {
            cx.checker.infer_closure_type_with_param_hints(
                params,
                variadic,
                *variadic_by_ref,
                return_type,
                body,
                captures,
                capture_refs,
                &cx.args[1],
                cx.env,
                &[cmp_ty.clone(), cmp_ty.clone()],
            )?;
        } else {
            cx.checker.infer_type(&cx.args[1], cx.env)?;
            let (cmp_arg, elem_binding) =
                crate::types::checker::builtins::comparator_dummy_arg_for_elem(&cmp_ty, cx.span);
            let dummy_args = vec![cmp_arg.clone(), cmp_arg];
            let mut env_with_elem;
            let cb_env: &crate::types::TypeEnv = match &elem_binding {
                Some((binding_name, binding_ty)) => {
                    env_with_elem = cx.env.clone();
                    env_with_elem.insert(binding_name.clone(), binding_ty.clone());
                    &env_with_elem
                }
                None => cx.env,
            };
            crate::types::checker::builtins::check_callback_builtin_call(
                cx.checker,
                &cx.args[1],
                &dummy_args,
                cx.span,
                cb_env,
                &label,
            )?;
        }
    } else {
        cx.checker.infer_type(&cx.args[1], cx.env)?;
        let cmp_arg =
            crate::types::checker::builtins::dummy_arg_for_array_scalar_elem(&arr_ty, cx.span);
        let dummy_args = vec![cmp_arg.clone(), cmp_arg];
        crate::types::checker::builtins::check_callback_builtin_call(
            cx.checker,
            &cx.args[1],
            &dummy_args,
            cx.span,
            cx.env,
            &label,
        )?;
    }
    Ok(PhpType::Void)
}

/// Lowers a `uasort` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_uasort(ctx, inst)
}
