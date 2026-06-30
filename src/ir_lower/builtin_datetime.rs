//! Purpose:
//! Demand-driven EIR lowering of the synthetic builtin date/time and calendar
//! class methods (`DateTime`, `DateTimeImmutable`, `DateTimeZone`, `DateInterval`,
//! `DatePeriod`). The checker injects these classes as ordinary PHP method bodies;
//! this module lowers only the methods a program actually references.
//!
//! Called from:
//! - `crate::ir_lower::program::lower()` after `main` is lowered, alongside the
//!   builtin SPL method lowering.
//!
//! Key details:
//! - Mirrors the builtin SPL method lowering: scans already-lowered EIR for
//!   `ObjectNew` / `MethodCall` / `StaticMethodCall` referencing a date/time class
//!   and lowers the referenced method bodies, iterating to a fixpoint for
//!   transitive references (for example `DateTime::diff` returning a
//!   `DateInterval`, or the calendar functions that desugar to
//!   `DateTime::__elephc_*` static calls).
//! - Instantiating a class also forces lowering every interface method it
//!   exposes, because object allocation requires the full interface vtable symbol
//!   set (`DateTimeInterface`, and `Iterator` for `DatePeriod`).

use std::collections::HashSet;

use crate::ir::{Function, Immediate, Module, Op};
use crate::ir_lower::function;
use crate::parser::ast::ExprKind;
use crate::types::{CheckResult, PhpType};

/// The synthetic builtin date/time classes injected by the checker.
const BUILTIN_DATETIME_CLASSES: &[&str] = &[
    "DateTime",
    "DateTimeImmutable",
    "DateTimeZone",
    "DateInterval",
    "DatePeriod",
];

/// Returns true when `name` is one of the synthetic builtin date/time classes.
fn is_builtin_datetime_class(name: &str) -> bool {
    BUILTIN_DATETIME_CLASSES.contains(&name.trim_start_matches('\\'))
}

/// Returns the normalized builtin date/time class named by `ty`, if any.
///
/// Accepts a concrete `Object(Class)` receiver as well as nullable/union receivers such as
/// `?DateTimeZone` (`Union([Object("DateTimeZone"), Void])`), whose codegen representation
/// collapses to `Mixed`. This lets the reference scan discover date/time methods invoked on a
/// nullable date/time receiver — e.g. the constructor's internal `$timezone->getName()` — so they
/// are lowered instead of dispatching to an unemitted symbol at runtime.
fn builtin_datetime_class_in_type(ty: &PhpType) -> Option<String> {
    match ty {
        PhpType::Object(name) => {
            let normalized = name.trim_start_matches('\\');
            is_builtin_datetime_class(normalized).then(|| normalized.to_string())
        }
        PhpType::Union(members) => members.iter().find_map(builtin_datetime_class_in_type),
        _ => None,
    }
}

/// Lowers every referenced synthetic date/time method into the EIR module.
///
/// Iterates to a fixpoint: each round scans all currently-lowered functions and
/// methods for references to a date/time class, lowers the newly-referenced
/// method bodies, and repeats until no further methods are discovered. The loop
/// terminates because the set of date/time methods is finite and each round
/// either appends at least one new method body or leaves the count unchanged.
pub(crate) fn lower_referenced_builtin_datetime_methods(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    lower_eval_date_alias_methods_if_needed(module, check_result, constants, fiber_return_sigs);
    loop {
        let mut methods = referenced_builtin_datetime_methods(module);
        methods.sort();
        methods.dedup();
        if methods.is_empty() {
            break;
        }

        let before = module.class_methods.len();
        for (class_name, method_key) in methods {
            lower_builtin_datetime_method(
                &class_name,
                &method_key,
                module,
                check_result,
                constants,
                fiber_return_sigs,
            );
        }
        if module.class_methods.len() == before {
            break;
        }
    }
}

/// Lowers DateTime-family methods that runtime eval aliases may call dynamically.
fn lower_eval_date_alias_methods_if_needed(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    if !module_uses_eval(module) {
        return;
    }
    let mut methods = eval_date_alias_builtin_datetime_methods(module);
    methods.sort();
    methods.dedup();
    for (class_name, method_key) in methods {
        lower_builtin_datetime_method(
            &class_name,
            &method_key,
            module,
            check_result,
            constants,
            fiber_return_sigs,
        );
    }
}

/// Returns the builtin DateTime-family methods reachable from eval alias dispatch.
fn eval_date_alias_builtin_datetime_methods(module: &Module) -> Vec<(String, String)> {
    let mut methods = Vec::new();
    for class_name in ["DateTime", "DateTimeImmutable", "DateTimeZone", "DateInterval"] {
        push_constructor_and_interface_methods(&mut methods, module, class_name);
    }
    for method_name in [
        "createFromFormat",
        "getLastErrors",
        "__elephc_date_parse_from_format",
        "__elephc_date_parse",
        "__elephc_date_sun_info",
        "__elephc_date_sunfunc",
        "__elephc_strptime",
        "__elephc_timezone_name_from_abbr",
        "__elephc_cal_to_jd",
        "__elephc_cal_from_jd",
        "__elephc_cal_days_in_month",
        "__elephc_cal_info",
        "__elephc_gregoriantojd",
        "__elephc_jdtogregorian",
        "__elephc_juliantojd",
        "__elephc_jdtojulian",
        "__elephc_frenchtojd",
        "__elephc_jdtofrench",
        "__elephc_jewishtojd",
        "__elephc_jdtojewish",
        "__elephc_jddayofweek",
        "__elephc_jdmonthname",
        "__elephc_jdtounix",
        "__elephc_unixtojd",
        "__elephc_easter_days",
        "__elephc_easter_date",
        "__elephc_gettimeofday",
        "__elephc_strftime",
        "diff",
        "format",
        "add",
        "sub",
        "modify",
        "getTimestamp",
        "setTimestamp",
        "getTimezone",
        "setTimezone",
        "getOffset",
        "setDate",
        "setISODate",
        "setTime",
    ] {
        methods.push(("DateTime".to_string(), php_method_key(method_name)));
        methods.push(("DateTimeImmutable".to_string(), php_method_key(method_name)));
    }
    for method_name in ["createFromDateString", "format"] {
        methods.push(("DateInterval".to_string(), php_method_key(method_name)));
    }
    for method_name in ["getName", "getOffset", "listIdentifiers"] {
        methods.push(("DateTimeZone".to_string(), php_method_key(method_name)));
    }
    methods
}

/// Returns true when the lowered module has any dependency on the eval bridge.
fn module_uses_eval(module: &Module) -> bool {
    module.required_runtime_features.eval
        || module
            .functions
            .iter()
            .chain(module.class_methods.iter())
            .chain(module.closures.iter())
            .chain(module.fiber_wrappers.iter())
            .chain(module.callback_wrappers.iter())
            .chain(module.extern_callback_trampolines.iter())
            .chain(module.runtime_callable_invokers.iter())
            .any(|function| function_uses_eval(module, function))
}

/// Returns true when one lowered function contains an eval bridge instruction.
fn function_uses_eval(module: &Module, function: &Function) -> bool {
    function
        .instructions
        .iter()
        .any(|inst| instruction_uses_eval(module, inst))
}

/// Returns true when one instruction requires eval runtime support.
fn instruction_uses_eval(module: &Module, inst: &crate::ir::Instruction) -> bool {
    matches!(
        inst.op,
        Op::EvalFunctionCall
            | Op::EvalFunctionCallArray
            | Op::EvalObjectNew
            | Op::EvalStaticMethodCall
            | Op::EvalFunctionExists
            | Op::EvalClassExists
            | Op::EvalConstantExists
            | Op::EvalConstantFetch
    ) || builtin_call_is_eval(module, inst)
}

/// Returns true when one lowered builtin call is PHP's `eval` construct.
fn builtin_call_is_eval(module: &Module, inst: &crate::ir::Instruction) -> bool {
    if inst.op != Op::BuiltinCall {
        return false;
    }
    let Some(Immediate::Data(data)) = inst.immediate else {
        return false;
    };
    module
        .data
        .function_names
        .get(data.as_raw() as usize)
        .is_some_and(|name| crate::names::php_symbol_key(name.trim_start_matches('\\')) == "eval")
}

/// Finds builtin date/time methods whose symbols are required by already-lowered EIR.
///
/// Returns `(class_name, method_key)` pairs for every `ObjectNew`,
/// `MethodCall`/`NullsafeMethodCall`, and `StaticMethodCall` that targets a
/// date/time class. `ObjectNew` additionally pulls in the constructor and the
/// full interface vtable required to allocate the object.
fn referenced_builtin_datetime_methods(module: &Module) -> Vec<(String, String)> {
    let mut methods = Vec::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for inst in &function.instructions {
            match inst.op {
                Op::ObjectNew => {
                    if let Some(class_name) = datetime_class_data_name(module, inst) {
                        push_constructor_and_interface_methods(&mut methods, module, class_name);
                    }
                }
                Op::MethodCall | Op::NullsafeMethodCall => {
                    let Some(receiver) = inst.operands.first().copied() else {
                        continue;
                    };
                    // Inspect the raw receiver type, not its codegen repr: a nullable date/time
                    // receiver such as a `?DateTimeZone` parameter collapses to `Mixed` under
                    // codegen_repr(), which would hide methods (e.g. the constructor's internal
                    // `$timezone->getName()`) and leave their symbols unemitted.
                    let Some(normalized) = function
                        .value(receiver)
                        .and_then(|value| builtin_datetime_class_in_type(&value.php_type))
                    else {
                        continue;
                    };
                    let Some(method_name) = string_data_name(module, inst) else {
                        continue;
                    };
                    let method_key = php_method_key(method_name);
                    let impl_class = method_impl_class(module, &normalized, &method_key);
                    methods.push((impl_class, method_key));
                }
                Op::StaticMethodCall => {
                    if let Some(name) = string_data_name(module, inst) {
                        if let Some((class_name, method_name)) = name.split_once("::") {
                            let normalized = class_name.trim_start_matches('\\');
                            if is_builtin_datetime_class(normalized) {
                                methods.push((normalized.to_string(), php_method_key(method_name)));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    methods
}

/// Enqueues a date/time class constructor plus every method its interfaces expose.
///
/// Object allocation requires the full interface vtable symbol set, so this walks
/// the class's interfaces (and their parents) and enqueues each declared method on
/// the date/time class that implements it.
fn push_constructor_and_interface_methods(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    class_name: &str,
) {
    methods.push((class_name.to_string(), php_method_key("__construct")));
    let Some(class_info) = module.class_infos.get(class_name) else {
        return;
    };
    let mut seen = HashSet::new();
    let mut stack = class_info
        .interfaces
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    while let Some(interface_name) = stack.pop() {
        if !seen.insert(interface_name.to_string()) {
            continue;
        }
        let Some(interface_info) = module.interface_infos.get(interface_name) else {
            continue;
        };
        for method_key in &interface_info.method_order {
            let impl_class = method_impl_class(module, class_name, method_key);
            if is_builtin_datetime_class(&impl_class) {
                methods.push((impl_class, method_key.clone()));
            }
        }
        stack.extend(interface_info.parents.iter().map(String::as_str));
    }
}

/// Resolves which class actually implements `method_key` for `class_name`.
///
/// Falls back to `class_name` when no implementing-class metadata is recorded.
fn method_impl_class(module: &Module, class_name: &str, method_key: &str) -> String {
    module
        .class_infos
        .get(class_name)
        .and_then(|class_info| class_info.method_impl_classes.get(method_key).cloned())
        .unwrap_or_else(|| class_name.to_string())
}

/// Lowers one synthetic date/time method body into `module.class_methods`.
///
/// No-op when the method is already lowered or has no synthetic body (so repeated
/// fixpoint rounds stay idempotent).
fn lower_builtin_datetime_method(
    class_name: &str,
    method_key: &str,
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
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
    if class_method_already_lowered(module, class_name, method_key, method.is_static) {
        return;
    }
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
fn class_method_already_lowered(
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

/// Returns the class-name immediate attached to an `ObjectNew` instruction when it
/// names a builtin date/time class.
fn datetime_class_data_name<'a>(
    module: &'a Module,
    inst: &crate::ir::Instruction,
) -> Option<&'a str> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return None;
    };
    let name = module.data.class_names.get(data.as_raw() as usize)?;
    is_builtin_datetime_class(name).then_some(name.as_str())
}

/// Returns the string immediate attached to an instruction.
fn string_data_name<'a>(module: &'a Module, inst: &crate::ir::Instruction) -> Option<&'a str> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return None;
    };
    module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Normalizes a PHP method name for metadata lookups (PHP method names are
/// case-insensitive).
fn php_method_key(method_name: &str) -> String {
    crate::names::php_symbol_key(method_name)
}
