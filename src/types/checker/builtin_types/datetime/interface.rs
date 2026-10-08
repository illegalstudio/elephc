//! Purpose:
//! DateTime backing properties, interface method contracts, and offset access.
//!
//! Called from:
//! - DateTime interface and concrete-class declaration injection.
//!
//! Key details:
//! - Interface and concrete method signatures remain synchronized, including microseconds.

use super::*;

/// Builds the `timestamp` (int) and `timezone_name` (str, default "UTC") backing properties.
pub(super) fn datetime_backing_properties() -> Vec<ClassProperty> {
    vec![
        property("timestamp", TypeExpr::Int, Expr::new(ExprKind::IntLiteral(0), dummy())),
        property(
            "timezone_name",
            TypeExpr::Str,
            Expr::new(ExprKind::StringLiteral("UTC".to_string()), dummy()),
        ),
        // Sub-second component (0..999999) preserved across operations; surfaced by getMicrosecond()
        // and the `u`/`v` format specifiers. elephc otherwise works at libc second resolution.
        property("microsecond", TypeExpr::Int, Expr::new(ExprKind::IntLiteral(0), dummy())),
        // Per-class static (0 = last createFromFormat succeeded, 1 = it failed) backing
        // getLastErrors()/date_get_last_errors(). Its storage now emits correctly for the used
        // synthetic class (see emit_static_property_initializers' emitted-class filter).
        {
            let mut p =
                property("lastErrorCount", TypeExpr::Int, Expr::new(ExprKind::IntLiteral(0), dummy()));
            p.is_static = true;
            p
        },
    ]
}

/// Builds an abstract (bodyless) interface method declaration.
pub(super) fn abstract_method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
) -> ClassMethod {
    ClassMethod {
        type_params: Vec::new(),
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: true,
        is_final: false,
        has_body: false,
        params,
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type,
        by_ref_return: false,
        body: Vec::new(),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// The `DateTimeInterface` method contract (`format`, `getTimestamp`, `getTimezone`).
pub(super) fn datetime_interface_methods() -> Vec<ClassMethod> {
    vec![
        abstract_method(
            "format",
            vec![("format".to_string(), Some(TypeExpr::Str), None, false)],
            Some(TypeExpr::Str),
        ),
        abstract_method("getTimestamp", Vec::new(), Some(TypeExpr::Int)),
        // PHP 8.4 promoted getMicrosecond() onto the interface; both concrete
        // classes implement it, and diff() reads it through the interface.
        abstract_method("getMicrosecond", Vec::new(), Some(TypeExpr::Int)),
        abstract_method(
            "getTimezone",
            Vec::new(),
            Some(TypeExpr::Named(Name::unqualified("DateTimeZone"))),
        ),
        abstract_method("getOffset", Vec::new(), Some(TypeExpr::Int)),
    ]
}

/// `DateTime`/`DateTimeImmutable::getOffset(): int` — UTC offset (seconds) of the object's own zone
/// at its stored instant, daylight-saving aware.
///
/// Like `DateTimeZone::getOffset` but reads `$this->timezone_name`/`$this->timestamp`: temporarily
/// applies the object's zone, reads the `date()` `Z` specifier, then restores the previous default.
pub(super) fn datetime_get_offset() -> ClassMethod {
    let call = |name: &str, args: Vec<Expr>| {
        Expr::new(ExprKind::FunctionCall { name: Name::unqualified(name), args }, dummy())
    };
    let var = |n: &str| Expr::new(ExprKind::Variable(n.to_string()), dummy());
    let expr_stmt = |e: Expr| Stmt::new(StmtKind::ExprStmt(e), dummy());
    let z_spec = Expr::new(ExprKind::StringLiteral("Z".to_string()), dummy());
    method(
        "getOffset",
        Vec::new(),
        Some(TypeExpr::Int),
        vec![
            // $__saved = date_default_timezone_get();
            Stmt::assign("__saved", call("date_default_timezone_get", Vec::new())),
            // date_default_timezone_set($this->timezone_name);
            expr_stmt(call("date_default_timezone_set", vec![this_property("timezone_name")])),
            // $__off = intval(date("Z", $this->timestamp));
            Stmt::assign(
                "__off",
                call("intval", vec![call("date", vec![z_spec, this_property("timestamp")])]),
            ),
            // date_default_timezone_set($__saved);  (restore the previous default)
            expr_stmt(call("date_default_timezone_set", vec![var("__saved")])),
            return_expr(var("__off")),
        ],
    )
}
