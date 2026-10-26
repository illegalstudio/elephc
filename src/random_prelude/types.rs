//! Purpose:
//! Declares the `Random` namespace types: the two engine interfaces, the
//! `IntervalBoundary` enum, and the extension's exception hierarchy.
//!
//! Called from:
//! - `crate::random_prelude::random_declarations`.
//!
//! Key details:
//! - `Random\Engine` is a real interface declaration, so user classes can implement it and
//!   `Random\Randomizer` dispatches to them through the ordinary vtable.
//! - The enum has no builder, so it is emitted as a raw `StmtKind::EnumDecl`; its four cases
//!   are the order PHP exposes them in.

use crate::parser::ast::{EnumCaseDecl, Program, Stmt, StmtKind, TypeExpr};
use crate::span::Span;
use crate::synthetic_class::{class, interface, method, s_namespace};

/// The four interval boundaries in declaration order.
const BOUNDARIES: &[&str] = &["ClosedOpen", "ClosedClosed", "OpenClosed", "OpenOpen"];

/// Builds the `Random` namespace declarations.
pub(super) fn declarations() -> Program {
    vec![s_namespace(
        "Random",
        vec![
            interface("Engine")
                .method(method("generate").returns(TypeExpr::Str))
                .build(),
            interface("CryptoSafeEngine").extends("Engine").build(),
            Stmt::new(
                StmtKind::EnumDecl {
                    name: "IntervalBoundary".to_string(),
                    generics: None,
                    backing_type: None,
                    cases: BOUNDARIES
                        .iter()
                        .map(|name| EnumCaseDecl {
                            name: (*name).to_string(),
                            value: None,
                            span: Span::dummy(),
                            attributes: Vec::new(),
                        })
                        .collect(),
                    implements: Vec::new(),
                    trait_uses: Vec::new(),
                    methods: Vec::new(),
                    constants: Vec::new(),
                },
                Span::dummy(),
            ),
            class("RandomError").extends("\\Error").build(),
            class("BrokenRandomEngineError")
                .extends("\\Random\\RandomError")
                .build(),
            class("RandomException").extends("\\Exception").build(),
        ],
    )]
}
