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
//!   `ObjectNew` / method/property calls / `StaticMethodCall` referencing a date/time class
//!   and lowers the referenced method bodies, iterating to a fixpoint for
//!   transitive references (for example `DateTime::diff` returning a
//!   `DateInterval`, or the calendar functions that desugar to
//!   `DateTime::__elephc_*` static calls).
//! - Instantiating a class also forces lowering every interface method it
//!   exposes, because object allocation requires the full interface vtable symbol
//!   set (`DateTimeInterface`, and `Iterator` for `DatePeriod`).

use std::collections::HashSet;

use crate::ir::{Function, Immediate, Module, Op, RuntimeCallTarget, RuntimeFnId};
use crate::ir_lower::function;
use crate::names::property_hook_get_method;
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

/// Returns every normalized builtin date/time class or descendant named by `ty`.
///
/// Accepts a concrete `Object(Class)` receiver as well as nullable/union receivers such as
/// `?DateTimeZone` (`Union([Object("DateTimeZone"), Void])`), whose codegen representation
/// collapses to `Mixed`. This lets the reference scan discover date/time methods invoked on a
/// nullable date/time receiver — e.g. the constructor's internal `$timezone->getName()` — so they
/// are lowered instead of dispatching to an unemitted symbol at runtime. Descendants retain their
/// concrete name so inherited method ownership is resolved through `method_impl_class()`.
fn builtin_datetime_classes_in_type(module: &Module, ty: &PhpType) -> Vec<String> {
    let mut classes = match ty {
        PhpType::Object(name) => {
            let normalized = name.trim_start_matches('\\');
            if normalized.eq_ignore_ascii_case("DateTimeInterface") {
                ["DateTime", "DateTimeImmutable"]
                    .into_iter()
                    .filter(|class| module.class_infos.contains_key(*class))
                    .map(str::to_string)
                    .collect()
            } else {
                class_is_or_extends_builtin_datetime(module, normalized)
                    .then(|| vec![normalized.to_string()])
                    .unwrap_or_default()
            }
        }
        PhpType::Union(members) => members
            .iter()
            .flat_map(|member| builtin_datetime_classes_in_type(module, member))
            .collect(),
        _ => Vec::new(),
    };
    classes.sort();
    classes.dedup();
    classes
}

/// Returns true when a class is one of the synthetic date classes or descends from one.
fn class_is_or_extends_builtin_datetime(module: &Module, class_name: &str) -> bool {
    let mut current = Some(class_name.trim_start_matches('\\'));
    while let Some(name) = current {
        if is_builtin_datetime_class(name) {
            return true;
        }
        current = module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
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
    lower_builtin_datetime_method(
        "DateTime",
        &php_method_key("__elephc_timezone_type"),
        module,
        check_result,
        constants,
        fiber_return_sigs,
    );
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
        for method in module.class_methods.iter_mut().skip(before) {
            method.flags.is_synthetic = true;
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
    if !eval_fragments_may_reach_dates(module) {
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

/// Returns the builtin DateTime-family methods reachable from eval alias dispatch and
/// runtime-generated `var_export()` restoration expressions.
fn eval_date_alias_builtin_datetime_methods(module: &Module) -> Vec<(String, String)> {
    let mut methods = Vec::new();
    for class_name in BUILTIN_DATETIME_CLASSES {
        push_constructor_and_interface_methods(&mut methods, module, class_name);
        methods.push(((*class_name).to_string(), php_method_key("__set_state")));
    }
    for method_name in EVAL_DATE_ALIAS_METHOD_NAMES {
        methods.push(("DateTime".to_string(), php_method_key(method_name)));
        methods.push(("DateTimeImmutable".to_string(), php_method_key(method_name)));
    }
    for method_name in ["createFromDateString", "format"] {
        methods.push(("DateInterval".to_string(), php_method_key(method_name)));
    }
    for method_name in DATE_TIMEZONE_ALIAS_METHOD_NAMES {
        methods.push(("DateTimeZone".to_string(), php_method_key(method_name)));
    }
    methods
}

/// The `DateTime`/`DateTimeImmutable` methods an eval alias can dispatch to.
///
/// A CONSTANT rather than a literal inside the emitter so the set has ONE home. It is read by
/// `eval_date_alias_builtin_datetime_methods` alone; `eval_fragments_may_reach_dates` gates the
/// whole emitter on whether the module contains a non-literal `eval` at all and does not consult
/// the names, so widening this list cannot make a fragment look harmless.
///
/// MISSING A NAME IS SILENT AND WRONG. The checker still declares the method, so the call type
/// checks and the failure lands at run time as `Cannot call abstract method` — there is no
/// diagnostic pointing back here.
const EVAL_DATE_ALIAS_METHOD_NAMES: &[&str] = &[
    "createFromFormat",
    // The other three static factories. `createFromFormat` was here alone, so a computed name
    // reaching any of these answered `Cannot call abstract method` — the declaration is visible to
    // the checker, and only the BODY was missing from the eval alias set:
    //
    //     $m = "createFrom" . "Timestamp";
    //     eval("return DateTime::" . $m . "(0);")
    //     php    : 1970          elephc : Fatal error: Cannot call abstract method
    //
    // Each name is pushed for BOTH DateTime and DateTimeImmutable, and a class that does not
    // declare one is skipped by `lower_builtin_datetime_method`, so the pairs that do not exist
    // (`createFromImmutable` on the immutable class, `createFromMutable` on the mutable one) cost
    // nothing and keep this list about the FAMILY rather than one class.
    "createFromTimestamp",
    "createFromInterface",
    "createFromImmutable",
    "createFromMutable",
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
];

/// The `DateTimeZone` methods an eval alias can dispatch to. Same reason as above.
const DATE_TIMEZONE_ALIAS_METHOD_NAMES: &[&str] = &[
    "getName",
    "getOffset",
    "listIdentifiers",
    "getLocation",
    "getTransitions",
    "listAbbreviations",
];

/// Returns true when this module's `eval` fragments could reach the date alias surface.
///
/// The surface above exists because `eval` resolves names at runtime: `eval('date_create()')`
/// reaches `DateTime::__construct` through a string, so every alias target has to be lowered in
/// advance. A fragment the AOT planner can compile does not do that — it becomes an ordinary EIR
/// function, lowered before this pass runs, so the reachability fixpoint below sees its calls the
/// way it sees any others.
///
/// So the question is not what the fragment NAMES but whether it still needs the bridge, and
/// `eval_literal_call_requires_bridge` is already the authority on that — the same predicate
/// `runtime_features` uses to decide whether to link the bridge at all. Asking it instead of
/// re-deriving an answer is what makes this correct for the cases a name scan cannot see: a
/// fragment doing `include "d.php"` names nothing, and the included file's `date_create()` is not
/// in the source this pass can read. The planner classifies that as bridge-only, and the surface
/// is emitted.
///
/// What it saves is not marginal. `createFromFormat` is 369 lines of PHP lowered twice, and on a
/// four-line program one `eval('echo 1;')` took the emitted assembly from 94 KB to 6.5 MB.
fn eval_fragments_may_reach_dates(module: &Module) -> bool {
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
        for (inst_index, inst) in function.instructions.iter().enumerate() {
            if !instruction_uses_eval(module, inst) {
                continue;
            }
            // Non-literal eval operations resolve their source entirely at runtime. Their
            // immediates are function-name metadata, not an eval fragment string, so feeding
            // them to the literal planner can accidentally classify an unrelated string-pool
            // entry as bridge-free and omit the dynamic DateTime method surface.
            if inst.op != Op::EvalLiteralCall {
                return true;
            }
            // Every eval op other than a literal call resolves its source at runtime, and
            // `eval_literal_call_requires_bridge` answers `true` for them on the same reasoning.
            if crate::ir_lower::program::eval_literal_call_requires_bridge(
                module, function, inst_index, inst,
            ) {
                return true;
            }
        }
    }
    false
}

/// Returns true when the lowered module has any dependency on the eval bridge.
fn module_uses_eval(module: &Module) -> bool {
    module.required_runtime_features.eval_bridge
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
        Op::EvalLiteralCall
            | Op::EvalFunctionCall
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
    if inst.op != Op::LanguageConstructCall {
        return false;
    }
    // A profiled call carries the SAME construct name, in a different immediate shape. Matching
    // only the bare form let `eval($computed)` past this filter unseen.
    let data = match inst.immediate {
        Some(Immediate::Data(data)) | Some(Immediate::ProfiledData { data, .. }) => data,
        _ => return false,
    };
    module
        .data
        .function_names
        .get(data.as_raw() as usize)
        .is_some_and(|name| crate::names::php_symbol_key(name.trim_start_matches('\\')) == "eval")
}

/// Finds builtin date/time methods whose symbols are required by already-lowered EIR.
///
/// Returns `(class_name, method_key)` pairs for every date object allocation,
/// method/property call, and `StaticMethodCall` that targets a
/// date/time class. Fixed allocation additionally pulls in the constructor and the
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
            if instruction_calls_unserialize(inst) {
                for class_name in BUILTIN_DATETIME_CLASSES {
                    push_unserialize_magic_methods(&mut methods, module, class_name);
                }
            }
            match inst.op {
                Op::ObjectNew | Op::ObjectNewWithoutConstructor => {
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
                    let receiver_ty = function.value(receiver).map(|v| &v.php_type);
                    let Some(method_name) = string_data_name(module, inst) else {
                        continue;
                    };
                    let method_key = php_method_key(method_name);
                    let concrete_classes = receiver_ty
                        .map(|ty| builtin_datetime_classes_in_type(module, ty))
                        .unwrap_or_default();
                    if !concrete_classes.is_empty() {
                        for class_name in concrete_classes {
                            let impl_class = method_impl_class(module, &class_name, &method_key);
                            methods.push((impl_class, method_key.clone()));
                        }
                        continue;
                    }
                    // When the receiver is `Mixed` (e.g. the result of `date_create()` which
                    // returns `DateTime|false`), codegen dispatches over every matching class.
                    if receiver_ty.is_some_and(|ty| {
                        matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
                    }) {
                        for class_name in BUILTIN_DATETIME_CLASSES {
                            let Some(class_info) = module.class_infos.get(*class_name) else {
                                continue;
                            };
                            if class_info.methods.contains_key(&method_key) {
                                let impl_class = method_impl_class(module, class_name, &method_key);
                                methods.push((impl_class, method_key.clone()));
                            }
                        }
                    }
                }
                Op::PropGet | Op::NullsafePropGet | Op::DynamicPropGet => {
                    let Some(receiver) = inst.operands.first().copied() else {
                        continue;
                    };
                    let receiver_ty = function.value(receiver).map(|value| &value.php_type);
                    let property = matches!(inst.op, Op::PropGet | Op::NullsafePropGet)
                        .then(|| string_data_name(module, inst))
                        .flatten();
                    let concrete_classes = receiver_ty
                        .map(|ty| builtin_datetime_classes_in_type(module, ty))
                        .unwrap_or_default();
                    if !concrete_classes.is_empty() {
                        for class_name in concrete_classes {
                            push_property_read_methods(
                                &mut methods,
                                module,
                                &class_name,
                                property,
                            );
                        }
                    } else if receiver_ty.is_some_and(|ty| {
                        matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
                    }) {
                        for class_name in BUILTIN_DATETIME_CLASSES {
                            push_property_read_methods(
                                &mut methods,
                                module,
                                class_name,
                                property,
                            );
                        }
                    }
                }
                Op::StaticMethodCall => {
                    if let Some(name) = string_data_name(module, inst) {
                        if let Some((class_name, method_name)) = name.split_once("::") {
                            if let Some(resolved_class) =
                                resolve_static_datetime_class(module, function, class_name)
                            {
                                let method_key = php_method_key(method_name);
                                let impl_class = static_method_impl_class(
                                    module,
                                    &resolved_class,
                                    &method_key,
                                );
                                if is_builtin_datetime_class(&impl_class) {
                                    methods.push((impl_class, method_key));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    // DatePeriod's synthetic iterator canonicalizes its Mixed cursor through these two static
    // factories. The SPL fixed point can add that helper immediately before this discovery pass,
    // so pin both symbols whenever the helper itself is present instead of relying on its erased
    // Mixed return type to rediscover the concrete targets.
    if module.class_methods.iter().any(|function| {
        function.name == "DatePeriod::__elephc_clone_iterator_value"
    }) {
        methods.push((
            "DateTime".to_string(),
            php_method_key("createFromInterface"),
        ));
        methods.push((
            "DateTimeImmutable".to_string(),
            php_method_key("createFromInterface"),
        ));
    }
    methods
}

/// Returns true when an EIR instruction invokes PHP's generic `unserialize()` runtime.
fn instruction_calls_unserialize(inst: &crate::ir::Instruction) -> bool {
    matches!(
        inst.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::Unserialize
        )))
    )
}

/// Enqueues date-object restoration hooks reachable only through generic `unserialize()`.
///
/// Runtime deserialization allocates a class by the serialized name, so no `ObjectNew` opcode
/// exists for the normal demand scan to discover. Both hook tables must nevertheless point at
/// emitted method bodies, including for literal payloads in programs that never call `serialize()`.
fn push_unserialize_magic_methods(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    class_name: &str,
) {
    for method_name in ["__unserialize", "__wakeup"] {
        let method_key = php_method_key(method_name);
        let impl_class = method_impl_class(module, class_name, &method_key);
        if is_builtin_datetime_class(&impl_class) {
            methods.push((impl_class, method_key));
        }
    }
}

/// Resolves a static-call class operand when it names a date/time class or descendant.
///
/// `parent`, `self`, and `static` are interpreted relative to the currently lowered class
/// method. This is required for user constructors that call an internal date/time
/// `parent::__construct()` body whose EIR symbol must be materialized on demand.
fn resolve_static_datetime_class(
    module: &Module,
    function: &Function,
    class_name: &str,
) -> Option<String> {
    let normalized = class_name.trim_start_matches('\\');
    let current_class = function
        .name
        .split_once("::")
        .map(|(owner, _)| owner.trim_start_matches('\\'));
    let resolved = match php_method_key(normalized).as_str() {
        "parent" => current_class
            .and_then(|owner| module.class_infos.get(owner))
            .and_then(|class_info| class_info.parent.as_deref())?,
        "self" | "static" => current_class?,
        _ => normalized,
    };
    class_is_or_extends_builtin_datetime(module, resolved).then(|| resolved.to_string())
}

/// Enqueues magic and synthetic hook getters that one property-read opcode may dispatch to.
fn push_property_read_methods(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    class_name: &str,
    property: Option<&str>,
) {
    let Some(class_info) = module.class_infos.get(class_name) else {
        return;
    };
    let magic_get_key = php_method_key("__get");
    if class_info.methods.contains_key(&magic_get_key) {
        let impl_class = method_impl_class(module, class_name, &magic_get_key);
        methods.push((impl_class, magic_get_key));
    }
    if let Some(property) = property {
        let hook_key = php_method_key(&property_hook_get_method(property));
        if class_info.methods.contains_key(&hook_key) {
            let impl_class = method_impl_class(module, class_name, &hook_key);
            methods.push((impl_class, hook_key));
        }
        return;
    }
    for method_key in class_info
        .methods
        .keys()
        .filter(|method_key| method_key.starts_with("__propget_"))
    {
        let impl_class = method_impl_class(module, class_name, method_key);
        methods.push((impl_class, method_key.clone()));
    }
}

/// Enqueues a date/time constructor, runtime hooks, and interface methods.
///
/// Object allocation requires the full interface vtable symbol set, so this walks
/// the class's interfaces (and their parents) and enqueues each declared method on
/// the date/time class that implements it. Runtime serialization tables also point
/// directly at inherited magic/debug methods, whose initialization guard must
/// also be emitted even without a source call.
fn push_constructor_and_interface_methods(
    methods: &mut Vec<(String, String)>,
    module: &Module,
    class_name: &str,
) {
    let constructor_key = php_method_key("__construct");
    let constructor_impl = method_impl_class(module, class_name, &constructor_key);
    if is_builtin_datetime_class(&constructor_impl) {
        methods.push((constructor_impl, constructor_key));
    }
    let Some(class_info) = module.class_infos.get(class_name) else {
        return;
    };
    for method_name in [
        "__serialize",
        "__unserialize",
        "__wakeup",
        "__elephc_debug_dump",
        "__elephc_print_r_dump",
        "__elephc_assert_initialized",
    ] {
        let method_key = php_method_key(method_name);
        let impl_class = method_impl_class(module, class_name, &method_key);
        if is_builtin_datetime_class(&impl_class) {
            methods.push((impl_class, method_key));
        }
    }
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
    let mut current = Some(class_name);
    while let Some(name) = current {
        let Some(class_info) = module.class_infos.get(name) else {
            break;
        };
        if let Some(impl_class) = class_info.method_impl_classes.get(method_key) {
            return impl_class.clone();
        }
        if let Some(parent) = class_info.parent.as_deref() {
            current = Some(parent);
            continue;
        }
        if class_info.methods.contains_key(method_key) {
            return name.to_string();
        }
        break;
    }
    class_name.to_string()
}

/// Resolves which class actually implements a static `method_key` for `class_name`, following
/// inherited static-method metadata before falling back to the requested receiver.
fn static_method_impl_class(module: &Module, class_name: &str, method_key: &str) -> String {
    let mut current = Some(class_name);
    while let Some(name) = current {
        let Some(class_info) = module.class_infos.get(name) else {
            break;
        };
        if let Some(impl_class) = class_info.static_method_impl_classes.get(method_key) {
            return impl_class.clone();
        }
        if let Some(parent) = class_info.parent.as_deref() {
            current = Some(parent);
            continue;
        }
        if class_info.methods.contains_key(method_key) {
            return name.to_string();
        }
        break;
    }
    class_name.to_string()
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

/// Returns the class-name immediate attached to a fixed object allocation when it names
/// a builtin date/time class.
fn datetime_class_data_name<'a>(
    module: &'a Module,
    inst: &crate::ir::Instruction,
) -> Option<&'a str> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return None;
    };
    let name = module.data.class_names.get(data.as_raw() as usize)?;
    class_is_or_extends_builtin_datetime(module, name).then_some(name.as_str())
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
