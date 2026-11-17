//! Purpose:
//! Builds the program-specific internal object comparator used by shared loose equality.
//!
//! Called from:
//! - `crate::ir_lower::program` after class methods have been lowered.
//!
//! Key details:
//! - Ordinary EIR static calls preserve source arguments and generated argc/collector slots.
//! - A result of 3 means no internal handler; unordered comparisons keep their distinct result.
//! - Programs without handlers use a constant stub that never clones borrowed operands.
//! - `__elephc_internal_order_compare` is php's ordering of two arrays or two objects, which the
//!   shared `__rt_php_compare` calls for a pair of non-scalars; 2 means uncomparable.

use super::*;
use crate::parser::ast::CastType;
use crate::synthetic_class::*;

/// Emits a borrowed two-cell comparator even when the program has no internal operator classes.
pub(in crate::ir_lower) fn lower_object_comparison_dispatch(module: &mut Module) {
    let mut classes = module.class_infos.keys().collect::<Vec<_>>();
    classes.sort();
    let mut statements = Vec::new();
    for name in classes {
        let Some(class) = elephc_builtin_contract::lookup_class(name) else { continue; };
        let Some(method) = class.operators.and_then(|operators| operators.compare) else { continue; };
        let condition = e_binop(e_instance_of(e_var("left"), name), BinOp::Or,
            e_instance_of(e_var("right"), name));
        statements.push(s_if(condition, vec![s_return(e_static_call(name, method,
            vec![e_var("left"), e_var("right")]))], Vec::new(), None));
    }
    let has_handlers = !statements.is_empty();
    statements.push(s_return(e_int(3)));
    let signature = FunctionSig {
        params: vec![("left".into(), PhpType::Mixed), ("right".into(), PhpType::Mixed)],
        param_type_exprs: vec![Some(t_mixed()); 2], param_attributes: vec![Vec::new(); 2],
        defaults: vec![None; 2], return_type: PhpType::Int, declared_return: true,
        by_ref_return: false, ref_params: vec![false; 2], declared_params: vec![true; 2],
        variadic: None, deprecation: None, is_generator: false,
    };
    let mut function = if has_handlers {
        super::builtin_wrappers::lower_builtin_callable_body(
            module, "__elephc_internal_object_compare", &signature, false, statements,
        )
    } else {
        lower_no_handler_stub(&signature)
    };
    function.flags.is_internal = true;
    function.flags.is_synthetic = true;
    module.add_function(function);
}

/// Symbol of the ordering comparator `__rt_php_compare` calls for two non-scalar operands.
pub(in crate::ir_lower) const ORDER_COMPARE_FUNCTION: &str = "__elephc_internal_order_compare";

/// Emits `__elephc_internal_order_compare(mixed $left, mixed $right): int`, php's `zend_compare`
/// for two arrays or two objects, which `__rt_php_compare` used to report as equal.
///
/// Two arrays order by element count, then key by key in the left array's order: a key the right
/// array lacks makes the pair uncomparable, otherwise the first element pair that differs (through
/// `<=>`, so nested arrays recurse) decides. Two objects are equal when they are the same object,
/// uncomparable when their classes differ, and otherwise compare their properties the same way
/// (`zend_std_compare_objects`); a `DateTimeInterface` pair orders by its instant. Answers -1, 0
/// or 1, `2` for an uncomparable pair (the runtime reports it like an unordered NaN: every
/// relational operator is false and `<=>` is 1), and `3` for any other pair, which keeps the
/// runtime's own rule.
pub(in crate::ir_lower) fn lower_order_comparison_dispatch(module: &mut Module) {
    let mut statements = Vec::new();
    if module.interface_infos.contains_key("DateTimeInterface") {
        let instant = |side: &str, method: &str| {
            e_method_call(e_var(side), method, vec![])
        };
        let micros = |side: &str| e_cast(CastType::Int, e_method_call(e_var(side), "format", vec![e_str("u")]));
        statements.push(s_if(
            e_binop(
                e_instance_of(e_var("left"), "DateTimeInterface"),
                BinOp::And,
                e_instance_of(e_var("right"), "DateTimeInterface"),
            ),
            vec![
                s_assign("seconds", e_binop(instant("left", "getTimestamp"), BinOp::Spaceship, instant("right", "getTimestamp"))),
                s_if(
                    e_binop(e_var("seconds"), BinOp::StrictNotEq, e_int(0)),
                    vec![s_return(e_var("seconds"))],
                    Vec::new(),
                    None,
                ),
                s_return(e_binop(micros("left"), BinOp::Spaceship, micros("right"))),
            ],
            Vec::new(),
            None,
        ));
    }
    let mut objects = vec![
        s_if(
            e_binop(e_var("left"), BinOp::StrictEq, e_var("right")),
            vec![s_return(e_int(0))],
            Vec::new(),
            None,
        ),
        s_if(
            e_binop(
                e_call("get_class", vec![e_var("left")]),
                BinOp::StrictNotEq,
                e_call("get_class", vec![e_var("right")]),
            ),
            vec![s_return(e_int(2))],
            Vec::new(),
            None,
        ),
        s_assign("left_props", e_cast(CastType::Array, e_var("left"))),
        s_assign("right_props", e_cast(CastType::Array, e_var("right"))),
    ];
    objects.extend(ordered_entries_walk("left_props", "right_props", "prop"));
    statements.push(s_if(
        e_binop(e_call("is_object", vec![e_var("left")]), BinOp::And, e_call("is_object", vec![e_var("right")])),
        objects,
        Vec::new(),
        None,
    ));
    statements.push(s_if(
        e_binop(e_call("is_array", vec![e_var("left")]), BinOp::And, e_call("is_array", vec![e_var("right")])),
        ordered_entries_walk("left", "right", "item"),
        Vec::new(),
        None,
    ));
    statements.push(s_return(e_int(3)));
    let signature = FunctionSig {
        params: vec![("left".into(), PhpType::Mixed), ("right".into(), PhpType::Mixed)],
        param_type_exprs: vec![Some(t_mixed()); 2], param_attributes: vec![Vec::new(); 2],
        defaults: vec![None; 2], return_type: PhpType::Int, declared_return: true,
        by_ref_return: false, ref_params: vec![false; 2], declared_params: vec![true; 2],
        variadic: None, deprecation: None, is_generator: false,
    };
    let mut function = super::builtin_wrappers::lower_builtin_callable_body(
        module, ORDER_COMPARE_FUNCTION, &signature, false, statements,
    );
    function.flags.is_internal = true;
    function.flags.is_synthetic = true;
    module.add_function(function);
}

/// php's `zend_hash_compare` with ordered values: the element counts, then every key of `left`
/// in order (absent from `right`: uncomparable, `2`), the first `<=>` that is not zero deciding.
/// `prefix` keeps the locals of the array walk and the property walk apart.
fn ordered_entries_walk(left: &str, right: &str, prefix: &str) -> Vec<Stmt> {
    let local = |name: &str| format!("{prefix}_{name}");
    let (left_count, right_count) = (local("left_count"), local("right_count"));
    let (key, value, order) = (local("key"), local("value"), local("order"));
    vec![
        s_assign(&left_count, e_call("count", vec![e_var(left)])),
        s_assign(&right_count, e_call("count", vec![e_var(right)])),
        s_if(
            e_binop(e_var(&left_count), BinOp::StrictNotEq, e_var(&right_count)),
            vec![s_return(e_ternary(
                e_binop(e_var(&left_count), BinOp::Lt, e_var(&right_count)),
                e_neg(e_int(1)),
                e_int(1),
            ))],
            Vec::new(),
            None,
        ),
        s_foreach(
            e_var(left),
            Some(&key),
            &value,
            vec![
                s_if(
                    e_not(e_call("array_key_exists", vec![e_var(&key), e_var(right)])),
                    vec![s_return(e_int(2))],
                    Vec::new(),
                    None,
                ),
                s_assign(
                    &order,
                    e_binop(e_var(&value), BinOp::Spaceship, e_index(e_var(right), e_var(&key))),
                ),
                s_if(
                    e_binop(e_var(&order), BinOp::StrictNotEq, e_int(0)),
                    vec![s_return(e_var(&order))],
                    Vec::new(),
                    None,
                ),
            ],
        ),
        s_return(e_int(0)),
    ]
}

/// Keeps the runtime callback ABI without creating unused owning parameter shadows.
fn lower_no_handler_stub(signature: &FunctionSig) -> Function {
    let mut function = Function::new(
        "__elephc_internal_object_compare".into(), IrType::I64, PhpType::Int,
    );
    function.params = function_params(signature);
    function.signature = Some(signature.clone());
    for parameter in function.params.clone() {
        function.add_local(Some(parameter.name), parameter.ir_type, parameter.php_type,
            crate::ir::LocalKind::BorrowedTemp);
    }
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", Vec::new());
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let result = builder.emit_const_i64(3);
    builder.terminate(Terminator::Return { value: Some(result) });
    function
}
