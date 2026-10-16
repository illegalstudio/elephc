//! Purpose:
//! Generates real Zend handlers, parameter validation and reflection metadata.
//!
//! Called from:
//! - The producer build path after EIR native export discovery.
//!
//! Key details:
//! - Zend parses and coerces arguments before any native runtime state is activated.
//! - The C adapter contains no replacement engine symbols or Rust stack frames.

use crate::{exports::ExportedFunction, parser::ast::ExprKind, types::PhpType};

/// Renders a complete host adapter using one fixed handler for each exported function.
pub fn render(name: &str, version: &str, exports: &[&ExportedFunction]) -> Result<String, String> {
    let mut registration = super::render_registration(name, version)?;
    let ident = format!("elephc_generated_{name}");
    let mut entries = String::new();
    let mut handlers = String::new();
    for export in exports {
        let symbol = &export.c_name;
        let sig = &export.source_sig;
        let required = sig.defaults.iter().rposition(Option::is_none).map_or(0, |i| i + 1);
        handlers.push_str(&format!("extern void {symbol}(const zval *const *);\n"));
        if export.php_void_return || sig.return_type == PhpType::Never {
            let ty = if export.php_void_return { "IS_VOID" } else { "IS_NEVER" };
            handlers.push_str(&format!("ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_{symbol}, 0, {required}, {ty}, 0)\n"));
        } else {
            handlers.push_str(&format!("ZEND_BEGIN_ARG_WITH_RETURN_TYPE_MASK_EX(arginfo_{symbol}, 0, {required}, {})\n", mask(&sig.return_type)?));
        }
        for (i, (param, ty)) in sig.params.iter().enumerate() {
            let expr = sig.defaults.get(i).and_then(Option::as_ref);
            let mut default = default_text(expr)?;
            if matches!(base_type(ty)?.0, PhpType::Float) {
                if let Some(crate::parser::ast::Expr { kind: ExprKind::IntLiteral(value), .. }) = expr {
                    default = Some(format!("{:?}", *value as f64));
                }
            }
            handlers.push_str(&format!("    ZEND_ARG_TYPE_MASK(0, {param}, {}, {})\n", mask(ty)?, default.as_ref().map(|s| c_string(s.as_bytes())).unwrap_or("NULL".into())));
        }
        handlers.push_str(&format!("ZEND_END_ARG_INFO()\nstatic ZEND_FUNCTION({symbol})\n{{\n    zval values[{}];\n", sig.params.len().max(1)));
        let mut parse = String::new();
        let mut normalize = String::new();
        for (i, (_, ty)) in sig.params.iter().enumerate() {
            let value = format!("values[{i}]");
            let initial = default_c(sig.defaults.get(i).and_then(Option::as_ref), &value)?;
            handlers.push_str(&format!("    {initial}\n"));
            if i == required { parse.push_str("        Z_PARAM_OPTIONAL\n"); }
            let (base, nullable) = base_type(ty)?;
            let base = &base;
            let suffix = if nullable { "_OR_NULL" } else { "" };
            if !nullable && matches!(base, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Str) {
                let expected = match base {
                    PhpType::Int => "int", PhpType::Float => "float", PhpType::Bool => "bool", _ => "string",
                };
                // Preserve source PHP null rejection before ZPP's legacy scalar coercion.
                handlers.push_str(&format!("    if (ZEND_NUM_ARGS() > {i}) {{ zval *supplied = ZEND_CALL_ARG(execute_data, {}); ZVAL_DEREF(supplied); if (Z_TYPE_P(supplied) == IS_NULL) {{ zend_argument_type_error({}, \"must be of type {expected}, null given\"); return; }} }}\n", i + 1, i + 1));
            }
            match base {
                PhpType::Int | PhpType::Float | PhpType::Bool => {
                    if nullable { handlers.push_str(&format!("    bool null_{i} = Z_TYPE({value}) == IS_NULL;\n")); }
                    let (ctype, macro_type, getter, setter) = match base {
                        PhpType::Int => ("zend_long", "LONG", "Z_LVAL", "ZVAL_LONG"),
                        PhpType::Float => ("double", "DOUBLE", "Z_DVAL", "ZVAL_DOUBLE"),
                        _ => ("bool", "BOOL", "zend_is_true", "ZVAL_BOOL"),
                    };
                    let getter = if base == &PhpType::Float {
                        format!("(Z_TYPE({value}) == IS_LONG ? (double)Z_LVAL({value}) : Z_DVAL({value}))")
                    } else if getter == "zend_is_true" { format!("zend_is_true(&{value})") } else { format!("{getter}({value})") };
                    handlers.push_str(&format!("    {ctype} arg_{i} = Z_TYPE({value}) == IS_NULL ? 0 : {getter};\n"));
                    let extra = if nullable { format!(", null_{i}") } else { String::new() };
                    parse.push_str(&format!("        Z_PARAM_{macro_type}{suffix}(arg_{i}{extra})\n"));
                    normalize.push_str(&format!("    {setter}(&{value}, arg_{i});\n"));
                    if nullable { normalize.push_str(&format!("    if (null_{i}) {{ ZVAL_NULL(&{value}); }}\n")); }
                }
                PhpType::Str => {
                    handlers.push_str(&format!("    char *arg_{i} = Z_TYPE({value}) == IS_NULL ? NULL : Z_STRVAL({value});\n    size_t len_{i} = Z_TYPE({value}) == IS_NULL ? 0 : Z_STRLEN({value});\n"));
                    parse.push_str(&format!("        Z_PARAM_STRING{suffix}(arg_{i}, len_{i})\n"));
                    // ZPP owns any coercion temporary until this handler returns.
                    normalize.push_str(&format!("    if (arg_{i}) {{ ZVAL_STR(&{value}, (zend_string *)(arg_{i} - offsetof(zend_string, val))); }} else {{ ZVAL_NULL(&{value}); }}\n"));
                }
                _ => {
                    handlers.push_str(&format!("    zval *arg_{i} = &{value};\n"));
                    parse.push_str(&format!("        Z_PARAM_ZVAL(arg_{i})\n"));
                    normalize.push_str(&format!("    ZVAL_DEREF(arg_{i});\n"));
                    if base.is_php_array() || matches!(base, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                        let null = if nullable { format!(" && Z_TYPE_P(arg_{i}) != IS_NULL") } else { String::new() };
                        normalize.push_str(&format!("    if (Z_TYPE_P(arg_{i}) != IS_ARRAY{null}) {{ zend_argument_type_error({}, \"must be of type array{}\"); return; }}\n", i + 1, if nullable { "|null" } else { "" }));
                    } else if base == &PhpType::False {
                        let null = if nullable { format!(" && Z_TYPE_P(arg_{i}) != IS_NULL") } else { String::new() };
                        normalize.push_str(&format!("    if (Z_TYPE_P(arg_{i}) != IS_FALSE{null}) {{ zend_argument_type_error({}, \"must be false{}\"); return; }}\n", i + 1, if nullable { "|null" } else { "" }));
                    } else if base == &PhpType::Void {
                        normalize.push_str(&format!("    if (Z_TYPE_P(arg_{i}) != IS_NULL) {{ zend_argument_type_error({}, \"must be null\"); return; }}\n", i + 1));
                    }
                    normalize.push_str(&format!("    ZVAL_COPY_VALUE(&{value}, arg_{i});\n"));
                }
            }
        }
        handlers.push_str(&format!("    ZEND_PARSE_PARAMETERS_START({required}, {})\n{parse}    ZEND_PARSE_PARAMETERS_END();\n{normalize}    ep_invoke(INTERNAL_FUNCTION_PARAM_PASSTHRU, {symbol}, values, {});\n}}\n", sig.params.len(), sig.params.len()));
        entries.push_str(&format!("    ZEND_RAW_FENTRY({}, zif_{symbol}, arginfo_{symbol}, 0, NULL, NULL)\n", c_string(export.name.trim_start_matches('\\').as_bytes())));
    }
    registration = registration.replace("static const zend_function_entry", &format!("{}\n{}\n{handlers}\nstatic const zend_function_entry", include_str!("transport.c"), include_str!("activation.c")));
    registration = registration.replace("    ZEND_FE_END", &format!("{entries}    ZEND_FE_END"));
    registration = registration.replacen("    return SUCCESS;", "    ep_state_size = (uintptr_t)elephc_state_end - (uintptr_t)elephc_state_begin;\n    ep_template = malloc(ep_state_size);\n    if (!ep_template) { return FAILURE; }\n    memcpy(ep_template, elephc_state_begin, ep_state_size);\n    return SUCCESS;", 1);
    // Registration metadata was validated before inserting the C templates.
    registration = registration.replace("@IDENT@", &ident);
    let shutdown = format!("static PHP_MSHUTDOWN_FUNCTION({ident})\n{{\n    (void)type;\n    (void)module_number;\n");
    registration = registration.replace(&shutdown, &format!("{shutdown}    free(ep_template);\n    ep_template = NULL;\n"));
    registration = registration.replace("    ZEND_TSRMLS_CACHE_UPDATE();\n#endif\n    return SUCCESS;", "    ZEND_TSRMLS_CACHE_UPDATE();\n#endif\n    EP_GLOBAL(snapshot) = malloc(ep_state_size);\n    if (!EP_GLOBAL(snapshot)) { return FAILURE; }\n    memcpy(EP_GLOBAL(snapshot), ep_template, ep_state_size);\n    return SUCCESS;");
    let rshutdown = format!("static PHP_RSHUTDOWN_FUNCTION({ident})\n{{\n    (void)type;\n    (void)module_number;\n");
    registration = registration.replace(&rshutdown, &format!("{rshutdown}    free(EP_GLOBAL(snapshot));\n    EP_GLOBAL(snapshot) = NULL;\n"));
    registration = registration.replace("    STANDARD_MODULE_PROPERTIES", &format!("    ZEND_MODULE_GLOBALS({ident}),\n    PHP_GINIT({ident}),\n    PHP_GSHUTDOWN({ident}),\n    NULL,\n    STANDARD_MODULE_PROPERTIES_EX"));
    Ok(registration)
}

/// Returns the nullable base accepted by Zend parameter parsing.
fn base_type(ty: &PhpType) -> Result<(PhpType, bool), String> {
    if ty.is_php_array() { return Ok((ty.clone(), false)); }
    if let PhpType::Union(members) = ty {
        let non_null = members.iter().filter(|ty| **ty != PhpType::Void).collect::<Vec<_>>();
        if non_null.len() != members.len() {
            if non_null.len() == 1 { return Ok((non_null[0].clone(), true)); }
            let array = PhpType::Union(non_null.into_iter().cloned().collect());
            if array.is_php_array() { return Ok((array, true)); }
        }
        return Err(format!("PHP export parameter union is not yet supported: {ty:?}"));
    }
    Ok((ty.clone(), false))
}

/// Encodes PHP-visible reflection types using the selected headers' type masks.
fn mask(ty: &PhpType) -> Result<String, String> {
    Ok(match ty {
        PhpType::Int => "MAY_BE_LONG".into(), PhpType::Float => "MAY_BE_DOUBLE".into(),
        PhpType::Str => "MAY_BE_STRING".into(), PhpType::Bool => "MAY_BE_BOOL".into(),
        PhpType::False => "MAY_BE_FALSE".into(), PhpType::Void => "MAY_BE_NULL".into(),
        PhpType::Never => "MAY_BE_NEVER".into(), PhpType::Mixed => "MAY_BE_ANY".into(),
        PhpType::Array(_) | PhpType::AssocArray { .. } => "MAY_BE_ARRAY".into(),
        PhpType::Union(members) => members.iter().map(mask).collect::<Result<Vec<_>, _>>()?.join(" | "),
        _ => return Err(format!("unsupported PHP extension type: {ty:?}")),
    })
}

/// Serializes byte strings without allowing source text to inject generated C.
fn c_string(bytes: &[u8]) -> String {
    let body = bytes.iter().map(|byte| format!("\\{:03o}", byte)).collect::<String>();
    format!("\"{body}\"")
}

/// Provides reflection defaults for the constant forms accepted by this boundary.
fn default_text(expr: Option<&crate::parser::ast::Expr>) -> Result<Option<String>, String> {
    let Some(expr) = expr else { return Ok(None); };
    Ok(Some(match &expr.kind {
        ExprKind::IntLiteral(v) => v.to_string(), ExprKind::FloatLiteral(v) => {
            if v.is_nan() { "NAN".into() } else if *v == f64::INFINITY { "INF".into() }
            else if *v == f64::NEG_INFINITY { "-INF".into() } else { format!("{v:?}") }
        },
        ExprKind::BoolLiteral(v) => v.to_string(), ExprKind::Null => "null".into(),
        ExprKind::ArrayLiteral(values) if values.is_empty() => "[]".into(),
        ExprKind::StringLiteral(v) => format!("\"{}\"", v.as_bytes().iter().map(|b| format!("\\x{b:02x}")).collect::<String>()),
        _ => return Err("PHP extension defaults currently require scalar, null or empty array constants".into()),
    }))
}

/// Initializes optional argument storage without allocating from the Zend heap.
fn default_c(expr: Option<&crate::parser::ast::Expr>, value: &str) -> Result<String, String> {
    Ok(match expr.map(|expr| &expr.kind) {
        None | Some(ExprKind::Null) => format!("ZVAL_NULL(&{value});"),
        Some(ExprKind::IntLiteral(v)) => format!("ZVAL_LONG(&{value}, (zend_long)UINT64_C({}));", *v as u64),
        Some(ExprKind::FloatLiteral(v)) => {
            let literal = if v.is_nan() { "__builtin_nan(\"\")".into() }
                else if *v == f64::INFINITY { "__builtin_inf()".into() }
                else if *v == f64::NEG_INFINITY { "-__builtin_inf()".into() }
                else { format!("{v:?}") };
            format!("ZVAL_DOUBLE(&{value}, {literal});")
        },
        Some(ExprKind::BoolLiteral(v)) => format!("ZVAL_BOOL(&{value}, {});", u8::from(*v)),
        Some(ExprKind::ArrayLiteral(values)) if values.is_empty() => format!("ZVAL_EMPTY_ARRAY(&{value});"),
        Some(ExprKind::StringLiteral(text)) => {
            let bytes = text.as_bytes();
            let symbol = value.replace('[', "_").replace(']', "");
            format!("static const struct {{ zend_refcounted_h gc; zend_ulong h; size_t len; char val[{}]; }} default_{symbol} = {{ {{1, {{0}}}}, 0, {}, {} }}; ZVAL_STR(&{value}, (zend_string *)&default_{symbol});",
                bytes.len() + 1, bytes.len(), c_string(bytes))
        },
        _ => return Err("PHP extension defaults currently require scalar, null or empty array constants".into()),
    })
}
