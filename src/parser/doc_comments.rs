//! Purpose:
//! Carries PHP 8.6 parameter doc comments through the existing parameter-attribute channel.
//!
//! Called from:
//! - Function, method, closure, arrow-function, and property-hook parameter parsers.
//!
//! Key details:
//! - The lexer attaches the last `/** ... */` comment to the next token; parameter parsers read
//!   it before the parameter and on the terminating comma or `)` for the trailing form.
//! - A synthetic `#[\__elephc_param_doc("...")]` attribute keeps the raw comment without
//!   widening every parameter tuple; Reflection filters it out and exposes it through
//!   `ReflectionParameter::getDocComment()`.

use crate::names::Name;
use crate::lexer::SpannedToken;
use crate::parser::ast::{Attribute, AttributeGroup, Expr, ExprKind};
use crate::span::Span;

/// Returns the doc comment attached to the token at `pos`, if any.
pub(crate) fn doc_comment_before(tokens: &[SpannedToken], pos: usize) -> Option<String> {
    tokens
        .get(pos)
        .and_then(|(_, metadata)| metadata.doc_comment())
        .map(str::to_string)
}

/// Builds the synthetic attribute that retains one parameter doc comment.
pub(crate) fn doc_comment_attribute(doc_comment: String) -> AttributeGroup {
    AttributeGroup {
        attributes: vec![Attribute {
            name: Name::unqualified("__elephc_param_doc"),
            args: vec![Expr::new(
                ExprKind::StringLiteral(doc_comment),
                Span::dummy(),
            )],
            span: Span::dummy(),
        }],
        span: Span::dummy(),
    }
}
