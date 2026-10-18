//! Purpose:
//! Builds internal declarations for Core attributes, Throwable constructors,
//! sensitive-value wrappers, and weak collections.
//!
//! Called from:
//! - The compilation pipeline and integration-test frontend before name resolution.
//!
//! Key details:
//! - Real method bodies follow the shared AST to EIR path on every supported target.
//! - Introspection and opaque eval retain the complete Core class surface.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, StmtKind, TypeExpr};
mod weak_map;
use crate::synthetic_class::{
    attr, class, e_array, e_array_assoc, e_binop, e_cast, e_index, e_int, e_method_call, e_new_fq, e_null, e_parent_call, e_str, e_this_prop, e_var,
    internal_declarations, method, s_assign, s_expr, s_if, s_prop_assign, s_throw, t_class, t_mixed, t_nullable,
};

/// Builds Core's predefined attributes and the value wrapper used in sensitive traces.
pub(crate) fn core_declarations() -> Program {
    internal_declarations(|| {
        let mut attribute = class("Attribute").final_()
            .prop("flags", TypeExpr::Int, None)
            .method(method("__construct")
                .param_default("flags", TypeExpr::Int, e_int(elephc_builtin_contract::CORE_ATTRIBUTE_TARGET_ALL))
                .body(vec![s_prop_assign(crate::synthetic_class::e_this(), "flags", e_var("flags"))]));
        for &(name, value) in elephc_builtin_contract::CORE_ATTRIBUTE_CONSTANTS {
            attribute = attribute.constant(name, e_int(value));
        }
        let mut declarations = vec![as_attribute(attribute.build(), 1)];
        for (name, targets) in [
            ("AllowDynamicProperties", 1), ("ReturnTypeWillChange", 4),
            ("SensitiveParameter", 32), ("Override", 12),
        ] {
            declarations.push(as_attribute(class(name).final_()
                .method(method("__construct")).build(), targets));
        }
        declarations.push(as_attribute(class("DelayedTargetValidation").final_().build(), elephc_builtin_contract::CORE_ATTRIBUTE_TARGET_ALL));
        let nullable_string = t_nullable(TypeExpr::Str);
        declarations.push(as_attribute(class("Deprecated").final_()
            .readonly_prop("message", nullable_string.clone())
            .readonly_prop("since", nullable_string.clone())
            .method(method("__construct")
                .param_default("message", nullable_string.clone(), e_null())
                .param_default("since", nullable_string.clone(), e_null())
                .body(vec![
                    s_prop_assign(crate::synthetic_class::e_this(), "message", e_var("message")),
                    s_prop_assign(crate::synthetic_class::e_this(), "since", e_var("since")),
                ]))
            .build(), 87));
        declarations.push(as_attribute(class("NoDiscard").final_()
            .readonly_prop("message", nullable_string.clone())
            .method(method("__construct")
                .param_default("message", nullable_string, e_null())
                .body(vec![s_prop_assign(crate::synthetic_class::e_this(), "message", e_var("message"))]))
            .build(), 6));

        let mut sensitive = class("SensitiveParameterValue").final_()
            .private_prop("value", t_mixed(), None)
            .method(method("__construct").param("value", t_mixed())
                .body(vec![s_prop_assign(crate::synthetic_class::e_this(), "value", e_var("value"))]))
            .method(method("getValue").returns(t_mixed()).returning(e_this_prop("value")))
            .method(method("__debugInfo").returns(crate::synthetic_class::t_array())
                .returning(e_array(vec![])))

            .build();
        if let StmtKind::ClassDecl { properties, .. } = &mut sensitive.kind {
            properties[0].readonly = true;
        }
        declarations.push(sensitive);
        declarations.push(class("WeakReference").final_()
            .private_prop("__elephc_target", TypeExpr::Int, Some(e_int(0)))
            .method(method("__construct").body(vec![s_throw(e_new_fq("Error", vec![e_str(
                "Direct instantiation of WeakReference is not allowed, use WeakReference::create instead",
            )]))]))
            .method(method("create").static_().param("object", t_class("object"))
                .returns(t_class("WeakReference")).body(vec![
                    crate::synthetic_class::s_if(crate::synthetic_class::e_not(
                        crate::synthetic_class::e_call("is_object", vec![e_var("object")])),
                        vec![s_throw(e_new_fq("TypeError", vec![e_str("WeakReference::create() argument must be an object")]))], vec![], None),
                    s_throw(e_new_fq("Error", vec![e_str(
                    "WeakReference native creation was not lowered",
                )]))]))
            .method(method("get").returns(t_nullable(t_class("object"))).returning(e_null()))
            .method(method("__debugInfo").returns(crate::synthetic_class::t_array())
                .returning(e_array_assoc(vec![(e_str("object"), e_method_call(crate::synthetic_class::e_this(), "get", vec![]))])))
            .method(method("__clone").body(vec![s_throw(e_new_fq("Error", vec![e_str(
                "Trying to clone an uncloneable object of class WeakReference",
            )]))]))
            .build());
        declarations.push(class("ErrorException").extends("Exception")
            .protected_prop("severity", TypeExpr::Int, Some(e_int(1)))
            .private_prop("__elephc_filename", t_nullable(TypeExpr::Str), Some(e_null()))
            .private_prop("__elephc_line", t_nullable(TypeExpr::Int), Some(e_null()))
            .method(method("__construct")
                .param_default("message", TypeExpr::Str, e_str(""))
                .param_default("code", TypeExpr::Int, e_int(0))
                .param_default("severity", TypeExpr::Int, e_int(1))
                .param_default("filename", t_nullable(TypeExpr::Str), e_null())
                .param_default("line", t_nullable(TypeExpr::Int), e_null())
                .param_default("previous", t_nullable(t_class("Throwable")), e_null())
                .variadic(crate::func_args::HIDDEN_ARGS_PARAM, Some(t_mixed()))
                .body(vec![
                    s_assign("__elephc_error_argc#gen", e_cast(CastType::Int,
                        e_index(e_var(crate::func_args::HIDDEN_ARGS_PARAM), e_int(0)))),
                    s_if(
                        e_binop(e_var("__elephc_error_argc#gen"), BinOp::Gt, e_int(6)),
                        vec![s_throw(e_new_fq("ArgumentCountError", vec![e_binop(
                            e_str("ErrorException::__construct() expects at most 6 arguments, "),
                            BinOp::Concat,
                            e_binop(e_var("__elephc_error_argc#gen"), BinOp::Concat, e_str(" given")),
                        )]))], vec![], None,
                    ),
                    s_if(
                        e_binop(e_var("__elephc_error_argc#gen"), BinOp::StrictEq, e_int(0)),
                        vec![s_assign("message", e_method_call(crate::synthetic_class::e_this(), "getMessage", vec![]))],
                        vec![], None,
                    ),
                    s_if(
                        e_binop(e_var("code"), BinOp::StrictEq, e_int(0)),
                        vec![s_assign("code", e_method_call(crate::synthetic_class::e_this(), "getCode", vec![]))],
                        vec![], None,
                    ),
                    s_if(
                        e_binop(e_var("previous"), BinOp::StrictEq, e_null()),
                        vec![s_assign("previous", e_method_call(crate::synthetic_class::e_this(), "getPrevious", vec![]))],
                        vec![], None,
                    ),
                    s_expr(e_parent_call("__construct", vec![e_var("message"), e_var("code"), e_var("previous")])),
                    s_prop_assign(crate::synthetic_class::e_this(), "severity", e_var("severity")),
                    s_if(
                        e_binop(e_var("filename"), BinOp::StrictNotEq, e_null()),
                        vec![s_prop_assign(crate::synthetic_class::e_this(), "__elephc_filename", e_var("filename"))],
                        vec![], None,
                    ),
                    s_if(
                        e_binop(e_var("line"), BinOp::StrictNotEq, e_null()),
                        vec![s_prop_assign(crate::synthetic_class::e_this(), "__elephc_line", e_var("line"))],
                        vec![(
                            e_binop(e_var("filename"), BinOp::StrictNotEq, e_null()),
                            vec![s_prop_assign(crate::synthetic_class::e_this(), "__elephc_line", e_int(0))],
                        )], None,
                    ),
                ]))
            .method(method("getSeverity").final_().returns(TypeExpr::Int).returning(e_this_prop("severity")))
            .build());
        declarations.extend(weak_map::declarations());
        declarations
    })
}

/// Attaches PHP's Attribute metadata to a built class declaration.
fn as_attribute(mut declaration: Stmt, targets: i64) -> Stmt {
    declaration.attributes.push(attr("\\Attribute", vec![e_int(targets)]));
    declaration
}

/// Injects the reachable Core class declarations, widening for introspection and eval.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        let usage = crate::prelude_prune::usage::collect(&program);
        let mut declarations = core_declarations();
        let dynamic = usage.introspects || usage.constructs_dynamic_class;
        let weak_map = dynamic || usage.classes.contains("weakmap") || usage.literals.contains("weakmap");
        let weak_reference = weak_map || usage.classes.contains("weakreference") || usage.literals.contains("weakreference");
        declarations.retain(|stmt| {
            let StmtKind::ClassDecl { name, .. } = &stmt.kind else { return false };
            if name == "WeakMap" || name == "__ElephcWeakMapIterator" { return weak_map; }
            if name == "WeakReference" { return weak_reference; }
            // Attribute classes are tiny and their names also appear in declaration metadata,
            // outside the usage walk's expression/class-reference inventory.
            true
        });
        inventory.record_program("core", &declarations);
        declarations.extend(program);
        declarations
    })
}

#[cfg(test)]
mod tests {
    /// Core declaration bodies and interfaces remain valid for every supported target.
    #[test]
    fn core_declarations_check_on_every_supported_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let tokens = crate::lexer::tokenize("<?php get_declared_classes();").unwrap();
            let program = crate::parser::parse(&tokens).unwrap();
            let mut inventory = crate::optimize::reachability::PreludeInventory::new();
            let program = super::inject_if_used(program, &mut inventory);
            let program = crate::name_resolver::resolve(program).unwrap();
            let target = crate::codegen_support::platform::Target::parse(target).unwrap();
            let result = crate::types::checker::check_types(&program, target)
                .unwrap_or_else(|error| panic!("Core declarations on {}: {error:?}", target.as_str()));
            assert!(result.classes.contains_key("WeakMap"));
            assert!(result.classes.contains_key("WeakReference"));
        }
    }
}
