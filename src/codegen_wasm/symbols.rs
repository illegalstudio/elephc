//! Purpose:
//! Defines the injective WebAssembly symbol ABI for PHP functions, methods,
//! closure bodies, and compiler-generated wrappers or dispatch stubs.
//!
//! Called from:
//! - `crate::codegen_wasm::function` selects the symbol for each EIR function.
//! - Call, object, method, and closure lowering use the category-specific helpers.
//!
//! Key details:
//! - PHP names are encoded through `crate::names::mangle_fqn`; no character is
//!   discarded, so distinct source names remain distinct.
//! - Every semantic category owns a disjoint prefix. A user-controlled name can
//!   therefore never impersonate a compiler-generated wrapper or dispatch stub.

use crate::ir::Function;
use crate::names::mangle_fqn;

/// Returns the internal symbol for a user-defined PHP free function.
pub(super) fn user_function_symbol(name: &str) -> String {
    format!("fn_u_{}", mangle_fqn(name))
}

/// Returns the internal symbol for a PHP class method.
pub(super) fn method_symbol(qualified_name: &str) -> String {
    format!("fn_m_{}", mangle_fqn(qualified_name))
}

/// Returns the internal symbol for a lowered closure body.
pub(super) fn closure_body_symbol(name: &str) -> String {
    format!("fn_c_{}", mangle_fqn(name))
}

/// Returns the internal symbol for a compiler-generated first-class-callable wrapper.
pub(super) fn fcc_wrapper_symbol(target_name: &str) -> String {
    format!("fn_gf_{}", mangle_fqn(target_name))
}

/// Returns the internal symbol for a compiler-generated closure ABI wrapper.
pub(super) fn closure_wrapper_symbol(closure_name: &str) -> String {
    format!("fn_gc_{}", mangle_fqn(closure_name))
}

/// Returns the internal symbol for an instance-method virtual-dispatch stub.
///
/// Each component carries the byte length of its injective encoding. This makes
/// the pair framing unambiguous even when either PHP identifier contains text
/// resembling the separator or another mangling escape.
pub(super) fn method_dispatch_symbol(introducer: &str, method_key: &str) -> String {
    let introducer = mangle_fqn(introducer);
    let method_key = mangle_fqn(method_key);
    format!(
        "fn_gd_{}_{}_{}_{}",
        introducer.len(),
        introducer,
        method_key.len(),
        method_key
    )
}

/// Returns the category-specific symbol for an EIR function definition.
///
/// The priority mirrors the mutually exclusive compiler-generated surfaces:
/// methods and closures retain their PHP-visible identity categories, while
/// internal wrappers use reserved generated categories. Ordinary and generator
/// functions remain in the user-free-function category because calls address
/// both by their PHP function name.
pub(super) fn function_symbol(function: &Function) -> String {
    if function.flags.is_method {
        method_symbol(&function.name)
    } else if function.flags.is_closure {
        closure_body_symbol(&function.name)
    } else if function.flags.is_fiber_wrapper {
        format!("fn_gfiber_{}", mangle_fqn(&function.name))
    } else if function.flags.is_callback_wrapper {
        format!("fn_gcallback_{}", mangle_fqn(&function.name))
    } else if function.flags.is_runtime_callable_invoker {
        format!("fn_gruntime_{}", mangle_fqn(&function.name))
    } else if function.flags.is_synthetic {
        format!("fn_gsynthetic_{}", mangle_fqn(&function.name))
    } else {
        user_function_symbol(&function.name)
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Regression tests for the injective and category-separated WASM symbol ABI.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests cover the punctuation collision that the former lossy sanitizer
    //!   caused, generated/user namespace separation, and tuple framing.

    use super::{
        closure_body_symbol, closure_wrapper_symbol, fcc_wrapper_symbol, method_dispatch_symbol,
        method_symbol, user_function_symbol,
    };

    /// Verifies punctuation and namespace separators cannot collapse to one symbol.
    #[test]
    fn php_names_are_encoded_injectively() {
        assert_ne!(user_function_symbol("A-B"), user_function_symbol("A_B"));
        assert_ne!(user_function_symbol("A\\B"), user_function_symbol("A_B"));
        assert_ne!(user_function_symbol("é"), user_function_symbol("_xC3__xA9_"));
    }

    /// Verifies identical raw text remains disjoint across semantic categories.
    #[test]
    fn semantic_categories_cannot_collide() {
        let name = "__fcc_wrap_A::run";
        let symbols = [
            user_function_symbol(name),
            method_symbol(name),
            closure_body_symbol(name),
            fcc_wrapper_symbol(name),
            closure_wrapper_symbol(name),
        ];
        for (index, left) in symbols.iter().enumerate() {
            for right in symbols.iter().skip(index + 1) {
                assert_ne!(left, right);
            }
        }
    }

    /// Verifies dispatch-symbol component boundaries cannot be rearranged.
    #[test]
    fn dispatch_components_are_unambiguously_framed() {
        assert_ne!(
            method_dispatch_symbol("A_B", "C"),
            method_dispatch_symbol("A", "B_C")
        );
        assert_ne!(
            method_dispatch_symbol("A::B", "C"),
            method_dispatch_symbol("A", "B::C")
        );
    }

    /// Verifies symbol generation is deterministic across repeated calls.
    #[test]
    fn symbols_are_stable() {
        assert_eq!(
            method_dispatch_symbol("Vendor\\Thing", "run_now"),
            method_dispatch_symbol("Vendor\\Thing", "run_now")
        );
        assert_eq!(
            closure_wrapper_symbol("__eir_closure_main_0"),
            closure_wrapper_symbol("__eir_closure_main_0")
        );
    }
}
