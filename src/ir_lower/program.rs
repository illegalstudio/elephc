//! Purpose:
//! Orchestrates AST-to-EIR lowering for a complete checked program.
//!
//! Called from:
//! - `crate::ir_lower::lower_program()`.
//!
//! Key details:
//! - Declaration bodies are lowered before synthetic `main`; declaration
//!   statements themselves are no-ops inside `main`.
//! - The module is validated before it is returned to CLI/test callers.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use crate::codegen::platform::Target;
use crate::codegen::RuntimeFeatures;
use crate::intrinsics::IntrinsicCall;
use crate::ir::{
    validate_module, Builder, ExternDecl, ExternParamDecl, Function, Immediate, IrType, LocalKind,
    Module, Op, Ownership, RuntimeCallTarget, RuntimeFnId, Terminator, TraitMethodInfo,
};
use crate::ir_lower::{builtin_datetime, function, LoweringError};
use crate::names::php_symbol_key;
use crate::parser::ast::{
    ClassMethod, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind, Visibility,
};
use crate::types::{CheckResult, ClassInfo, FunctionSig, InterfaceInfo, PhpType};
use crate::types::checker::{
    set_state_contract_error, SetStateContractViolation,
};

mod metadata;
mod runtime_features;
mod eval_aot;
/// Read by `builtin_datetime`, which decides whether a module's eval fragments can reach the
/// date alias surface — a question only the fragment text can answer.
/// Read by `builtin_datetime`, which asks whether a module's eval fragments can reach the date
/// alias surface. `eval_literal_call_requires_bridge` is the authority on that: a fragment the
/// planner can compile ahead of time resolves its names through ordinary lowering, where the
/// reachability fixpoint already sees them; one that still needs the bridge resolves them at
/// runtime and can reach anything, including through an `include`.
pub(crate) use runtime_features::eval_literal_call_requires_bridge;
mod declaration_metadata;
mod function_declarations;
mod class_methods;
mod spl_discovery;
mod spl_metadata;
mod spl_support;
mod spl_lowering;

use metadata::*;
use runtime_features::*;
use eval_aot::*;
use declaration_metadata::*;
use function_declarations::*;
use class_methods::*;
use spl_discovery::*;
use spl_metadata::*;
use spl_support::*;
use spl_lowering::*;

pub(super) use eval_aot::all_lowered_functions;
pub(super) use runtime_features::include_lowered_runtime_features;
pub(super) use spl_discovery::{
    class_data_name, dynamic_object_new_metadata_names, php_method_key, string_data_name,
};
pub(super) use spl_lowering::class_method_already_lowered;

/// Lowers an optimized typed AST program into a validated EIR module.
///
/// `web` mirrors the CLI `--web` flag (the same source
/// `codegen_ir::block_emit::emit_module` receives) and is stored on the
/// returned module so every lowering entry point in `function.rs` can gate
/// request-superglobal type seeding on it; see `Module::web`.
pub(crate) fn lower(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
    source_path: Option<&Path>,
    web: bool,
) -> Result<Module, LoweringError> {
    let mut module = Module::new(target);
    module.required_runtime_features.timelib = check_result
        .required_libraries
        .iter()
        .any(|library| library == "elephc_tz");
    module.source_path = source_path.map(canonical_source_path);
    module.web = web;
    let constants = crate::codegen::collect_constants(program, target.platform);
    module.global_constants = constants.clone();
    let fiber_return_sigs = crate::ir_lower::fibers::collect_fiber_return_sigs(program);
    populate_metadata(&mut module, program, check_result);
    lower_function_declarations(
        program,
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    lower_class_like_methods(
        program,
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    lower_property_init_thunks(&mut module, check_result, &constants, &fiber_return_sigs);
    let warnings = crate::types::checker::set_state_visibility_warnings(program);
    if let Some((owner_name, line, violation)) = set_state_contract_error(program) {
        lower_set_state_contract_fatal(&mut module, &warnings, &owner_name, line, &violation);
    } else {
        let warning_program = if warnings.is_empty() {
            None
        } else {
            let mut with_warnings = Vec::with_capacity(program.len() + warnings.len());
            for (owner_name, line) in warnings {
                with_warnings.push(crate::synthetic_class::s_expr(
                    crate::synthetic_class::e_call(
                        "__elephc_diag_warning",
                        vec![
                            crate::synthetic_class::e_str(&format!(
                                "\nWarning: The magic method {owner_name}::__set_state() must have public visibility"
                            )),
                            crate::synthetic_class::e_int(i64::from(line)),
                        ],
                    ),
                ));
            }
            with_warnings.extend(program.iter().cloned());
            Some(with_warnings)
        };
        let main_program = warning_program.as_ref().unwrap_or(program);
        function::lower_main(
            main_program,
            &mut module,
            check_result,
            &constants,
            &fiber_return_sigs,
        );
    }
    lower_literal_eval_aot_functions(&mut module, check_result, &constants, &fiber_return_sigs);
    lower_dynamic_constructor_thunks(&mut module, check_result, &constants, &fiber_return_sigs);
    include_lowered_runtime_features(&mut module);
    super::reflection::lower_referenced_builtin_methods(
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    lower_referenced_builtin_spl_methods(&mut module, check_result, &constants, &fiber_return_sigs);
    builtin_datetime::lower_referenced_builtin_datetime_methods(
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    // Date/time aggregate methods can instantiate SPL helpers (notably
    // `DatePeriod::getIterator()` -> `InternalIterator`) after the first SPL
    // discovery pass has completed. Re-run the fixed-point collector so those
    // newly referenced constructors and interface methods receive EIR bodies.
    lower_referenced_builtin_spl_methods(&mut module, check_result, &constants, &fiber_return_sigs);
    // The second SPL round can itself add DatePeriod helpers whose erased Mixed dispatch reaches
    // DateTime conversion factories. Close that reverse edge before validation/linking.
    builtin_datetime::lower_referenced_builtin_datetime_methods(
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    include_lowered_runtime_features(&mut module);
    super::effect_refinement::refine_module(&mut module);
    validate_module(&module)?;
    Ok(module)
}

/// Emits PHP's declaration-time fatal for an invalid `__set_state()` contract as synthetic main.
fn lower_set_state_contract_fatal(
    module: &mut Module,
    warnings: &[(String, u32)],
    owner_name: &str,
    line: u32,
    violation: &SetStateContractViolation,
) {
    let source_path = module.source_path.as_deref().unwrap_or("Unknown");
    let detail = match violation {
        SetStateContractViolation::Arity => {
            format!("Method {owner_name}::__set_state() must take exactly 1 argument")
        }
        SetStateContractViolation::NonStatic => {
            format!("Method {owner_name}::__set_state() must be static")
        }
        SetStateContractViolation::ByReference => {
            format!("Method {owner_name}::__set_state() cannot take arguments by reference")
        }
        SetStateContractViolation::ParameterType { name } => format!(
            "{owner_name}::__set_state(): Parameter #1 (${name}) must be of type array when declared"
        ),
        SetStateContractViolation::ReturnType => {
            format!("{owner_name}::__set_state(): Return type must be object when declared")
        }
    };
    let warning_messages = warnings
        .iter()
        .map(|(warning_owner, warning_line)| {
            (
                module.data.intern_string(&format!(
                    "\nWarning: The magic method {warning_owner}::__set_state() must have public visibility"
                )),
                *warning_line,
            )
        })
        .collect::<Vec<_>>();
    let message = module.data.intern_string(&format!(
        "\nFatal error: {detail} in {source_path} on line {line}\nStack trace:\n#0 {{main}}\n"
    ));
    let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    function.flags.is_main = true;
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", Vec::new());
    builder.set_entry(entry);
    builder.position_at_end(entry);
    for (warning_message, warning_line) in warning_messages {
        let warning_message = builder.emit_const_str(warning_message);
        let warning_line = builder.emit_const_i64(i64::from(warning_line));
        builder.emit(
            Op::RuntimeCall,
            vec![warning_message, warning_line],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ElephcDiagWarning,
            ))),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
    }
    builder.terminate(Terminator::Fatal { message });
    module.add_function(function);
}

#[cfg(test)]
mod set_state_contract_tests {
    use super::*;

    /// Verifies the declaration scan rejects a variadic-only `__set_state` method.
    #[test]
    fn detects_variadic_only_set_state() {
        let tokens = crate::lexer::tokenize(
            "<?php class Bad { public static function __set_state(...$state) {} }",
        )
        .expect("tokenize set-state fixture");
        let program = crate::parser::parse(&tokens).expect("parse set-state fixture");
        assert!(matches!(
            set_state_contract_error(&program),
            Some((owner, _, SetStateContractViolation::Arity)) if owner == "Bad"
        ));
    }

    /// Verifies the declaration scan accepts one fixed parameter and a variadic tail.
    #[test]
    fn accepts_fixed_set_state_parameter_with_variadic_tail() {
        let tokens = crate::lexer::tokenize(
            "<?php class Good { public static function __set_state($state, ...$rest) {} }",
        )
        .expect("tokenize set-state fixture");
        let program = crate::parser::parse(&tokens).expect("parse set-state fixture");
        assert!(set_state_contract_error(&program).is_none());
    }
}
