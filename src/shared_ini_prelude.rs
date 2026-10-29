//! Purpose:
//! Builds shared Core/mbstring INI routing used by both CLI and web compatibility wrappers.
//!
//! Called from:
//! - The opcache, web, and version prelude declaration builders.
//!
//! Key details:
//! - Directive names derive from neutral catalogs; public declaration guards remain with each prelude.
//! - Extension filters use exact lowercase names, and combined enumeration sorts all keys.
//! - `memory_limit` is the one MUTABLE Core directive: `ini_get()` answers the raw configured
//!   string from a hidden static store and `ini_set()` moves both that string and the live
//!   allocator ceiling (`_heap_limit`) through `__elephc_set_memory_limit`.

use crate::{parser::ast::{BinOp, CastType, Expr, Stmt, TypeExpr}, synthetic_class::*};
use elephc_builtin_contract::mbstring_abi::ini::{catalog, core, INI_GET, INI_SET, INI_RESTORE, INI_GET_ALL};

/// Routes a known shared directive before the existing session/opcache fallback.
pub(crate) fn directive(operation: u32) -> Stmt {
    let condition = core::DIRECTIVES.iter().chain(catalog::DIRECTIVES.iter())
        .map(|entry| e_binop(e_var("option"), BinOp::StrictEq, e_str(entry.name)))
        .reduce(|left, right| e_binop(left, BinOp::Or, right)).expect("shared INI catalog");
    let value = if operation == INI_SET { e_cast(CastType::String, e_var("value")) } else { e_str("") };
    let call = e_call("__elephc_shared_ini", vec![e_int(i64::from(operation)), e_var("option"), value, e_bool(false)]);
    let body = match operation {
        INI_GET | INI_SET => vec![s_return(call)],
        INI_RESTORE => vec![s_expr(call), s_return_void()],
        _ => unreachable!("single-directive INI operation"),
    };
    s_if(condition, body, vec![], None)
}

/// Returns the startup `memory_limit` text: the last `--ini memory_limit` override exactly as
/// it was spelled, or PHP's compiled default.
///
/// `ini_get()` reports the CONFIGURED string, not a normalized byte count, so the raw spelling
/// (for example `8M`, `-1`, or ` 12M `) has to survive into the injected state helper.
pub(crate) fn memory_limit_initial(overrides: &[(String, String)]) -> String {
    overrides
        .iter()
        .rev()
        .find(|(key, _)| key.eq_ignore_ascii_case("memory_limit"))
        .map(|(_, value)| value.clone())
        .unwrap_or_else(|| "128M".to_string())
}

/// Routes `ini_get('memory_limit')` before the OPcache/session fallbacks.
///
/// `memory_limit` is runtime state rather than a compiled-in report: `ini_set()` moves it, so
/// the answer comes from [`memory_limit_state_decls`]'s hidden static store.
pub(crate) fn memory_limit_get() -> Stmt {
    s_if(
        e_binop(e_var("option"), BinOp::StrictEq, e_str("memory_limit")),
        vec![s_return(e_call("__elephc_memory_limit_raw", vec![]))],
        vec![],
        None,
    )
}

/// Builds the hidden `memory_limit` state helpers used by `ini_get()`, `ini_set()`,
/// `ini_restore()`, and `ini_get_all()`.
///
/// `__elephc_memory_limit_raw(?string $value = null, bool $update = false)` keeps the runtime
/// string in a static local; `$update` distinguishes a read from a write so an empty string
/// (which `ini_set('memory_limit', '')` legitimately stores) is not mistaken for "read only".
/// `__elephc_memory_limit_startup()` and `__elephc_memory_limit_startup_bytes()` expose the
/// compile-time value so restore and `ini_get_all()` can report it without a second parse.
pub(crate) fn memory_limit_state_decls(initial: &str) -> Vec<Stmt> {
    // The startup ceiling is a compile-time constant. `-1` (and `0`) mean "unlimited", which the
    // fixed arena implements as its full capacity in the runtime setter.
    let startup_bytes = match crate::codegen_support::parse_memory_limit_bytes(initial) {
        Ok(Some(bytes)) => i64::try_from(bytes).unwrap_or(i64::MAX),
        _ => -1,
    };
    vec![
        function("__elephc_memory_limit_raw")
            .param_default("value", t_nullable(TypeExpr::Str), e_null())
            .param_default("update", TypeExpr::Bool, e_bool(false))
            .returns(TypeExpr::Str)
            .body(vec![
                s_static("raw", e_str(initial)),
                s_if(
                    e_var("update"),
                    vec![s_assign(
                        "raw",
                        e_cast(CastType::String, e_var("value")),
                    )],
                    vec![],
                    None,
                ),
                s_return(e_var("raw")),
            ])
            .build(),
        function("__elephc_memory_limit_startup")
            .returns(TypeExpr::Str)
            .returning(e_str(initial))
            .build(),
        function("__elephc_memory_limit_startup_bytes")
            .returns(TypeExpr::Int)
            .returning(e_int(startup_bytes))
            .build(),
    ]
}

/// Reinstalls the compile-time `memory_limit` text and ceiling, for `ini_restore()`.
///
/// The startup byte count is baked into the state helper, so restore does not need the runtime
/// quantity parser (which is injected only with the OPcache INI surface).
pub(crate) fn memory_limit_restore() -> Stmt {
    s_if(
        e_binop(e_var("option"), BinOp::StrictEq, e_str("memory_limit")),
        vec![
            s_expr(e_call(
                "__elephc_set_memory_limit",
                vec![e_call("__elephc_memory_limit_startup_bytes", vec![])],
            )),
            s_expr(e_call(
                "__elephc_memory_limit_raw",
                vec![e_call("__elephc_memory_limit_startup", vec![]), e_bool(true)],
            )),
            s_return_void(),
        ],
        vec![],
        None,
    )
}

/// Handles `ini_set('memory_limit', $value)` with php-src's update-handler rules.
///
/// The value is parsed with the same `zend_ini_parse_quantity` mirror the `opcache.*` arms use;
/// `-1` means unlimited. A finite request below the current live heap usage is refused with
/// PHP's `Failed to set memory limit ...` warning and the previous value stays installed.
/// Raising above the reserved arena is accepted (as in PHP) but the allocator clamps the
/// effective ceiling to `_heap_max`, because this runtime cannot grow its reservation.
///
/// `ini_set()` returns the PREVIOUS raw string, read before the write.
pub(crate) fn memory_limit_set(web: bool) -> Stmt {
    let message = e_binop(
        e_binop(
            e_binop(
                e_str("Failed to set memory limit to "),
                BinOp::Concat,
                e_var("__elephc_ml_bytes"),
            ),
            BinOp::Concat,
            e_str(" bytes (Current memory usage is "),
        ),
        BinOp::Concat,
        e_binop(
            e_var("__elephc_ml_current"),
            BinOp::Concat,
            e_str(" bytes)"),
        ),
    );
    let warning = if web {
        s_expr(e_call(
            "__elephc_web_trigger_error",
            vec![message, e_const("E_WARNING")],
        ))
    } else {
        s_expr(e_call("__elephc_builtin_warning", vec![message]))
    };
    let quantity_message = e_binop(
        e_str("Invalid \"memory_limit\" setting. "),
        BinOp::Concat,
        e_var("__elephc_ml_diag"),
    );
    let quantity_warning = if web {
        s_expr(e_call(
            "__elephc_web_trigger_error",
            vec![quantity_message, e_const("E_WARNING")],
        ))
    } else {
        s_expr(e_call("__elephc_builtin_warning", vec![quantity_message]))
    };
    s_if(
        e_binop(e_var("option"), BinOp::StrictEq, e_str("memory_limit")),
        vec![
            s_assign(
                "__elephc_ml_raw",
                e_cast(CastType::String, e_var("value")),
            ),
            s_assign(
                "__elephc_ml_result",
                e_call(
                    "__elephc_ini_uquantity",
                    vec![e_var("__elephc_ml_raw")],
                ),
            ),
            s_assign(
                "__elephc_ml_bytes",
                e_cast(
                    CastType::Int,
                    e_index(e_var("__elephc_ml_result"), e_int(0)),
                ),
            ),
            s_assign(
                "__elephc_ml_diag",
                e_cast(
                    CastType::String,
                    e_index(e_var("__elephc_ml_result"), e_int(1)),
                ),
            ),
            s_if(
                e_binop(
                    e_var("__elephc_ml_diag"),
                    BinOp::StrictNotEq,
                    e_str(""),
                ),
                vec![quantity_warning],
                vec![],
                None,
            ),
            // php-src compares against `zend_memory_usage(true)`, whose allocator reports
            // whole 2 MiB chunks. Round the live footprint the same way; the 2 MiB floor is
            // also why `0` and `1M` are refused at startup even when almost nothing is live.
            s_assign(
                "__elephc_ml_current",
                e_call("memory_get_usage", vec![]),
            ),
            s_if(
                e_binop(
                    e_var("__elephc_ml_current"),
                    BinOp::Lt,
                    e_int(2_097_152),
                ),
                vec![s_assign("__elephc_ml_current", e_int(2_097_152))],
                vec![],
                Some(vec![s_assign(
                    "__elephc_ml_current",
                    e_binop(
                        e_call(
                            "intdiv",
                            vec![
                                e_binop(
                                    e_var("__elephc_ml_current"),
                                    BinOp::Add,
                                    e_int(2_097_151),
                                ),
                                e_int(2_097_152),
                            ],
                        ),
                        BinOp::Mul,
                        e_int(2_097_152),
                    ),
                )]),
            ),
            // `-1` is the only unlimited spelling; every finite value must be at least the
            // current real usage.
            s_if(
                e_binop(
                    e_binop(
                        e_var("__elephc_ml_bytes"),
                        BinOp::StrictNotEq,
                        e_neg(e_int(1)),
                    ),
                    BinOp::And,
                    e_binop(
                        e_var("__elephc_ml_bytes"),
                        BinOp::Lt,
                        e_var("__elephc_ml_current"),
                    ),
                ),
                vec![warning, s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_expr(e_call(
                "__elephc_set_memory_limit",
                vec![e_var("__elephc_ml_bytes")],
            )),
            s_assign(
                "__elephc_ml_previous",
                e_call("__elephc_memory_limit_raw", vec![]),
            ),
            s_expr(e_call(
                "__elephc_memory_limit_raw",
                vec![e_var("__elephc_ml_raw"), e_bool(true)],
            )),
            s_return(e_var("__elephc_ml_previous")),
        ],
        vec![],
        None,
    )
}

/// Handles mbstring's explicit extension filter before legacy unknown-module diagnostics.
pub(crate) fn all_prefix() -> Vec<Stmt> {
    ["mbstring"].into_iter().map(|module| s_if(
        e_binop(e_var("extension"), BinOp::StrictEq, e_str(module)),
        vec![s_return(e_cast(CastType::Array, all(module, e_var("details"))))], vec![], None,
    )).collect()
}

/// Builds the same extension filtering and sorted enumeration around CLI or session-aware legacy rows.
pub(crate) fn all_declaration(web: bool) -> Stmt {
    let mut unsupported = e_binop(e_binop(e_var("extension"), BinOp::StrictNotEq, e_null()), BinOp::And,
        e_binop(e_var("extension"), BinOp::StrictNotEq, e_str("zend opcache")));
    unsupported = e_binop(unsupported, BinOp::And, e_binop(e_var("extension"), BinOp::StrictNotEq, e_str("core")));
    if web { unsupported = e_binop(unsupported, BinOp::And,
        e_binop(e_var("extension"), BinOp::StrictNotEq, e_str("session"))); }
    let legacy = |details| e_call(match (web, details) {
        (true, true) => "__elephc_ini_all_details", (true, false) => "__elephc_ini_all_plain",
        (false, true) => "__elephc_opcache_ini_all_details", (false, false) => "__elephc_opcache_ini_all_plain",
    }, if web { vec![e_var("extension")] } else { vec![] });
    let mut body = all_prefix();
    let message = e_binop(e_binop(e_str("ini_get_all(): Extension \""), BinOp::Concat,
        e_var("extension")), BinOp::Concat, e_str("\" cannot be found"));
    let warning = if web { e_call("__elephc_web_trigger_error", vec![message, e_const("E_WARNING")]) }
        else { e_call("fwrite", vec![e_const("STDERR"), e_binop(e_binop(e_str("Warning: "), BinOp::Concat, message),
            BinOp::Concat, e_str("\n"))]) };
    body.push(s_if(unsupported, vec![
        s_if(e_call("__elephc_ini_module_known", vec![e_var("extension")]), vec![s_return(e_array(vec![]))], vec![], None),
        s_expr(warning),
        s_return(e_bool(false)),
    ], vec![], None));
    body.push(s_if(e_var("details"), all_result(legacy(true), true), vec![], None));
    body.extend(all_result(legacy(false), false));
    function("ini_get_all")
        .param_default("extension", t_nullable(TypeExpr::Str), e_null())
        .param_default("details", TypeExpr::Bool, e_bool(true))
        .returns(t_union(vec![t_array(), TypeExpr::False]))
        .body(body).build()
}

/// Adds shared rows for null/Core enumeration and sorts the final combined result.
pub(crate) fn all_result(legacy: Expr, details: bool) -> Vec<Stmt> {
    let suffix = if details { "details" } else { "plain" };
    let result = format!("__elephc_shared_all_{suffix}");
    let mut body = vec![s_assign(&result, e_array(vec![]))];
    for (index, source) in [all("core", e_bool(details)), all("mbstring", e_bool(details)), legacy.clone()].into_iter().enumerate() {
        let key = format!("__elephc_shared_key_{suffix}_{index}");
        let value = format!("__elephc_shared_value_{suffix}_{index}");
        body.push(s_foreach(source, Some(&key), &value, vec![
            s_array_assign(&result, e_cast(CastType::String, e_var(&key)), e_var(&value)),
        ]));
    }
    // `memory_limit` is a Core directive whose local value moves at runtime, so it cannot come
    // from the static Core catalog. It joins only the null/Core filters, as in php-src.
    let memory_limit_row = if details {
        e_array_assoc(vec![
            (e_str("global_value"), e_call("__elephc_memory_limit_startup", vec![])),
            (e_str("local_value"), e_call("__elephc_memory_limit_raw", vec![])),
            (e_str("access"), e_int(7)),
        ])
    } else {
        e_call("__elephc_memory_limit_raw", vec![])
    };
    body.push(s_if(
        e_binop(
            e_binop(e_var("extension"), BinOp::StrictEq, e_null()),
            BinOp::Or,
            e_binop(e_var("extension"), BinOp::StrictEq, e_str("core")),
        ),
        vec![s_array_assign(&result, e_str("memory_limit"), memory_limit_row)],
        vec![],
        None,
    ));
    body.push(s_expr(e_call("ksort", vec![e_var(&result)])));
    body.push(s_return(e_var(&result)));
    // PHP's Core module number is zero, so its explicit name also disables per-module filtering.
    vec![s_if(e_binop(e_binop(e_var("extension"), BinOp::StrictEq, e_null()), BinOp::Or,
        e_binop(e_var("extension"), BinOp::StrictEq, e_str("core"))), body, vec![], None), s_return(legacy)]
}

/// Forms one typed internal invocation without redeclaring metadata or exposing bridge operations publicly.
fn all(module: &str, details: Expr) -> Expr {
    e_call("__elephc_shared_ini", vec![e_int(i64::from(INI_GET_ALL)), e_str(module), e_str(""), details])
}
