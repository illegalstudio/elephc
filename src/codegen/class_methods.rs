//! Purpose:
//! Builds and emits codegen signatures for class, interface, enum, and trait methods.
//! Handles receiver layout, static dispatch symbols, and method body emission.
//!
//! Called from:
//! - `crate::codegen::generate()` when class metadata contains methods
//!
//! Key details:
//! - Generated signatures must line up with object dispatch, vtables, and inherited method metadata.

use std::collections::{HashMap, HashSet};

use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::functions;
use crate::names::{method_symbol, php_symbol_key, static_method_symbol};
use crate::parser::ast::ExprKind;
use crate::types::{
    ClassInfo, ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo, PackedClassInfo,
    PhpType,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_class_methods(
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_name: &str,
    class_info: &ClassInfo,
    functions: &HashMap<String, FunctionSig>,
    function_variant_groups: &HashSet<String>,
    global_constants: &HashMap<String, (ExprKind, PhpType)>,
    interfaces: &HashMap<String, InterfaceInfo>,
    classes: &HashMap<String, ClassInfo>,
    packed_classes: &HashMap<String, PackedClassInfo>,
    extern_functions: &HashMap<String, ExternFunctionSig>,
    extern_classes: &HashMap<String, ExternClassInfo>,
    extern_globals: &HashMap<String, PhpType>,
) {
    for method in &class_info.method_decls {
        let method_key = php_symbol_key(&method.name);
        if method.is_abstract {
            continue;
        }
        let (label, sig) = if method.is_static {
            build_static_method_codegen_sig(class_name, class_info, &method_key, method)
        } else {
            build_instance_method_codegen_sig(class_name, class_info, &method_key, method)
        };
        let epilogue_label = format!("{}_epilogue", label);
        functions::emit_method(
            emitter,
            data,
            &label,
            &epilogue_label,
            &sig,
            &method.body,
            functions,
            function_variant_groups,
            global_constants,
            interfaces,
            classes,
            packed_classes,
            class_name,
            extern_functions,
            extern_classes,
            extern_globals,
        );
    }
}

fn build_static_method_codegen_sig(
    class_name: &str,
    class_info: &ClassInfo,
    method_key: &str,
    method: &crate::parser::ast::ClassMethod,
) -> (String, FunctionSig) {
    let label = static_method_symbol(class_name, method_key);
    let class_static_sig = class_info.static_methods.get(method_key);
    let mut params: Vec<(String, PhpType)> =
        vec![("__elephc_called_class_id".to_string(), PhpType::Int)];
    if let Some(sig) = class_static_sig {
        params.extend(sig.params.clone());
    } else {
        params.extend(
            method
                .params
                .iter()
                .map(|(n, _, _, _)| (n.clone(), PhpType::Int)),
        );
    }
    let mut defaults: Vec<Option<crate::parser::ast::Expr>> = vec![None];
    if let Some(sig) = class_static_sig {
        defaults.extend(sig.defaults.clone());
    } else {
        defaults.extend(method.params.iter().map(|(_, _, d, _)| d.clone()));
        if method.variadic.is_some() {
            defaults.push(None);
        }
    }
    let mut ref_params: Vec<bool> = vec![false];
    if let Some(sig) = class_static_sig {
        ref_params.extend(sig.ref_params.clone());
    } else {
        ref_params.extend(method.params.iter().map(|(_, _, _, r)| *r));
        if method.variadic.is_some() {
            ref_params.push(false);
        }
    }
    let mut declared_params: Vec<bool> = vec![false];
    if let Some(sig) = class_static_sig {
        declared_params.extend(sig.declared_params.clone());
    } else {
        declared_params.extend(
            method
                .params
                .iter()
                .map(|(_, type_ann, _, _)| type_ann.is_some()),
        );
        if method.variadic.is_some() {
            declared_params.push(false);
        }
    }
    let return_type = class_static_sig
        .map(|s| s.return_type.clone())
        .unwrap_or(PhpType::Int);
    let declared_return = class_static_sig
        .map(|s| s.declared_return)
        .unwrap_or(method.return_type.is_some());
    (
        label,
        FunctionSig {
            params,
            defaults,
            return_type,
            declared_return,
            ref_params,
            declared_params,
            variadic: method.variadic.clone(),
            deprecation: None,
        },
    )
}

fn build_instance_method_codegen_sig(
    class_name: &str,
    class_info: &ClassInfo,
    method_key: &str,
    method: &crate::parser::ast::ClassMethod,
) -> (String, FunctionSig) {
    let label = method_symbol(class_name, method_key);
    let class_method_sig = class_info.methods.get(method_key);
    let mut params: Vec<(String, PhpType)> = vec![
        ("this".to_string(), PhpType::Object(class_name.to_string())),
    ];
    if let Some(sig) = class_method_sig {
        params.extend(sig.params.clone());
    } else {
        params.extend(
            method
                .params
                .iter()
                .map(|(n, _, _, _)| (n.clone(), PhpType::Int)),
        );
    }
    let mut defaults: Vec<Option<crate::parser::ast::Expr>> = vec![None];
    if let Some(sig) = class_method_sig {
        defaults.extend(sig.defaults.clone());
    } else {
        defaults.extend(method.params.iter().map(|(_, _, d, _)| d.clone()));
        if method.variadic.is_some() {
            defaults.push(None);
        }
    }
    let mut ref_params: Vec<bool> = vec![false];
    if let Some(sig) = class_method_sig {
        ref_params.extend(sig.ref_params.clone());
    } else {
        ref_params.extend(method.params.iter().map(|(_, _, _, r)| *r));
        if method.variadic.is_some() {
            ref_params.push(false);
        }
    }
    let mut declared_params: Vec<bool> = vec![false];
    if let Some(sig) = class_method_sig {
        declared_params.extend(sig.declared_params.clone());
    } else {
        declared_params.extend(
            method
                .params
                .iter()
                .map(|(_, type_ann, _, _)| type_ann.is_some()),
        );
        if method.variadic.is_some() {
            declared_params.push(false);
        }
    }
    let return_type = class_method_sig
        .map(|s| s.return_type.clone())
        .unwrap_or(PhpType::Int);
    let declared_return = class_method_sig
        .map(|s| s.declared_return)
        .unwrap_or(method.return_type.is_some());
    (
        label,
        FunctionSig {
            params,
            defaults,
            return_type,
            declared_return,
            ref_params,
            declared_params,
            variadic: method.variadic.clone(),
            deprecation: None,
        },
    )
}
