//! Purpose:
//! Builds Reflection owner constructors, attribute accessors, and variadic defaults.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - Synthetic callable metadata stays aligned with direct special lowering.

use super::*;

/// Builds a public `__construct` method for a reflection owner class using the
/// provided parameter list: each tuple is (name, type_expr, default, by_ref).
pub(super) fn builtin_reflection_owner_constructor_method(
    params: Vec<(&str, Option<TypeExpr>, Option<Expr>, bool)>,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: "__construct".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: params
            .into_iter()
            .map(|(name, ty, default, by_ref)| (name.to_string(), ty, default, by_ref))
            .collect(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: None,
        by_ref_return: false,
        body: Vec::new(),
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns PHP's public `getAttributes(?string $name = null, int $flags = 0)` method.
///
/// The body filters the collected attributes the way php does:
///
/// ```php
/// $matching = array_slice($this->__attrs, 0, 0);
/// foreach ($this->__attrs as $attribute) {
///     $attributeName = (string) $attribute->getName();
///     $keep = $name === null ? true : (($flags & ReflectionAttribute::IS_INSTANCEOF) !== 0
///         ? is_a($attributeName, (string) $name, true)
///         : strcasecmp(ltrim($attributeName, '\\'), (string) $name) === 0);
///     if ($keep) { $matching[] = $attribute; }
/// }
/// return $matching;
/// ```
///
/// It used to return every attribute whatever was asked for. symfony/routing's attribute loader
/// asks each method for `Route::class` with `IS_INSTANCEOF`, got the `#[Required]` on
/// `AbstractController::setContainer()` back as a route, and the console's route collection came
/// out empty.
///
/// PHP validates `$flags` first, as php-src does: any value but `0` and
/// `ReflectionAttribute::IS_INSTANCEOF` (2) raises PHP's `ValueError`, named after
/// `declaring_owner` (the class PHP declares the method on, such as
/// `ReflectionFunctionAbstract` for `ReflectionMethod`), whatever `$name` holds. With
/// `IS_INSTANCEOF` the filter is `is_a($attributeName, $name, true)`.
///
/// The exact-name filter folds ASCII case but does not resolve a leading separator in `$name`,
/// as PHP 8.5 does: `getAttributes("markerone")` finds `#[MarkerOne]` while
/// `getAttributes("\\MarkerOne")` finds nothing.
pub(super) fn builtin_reflection_owner_get_attributes_method(declaring_owner: &str) -> ClassMethod {
    use crate::synthetic_class::{
        e_binop, e_bool, e_call, e_cast, e_int, e_method_call, e_null, e_str, e_ternary,
        e_this_prop, e_var, s_array_push, s_assign, s_foreach, s_if, s_return,
    };
    use crate::parser::ast::CastType;
    const IS_INSTANCEOF: i64 = 2;
    let dummy_span = crate::span::Span::dummy();
    let attributes = || e_this_prop("__attrs");
    let unqualified = |value: Expr| e_call("ltrim", vec![value, e_str("\\")]);
    let wanted = || e_cast(CastType::String, e_var("name"));
    // `__attrs` is typed `array<ReflectionAttribute>` (`patch_reflection_attribute_result`), so
    // the pushed values keep the object representation the caller reads the result with.
    // No early `return $this->__attrs` for the unfiltered call: returning the property converted
    // its object array to the boxed result in place, and the next call freed those cells twice.
    // `($flags & ~ReflectionAttribute::IS_INSTANCEOF) !== 0`: php-src's validity test.
    let invalid_flags = binary_expr(
        binary_expr(
            variable_expr("flags", dummy_span),
            BinOp::BitAnd,
            Expr::new(ExprKind::IntLiteral(!2), dummy_span),
            dummy_span,
        ),
        BinOp::StrictNotEq,
        Expr::new(ExprKind::IntLiteral(0), dummy_span),
        dummy_span,
    );
    let invalid_flags_message = format!(
        "{declaring_owner}::getAttributes(): Argument #2 ($flags) must be a valid attribute filter flag"
    );
    let body = vec![
        Stmt::new(
            StmtKind::If {
                condition: invalid_flags,
                then_body: vec![Stmt::new(
                    StmtKind::Throw(Expr::new(
                        ExprKind::NewObject {
                            class_name: Name::unqualified("ValueError"),
                            args: vec![string_lit(&invalid_flags_message, dummy_span)],
                        },
                        dummy_span,
                    )),
                    dummy_span,
                )],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            dummy_span,
        ),
        // Seeded as an EMPTY SLICE of the property rather than `[]`: this body is lowered without a
        // checker pass, and a `[]` grown inside the loop is widened to `array<mixed>` at the loop
        // header -- boxed cells the caller then read as attribute objects.
        s_assign("matching", e_call("array_slice", vec![attributes(), e_int(0), e_int(0)])),
        s_foreach(
            attributes(),
            None,
            "attribute",
            vec![
                s_assign(
                    "attributeName",
                    e_cast(CastType::String, e_method_call(e_var("attribute"), "getName", vec![])),
                ),
                s_assign(
                    "keep",
                    e_ternary(
                        e_binop(e_var("name"), BinOp::StrictEq, e_null()),
                        e_bool(true),
                        e_ternary(
                            e_binop(
                                e_binop(e_var("flags"), BinOp::BitAnd, e_int(IS_INSTANCEOF)),
                                BinOp::StrictNotEq,
                                e_int(0),
                            ),
                            e_call("is_a", vec![e_var("attributeName"), wanted(), e_bool(true)]),
                            e_binop(
                                e_call(
                                    "strcasecmp",
                                    vec![unqualified(e_var("attributeName")), wanted()],
                                ),
                                BinOp::StrictEq,
                                e_int(0),
                            ),
                        ),
                    ),
                ),
                s_if(
                    e_var("keep"),
                    vec![s_array_push("matching", e_var("attribute"))],
                    vec![],
                    None,
                ),
            ],
        ),
        s_return(e_var("matching")),
    ];
    ClassMethod {
        type_params: Vec::new(),
        name: "getAttributes".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "name".to_string(),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Str))),
                null_lit(),
                false,
            ),
            ("flags".to_string(), Some(TypeExpr::Int), int_lit(0), false),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(array_type()),
        by_ref_return: false,
        body,
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Marks a synthesized variadic method signature as callable with no variadic arguments.
pub(super) fn make_reflection_variadic_optional(sig: &mut crate::types::FunctionSig) {
    if sig.variadic.is_some() {
        if let Some(default) = sig.defaults.last_mut() {
            *default = empty_array();
        }
    }
}
