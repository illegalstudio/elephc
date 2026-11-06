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

use super::*;
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
