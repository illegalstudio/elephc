use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Stmt, StmtKind};
use crate::types::PhpType;

/// Infer a function's return type by scanning its body for Return statements.
/// This is a syntactic/heuristic check — no full type inference.
/// Used for functions that are never called directly (only used as callbacks).
pub fn infer_return_type_syntactic(body: &[Stmt]) -> PhpType {
    let mut types = Vec::new();
    for stmt in body {
        collect_return_types_syntactic(stmt, &mut types);
    }
    if types.is_empty() {
        return PhpType::Int;
    }
    // Pick the widest type across all return statements
    let mut result = types[0].clone();
    for ty in &types[1..] {
        result = wider_type_syntactic(&result, ty);
    }
    result
}

fn collect_return_types_syntactic(stmt: &Stmt, types: &mut Vec<PhpType>) {
    match &stmt.kind {
        StmtKind::NamespaceDecl { .. } | StmtKind::UseDecl { .. } => {}
        StmtKind::NamespaceBlock { body, .. } => {
            for inner in body {
                collect_return_types_syntactic(inner, types);
            }
        }
        StmtKind::Return(Some(expr)) => {
            types.push(infer_expr_type_syntactic(expr));
        }
        StmtKind::Return(None) => {
            types.push(PhpType::Void);
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_return_types_syntactic(s, types);
            }
            for (_, body) in elseif_clauses {
                for s in body {
                    collect_return_types_syntactic(s, types);
                }
            }
            if let Some(body) = else_body {
                for s in body {
                    collect_return_types_syntactic(s, types);
                }
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. } => {
            for s in body {
                collect_return_types_syntactic(s, types);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            for s in try_body {
                collect_return_types_syntactic(s, types);
            }
            for catch_clause in catches {
                for s in &catch_clause.body {
                    collect_return_types_syntactic(s, types);
                }
            }
            if let Some(body) = finally_body {
                for s in body {
                    collect_return_types_syntactic(s, types);
                }
            }
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                for s in body {
                    collect_return_types_syntactic(s, types);
                }
            }
            if let Some(body) = default {
                for s in body {
                    collect_return_types_syntactic(s, types);
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn wider_type_syntactic(a: &PhpType, b: &PhpType) -> PhpType {
    if a == b {
        return a.clone();
    }
    if *a == PhpType::Str || *b == PhpType::Str {
        return PhpType::Str;
    }
    if *a == PhpType::Float || *b == PhpType::Float {
        return PhpType::Float;
    }
    if *a == PhpType::Void {
        return b.clone();
    }
    if *b == PhpType::Void {
        return a.clone();
    }
    a.clone()
}

pub fn infer_expr_type_syntactic(expr: &Expr) -> PhpType {
    match &expr.kind {
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::FloatLiteral(_) => PhpType::Float,
        ExprKind::BoolLiteral(_) => PhpType::Bool,
        ExprKind::Null => PhpType::Void,
        ExprKind::Cast {
            target: CastType::String,
            ..
        } => PhpType::Str,
        ExprKind::Cast {
            target: CastType::Int,
            ..
        } => PhpType::Int,
        ExprKind::Cast {
            target: CastType::Float,
            ..
        } => PhpType::Float,
        ExprKind::Cast {
            target: CastType::Bool,
            ..
        } => PhpType::Bool,
        ExprKind::FunctionCall { name, args } => match name.as_str() {
            "substr" | "strtolower" | "strtoupper" | "trim" | "ltrim" | "rtrim" | "str_repeat"
            | "strrev" | "chr" | "str_replace" | "str_ireplace" | "ucfirst" | "lcfirst"
            | "ucwords" | "str_pad" | "implode" | "sprintf" | "nl2br" | "wordwrap" | "md5"
            | "sha1" | "hash" | "substr_replace" | "addslashes" | "stripslashes"
            | "htmlspecialchars" | "html_entity_decode" | "urlencode" | "urldecode"
            | "base64_encode" | "base64_decode" | "bin2hex" | "hex2bin" | "number_format"
            | "date" | "json_encode" | "gettype" | "str_word_count" | "chunk_split" => PhpType::Str,
            "strlen" | "strpos" | "strrpos" | "ord" | "count" | "intval" | "abs" | "intdiv"
            | "rand" | "time" => PhpType::Int,
            "floatval" | "floor" | "ceil" | "round" | "sqrt" | "pow" | "fmod" | "sin" | "cos"
            | "tan" | "asin" | "acos" | "atan" | "atan2" | "sinh" | "cosh" | "tanh" | "log"
            | "log2" | "log10" | "exp" | "hypot" | "pi" | "deg2rad" | "rad2deg" => PhpType::Float,
            "ptr" | "ptr_null" => PhpType::Pointer(None),
            "ptr_offset" => {
                if let Some(first_arg) = args.first() {
                    match infer_expr_type_syntactic(first_arg) {
                        PhpType::Pointer(tag) => PhpType::Pointer(tag),
                        _ => PhpType::Pointer(None),
                    }
                } else {
                    PhpType::Pointer(None)
                }
            }
            "ptr_is_null" => PhpType::Bool,
            "ptr_sizeof" | "ptr_get" | "ptr_read8" | "ptr_read32" => PhpType::Int,
            _ => PhpType::Int,
        },
        ExprKind::NullCoalesce { value, default } => {
            let left_ty = infer_expr_type_syntactic(value);
            let right_ty = infer_expr_type_syntactic(default);
            wider_type_syntactic(&left_ty, &right_ty)
        }
        ExprKind::Throw(_) => PhpType::Void,
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_ty = infer_expr_type_syntactic(then_expr);
            let else_ty = infer_expr_type_syntactic(else_expr);
            if then_ty == else_ty {
                then_ty
            } else if then_ty == PhpType::Str || else_ty == PhpType::Str {
                PhpType::Str
            } else if then_ty == PhpType::Float || else_ty == PhpType::Float {
                PhpType::Float
            } else {
                then_ty
            }
        }
        ExprKind::Match { arms, default, .. } => {
            let mut result_ty = default
                .as_ref()
                .map(|expr| infer_expr_type_syntactic(expr))
                .unwrap_or(PhpType::Void);
            for (_, arm_expr) in arms {
                let arm_ty = infer_expr_type_syntactic(arm_expr);
                result_ty = wider_type_syntactic(&result_ty, &arm_ty);
            }
            result_ty
        }
        ExprKind::ArrayLiteral(elems) => {
            let mut elem_ty = elems
                .first()
                .map(infer_expr_type_syntactic)
                .unwrap_or(PhpType::Mixed);
            for elem in elems.iter().skip(1) {
                elem_ty = wider_type_syntactic(&elem_ty, &infer_expr_type_syntactic(elem));
            }
            PhpType::Array(Box::new(elem_ty))
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            let mut key_ty = entries
                .first()
                .map(|(key, _)| infer_expr_type_syntactic(key))
                .unwrap_or(PhpType::Mixed);
            let mut value_ty = entries
                .first()
                .map(|(_, value)| infer_expr_type_syntactic(value))
                .unwrap_or(PhpType::Mixed);
            for (key, value) in entries.iter().skip(1) {
                key_ty = wider_type_syntactic(&key_ty, &infer_expr_type_syntactic(key));
                value_ty = wider_type_syntactic(&value_ty, &infer_expr_type_syntactic(value));
            }
            PhpType::AssocArray {
                key: Box::new(key_ty),
                value: Box::new(value_ty),
            }
        }
        ExprKind::NewObject { class_name, .. } => PhpType::Object(class_name.as_str().to_string()),
        ExprKind::EnumCase { enum_name, .. } => PhpType::Object(enum_name.as_str().to_string()),
        ExprKind::This => PhpType::Object(String::new()),
        ExprKind::PtrCast { target_type, .. } => PhpType::Pointer(Some(target_type.clone())),
        ExprKind::BinaryOp { left, op, right } => match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Mod => {
                let lt = infer_expr_type_syntactic(left);
                let rt = infer_expr_type_syntactic(right);
                if lt == PhpType::Float || rt == PhpType::Float {
                    PhpType::Float
                } else {
                    PhpType::Int
                }
            }
            BinOp::Div | BinOp::Pow => PhpType::Float,
            BinOp::Eq
            | BinOp::NotEq
            | BinOp::Lt
            | BinOp::Gt
            | BinOp::LtEq
            | BinOp::GtEq
            | BinOp::StrictEq
            | BinOp::StrictNotEq
            | BinOp::And
            | BinOp::Or => PhpType::Bool,
            BinOp::Concat => PhpType::Str,
            _ => PhpType::Int,
        },
        _ => PhpType::Int,
    }
}
