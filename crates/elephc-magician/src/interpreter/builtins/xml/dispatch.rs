//! Purpose:
//! Forwards eval calls of the `xml_*` / `xmlwriter_*` builtins to the compiled xml
//! prelude the host program registered, in every call shape eval has.
//!
//! Called from:
//! - Source-level dispatch (`eval_builtin_xml_call`), declarative direct hooks
//!   (`eval_builtin_xml_expr_call`), by-value callable dispatch (`eval_xml_values_result`),
//!   the by-reference dynamic-callable arm in `registry::dynamic_mutation`
//!   (`eval_xml_evaluated_call`), the `function_exists()` / `is_callable()` probes
//!   (`eval_xml_builtin_linked`) and `get_loaded_extensions()` (`eval_xml_bridge_linked`).
//!
//! Key details:
//! - Most names are ordinary prelude functions the host registered as native functions,
//!   so the call goes through the native-function bridge (named arguments bind by the host
//!   signature). The ten REGISTRY builtins of the surface — the nine `xml_set_*_handler()`
//!   setters and `xml_parse_into_struct()` — are AOT checker/lowering homes with no
//!   registered function behind them; eval reaches the same prelude code by calling the
//!   `XMLParser` object's `__elephc_*` methods through the native class bridge.
//! - `xml_parse_into_struct()` writes `$values` / `$index` back through the captured caller
//!   lvalues in every shape that carries them — a direct call, `$f(...)`, a first-class
//!   callable, `call_user_func_array()` with `&$refs` (positional or named); only a genuine
//!   by-value `call_user_func()` warns "must be passed by reference, value given", the
//!   interpreter-wide shape every by-reference builtin uses (`curl_multi_exec()`,
//!   `preg_match()`, ...).
//! - The compiled parser dispatches handlers through the compiled `is_callable()`, which
//!   only sees compiled symbols: an eval closure, an eval-declared function or an object of
//!   an eval-declared class is just a `stdClass` cell / unknown name to it. The setters and
//!   `xml_set_object()` therefore reject such values up front with a clear catchable
//!   `Error` instead of letting the prelude report a misleading `ValueError`.
//! - A host that did not link the bridge has no registered `xml_*` functions: the 54
//!   prelude-provided functions then raise PHP's catchable `Error: Call to undefined
//!   function name()` and `function_exists()` answers false, as in the compiled program.
//!   The ten registry builtins exist on both backends regardless, so they still reach their
//!   argument checks (`xml_set_element_handler(null, ...)` is PHP's `TypeError` either way).

use super::*;

/// The registry setters: PHP name, the `XMLParser` method eval forwards to, parameter names.
const HANDLER_SETTERS: &[(&str, &str, &[&str])] = &[
    (
        "xml_set_element_handler",
        "__elephc_set_element_handler",
        &["parser", "start_handler", "end_handler"],
    ),
    (
        "xml_set_character_data_handler",
        "__elephc_set_character_data_handler",
        &["parser", "handler"],
    ),
    (
        "xml_set_processing_instruction_handler",
        "__elephc_set_processing_instruction_handler",
        &["parser", "handler"],
    ),
    ("xml_set_default_handler", "__elephc_set_default_handler", &["parser", "handler"]),
    (
        "xml_set_unparsed_entity_decl_handler",
        "__elephc_set_unparsed_entity_decl_handler",
        &["parser", "handler"],
    ),
    (
        "xml_set_notation_decl_handler",
        "__elephc_set_notation_decl_handler",
        &["parser", "handler"],
    ),
    (
        "xml_set_external_entity_ref_handler",
        "__elephc_set_external_entity_ref_handler",
        &["parser", "handler"],
    ),
    (
        "xml_set_start_namespace_decl_handler",
        "__elephc_set_start_namespace_decl_handler",
        &["parser", "handler"],
    ),
    (
        "xml_set_end_namespace_decl_handler",
        "__elephc_set_end_namespace_decl_handler",
        &["parser", "handler"],
    ),
];

/// The parameter names of `xml_parse_into_struct(XMLParser $parser, string $data, &$values, &$index = null)`.
const PARSE_INTO_STRUCT_PARAMS: &[&str] = &["parser", "data", "values", "index"];

/// Returns whether `name` is one of the xml surface's PHP-visible builtins.
pub(in crate::interpreter) fn eval_xml_builtin_name(name: &str) -> bool {
    (name.starts_with("xml_") || name.starts_with("xmlwriter_"))
        && eval_php_visible_builtin_exists(name)
}

/// Returns whether the host program linked the xml bridge, i.e. registered the prelude.
pub(in crate::interpreter) fn eval_xml_bridge_linked(context: &ElephcEvalContext) -> bool {
    context.native_function("xml_parser_create").is_some()
}

/// Returns whether `key` (lowercase) is one of the ten registry builtins — the nine handler
/// setters and `xml_parse_into_struct()` — which exist on both backends whether or not the
/// host linked the bridge.
fn eval_xml_registry_builtin(key: &str) -> bool {
    key == "xml_parse_into_struct" || HANDLER_SETTERS.iter().any(|(setter, _, _)| *setter == key)
}

/// Raises PHP's undefined-function error for a prelude-provided xml function the host
/// cannot serve; the registry builtins pass through to their own argument checks.
fn eval_xml_require_bridge(
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if eval_xml_bridge_linked(context) || eval_xml_registry_builtin(&name.to_ascii_lowercase()) {
        return Ok(());
    }
    eval_xml_throw_undefined(name, context, values)
}

/// Returns false for a prelude-provided xml function the host program cannot call because
/// it never linked the bridge, so `function_exists()` agrees with the compiled program:
/// there the prelude declares those functions only when the bridge is injected, while the
/// ten registry builtins (`xml_parse_into_struct()` and the `xml_set_*_handler()` setters)
/// exist as compiler builtins on both backends whether or not a parser can be created.
pub(in crate::interpreter) fn eval_xml_builtin_linked(
    context: &ElephcEvalContext,
    name: &str,
) -> bool {
    if !eval_xml_builtin_name(name) || eval_xml_bridge_linked(context) {
        return true;
    }
    elephc_builtin_contract::lookup(name)
        .is_some_and(|contract| contract.kind == elephc_builtin_contract::BuiltinKind::Function)
}

/// Raises PHP's undefined-function error for an xml call the host cannot serve.
fn eval_xml_throw_undefined<T>(
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(&format!("Call to undefined function {name}()"), context, values)
}

/// Looks up the host's compiled prelude function, or raises PHP's undefined-function error.
fn eval_xml_host_function(
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<NativeFunction, EvalStatus> {
    let key = name.to_ascii_lowercase();
    match context.native_function(&key) {
        Some(function) => Ok(function),
        None => eval_xml_throw_undefined(name, context, values),
    }
}

/// Evaluates a source-level xml call with named/spread binding and live references.
pub(in crate::interpreter) fn eval_builtin_xml_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_xml_require_bridge(name, context, values)?;
    with_eval_call_arguments(args, context, scope, values, |evaluated, context, _, values| {
        eval_xml_forward_evaluated(name, evaluated, context, values)
    })
}

/// Evaluates positional expression hooks when registry dispatch is invoked directly; the
/// arguments still resolve their by-reference lvalues for `xml_parse_into_struct()`.
pub(in crate::interpreter) fn eval_builtin_xml_expr_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = args.iter().cloned().map(EvalCallArg::positional).collect::<Vec<_>>();
    eval_builtin_xml_call(name, &args, context, scope, values)
}

/// Evaluates an already-bound callable xml invocation by value.
pub(in crate::interpreter) fn eval_xml_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_xml_require_bridge(name, context, values)?;
    let evaluated = evaluated_args
        .iter()
        .copied()
        .map(|value| EvaluatedCallArg {
            name: None,
            value,
            ref_target: None,
        })
        .collect();
    eval_xml_forward_evaluated(name, evaluated, context, values)
}

/// Evaluates a dynamic-callable xml invocation (`$f(...)`, `xml_parse_into_struct(...)`,
/// `call_user_func_array()` with `&$refs`) while retaining the captured writeback targets,
/// so `xml_parse_into_struct()` reaches its `$values` / `$index` lvalues in every shape.
pub(in crate::interpreter) fn eval_xml_evaluated_call(
    name: &str,
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_xml_require_bridge(name, context, values)?;
    eval_xml_forward_evaluated(name, evaluated_args.to_vec(), context, values)
}

/// Routes evaluated arguments: registry builtins to the parser's methods, everything else
/// to the host function bound by its signature.
fn eval_xml_forward_evaluated(
    name: &str,
    evaluated: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = name.to_ascii_lowercase();
    if let Some((_, method, params)) = HANDLER_SETTERS.iter().find(|(setter, _, _)| *setter == key) {
        return eval_xml_set_handler(&key, method, params, &evaluated, context, values);
    }
    if key == "xml_parse_into_struct" {
        return eval_xml_parse_into_struct(&evaluated, context, values);
    }
    if key == "xml_set_object" {
        return eval_xml_set_object(&evaluated, context, values);
    }
    let function = eval_xml_host_function(name, context, values)?;
    let bound = bind_evaluated_native_function_args(&function, evaluated, context, values)?;
    eval_native_function_with_values(function, bound, context, values)
}

/// Resolves a bound `$parser` argument to a live `XMLParser`, raising PHP's `TypeError`
/// ("Argument #1 ($parser) must be of type XMLParser, X given") for anything else.
fn eval_xml_parser_argument(
    function: &str,
    bound: &[Option<EvaluatedCallArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let parser = required_evaluated_ref_arg(bound, 0)?.value;
    if values.type_tag(parser)? == EVAL_TAG_OBJECT && values.object_is_a(parser, "XMLParser", false)? {
        return Ok(parser);
    }
    let given = eval_xml_given_type_name(parser, context, values)?;
    eval_throw_type_error(
        &format!("{function}(): Argument #1 ($parser) must be of type XMLParser, {given} given"),
        context,
        values,
    )
}

/// Names a value the way PHP's argument `TypeError` does (`zend_zval_value_name()`):
/// `null`, `true` / `false`, the zend scalar names (`int`, `float`, `string`, `array`), a
/// compiled closure's `Closure`, or an object's class — never `gettype()`'s `integer` /
/// `double` / `boolean`. An eval-declared class is looked up in the dynamic-object registry
/// first, since its backing runtime cell is a `stdClass`.
fn eval_xml_given_type_name(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let tag = values.type_tag(value)?;
    if tag == EVAL_TAG_OBJECT {
        let identity = values.object_identity(value)?;
        if let Some(name) = context.dynamic_object_class_name(identity) {
            return Ok(name);
        }
        let class_name = values.object_class_name(value)?;
        let bytes = values.string_bytes(class_name)?;
        return Ok(String::from_utf8_lossy(&bytes).into_owned());
    }
    let name = match tag {
        EVAL_TAG_NULL => "null",
        EVAL_TAG_BOOL if values.truthy(value)? => "true",
        EVAL_TAG_BOOL => "false",
        EVAL_TAG_INT => "int",
        EVAL_TAG_FLOAT => "float",
        EVAL_TAG_STRING => "string",
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array",
        EVAL_TAG_RESOURCE => "resource",
        EVAL_TAG_CALLABLE => "Closure",
        _ => "mixed",
    };
    Ok(name.to_string())
}

/// Returns whether a handler value can only be resolved by this eval context — an eval
/// closure or an object of an eval-declared class, a string naming an eval-declared
/// function or `Class::method`, or a `[$receiver, 'method']` pair whose receiver is one —
/// which the compiled parser's dispatcher (the compiled `is_callable()` over compiled
/// symbols) can never invoke. A compiled closure handed in from the host (`EVAL_TAG_CALLABLE`)
/// and every host object / compiled name pass.
fn eval_xml_callable_declared_in_eval(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_OBJECT => eval_xml_object_declared_in_eval(value, context, values),
        EVAL_TAG_STRING => {
            let name = String::from_utf8_lossy(&values.string_bytes(value)?).into_owned();
            Ok(eval_xml_callable_name_declared_in_eval(&name, context))
        }
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
            if values.array_len(value)? != 2 {
                return Ok(false);
            }
            let zero = values.int(0)?;
            let receiver = values.array_get(value, zero);
            values.release(zero)?;
            let receiver = receiver?;
            match values.type_tag(receiver)? {
                EVAL_TAG_OBJECT => eval_xml_object_declared_in_eval(receiver, context, values),
                EVAL_TAG_STRING => {
                    let class = String::from_utf8_lossy(&values.string_bytes(receiver)?).into_owned();
                    Ok(context.has_class(&class))
                }
                _ => Ok(false),
            }
        }
        _ => Ok(false),
    }
}

/// Returns whether an object exists only in this eval context: an eval closure (registered
/// under the `Closure` identity) or an instance of an eval-declared class, both of which
/// the host sees as a bare `stdClass` cell.
fn eval_xml_object_declared_in_eval(
    object: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let identity = values.object_identity(object)?;
    Ok(context.dynamic_object_class_name(identity).is_some())
}

/// Returns whether a string callable names an eval-declared function, or a method of an
/// eval-declared class in the `Class::method` form.
fn eval_xml_callable_name_declared_in_eval(name: &str, context: &ElephcEvalContext) -> bool {
    let name = name.trim_start_matches('\\');
    if let Some((class, _)) = name.split_once("::") {
        return context.has_class(class);
    }
    context.function(&name.to_ascii_lowercase()).is_some()
}

/// Forwards `xml_set_object()` to the host after PHP's parser check and after rejecting an
/// object the compiled parser could never dispatch a method on: one whose class (or
/// `Closure` identity) exists only in this eval context, since the prelude would see just
/// its backing `stdClass` cell and report "method stdClass::x() does not exist".
fn eval_xml_set_object(
    evaluated: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let function = eval_xml_host_function("xml_set_object", context, values)?;
    if let Ok((bound, _)) = bind_evaluated_ref_builtin_args(&["parser", "object"], evaluated, false) {
        if optional_evaluated_ref_arg(&bound, 0).is_some() {
            eval_xml_parser_argument("xml_set_object", &bound, context, values)?;
        }
        if let Some(object) = optional_evaluated_ref_arg(&bound, 1) {
            if values.type_tag(object.value)? == EVAL_TAG_OBJECT
                && eval_xml_object_declared_in_eval(object.value, context, values)?
            {
                return eval_throw_error(
                    "xml_set_object(): objects of classes declared inside eval() cannot be bound as handler targets",
                    context,
                    values,
                );
            }
        }
    }
    let bound = bind_evaluated_native_function_args(&function, evaluated.to_vec(), context, values)?;
    eval_native_function_with_values(function, bound, context, values)
}

/// Installs handlers through the parser's `__elephc_set_*` method, which applies the
/// php-src binding rules (`xml_set_object()` method names, `null` / `""` clearing, the
/// `ValueError` for a non-callable) exactly as the compiled program does — after rejecting
/// any handler that exists only inside eval, which that compiled binding could never invoke.
fn eval_xml_set_handler(
    function: &str,
    method: &str,
    params: &[&str],
    evaluated: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (bound, _) = bind_evaluated_ref_builtin_args(params, evaluated, false)?;
    let parser = eval_xml_parser_argument(function, &bound, context, values)?;
    let mut handlers = Vec::with_capacity(params.len() - 1);
    for index in 1..params.len() {
        let handler = required_evaluated_ref_arg(&bound, index)?.value;
        if eval_xml_callable_declared_in_eval(handler, context, values)? {
            return eval_throw_error(
                &format!(
                    "{function}(): handlers declared inside eval() cannot be invoked by the compiled parser; declare the handler in compiled code"
                ),
                context,
                values,
            );
        }
        handlers.push(handler);
    }
    eval_native_method_with_positional_values_unchecked_bridge_scope(
        parser,
        "XMLParser",
        method,
        handlers,
        Some("XMLParser"),
        None,
        context,
        values,
    )
}

/// Runs `xml_parse_into_struct()` through the parser's `__elephc_parse_into_struct()` and
/// writes the accumulated `$values` / `$index` arrays back to the caller's lvalues.
fn eval_xml_parse_into_struct(
    evaluated: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (bound, _) = bind_evaluated_ref_builtin_args(PARSE_INTO_STRUCT_PARAMS, evaluated, false)?;
    let parser = eval_xml_parser_argument("xml_parse_into_struct", &bound, context, values)?;
    let data = required_evaluated_ref_arg(&bound, 1)?.value;
    let values_target = required_evaluated_ref_arg(&bound, 2)?.ref_target.clone();
    let index_arg = optional_evaluated_ref_arg(&bound, 3);
    let with_index = values.bool_value(index_arg.is_some())?;
    let result = eval_native_method_with_positional_values_unchecked_bridge_scope(
        parser,
        "XMLParser",
        "__elephc_parse_into_struct",
        vec![data, with_index],
        Some("XMLParser"),
        None,
        context,
        values,
    )?;
    let struct_values = eval_native_method_with_positional_values_unchecked_bridge_scope(
        parser,
        "XMLParser",
        "__elephc_struct_values",
        Vec::new(),
        Some("XMLParser"),
        None,
        context,
        values,
    )?;
    eval_xml_write_output("values", 3, &values_target, struct_values, context, values)?;
    if let Some(index_arg) = index_arg {
        let struct_index = eval_native_method_with_positional_values_unchecked_bridge_scope(
            parser,
            "XMLParser",
            "__elephc_struct_index",
            Vec::new(),
            Some("XMLParser"),
            None,
            context,
            values,
        )?;
        eval_xml_write_output("index", 4, &index_arg.ref_target.clone(), struct_index, context, values)?;
    }
    Ok(result)
}

/// Writes one `xml_parse_into_struct()` output through its captured lvalue, or warns when
/// the call shape carried no reference to write through.
fn eval_xml_write_output(
    parameter: &str,
    position: usize,
    target: &Option<EvalReferenceTarget>,
    output: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match target {
        Some(target) => eval_write_direct_ref_target(
            target,
            output,
            context,
            values,
            Some(ScopeCellOwnership::Owned),
        ),
        None => {
            values.release(output)?;
            values.warning(&format!(
                "xml_parse_into_struct(): Argument #{position} (${parameter}) must be passed by reference, value given"
            ))
        }
    }
}
