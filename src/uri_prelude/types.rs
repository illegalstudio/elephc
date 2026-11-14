//! Purpose:
//! Builds the `Uri` namespace of the URI prelude from the synthetic AST: the exception
//! hierarchy and `UriComparisonMode`.
//!
//! Called from:
//! - `crate::uri_prelude::uri_declarations`.
//!
//! Key details:
//! - Declarations are built in `SourceMode::Php`, not internal mode, so user-facing method
//!   and debug-info behavior is the ordinary class pipeline.

use crate::parser::ast::{EnumCaseDecl, Stmt, StmtKind};
use crate::span::Span;
use crate::synthetic_class::*;

/// Builds one URI prelude declaration.
pub(super) fn uri_types_declaration() -> Stmt {
    s_namespace("Uri", vec![
        class("UriException").span(Span::new(1, 1))
            .extends("\\Exception")
            .build(),
        class("UriError").span(Span::new(1, 1))
            .extends("\\Error")
            .build(),
        class("InvalidUriException").span(Span::new(1, 1))
            .extends("\\Uri\\UriException")
            .build(),
        Stmt::new(StmtKind::EnumDecl { name: "UriComparisonMode".to_string(), generics: None, backing_type: None, cases: vec![EnumCaseDecl { name: "IncludeFragment".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "ExcludeFragment".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }], implements: Vec::new(), trait_uses: Vec::new(), methods: Vec::new(), constants: Vec::new() }, Span::dummy())
    ])
}
