//! Purpose:
//! Declares the immutable BCMath Number class through the normal synthetic AST pipeline.
//!
//! Called from:
//! - The compiler pipeline and codegen test harness before name resolution.
//!
//! Key details:
//! - Decimal work goes through internal builtin contracts backed by the shared Rust engine.
//! - Public readonly properties own ordinary PHP strings and need no custom finalizer.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, StmtKind, TypeExpr};
use crate::synthetic_class::*;

mod rounding;
mod operators;

pub(crate) const NUMBER_CLASS: &str = "\\BcMath\\Number";

/// Anchors engine helpers and PHP builtins outside the Number namespace.
fn global_call(name: &str, args: Vec<crate::parser::ast::Expr>) -> crate::parser::ast::Expr {
    e_call(&format!("\\{name}"), args)
}

/// Returns the operand type accepted by immutable Number methods.
fn operand_type() -> TypeExpr {
    TypeExpr::Union(vec![t_class(NUMBER_CLASS), TypeExpr::Str, TypeExpr::Int])
}

/// Converts a value through PHP's normal Stringable/string conversion rules.
fn decimal(value: crate::parser::ast::Expr) -> crate::parser::ast::Expr {
    e_cast(CastType::String, value)
}

/// Applies the internal string-or-integer parameter conversion before decimal parsing.
fn parameter_decimal(value: crate::parser::ast::Expr) -> crate::parser::ast::Expr {
    global_call("__elephc_bcnumber_parameter", vec![value])
}

/// Builds a typed internal decimal operation with ordinary evaluated call arguments.
fn operation(name: &str, args: Vec<crate::parser::ast::Expr>) -> crate::parser::ast::Expr {
    global_call(&format!("__elephc_bcnumber_{name}"), args)
}

/// Initializes readonly Number storage from one normalized decimal string.
fn initialize(value: crate::parser::ast::Expr) -> Vec<Stmt> {
    vec![
        s_if(e_call("isset", vec![e_this_prop("value")]), vec![s_throw(e_new_fq("Error", vec![
            e_str("Cannot modify readonly property BcMath\\Number::$value"),
        ]))], vec![], None),
        s_assign("_bcvalue", operation("create", vec![value])),
        s_prop_assign(e_this(), "value", e_var("_bcvalue")),
        s_assign("_bcpoint", global_call("strpos", vec![e_var("_bcvalue"), e_str(".")])),
        s_if(
            e_binop(e_var("_bcpoint"), BinOp::StrictEq, e_bool(false)),
            vec![s_prop_assign(e_this(), "scale", e_int(0))],
            vec![],
            Some(vec![s_prop_assign(e_this(), "scale", e_cast(CastType::Int,
                e_binop(e_binop(global_call("strlen", vec![e_var("_bcvalue")]), BinOp::Sub,
                    e_cast(CastType::Int, e_var("_bcpoint"))), BinOp::Sub, e_int(1))))]),
        ),
    ]
}

/// Builds the complete public Number method surface and internal compiler adapters.
pub(crate) fn bcmath_declarations() -> Program {
    internal_declarations(|| {
        let mut number = class("Number")
            .final_()
            .implements("\\Stringable")
            .readonly_prop("value", TypeExpr::Str)
            .readonly_prop("scale", TypeExpr::Int)
            .method(method("__construct")
                .param("num", TypeExpr::Union(vec![TypeExpr::Str, TypeExpr::Int]))
                .body(initialize(parameter_decimal(e_var("num")))))
            .method(method("__toString").returns(TypeExpr::Str).returning(e_this_prop("value")))
            .method(method("__serialize").returns(t_array())
                .returning(e_array_assoc(vec![(e_str("value"), e_this_prop("value"))])));

        for name in ["add", "sub", "mul", "div", "mod", "pow", "compare"] {
            let parameter = if name == "pow" { "exponent" } else { "num" };
            let args = vec![e_this_prop("value"), parameter_decimal(e_var(parameter)), e_var("scale")];
            let result = operation(if name == "compare" { "comp" } else { name }, args);
            number = number.method(method(name)
                .param(parameter, operand_type())
                .param_default("scale", t_nullable(TypeExpr::Int), e_null())
                .returns(if name == "compare" { TypeExpr::Int } else { t_class(NUMBER_CLASS) })
                .returning(if name == "compare" { result } else { e_new_fq(NUMBER_CLASS, vec![result]) }));
        }
        for name in ["ceil", "floor"] {
            number = number.method(method(name).returns(t_class(NUMBER_CLASS))
                .returning(e_new_fq(NUMBER_CLASS, vec![operation(name, vec![e_this_prop("value")])])));
        }
        number = operators::add_adapters(number);
        number = number
            .method(method("sqrt")
                .param_default("scale", t_nullable(TypeExpr::Int), e_null())
                .returns(t_class(NUMBER_CLASS))
                .returning(e_new_fq(NUMBER_CLASS, vec![operation("sqrt", vec![e_this_prop("value"), e_var("scale")])])))
            .method(method("powmod")
                .param("exponent", operand_type()).param("modulus", operand_type())
                .param_default("scale", t_nullable(TypeExpr::Int), e_null())
                .returns(t_class(NUMBER_CLASS))
                .returning(e_new_fq(NUMBER_CLASS, vec![operation("powmod", vec![
                    e_this_prop("value"), parameter_decimal(e_var("exponent")), parameter_decimal(e_var("modulus")), e_var("scale"),
                ])])))
            .method(method("divmod")
                .param("num", operand_type())
                .param_default("scale", t_nullable(TypeExpr::Int), e_null())
                .returns(TypeExpr::Array(Box::new(t_class(NUMBER_CLASS))))
                .body(vec![
                    s_assign("_bcpair", operation("divmod", vec![e_this_prop("value"), parameter_decimal(e_var("num")), e_var("scale")])),
                    s_return(e_array(vec![
                        e_new_fq(NUMBER_CLASS, vec![e_index(e_var("_bcpair"), e_int(0))]),
                        e_new_fq(NUMBER_CLASS, vec![e_index(e_var("_bcpair"), e_int(1))]),
                    ])),
                ]))
            .method(method("round")
                .param_default("precision", TypeExpr::Int, e_int(0))
                .param_default("mode", t_class("\\RoundingMode"), e_class_const("\\RoundingMode", "HalfAwayFromZero"))
                .returns(t_class(NUMBER_CLASS))
                .returning(e_new_fq(NUMBER_CLASS, vec![operation("round", vec![e_this_prop("value"), e_var("precision"), global_call("__elephc_rounding_mode", vec![e_var("mode")])])])));

        let invalid = || s_throw(e_new_fq("Exception", vec![e_str("Invalid serialization data for BcMath\\Number object")]));
        let mut restore = vec![
            s_if(e_not(global_call("array_key_exists", vec![e_str("value"), e_var("data")])), vec![invalid()], vec![], None),
            s_assign("_bcserialized", e_index(e_var("data"), e_str("value"))),
            s_if(e_not(global_call("is_string", vec![e_var("_bcserialized")])), vec![invalid()], vec![], None),
            s_if(e_binop(e_var("_bcserialized"), BinOp::StrictEq, e_str("")), vec![invalid()], vec![], None),
        ];
        restore.push(s_try(vec![s_expr(e_method_call(e_this(), "__construct", vec![decimal(e_var("_bcserialized"))]))],
            vec![(vec!["\\ValueError"], None, vec![invalid()])], None));
        number = number.method(method("__unserialize").param("data", t_array()).returns(TypeExpr::Void).body(restore));

        let mut declaration = number.build();
        if let StmtKind::ClassDecl { is_readonly_class, .. } = &mut declaration.kind {
            *is_readonly_class = true;
        }
        let mut declarations = rounding::declarations();
        declarations.extend(operators::declarations());
        declarations.push(s_namespace("BcMath", vec![declaration]));
        declarations
    })
}

/// Injects Number when referenced, reflected, evaluated, or explicitly enabled.
pub fn inject_if_used(
    program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        let usage = crate::prelude_prune::usage::collect(&program);
        let reflected = ["get_declared_classes", "get_declared_interfaces", "get_declared_traits"]
            .iter().any(|name| usage.functions.contains(*name));
        let named_in_source = usage.literals.iter().any(|literal| literal.contains("bcmath\\number"));
        if !force && !reflected && !named_in_source
            && !usage.classes.iter().any(|name| name.rsplit('\\').next() == Some("number") || name == "roundingmode")
        {
            return program;
        }
        let mut declarations = bcmath_declarations();
        inventory.record_program("bcmath", &declarations);
        declarations.extend(program);
        declarations
    })
}
