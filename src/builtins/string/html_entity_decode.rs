//! Purpose:
//! Home of the PHP `html_entity_decode` builtin and its backend-neutral runtime semantics.
//!
//! Called from:
//! - The builtin registry, checker, optimizer, and AST-to-EIR builtin lowering path.
//!
//! Key details:
//! - The typed runtime target has a validated `Str -> Str` EIR signature.
//! - The current runtime decoder implements the default quote/doctype policy and the five
//!   entities it shares with `htmlspecialchars`; non-default flags or encodings are rejected
//!   here instead of silently decoding with the wrong policy.
//! - Concrete helper symbols and registers are selected only by the target backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::PhpType;

builtin! {
    contract: "html_entity_decode",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::HtmlEntityDecode,
    ),
}

/// Validates the flags/encoding operands the unary decoder cannot honour.
///
/// PHP's default flags are `ENT_QUOTES | ENT_SUBSTITUTE | ENT_HTML401` (11); `ENT_QUOTES`
/// alone (3) has the same effect on the five entities this decoder supports. Anything else
/// (notably `ENT_NOQUOTES` and `ENT_COMPAT`), a dynamic flag operand, or a non-UTF-8 encoding
/// would silently decode with the wrong policy, so it is refused at compile time.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(flags) = cx.args.get(1) {
        if !matches!(constant_flags(flags), Some(3 | 11)) {
            return Err(CompileError::new(
                flags.span,
                "html_entity_decode() currently supports only the default quote flags \
                 (ENT_QUOTES|ENT_SUBSTITUTE|ENT_HTML401)",
            ));
        }
    }
    if let Some(encoding) = cx.args.get(2) {
        let accepted = match &encoding.kind {
            ExprKind::StringLiteral(value) => value.eq_ignore_ascii_case("UTF-8"),
            ExprKind::Null => true,
            _ => false,
        };
        if !accepted {
            return Err(CompileError::new(
                encoding.span,
                "html_entity_decode() currently supports only the UTF-8 encoding",
            ));
        }
    }
    Ok(PhpType::Str)
}

/// Resolves the subset of `ENT_*` quote flags the runtime decoder implements.
///
/// The checker runs before constant folding, so `ENT_QUOTES | ENT_SUBSTITUTE` is still a
/// binary expression. Only the default policy (3 or 11) is accepted; `ENT_NOQUOTES`,
/// `ENT_COMPAT`, and any doctype other than HTML 4.01 stay refused.
fn constant_flags(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => match name.as_str().trim_start_matches('\\') {
            "ENT_QUOTES" => Some(3),
            "ENT_SUBSTITUTE" => Some(8),
            "ENT_HTML401" => Some(0),
            "ENT_NOQUOTES" => Some(0),
            "ENT_COMPAT" => Some(2),
            _ => None,
        },
        ExprKind::BinaryOp {
            left,
            op: BinOp::BitOr,
            right,
        } => Some(constant_flags(left)? | constant_flags(right)?),
        _ => None,
    }
}
