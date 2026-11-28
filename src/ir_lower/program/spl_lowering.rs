//! Purpose:
//! Supported builtin SPL method body lowering and deduplication.
//!
//! Called from:
//! - `crate::ir_lower::program`.
//!
//! Key details:
//! - Keeps program metadata deterministic and EIR lowering behavior unchanged.

use super::*;

/// Lowers one supported builtin SPL method body if it has not already been emitted.
pub(super) fn lower_builtin_spl_method(
    class_name: &str,
    method_key: &str,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &crate::types::GlobalConstants,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    if class_method_already_lowered(module, class_name, method_key, false)
        || !is_supported_builtin_spl_method(class_name, method_key)
        || runtime_intrinsic_method_has_wrapper(class_name, method_key, false)
    {
        return;
    }
    let Some(class_info) = check_result.classes.get(class_name) else {
        return;
    };
    let Some(method) = class_info
        .method_decls
        .iter()
        .find(|method| php_method_key(&method.name) == method_key && method.has_body)
    else {
        return;
    };
    function::lower_class_method(
        class_name,
        &method.name,
        method.is_static,
        &method.params,
        method.return_type.as_ref(),
        &method.body,
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
}

/// Returns true when `module.class_methods` already contains a class-method body.
pub(in crate::ir_lower) fn class_method_already_lowered(
    module: &Module,
    class_name: &str,
    method_key: &str,
    is_static: bool,
) -> bool {
    module.class_methods.iter().any(|function| {
        function.flags.is_static == is_static
            && function
                .name
                .rsplit_once("::")
                .is_some_and(|(candidate_class, candidate_method)| {
                    candidate_class == class_name && php_method_key(candidate_method) == method_key
                })
    })
}
