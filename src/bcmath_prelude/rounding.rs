//! Purpose:
//! Declares PHP 8.4's rounding-mode enum and maps its cases to the decimal engine.
//!
//! Called from:
//! - The BCMath prelude's class and rounding-function declarations.
//!
//! Key details:
//! - The PHP enum is unbacked; integer mode values stay internal engine details.

use crate::parser::ast::{EnumCaseDecl, Program, Stmt, StmtKind, TypeExpr};
use crate::span::Span;
use crate::synthetic_class::*;

/// The eight public enum cases and their existing engine rounding-mode values.
const MODES: &[(&str, i64)] = &[
    ("HalfAwayFromZero", 1), ("HalfTowardsZero", 2), ("HalfEven", 3), ("HalfOdd", 4),
    ("TowardsZero", 7), ("AwayFromZero", 8), ("NegativeInfinity", 6), ("PositiveInfinity", 5),
];

/// Builds the neutral enum and an internal typed conversion function.
pub(super) fn declarations() -> Program {
    vec![
        Stmt::new(StmtKind::EnumDecl {
            name: "RoundingMode".to_string(), generics: None, backing_type: None,
            cases: MODES.iter().map(|(name, _)| EnumCaseDecl {
                name: (*name).to_string(), value: None, span: Span::dummy(), attributes: vec![],
            }).collect(),
            implements: vec![], trait_uses: vec![], methods: vec![], constants: vec![],
        }, Span::dummy()),
        function("__elephc_rounding_mode")
            .param("_bcmode", t_class("RoundingMode"))
            .returns(TypeExpr::Int)
            .returning(e_match(e_prop(e_var("_bcmode"), "name"),
                MODES.iter().map(|(name, value)| (vec![e_str(name)], e_int(*value))).collect(), None))
            .build(),
    ]
}
