//! Purpose:
//! Owns request-input seeding for `filter.default`: raw copies for `filter_input()` and the
//! filtered superglobals PHP exposes to user code.
//!
//! Called from:
//! - `crate::filter_prelude::filter_declarations`.
//!
//! Key details:
//! - `--web` bootstrap statements run before the filter prelude's top-level initializer, so the
//!   initializer sees the parsed request arrays and can snapshot raw storage before filtering.
//! - The raw copies live in `__ElephcFilterState::$raw`, keyed by the `INPUT_*` value.
//! - `filter.default` is a system INI directive: its value is baked at compile time and applied
//!   once per request; `filter.default_flags` supplies the filter flags.

use crate::parser::ast::{BinOp, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    class, e_array, e_assign, e_binop, e_bool, e_call, e_cast, e_const, e_index, e_int, e_static_prop,
    e_str, e_var, function, s_array_assign, s_assign, s_expr, s_foreach, s_if, s_return,
    s_return_void, s_switch, t_array, t_int, t_mixed,
};

/// The compile-time filter.default configuration baked into one program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FilterDefault {
    /// PHP-visible filter name (`unsafe_raw`, `special_chars`, ...), or empty for none.
    pub(crate) name: String,
    /// `filter.default_flags` parsed with PHP's leading-integer rule.
    pub(crate) flags: i64,
}

impl Default for FilterDefault {
    /// Returns php-src's default: no filtering beyond `unsafe_raw`, with the internal
    /// `filter.default_flags` default `FILTER_FLAG_NO_ENCODE_QUOTES`.
    fn default() -> Self {
        Self {
            name: "unsafe_raw".to_string(),
            flags: 0x80,
        }
    }
}

/// Resolves a filter.default name with php-src's case-insensitive `strcasecmp` rule.
///
/// Unknown and empty values fall back to `unsafe_raw`, exactly like `UpdateDefaultFilter`.
fn canonical_filter_name(value: &str) -> &'static str {
    let value = value.trim();
    super::surface::FILTER_TABLE
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(value))
        .map_or("unsafe_raw", |(name, _)| *name)
}

/// Returns the filter.default configuration from the compiler's `--ini` overrides.
pub(crate) fn config_from_overrides(overrides: &[(String, String)]) -> FilterDefault {
    let mut config = FilterDefault::default();
    for (name, value) in overrides {
        match name.as_str() {
            "filter.default" => config.name = canonical_filter_name(value).to_string(),
            "filter.default_flags" => config.flags = leading_integer(value),
            _ => {}
        }
    }
    config
}

/// Parses PHP's `atoi` shape: optional whitespace/sign followed by decimal digits.
fn leading_integer(value: &str) -> i64 {
    let value = value.trim_start();
    let bytes = value.as_bytes();
    let mut index = 0usize;
    let mut negative = false;
    if bytes.first() == Some(&b'-') { negative = true; index = 1; }
    else if bytes.first() == Some(&b'+') { index = 1; }
    let mut result: i64 = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        result = result.saturating_mul(10).saturating_add(i64::from(bytes[index] - b'0'));
        index += 1;
    }
    if negative { -result } else { result }
}

/// Returns the request-storage initialization declarations.
pub(super) fn declarations(config: &FilterDefault) -> Program {
    vec![
        state_class(),
        apply_default_decl(),
        seed_request_decl(),
        initialize_request_decl(config),
    ]
}

/// Declares the per-program raw request-storage cell.
fn state_class() -> Stmt {
    class("__ElephcFilterState")
        .static_prop("raw", t_array(), Some(e_array(vec![])))
        .static_prop("deprecated", crate::synthetic_class::t_bool(), Some(e_bool(false)))
        .build()
}

/// Builds `__elephc_filter_apply_default(mixed, int, int, bool): mixed`, recursing through arrays.
///
/// `filter.default` stores the filter's own result in superglobals. `parse_str()` is the one
/// caller that stringifies, because PHP's PARSE_STRING input path converts each filtered value.
fn apply_default_decl() -> Stmt {
    function("__elephc_filter_apply_default")
        .param("value", t_mixed())
        .param("filter", t_int())
        .param("flags", t_int())
        .param_default("stringify", crate::synthetic_class::t_bool(), e_bool(false))
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_call("is_array", vec![e_var("value")]),
                vec![
                    s_assign("out", e_array(vec![])),
                    s_foreach(e_var("value"), Some("key"), "element", vec![s_array_assign(
                        "out",
                        e_var("key"),
                        e_call("__elephc_filter_apply_default", vec![
                            e_var("element"),
                            e_var("filter"),
                            e_var("flags"),
                            e_var("stringify"),
                        ]),
                    )]),
                    s_return(e_var("out")),
                ],
                vec![],
                None,
            ),
            s_assign(
                "filtered",
                e_call("filter_var", vec![e_var("value"), e_var("filter"), e_var("flags")]),
            ),
            s_if(
                e_binop(
                    e_binop(e_var("filtered"), BinOp::StrictEq, e_bool(false)),
                    BinOp::Or,
                    e_call("is_null", vec![e_var("filtered")]),
                ),
                vec![s_if(
                    e_var("stringify"),
                    vec![s_return(e_str(""))],
                    vec![],
                    Some(vec![s_return(e_var("filtered"))]),
                )],
                vec![],
                None,
            ),
            s_if(
                e_var("stringify"),
                vec![s_return(e_cast(crate::parser::ast::CastType::String, e_var("filtered")))],
                vec![],
                Some(vec![s_return(e_var("filtered"))]),
            ),
        ])
        .build()
}

/// Assigns one filtered request array back to its superglobal element by element.
fn assign_global(name: &str) -> Vec<Stmt> {
    vec![
        s_assign(name, e_array(vec![])),
        s_foreach(e_var("filtered"), Some("key"), "value", vec![s_array_assign(
            name,
            e_var("key"),
            e_var("value"),
        )]),
    ]
}

/// Builds `__elephc_filter_seed_request(int, mixed, string, int, bool, bool, bool): void`.
fn seed_request_decl() -> Stmt {
    function("__elephc_filter_seed_request")
        .param("type", t_int())
        .param("values", t_mixed())
        .param("default", TypeExpr::Str)
        .param("flags", t_int())
        .param_default("deprecate", crate::synthetic_class::t_bool(), e_bool(true))
        .param_default("snapshot", crate::synthetic_class::t_bool(), e_bool(true))
        .param_default("assign", crate::synthetic_class::t_bool(), e_bool(true))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_var("snapshot"),
                vec![
                    s_assign("raw", e_array(vec![])),
                    s_foreach(e_var("values"), Some("key"), "value", vec![s_array_assign(
                        "raw",
                        e_var("key"),
                        e_var("value"),
                    )]),
                    s_if(
                        crate::synthetic_class::e_not(e_call(
                            "is_array",
                            vec![e_static_prop("__ElephcFilterState", "raw")],
                        )),
                        vec![s_expr(e_assign(
                            e_static_prop("__ElephcFilterState", "raw"),
                            e_array(vec![]),
                        ))],
                        vec![],
                        None,
                    ),
                    s_expr(e_assign(
                        e_index(e_static_prop("__ElephcFilterState", "raw"), e_var("type")),
                        e_var("raw"),
                    )),
                ],
                vec![],
                Some(vec![s_assign("raw", e_var("values"))]),
            ),
            s_assign("filtered", e_var("raw")),
            s_if(
                e_binop(
                    e_binop(e_var("default"), BinOp::StrictNotEq, e_str("")),
                    BinOp::And,
                    e_binop(e_var("default"), BinOp::StrictNotEq, e_str("unsafe_raw")),
                ),
                vec![s_if(
                    e_binop(e_call("filter_id", vec![e_var("default")]), BinOp::StrictNotEq, e_bool(false)),
                    vec![
                        s_assign("filtered", e_array(vec![])),
                        s_foreach(e_var("raw"), Some("key"), "value", vec![s_array_assign(
                            "filtered",
                            e_var("key"),
                            e_call("__elephc_filter_apply_default", vec![
                                e_var("value"),
                                e_cast(crate::parser::ast::CastType::Int, e_call("filter_id", vec![e_var("default")])),
                                e_var("flags"),
                                e_bool(false),
                            ]),
                        )]),
                        s_if(
                            e_binop(
                                e_var("deprecate"),
                                BinOp::And,
                                crate::synthetic_class::e_not(e_static_prop(
                                    "__ElephcFilterState",
                                    "deprecated",
                                )),
                            ),
                            vec![
                                s_expr(e_call("__elephc_builtin_deprecated", vec![
                                    e_str("The filter.default ini setting is deprecated"),
                                    e_str("Unknown"),
                                    e_int(0),
                                ])),
                                s_expr(e_assign(
                                    e_static_prop("__ElephcFilterState", "deprecated"),
                                    e_bool(true),
                                )),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                )],
                vec![],
                None,
            ),
            s_if(
                e_var("assign"),
                vec![
                    s_switch(
                        e_var("type"),
                        vec![
                            (vec![e_const("INPUT_GET")], assign_global("_GET")),
                            (vec![e_const("INPUT_POST")], assign_global("_POST")),
                            (vec![e_const("INPUT_COOKIE")], assign_global("_COOKIE")),
                            (vec![e_const("INPUT_ENV")], assign_global("_ENV")),
                            (vec![e_const("INPUT_SERVER")], assign_global("_SERVER")),
                        ],
                        Some(vec![]),
                    ),
                    s_if(
                        e_binop(
                            e_binop(e_var("type"), BinOp::StrictEq, e_const("INPUT_GET")),
                            BinOp::Or,
                            e_binop(e_var("type"), BinOp::StrictEq, e_const("INPUT_POST")),
                        ),
                        vec![
                            s_assign("_REQUEST", e_array(vec![])),
                            s_foreach(e_var("_GET"), Some("key"), "value", vec![s_array_assign(
                                "_REQUEST",
                                e_var("key"),
                                e_var("value"),
                            )]),
                            s_foreach(e_var("_POST"), Some("key"), "value", vec![s_array_assign(
                                "_REQUEST",
                                e_var("key"),
                                e_var("value"),
                            )]),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_return_void(),
        ])
        .build()
}

/// Builds the top-level initializer that snapshots every request array and applies the baked
/// `filter.default` configuration.
///
/// `$snapshot_raw` is true for web builds, where `filter_input()` must keep the original
/// request payload. CLI builds use the optimizer's SAPI seed instead and never call this
/// initializer; the test harness seeds request arrays directly through `seed_request`.
fn initialize_request_decl(config: &FilterDefault) -> Stmt {
    let call = |constant: &str, global: &str| {
        s_expr(e_call("__elephc_filter_seed_request", vec![
            e_const(constant),
            e_var(global),
            e_str(&config.name),
            e_int(config.flags),
            e_bool(true),
            e_var("snapshot_raw"),
            e_bool(config.name != "unsafe_raw"),
        ]))
    };
    function("__elephc_filter_initialize_request")
        .param_default("snapshot_raw", crate::synthetic_class::t_bool(), e_bool(true))
        .returns(TypeExpr::Void)
        .body(vec![
            call("INPUT_GET", "_GET"),
            call("INPUT_POST", "_POST"),
            call("INPUT_COOKIE", "_COOKIE"),
            call("INPUT_ENV", "_ENV"),
            call("INPUT_SERVER", "_SERVER"),
            s_return_void(),
        ])
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uses php-src's case-insensitive filter.default table and falls back to unsafe_raw.
    #[test]
    fn filter_default_name_resolution_matches_php() {
        let config = config_from_overrides(&[
            ("filter.default".to_string(), "INT".to_string()),
            ("filter.default_flags".to_string(), "4".to_string()),
        ]);
        assert_eq!(config.name, "int");
        assert_eq!(config.flags, 4);

        let stripped = config_from_overrides(&[(
            "filter.default".to_string(),
            "STRIPPED".to_string(),
        )]);
        assert_eq!(stripped.name, "stripped");

        for value in ["", "not-a-filter", "  "] {
            let fallback = config_from_overrides(&[(
                "filter.default".to_string(),
                value.to_string(),
            )]);
            assert_eq!(fallback.name, "unsafe_raw", "{value:?}");
        }
    }

    /// Keeps PHP's internal default flags when filter.default_flags is absent.
    #[test]
    fn filter_default_flags_default_to_no_encode_quotes() {
        assert_eq!(FilterDefault::default().flags, 0x80);
        let config = config_from_overrides(&[(
            "filter.default".to_string(),
            "string".to_string(),
        )]);
        assert_eq!(config.flags, 0x80);
        let explicit =
            config_from_overrides(&[("filter.default_flags".to_string(), "0".to_string())]);
        assert_eq!(explicit.flags, 0);
    }
}
