//! Purpose:
//! Injects the two elephc-PHP helpers that back PHP's `(object)` cast:
//! `__elephc_cast_object` (the statically non-object source) and
//! `__elephc_cast_object_dynamic` (a runtime-typed source that may already hold an object).
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`, AFTER
//!   `autoload::run`, so `ir_lower::expr::lower_cast` can lower a `(object)` cast to an
//!   ordinary call to the injected declaration.
//!
//! Key details:
//! - Implemented as a prelude rather than a runtime helper because PHP's conversion is a
//!   key walk plus dynamic property writes — both already exist as ordinary elephc-PHP, so
//!   every supported target gets the cast with no per-target assembly (the same reason
//!   `var_export_prelude` is a prelude).
//! - PHP's rules, reproduced exactly: an array becomes a stdClass whose property names are
//!   the array's keys rendered as strings; `null` becomes an empty stdClass; every other
//!   non-object value becomes a stdClass with a single `scalar` property; an object is
//!   returned UNCHANGED (same instance, not a copy).
//! - The split into two helpers is what keeps the cast's static type precise.
//!   `__elephc_cast_object` is declared `: stdClass` and is what `lower_cast` calls when the
//!   source cannot be an object, so `(object) ['a' => 1]` types as `stdClass` and property
//!   reads stay on the nominal path. Only a `mixed`/union source needs the `: mixed`
//!   `__elephc_cast_object_dynamic`, because the object it returns unchanged may be of any
//!   class.
//! - Pay-for-use: injected only when the program spells an object cast anywhere, detected
//!   through the shared exhaustive walk in `opcache_prelude::detect` rather than a second
//!   traversal of its own.
//! - Injection runs LATE, past the pipeline's own name-resolution pass, because the cast is
//!   detected syntactically and an autoloaded class file is not part of the AST before
//!   `autoload::run`. The declarations are name-resolved on the way in, the way `autoload::run`
//!   resolves the files it splices.

use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{BinOp, CastType, Program, TypeExpr};
use crate::opcache_prelude::detect::{self, Symbol, SymbolKind};
use crate::span::Span;
use crate::synthetic_class::{
    e_assign, e_binop, e_call, e_cast, e_dyn_prop, e_not, e_null, e_var, function,
    internal_declarations, s_assign, s_expr, s_foreach, s_if, s_prop_assign, s_return, t_mixed,
};

/// The PHP property name a scalar, bool, float or resource source is stored under.
///
/// php-src uses the literal `scalar` for every non-array, non-null, non-object value, so
/// `((object) 42)->scalar` is `42`. The name is not configurable and programs read it back.
const SCALAR_PROPERTY: &str = "scalar";

/// Builds `__elephc_cast_object($value): stdClass` — PHP's `(object)` conversion for a
/// source that is statically known not to be an object.
///
/// The array arm renders each key with `(string)`, which is what makes an integer-keyed
/// array's properties the numeric-string names PHP produces (`((object) [0 => 'a'])->{'0'}`).
fn cast_object_decl() -> crate::parser::ast::Stmt {
    function("__elephc_cast_object")
        .param("value", t_mixed())
        .returns(TypeExpr::Named(Name::from("stdClass".to_string())))
        .body(vec![
            s_assign("object", crate::synthetic_class::e_new("stdClass", vec![])),
            s_if(
                e_call("is_array", vec![e_var("value")]),
                vec![
                    s_foreach(
                        e_var("value"),
                        Some("key"),
                        "element",
                        vec![
                            s_assign("name", e_cast(CastType::String, e_var("key"))),
                            s_expr(e_assign(
                                e_dyn_prop(e_var("object"), e_var("name")),
                                e_var("element"),
                            )),
                        ],
                    ),
                    s_return(e_var("object")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                vec![s_return(e_var("object"))],
                vec![],
                None,
            ),
            s_prop_assign(e_var("object"), SCALAR_PROPERTY, e_var("value")),
            s_return(e_var("object")),
        ])
        .build()
}

/// Builds `__elephc_cast_object_dynamic($value): mixed` — the runtime-typed entry point.
///
/// PHP's `(object)` is the IDENTITY on an object: the same instance comes back, so a later
/// `===` against the source still holds and a mutation through either name is visible
/// through the other. That arm is why this helper returns `mixed` rather than `stdClass` —
/// the instance it hands back can be of any class.
///
/// The body CONVERTS IN PLACE and returns `$value` once, rather than returning the parameter
/// early from an `is_object()` arm and the converted object from a second `return`. The two
/// spellings are equivalent PHP, but the early-return one is miscompiled today: a `mixed`
/// function with a CONDITIONAL `return $param;` alongside another return corrupts the
/// refcount of an object payload (`heap debug detected bad refcount`), where the same function
/// with a single trailing `return $param;` is clean. That is a pre-existing return-alias hole
/// reachable from ordinary user PHP — `function f(mixed $v): mixed { if (is_object($v)) {
/// return $v; } return $v; }` reproduces it with no cast involved — and this shape is written
/// to stay out of it rather than to work around it here.
fn cast_object_dynamic_decl() -> crate::parser::ast::Stmt {
    function("__elephc_cast_object_dynamic")
        .param("value", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_not(e_call("is_object", vec![e_var("value")])),
                vec![s_assign(
                    "value",
                    e_call("__elephc_cast_object", vec![e_var("value")]),
                )],
                vec![],
                None,
            ),
            s_return(e_var("value")),
        ])
        .build()
}

/// Builds both object-cast helpers.
pub(crate) fn object_cast_declarations() -> Program {
    internal_declarations(|| vec![cast_object_decl(), cast_object_dynamic_decl()])
}

/// Returns whether the program spells a `(object)` cast anywhere.
///
/// Rides on `opcache_prelude::detect`'s single exhaustive walk (see
/// [`SymbolKind::ObjectCast`]) so no second traversal can drift out of step with the AST.
///
/// Runs under `crate::compiler_stack::with_compiler_stack` (issue #1149): the scan walks the whole
/// program, so an embedder calling it without the CLI driver's wrapper gets the same stack budget.
pub fn program_uses_object_cast(program: &[crate::parser::ast::Stmt]) -> bool {
    crate::compiler_stack::with_compiler_stack(|| {
        program_uses_object_cast_on_compiler_stack(program)
    })
}

/// The body of [`program_uses_object_cast`], run on the stack `with_compiler_stack` sized.
fn program_uses_object_cast_on_compiler_stack(program: &[crate::parser::ast::Stmt]) -> bool {
    detect::first_reference(program, Symbol::syntactic(SymbolKind::ObjectCast)).is_some()
}

/// Returns the span of a user declaration of either helper name, if the program has one.
fn declared_helper(program: &[crate::parser::ast::Stmt]) -> Option<(&'static str, Span)> {
    for helper in [CAST_HELPER, DYNAMIC_CAST_HELPER] {
        if let Some(span) = detect::first_declaration(program, helper) {
            return Some((helper, span));
        }
    }
    None
}

/// Prepends the object-cast helpers when the program contains an object cast; otherwise
/// returns the program unchanged so unrelated binaries pay nothing. The prelude is hoisted
/// function declarations only, so prepending does not change top-level execution order.
///
/// Injection runs AFTER the pipeline's name-resolution pass (see the call site in
/// `pipeline::compile`, which is positioned there so a cast inside an autoloaded class file is
/// detected at all), so the declarations are resolved here the way `autoload::run` resolves the
/// files it splices in.
///
/// A program that declares either helper name itself is REJECTED rather than silently having
/// its own definition win: `ir_lower` lowers every `(object)` cast to a call on that name, so a
/// user definition would not merely shadow the prelude, it would become the cast's semantics.
/// Prepending regardless was no better — the checker reported `Duplicate function declaration`
/// at the user's own line with no hint of why.
///
/// Runs under `crate::compiler_stack::with_compiler_stack` (issue #1149): its usage scan walks the
/// whole program, so an embedder calling it without the CLI driver's wrapper gets the same stack
/// budget.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Result<Program, CompileError> {
    crate::compiler_stack::with_compiler_stack(|| {
        inject_if_used_on_compiler_stack(program, inventory)
    })
}

/// The body of [`inject_if_used`], run on the stack `with_compiler_stack` sized.
fn inject_if_used_on_compiler_stack(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Result<Program, CompileError> {
    if !program_uses_object_cast(&program) {
        return Ok(program);
    }
    if let Some((helper, span)) = declared_helper(&program) {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot declare {}(): the name is reserved for the compiler's `(object)` cast \
                 helper, which this program's `(object)` cast is lowered to. Rename the function.",
                helper
            ),
        ));
    }
    let mut combined = crate::name_resolver::resolve(object_cast_declarations())?;
    inventory.record_program("object_cast", &combined);
    combined.extend(program);
    Ok(combined)
}

/// The name of the helper `ir_lower` calls for a source that cannot be an object.
pub(crate) const CAST_HELPER: &str = "__elephc_cast_object";

/// The name of the helper `ir_lower` calls for a runtime-typed source.
pub(crate) const DYNAMIC_CAST_HELPER: &str = "__elephc_cast_object_dynamic";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// Parses a fixture the way the pipeline does, so the detector sees a real AST.
    fn parsed(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// A program with no object cast must carry neither helper: the prelude is pay-for-use.
    #[test]
    fn a_program_without_an_object_cast_carries_nothing() {
        assert!(!program_uses_object_cast(&parsed("<?php $a = (array) 1;")));
    }

    /// The cast is detected inside a function body, not only at top level.
    #[test]
    fn an_object_cast_inside_a_function_is_detected() {
        assert!(program_uses_object_cast(&parsed(
            "<?php function f($v) { return (object) $v; }"
        )));
    }

    /// `__elephc_cast_object` is declared `: stdClass`, which is what keeps
    /// `(object) ['a' => 1]` on the nominal object path instead of degrading to `mixed`.
    #[test]
    fn the_static_helper_returns_stdclass() {
        let decl = object_cast_declarations()
            .into_iter()
            .next()
            .expect("the static helper is declared first");
        let StmtKind::FunctionDecl {
            name, return_type, ..
        } = &decl.kind
        else {
            panic!("expected a function declaration");
        };
        assert_eq!(name.as_str(), CAST_HELPER);
        assert_eq!(
            return_type.as_ref(),
            Some(&TypeExpr::Named(Name::from("stdClass".to_string())))
        );
    }
}
