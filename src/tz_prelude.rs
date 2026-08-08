//! Purpose:
//! The timezone-introspection standard-library surface
//! (`timezone_location_get`/`timezone_transitions_get`/`timezone_abbreviations_list`
//! plus the marshalling helpers the `DateTimeZone` OOP methods delegate to),
//! declared in Rust through `crate::synthetic_class`. Declares the `elephc_tz` bridge
//! externs and parses their serialized output into PHP arrays, so the feature compiles
//! through the normal pipeline (functions, extern C-ABI calls, arrays) with no new codegen.
//!
//! Called from:
//! - `crate::pipeline::compile()` via `inject_if_used`, after include/PDO
//!   injection and before name resolution.
//!
//! Key details:
//! - The prelude is injected only when a program references the introspection
//!   surface (see `detect`), so non-tz binaries never declare the `elephc_tz`
//!   externs and never link `libelephc_tz.a`. Its presence (the
//!   `__elephc_tz_location_get` marker) is what gates adding the three OOP methods
//!   to the synthetic `DateTimeZone` (see `inject_builtin_datetime`).
//! - `getTransitions($begin,$end)` is handled by one windowing routine whose
//!   defaults (`PHP_INT_MIN`/`PHP_INT_MAX`) reduce exactly to PHP's full no-arg
//!   list, reusing the bridge's row-0 `time` so `gmdate` is never asked to format
//!   `PHP_INT_MIN`.

use crate::parser::ast::{CType, Program, TypeExpr};
use crate::synthetic_class::{
    e_int, e_method_call, e_static_call, e_var, extern_fn, function, internal_declarations,
};

mod detect;

/// The bridge these externs bind to. A program that never touches the introspection
/// surface must not declare them, or it links `libelephc_tz.a` for nothing.
const TZ_BRIDGE: &str = "elephc_tz";

/// Builds the timezone-introspection prelude: the `elephc_tz` extern block the synthetic
/// `DateTimeZone` methods call into, plus the three procedural aliases that delegate to
/// those methods. The array marshalling lives in the methods (see `inject_builtin_datetime`),
/// so it is written once; the procedural functions are thin wrappers, matching PHP's
/// procedural/OOP duality.
///
/// `getTransitions`'s window defaults are integer LITERALS, not constant references:
/// `PHP_INT_MIN`/`PHP_INT_MAX` are dedicated lexer tokens that never reach the parser as
/// names, so the PHP form produced `IntLiteral` here too. They reduce exactly to PHP's
/// full no-arg list.
pub(crate) fn tz_declarations() -> Program {
    internal_declarations(|| {
        vec![
            extern_fn("elephc_tz_location", TZ_BRIDGE)
                .param("zone", CType::Str)
                .returns(CType::Str)
                .build(),
            extern_fn("elephc_tz_transitions", TZ_BRIDGE)
                .param("zone", CType::Str)
                .returns(CType::Str)
                .build(),
            extern_fn("elephc_tz_abbreviations", TZ_BRIDGE)
                .returns(CType::Str)
                .build(),
            function("timezone_location_get")
                .param("object", t_datetimezone())
                .returning(e_method_call(e_var("object"), "getLocation", vec![]))
                .build(),
            function("timezone_transitions_get")
                .param("object", t_datetimezone())
                .param_default("timestampBegin", TypeExpr::Int, e_int(i64::MIN))
                .param_default("timestampEnd", TypeExpr::Int, e_int(i64::MAX))
                .returning(e_method_call(
                    e_var("object"),
                    "getTransitions",
                    vec![e_var("timestampBegin"), e_var("timestampEnd")],
                ))
                .build(),
            function("timezone_abbreviations_list")
                .returning(e_static_call("DateTimeZone", "listAbbreviations", vec![]))
                .build(),
        ]
    })
}

/// The class the three procedural wrappers delegate to.
fn t_datetimezone() -> TypeExpr {
    crate::synthetic_class::t_class("DateTimeZone")
}

/// Prepends the timezone-introspection prelude to `program` when it references the
/// introspection surface, so the `elephc_tz` externs and helper functions compile
/// through the normal pipeline only for programs that use them. The prelude is
/// declarations only (extern block + functions), which are hoisted, so prepending
/// does not change top-level execution order.
///
/// `force` (set by `--with-tz`) bypasses the usage scan so the timezone surface
/// is always injected, making it available even when auto-detection would not see
/// the usage.
pub fn inject_if_used(program: Program, force: bool) -> Program {
    if !force && !detect::program_uses_tz_introspection(&program) {
        return program;
    }
    let mut combined = tz_declarations();
    combined.extend(program);
    combined
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// The surface is fixed: three externs, then the three procedural wrappers.
    #[test]
    fn declares_three_externs_and_three_wrappers() {
        let declared: Vec<String> = tz_declarations()
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::ExternFunctionDecl { name, .. } | StmtKind::FunctionDecl { name, .. } => {
                    Some(name.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            declared,
            vec![
                "elephc_tz_location",
                "elephc_tz_transitions",
                "elephc_tz_abbreviations",
                "timezone_location_get",
                "timezone_transitions_get",
                "timezone_abbreviations_list",
            ]
        );
    }

    /// Every extern must name the `elephc_tz` library, or the bridge is not linked and the
    /// symbol resolves to nothing at link time.
    #[test]
    fn every_extern_names_the_bridge() {
        for stmt in tz_declarations() {
            let StmtKind::ExternFunctionDecl { name, library, .. } = &stmt.kind else {
                continue;
            };
            assert_eq!(
                library.as_deref(),
                Some(TZ_BRIDGE),
                "{} must bind to {}",
                name,
                TZ_BRIDGE
            );
        }
    }

    /// The window defaults reduce to PHP's full no-arg list. They are integer literals
    /// because `PHP_INT_MIN`/`PHP_INT_MAX` are lexer tokens, not parsed constant names.
    #[test]
    fn the_transition_window_defaults_span_the_whole_range() {
        let transitions = tz_declarations()
            .into_iter()
            .find(|stmt| {
                matches!(&stmt.kind, StmtKind::FunctionDecl { name, .. } if name == "timezone_transitions_get")
            })
            .expect("timezone_transitions_get must be declared");
        let StmtKind::FunctionDecl { params, .. } = &transitions.kind else {
            unreachable!("filtered above");
        };
        let defaults: Vec<&crate::parser::ast::ExprKind> = params
            .iter()
            .filter_map(|(_, _, default, _)| default.as_ref().map(|expr| &expr.kind))
            .collect();
        assert_eq!(
            defaults,
            vec![
                &crate::parser::ast::ExprKind::IntLiteral(i64::MIN),
                &crate::parser::ast::ExprKind::IntLiteral(i64::MAX),
            ]
        );
    }
}
