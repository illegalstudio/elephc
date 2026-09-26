//! Purpose:
//! Builds DateTime static methods backing solar and procedural parsing functions.
//!
//! Called from:
//! - DateTime declaration injection.
//!
//! Key details:
//! - Method signatures expose heterogeneous procedural results as the shared mixed type.

use super::*;

/// Builds the internal static `__elephc_sun_rs(...)` core shared by `date_sun_info()`,
/// `date_sunrise()`, and `date_sunset()`. See `SUN_RS_SRC` for the algorithm and return shape.
pub(super) fn datetime_sun_rs() -> ClassMethod {
    let body = super::bodies::sun_rs();
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_sun_rs".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("t_utc_sse".to_string(), Some(TypeExpr::Int), None, false),
            ("lon".to_string(), Some(TypeExpr::Float), None, false),
            ("lat".to_string(), Some(TypeExpr::Float), None, false),
            ("altit".to_string(), Some(TypeExpr::Float), None, false),
            ("limb".to_string(), Some(TypeExpr::Int), None, false),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `__elephc_sun_val($rc, $tsval)` selector shared by `date_sun_info()`.
/// Returns `bool` for the polar all-day/all-night edge cases and the precomputed `int` timestamp
/// otherwise; the `mixed` return type preserves each branch's runtime tag. See `SUN_VAL_SRC`.
pub(super) fn datetime_sun_val() -> ClassMethod {
    let body = super::bodies::sun_val();
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_sun_val".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("rc".to_string(), Some(TypeExpr::Int), None, false),
            ("tsval".to_string(), Some(TypeExpr::Int), None, false),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `__elephc_date_sun_info($timestamp, $latitude, $longitude)` method on
/// `DateTime` backing the `date_sun_info()` procedural function. See `SUN_INFO_SRC`.
pub(super) fn datetime_sun_info() -> ClassMethod {
    let body = super::bodies::sun_info();
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_date_sun_info".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("timestamp".to_string(), Some(TypeExpr::Int), None, false),
            ("latitude".to_string(), Some(TypeExpr::Float), None, false),
            ("longitude".to_string(), Some(TypeExpr::Float), None, false),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `__elephc_date_sunfunc(...)` method on `DateTime` backing both
/// `date_sunrise()` (`$which == 0`) and `date_sunset()` (`$which == 1`). See `SUNFUNC_SRC`. The
/// optional latitude/longitude/zenith parameters default to a `-999` sentinel so the body can
/// substitute PHP's ini defaults; `$returnFormat` defaults to `SUNFUNCS_RET_STRING` (1).
pub(super) fn datetime_sunfunc() -> ClassMethod {
    let body = super::bodies::sunfunc();
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_date_sunfunc".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("which".to_string(), Some(TypeExpr::Int), None, false),
            ("timestamp".to_string(), Some(TypeExpr::Int), None, false),
            (
                "returnFormat".to_string(),
                Some(TypeExpr::Int),
                Some(Expr::new(ExprKind::IntLiteral(1), dummy())),
                false,
            ),
            (
                "latitude".to_string(),
                Some(TypeExpr::Float),
                Some(Expr::new(ExprKind::FloatLiteral(-1000.0), dummy())),
                false,
            ),
            (
                "longitude".to_string(),
                Some(TypeExpr::Float),
                Some(Expr::new(ExprKind::FloatLiteral(-1000.0), dummy())),
                false,
            ),
            (
                "zenith".to_string(),
                Some(TypeExpr::Float),
                Some(Expr::new(ExprKind::FloatLiteral(-1000.0), dummy())),
                false,
            ),
            (
                "utcOffset".to_string(),
                Some(TypeExpr::Float),
                Some(Expr::new(ExprKind::FloatLiteral(0.0), dummy())),
                false,
            ),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `__elephc_date_parse(string $datetime)` method on `DateTime` backing
/// the `date_parse()` procedural function (the name resolver desugars the call to it). Returns the
/// same component array as `date_parse_from_format`. Self-contained parsed-source body.
pub(super) fn datetime_date_parse() -> ClassMethod {
    let body = super::bodies::date_parse();
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_date_parse".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("datetime".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}
