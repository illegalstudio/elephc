use crate::parser::ast::{BinOp, Expr, ExprKind};

/// Check if an expression produces a string result.
pub(super) fn expr_is_str(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_) => true,
        ExprKind::BinaryOp {
            op: BinOp::Concat, ..
        } => true,
        ExprKind::FunctionCall { name, .. } => {
            matches!(
                name.as_str(),
                "substr"
                    | "strtolower"
                    | "strtoupper"
                    | "trim"
                    | "ltrim"
                    | "rtrim"
                    | "str_repeat"
                    | "strrev"
                    | "chr"
                    | "str_replace"
                    | "ucfirst"
                    | "lcfirst"
                    | "ucwords"
                    | "str_pad"
                    | "implode"
                    | "join"
                    | "sprintf"
                    | "str_word_count"
                    | "nl2br"
                    | "wordwrap"
                    | "number_format"
                    | "chunk_split"
                    | "md5"
                    | "sha1"
                    | "hash"
            )
        }
        ExprKind::Cast {
            target: crate::parser::ast::CastType::String,
            ..
        } => true,
        _ => false,
    }
}
