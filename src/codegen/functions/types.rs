use crate::codegen::context::Context;
use crate::parser::ast::{Expr, ExprKind, TypeExpr};
use crate::types::{
    array_key_type_from_value_type, merge_array_key_types, normalized_array_key_type, FunctionSig,
    PhpType,
};

pub fn infer_local_type_pub(expr: &Expr, sig: &FunctionSig) -> PhpType {
    infer_local_type(expr, sig, None)
}

pub fn infer_local_type_with_ctx(expr: &Expr, sig: &FunctionSig, ctx: &Context) -> PhpType {
    infer_local_type(expr, sig, Some(ctx))
}

pub fn infer_contextual_type(expr: &Expr, ctx: &Context) -> PhpType {
    let empty_sig = FunctionSig {
        params: Vec::new(),
        defaults: Vec::new(),
        return_type: PhpType::Void,
        declared_return: false,
        ref_params: Vec::new(),
        declared_params: Vec::new(),
        variadic: None,
    };
    infer_local_type(expr, &empty_sig, Some(ctx))
}

/// Extract the canonical object class name from a type that statically
/// resolves to an object — either directly (`Object("Foo")`) or as the
/// single object member of a nullable / object-only union
/// (`Union([Object("Foo"), Void])`). Returns `None` for `Mixed` and any
/// union that mixes multiple classes or non-object members.
pub(crate) fn singular_object_class(ty: &PhpType) -> Option<&str> {
    match ty {
        PhpType::Object(name) => Some(name.as_str()),
        PhpType::Union(members) => {
            let mut found: Option<&str> = None;
            for member in members {
                match member {
                    PhpType::Void => continue,
                    PhpType::Object(name) => {
                        if found.is_some_and(|existing| existing != name.as_str()) {
                            return None;
                        }
                        found = Some(name.as_str());
                    }
                    _ => return None,
                }
            }
            found
        }
        _ => None,
    }
}

fn wider_of(a: &PhpType, b: &PhpType) -> PhpType {
    if a == b {
        return a.clone();
    }
    if matches!(a, PhpType::Mixed | PhpType::Union(_))
        || matches!(b, PhpType::Mixed | PhpType::Union(_))
    {
        return PhpType::Mixed;
    }
    if *a == PhpType::Str || *b == PhpType::Str {
        return PhpType::Str;
    }
    if *a == PhpType::Float || *b == PhpType::Float {
        return PhpType::Float;
    }
    if matches!(a, PhpType::Array(_)) || matches!(b, PhpType::Array(_)) {
        return a.clone();
    }
    if matches!(a, PhpType::Object(_)) || matches!(b, PhpType::Object(_)) {
        return a.clone();
    }
    a.clone()
}

fn mixed_container_value_type(ty: PhpType) -> PhpType {
    if matches!(ty, PhpType::Iterable) {
        PhpType::Mixed
    } else {
        ty
    }
}

fn array_union_type(a: &PhpType, b: &PhpType) -> Option<PhpType> {
    match (a, b) {
        (PhpType::Array(left), PhpType::Array(right)) if left == right => {
            Some(PhpType::Array(left.clone()))
        }
        (
            PhpType::AssocArray {
                key: left_key,
                value: left_value,
            },
            PhpType::AssocArray {
                key: right_key,
                value: right_value,
            },
        ) => {
            let key = if left_key == right_key {
                left_key.clone()
            } else {
                Box::new(PhpType::Mixed)
            };
            let value = if left_value == right_value {
                left_value.clone()
            } else {
                Box::new(PhpType::Mixed)
            };
            Some(PhpType::AssocArray { key, value })
        }
        _ => None,
    }
}

fn array_like_key_type(ty: &PhpType) -> PhpType {
    match ty {
        PhpType::Array(_) => PhpType::Int,
        PhpType::AssocArray { key, .. } => *key.clone(),
        _ => PhpType::Int,
    }
}

fn array_like_value_type(ty: &PhpType) -> PhpType {
    match ty {
        PhpType::Array(value) => *value.clone(),
        PhpType::AssocArray { value, .. } => *value.clone(),
        _ => PhpType::Int,
    }
}

fn indexed_array_value_type(ty: &PhpType, fallback: PhpType) -> PhpType {
    match ty {
        PhpType::Array(value) => *value.clone(),
        _ => fallback,
    }
}

fn is_empty_indexed_array_literal(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::ArrayLiteral(elems) if elems.is_empty())
}

fn resolve_buffer_element_type(type_expr: &TypeExpr, ctx: &Context) -> PhpType {
    match type_expr {
        TypeExpr::Int => PhpType::Int,
        TypeExpr::Float => PhpType::Float,
        TypeExpr::Bool => PhpType::Bool,
        TypeExpr::Ptr(target) => {
            PhpType::Pointer(target.as_ref().map(|name| name.as_str().to_string()))
        }
        TypeExpr::Named(name) => {
            if ctx.packed_classes.contains_key(name.as_str()) {
                PhpType::Packed(name.as_str().to_string())
            } else {
                PhpType::Int
            }
        }
        TypeExpr::Str => PhpType::Str,
        TypeExpr::Void => PhpType::Void,
        TypeExpr::Never => PhpType::Never,
        TypeExpr::Buffer(inner) => {
            PhpType::Buffer(Box::new(resolve_buffer_element_type(inner, ctx)))
        }
        TypeExpr::Iterable | TypeExpr::Nullable(_) | TypeExpr::Union(_) => PhpType::Int,
    }
}

pub(crate) fn codegen_declared_type(type_expr: &TypeExpr, ctx: &Context) -> PhpType {
    match type_expr {
        TypeExpr::Int => PhpType::Int,
        TypeExpr::Float => PhpType::Float,
        TypeExpr::Bool => PhpType::Bool,
        TypeExpr::Str => PhpType::Str,
        TypeExpr::Void => PhpType::Void,
        TypeExpr::Never => PhpType::Never,
        TypeExpr::Iterable => PhpType::Iterable,
        TypeExpr::Ptr(target) => {
            PhpType::Pointer(target.as_ref().map(|name| name.as_str().to_string()))
        }
        TypeExpr::Buffer(inner) => {
            PhpType::Buffer(Box::new(resolve_buffer_element_type(inner, ctx)))
        }
        TypeExpr::Named(name) => match name.as_str() {
            "string" => PhpType::Str,
            "mixed" => PhpType::Mixed,
            "callable" => PhpType::Callable,
            "void" => PhpType::Void,
            "array" => PhpType::Array(Box::new(PhpType::Int)),
            _ if ctx.packed_classes.contains_key(name.as_str()) => {
                PhpType::Packed(name.as_str().to_string())
            }
            _ if ctx.classes.contains_key(name.as_str())
                || ctx.interfaces.contains_key(name.as_str())
                || ctx.extern_classes.contains_key(name.as_str()) =>
            {
                PhpType::Object(name.as_str().to_string())
            }
            _ => PhpType::Int,
        },
        TypeExpr::Nullable(_) | TypeExpr::Union(_) => PhpType::Mixed,
    }
}

pub(crate) fn codegen_static_type(type_expr: &TypeExpr, ctx: &Context) -> PhpType {
    match type_expr {
        TypeExpr::Nullable(inner) => {
            merge_union_members(vec![codegen_static_type(inner, ctx), PhpType::Void])
        }
        TypeExpr::Union(members) => merge_union_members(
            members
                .iter()
                .map(|member| codegen_static_type(member, ctx))
                .collect(),
        ),
        _ => codegen_declared_type(type_expr, ctx),
    }
}

fn merge_union_members(members: Vec<PhpType>) -> PhpType {
    let mut flat = Vec::new();
    for member in members {
        match member {
            PhpType::Union(inner) => flat.extend(inner),
            PhpType::Mixed => return PhpType::Mixed,
            other => flat.push(other),
        }
    }
    let mut deduped = Vec::new();
    for member in flat {
        if !deduped.iter().any(|existing| existing == &member) {
            deduped.push(member);
        }
    }
    if deduped.len() == 1 {
        deduped.pop().expect("union member exists")
    } else {
        PhpType::Union(deduped)
    }
}

pub(super) fn infer_local_type(
    expr: &Expr,
    sig: &FunctionSig,
    ctx: Option<&Context>,
) -> PhpType {
    match &expr.kind {
        ExprKind::BoolLiteral(_) => PhpType::Bool,
        ExprKind::Null => PhpType::Void,
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::FloatLiteral(_) => PhpType::Float,
        ExprKind::Variable(name) => {
            for (pname, pty) in &sig.params {
                if pname == name {
                    return pty.clone();
                }
            }
            if let Some(c) = ctx {
                if let Some(var) = c.variables.get(name) {
                    return var.static_ty.clone();
                }
            }
            PhpType::Int
        }
        ExprKind::ArrayLiteral(elems) => {
            let elem_ty = if elems.is_empty() {
                PhpType::Int
            } else {
                mixed_container_value_type(infer_local_type(&elems[0], sig, ctx))
            };
            PhpType::Array(Box::new(elem_ty))
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            let mut key_ty = pairs
                .first()
                .map(|(key, _)| normalized_array_key_type(key, infer_local_type(key, sig, ctx)))
                .unwrap_or(PhpType::Mixed);
            let mut value_ty = pairs
                .first()
                .map(|(_, value)| mixed_container_value_type(infer_local_type(value, sig, ctx)))
                .unwrap_or(PhpType::Mixed);
            for (key, value) in pairs.iter().skip(1) {
                key_ty = merge_array_key_types(
                    key_ty,
                    normalized_array_key_type(key, infer_local_type(key, sig, ctx)),
                );
                let next_ty = mixed_container_value_type(infer_local_type(value, sig, ctx));
                if next_ty != value_ty {
                    value_ty = PhpType::Mixed;
                }
            }
            PhpType::AssocArray {
                key: Box::new(key_ty),
                value: Box::new(value_ty),
            }
        }
        ExprKind::ArrayAccess { array, .. } => match infer_local_type(array, sig, ctx) {
            PhpType::Str => PhpType::Str,
            PhpType::Array(t) => *t,
            PhpType::AssocArray { value, .. } => *value,
            PhpType::Buffer(t) => match *t {
                PhpType::Packed(name) => PhpType::Pointer(Some(name)),
                other => other,
            },
            _ => PhpType::Int,
        },
        ExprKind::Negate(inner) => {
            let inner_ty = infer_local_type(inner, sig, ctx);
            if inner_ty == PhpType::Float {
                PhpType::Float
            } else {
                PhpType::Int
            }
        }
        ExprKind::Not(_) => PhpType::Bool,
        ExprKind::BitNot(_) => PhpType::Int,
        ExprKind::ErrorSuppress(inner) => infer_local_type(inner, sig, ctx),
        ExprKind::NullCoalesce { value, default } => {
            let left = infer_local_type(value, sig, ctx);
            let right = infer_local_type(default, sig, ctx);
            wider_of(&left, &right)
        }
        ExprKind::Assignment { value, .. } => infer_local_type(value, sig, ctx),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_ty = infer_local_type(then_expr, sig, ctx);
            let else_ty = infer_local_type(else_expr, sig, ctx);
            wider_of(&then_ty, &else_ty)
        }
        ExprKind::ShortTernary { value, default } => {
            let value_ty = infer_local_type(value, sig, ctx);
            let default_ty = infer_local_type(default, sig, ctx);
            wider_of(&value_ty, &default_ty)
        }
        ExprKind::BinaryOp { left, op, right } => {
            use crate::parser::ast::BinOp;
            match op {
                BinOp::Concat => PhpType::Str,
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::LtEq
                | BinOp::GtEq
                | BinOp::StrictEq
                | BinOp::StrictNotEq
                | BinOp::And
                | BinOp::Or
                | BinOp::Xor => PhpType::Bool,
                BinOp::BitAnd
                | BinOp::BitOr
                | BinOp::BitXor
                | BinOp::ShiftLeft
                | BinOp::ShiftRight
                | BinOp::Spaceship => PhpType::Int,
                BinOp::NullCoalesce => {
                    let lt = infer_local_type(left, sig, ctx);
                    let rt = infer_local_type(right, sig, ctx);
                    wider_of(&lt, &rt)
                }
                BinOp::Div | BinOp::Pow => PhpType::Float,
                BinOp::Add => {
                    let lt = infer_local_type(left, sig, ctx);
                    let rt = infer_local_type(right, sig, ctx);
                    if matches!((&lt, &rt), (PhpType::Array(_), PhpType::Array(_)))
                        && is_empty_indexed_array_literal(left)
                    {
                        rt
                    } else if matches!((&lt, &rt), (PhpType::Array(_), PhpType::Array(_)))
                        && is_empty_indexed_array_literal(right)
                    {
                        lt
                    } else if let Some(ty) = array_union_type(&lt, &rt) {
                        ty
                    } else if lt == PhpType::Float || rt == PhpType::Float {
                        PhpType::Float
                    } else {
                        PhpType::Int
                    }
                }
                BinOp::Sub | BinOp::Mul | BinOp::Mod => {
                    let lt = infer_local_type(left, sig, ctx);
                    let rt = infer_local_type(right, sig, ctx);
                    if lt == PhpType::Float || rt == PhpType::Float {
                        PhpType::Float
                    } else {
                        PhpType::Int
                    }
                }
            }
        }
        ExprKind::InstanceOf { .. } => PhpType::Bool,
        ExprKind::FunctionCall { name, args } => {
            match name.as_str() {
                "strtolower" | "strtoupper" | "ucfirst" | "lcfirst" | "ucwords" | "trim"
                | "ltrim" | "rtrim" | "substr" | "str_repeat" | "strrev" | "str_replace"
                | "str_ireplace" | "substr_replace" | "str_pad" | "chr" | "implode" | "join"
                | "sprintf" | "number_format" | "nl2br" | "wordwrap" | "addslashes"
                | "stripslashes" | "htmlspecialchars" | "html_entity_decode" | "htmlentities"
                | "urlencode" | "urldecode" | "rawurlencode" | "rawurldecode" | "base64_encode"
                | "base64_decode" | "bin2hex" | "hex2bin" | "md5" | "sha1" | "hash" | "gettype"
                | "strstr" | "readline" | "date" | "json_encode" | "php_uname" | "phpversion"
                | "tempnam" | "getcwd" | "shell_exec" => PhpType::Str,
                "array_keys" => {
                    let arr_ty = args
                        .first()
                        .map(|arg| infer_local_type(arg, sig, ctx))
                        .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Int)));
                    PhpType::Array(Box::new(array_like_key_type(&arr_ty)))
                }
                "array_values" => {
                    let arr_ty = args
                        .first()
                        .map(|arg| infer_local_type(arg, sig, ctx))
                        .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Int)));
                    PhpType::Array(Box::new(array_like_value_type(&arr_ty)))
                }
                "array_combine" => {
                    let key_ty = args
                        .first()
                        .map(|arg| infer_local_type(arg, sig, ctx))
                        .map(|ty| indexed_array_value_type(&ty, PhpType::Str))
                        .map(array_key_type_from_value_type)
                        .unwrap_or(PhpType::Str);
                    let value_ty = args
                        .get(1)
                        .map(|arg| infer_local_type(arg, sig, ctx))
                        .map(|ty| indexed_array_value_type(&ty, PhpType::Int))
                        .unwrap_or(PhpType::Int);
                    PhpType::AssocArray {
                        key: Box::new(key_ty),
                        value: Box::new(value_ty),
                    }
                }
                "array_fill_keys" => {
                    let key_ty = args
                        .first()
                        .map(|arg| infer_local_type(arg, sig, ctx))
                        .map(|ty| indexed_array_value_type(&ty, PhpType::Str))
                        .map(array_key_type_from_value_type)
                        .unwrap_or(PhpType::Str);
                    let value_ty = args
                        .get(1)
                        .map(|arg| infer_local_type(arg, sig, ctx))
                        .unwrap_or(PhpType::Int);
                    PhpType::AssocArray {
                        key: Box::new(key_ty),
                        value: Box::new(value_ty),
                    }
                }
                "array_flip" => {
                    let arr_ty = args
                        .first()
                        .map(|arg| infer_local_type(arg, sig, ctx))
                        .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Int)));
                    match arr_ty {
                        PhpType::Array(value) => PhpType::AssocArray {
                            key: Box::new(array_key_type_from_value_type(*value)),
                            value: Box::new(PhpType::Int),
                        },
                        PhpType::AssocArray { key, value } => PhpType::AssocArray {
                            key: Box::new(array_key_type_from_value_type(*value)),
                            value: key,
                        },
                        _ => PhpType::AssocArray {
                            key: Box::new(PhpType::Int),
                            value: Box::new(PhpType::Int),
                        },
                    }
                }
                "array_diff_key" | "array_intersect_key" => args
                    .first()
                    .map(|arg| infer_local_type(arg, sig, ctx))
                    .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Int))),
                "explode"
                | "str_split"
                | "file"
                | "scandir"
                | "glob"
                | "array_merge"
                | "array_slice"
                | "array_reverse"
                | "array_unique"
                | "array_chunk"
                | "array_pad"
                | "array_fill"
                | "array_diff"
                | "array_intersect"
                | "array_splice"
                | "array_column"
                | "array_map"
                | "array_filter"
                | "range"
                | "array_rand"
                | "sscanf"
                | "fgetcsv"
                | "preg_split" => {
                    if name.as_str() == "explode"
                        || name.as_str() == "str_split"
                        || name.as_str() == "file"
                        || name.as_str() == "scandir"
                        || name.as_str() == "glob"
                        || name.as_str() == "fgetcsv"
                        || name.as_str() == "preg_split"
                    {
                        PhpType::Array(Box::new(PhpType::Str))
                    } else if !args.is_empty() {
                        let arr_ty = infer_local_type(&args[0], sig, ctx);
                        match arr_ty {
                            PhpType::Array(t) => PhpType::Array(t),
                            _ => PhpType::Array(Box::new(PhpType::Int)),
                        }
                    } else {
                        PhpType::Array(Box::new(PhpType::Int))
                    }
                }
                "floatval" | "floor" | "ceil" | "round" | "sqrt" | "pow" | "fmod" | "fdiv"
                | "microtime" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2"
                | "sinh" | "cosh" | "tanh" | "log" | "log2" | "log10" | "exp" | "hypot" | "pi"
                | "deg2rad" | "rad2deg" => PhpType::Float,
                "is_int" | "is_float" | "is_string" | "is_bool" | "is_null" | "is_numeric"
                | "is_nan" | "is_finite" | "is_infinite" | "is_array" | "empty" | "isset"
                | "is_file" | "is_dir" | "is_readable" | "is_writable" | "file_exists"
                | "in_array" | "array_key_exists" | "str_contains" | "str_starts_with"
                | "str_ends_with" | "ctype_alpha" | "ctype_digit" | "ctype_alnum"
                | "ctype_space" | "function_exists" | "ptr_is_null" => PhpType::Bool,
                "define" => PhpType::Bool,
                "strpos" | "strrpos" | "array_search" | "file_get_contents" => PhpType::Mixed,
                "abs" => {
                    if !args.is_empty() {
                        let t = infer_local_type(&args[0], sig, ctx);
                        if t == PhpType::Float {
                            PhpType::Float
                        } else {
                            PhpType::Int
                        }
                    } else {
                        PhpType::Int
                    }
                }
                "min" | "max" => {
                    if args.len() >= 2 {
                        let t0 = infer_local_type(&args[0], sig, ctx);
                        let t1 = infer_local_type(&args[1], sig, ctx);
                        if t0 == PhpType::Float || t1 == PhpType::Float {
                            PhpType::Float
                        } else {
                            PhpType::Int
                        }
                    } else {
                        PhpType::Int
                    }
                }
                "ptr" | "ptr_null" => PhpType::Pointer(None),
                "buffer_len" => PhpType::Int,
                "ptr_offset" => {
                    if let Some(first_arg) = args.first() {
                        match infer_local_type(first_arg, sig, ctx) {
                            PhpType::Pointer(tag) => PhpType::Pointer(tag),
                            _ => PhpType::Pointer(None),
                        }
                    } else {
                        PhpType::Pointer(None)
                    }
                }
                "ptr_get" | "ptr_read8" | "ptr_read32" | "ptr_sizeof" => PhpType::Int,
                _ => {
                    if let Some(c) = ctx {
                        if let Some(fn_sig) = c.functions.get(name.as_str()) {
                            return fn_sig.return_type.clone();
                        }
                    }
                    PhpType::Int
                }
            }
        }
        ExprKind::Cast { target, .. } => {
            use crate::parser::ast::CastType;
            match target {
                CastType::Int => PhpType::Int,
                CastType::Float => PhpType::Float,
                CastType::String => PhpType::Str,
                CastType::Bool => PhpType::Bool,
                CastType::Array => PhpType::Array(Box::new(PhpType::Int)),
            }
        }
        ExprKind::Closure { .. } => PhpType::Callable,
        ExprKind::ClosureCall { var, .. } => {
            if let Some(c) = ctx {
                if let Some(sig) = c.closure_sigs.get(var) {
                    return sig.return_type.clone();
                }
            }
            PhpType::Int
        }
        ExprKind::ExprCall { callee, .. } => {
            if let Some(c) = ctx {
                match &callee.kind {
                    ExprKind::Variable(var_name) => {
                        if let Some(sig) = c.closure_sigs.get(var_name) {
                            return sig.return_type.clone();
                        }
                    }
                    ExprKind::ArrayAccess { array, .. } => {
                        if let ExprKind::Variable(arr_name) = &array.kind {
                            if let Some(sig) = c.closure_sigs.get(arr_name) {
                                return sig.return_type.clone();
                            }
                        }
                    }
                    _ => {}
                }
            }
            if let ExprKind::Closure {
                return_type: Some(type_ann),
                ..
            } = &callee.kind
            {
                return ctx
                    .map(|c| codegen_static_type(type_ann, c))
                    .unwrap_or(PhpType::Mixed);
            }
            if let ExprKind::Closure { body, .. } = &callee.kind {
                return crate::types::checker::infer_return_type_syntactic(body);
            }
            PhpType::Int
        }
        ExprKind::ConstRef(name) => ctx
            .and_then(|c| c.constants.get(name.as_str()).map(|(_, ty)| ty.clone()))
            .unwrap_or(PhpType::Int),
        ExprKind::EnumCase { enum_name, .. } => PhpType::Object(enum_name.as_str().to_string()),
        ExprKind::Spread(inner) => infer_local_type(inner, sig, ctx),
        ExprKind::NamedArg { value, .. } => infer_local_type(value, sig, ctx),
        ExprKind::NewObject { class_name, .. } => PhpType::Object(class_name.as_str().to_string()),
        ExprKind::BufferNew { element_type, .. } => {
            if let Some(c) = ctx {
                let elem_ty = resolve_buffer_element_type(element_type, c);
                PhpType::Buffer(Box::new(elem_ty))
            } else {
                PhpType::Buffer(Box::new(PhpType::Int))
            }
        }
        ExprKind::PropertyAccess { object, property } => {
            if let Some(c) = ctx {
                if let Some((cn, nullable)) = nullsafe_context_class(object, sig, c) {
                    if let Some(ci) = c.classes.get(&cn) {
                        if let Some((_, ty)) = ci.properties.iter().find(|(n, _)| n == property) {
                            return if nullable {
                                merge_union_members(vec![ty.clone(), PhpType::Void])
                            } else {
                                ty.clone()
                            };
                        }
                        if let Some(sig) = ci.methods.get("__get") {
                            return if nullable {
                                merge_union_members(vec![sig.return_type.clone(), PhpType::Void])
                            } else {
                                sig.return_type.clone()
                            };
                        }
                    }
                }
                let obj_ty = infer_local_type(object, sig, Some(c));
                if let PhpType::Pointer(Some(cn)) = &obj_ty {
                    if let Some(ci) = c.extern_classes.get(cn) {
                        if let Some(field) = ci.fields.iter().find(|field| field.name == *property)
                        {
                            return field.php_type.clone();
                        }
                    }
                    if let Some(ci) = c.packed_classes.get(cn) {
                        if let Some(field) = ci.fields.iter().find(|field| field.name == *property)
                        {
                            return field.php_type.clone();
                        }
                    }
                }
            }
            PhpType::Int
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            if let Some(c) = ctx {
                if let Some((cn, nullable)) = nullsafe_context_class(object, sig, c) {
                    if let Some(ci) = c.classes.get(&cn) {
                        if let Some((_, ty)) = ci.properties.iter().find(|(n, _)| n == property) {
                            return if nullable {
                                merge_union_members(vec![ty.clone(), PhpType::Void])
                            } else {
                                ty.clone()
                            };
                        }
                        if let Some(sig) = ci.methods.get("__get") {
                            return if nullable {
                                merge_union_members(vec![sig.return_type.clone(), PhpType::Void])
                            } else {
                                sig.return_type.clone()
                            };
                        }
                    }
                }
            }
            PhpType::Void
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            if let Some(c) = ctx {
                let class_name = match receiver {
                    crate::parser::ast::StaticReceiver::Named(class_name) => {
                        class_name.as_str().to_string()
                    }
                    crate::parser::ast::StaticReceiver::Self_
                    | crate::parser::ast::StaticReceiver::Static => {
                        if let Some(current_class) = &c.current_class {
                            current_class.clone()
                        } else {
                            return PhpType::Int;
                        }
                    }
                    crate::parser::ast::StaticReceiver::Parent => {
                        if let Some(current_class) = &c.current_class {
                            if let Some(parent_name) = c
                                .classes
                                .get(current_class)
                                .and_then(|ci| ci.parent.as_ref())
                            {
                                parent_name.clone()
                            } else {
                                return PhpType::Int;
                            }
                        } else {
                            return PhpType::Int;
                        }
                    }
                };
                if let Some(ci) = c.classes.get(&class_name) {
                    if let Some((_, ty)) = ci
                        .static_properties
                        .iter()
                        .find(|(name, _)| name == property)
                    {
                        return ty.clone();
                    }
                }
            }
            PhpType::Int
        }
        ExprKind::MethodCall { object, method, .. } => {
            if let Some(c) = ctx {
                let obj_ty = infer_local_type(object, sig, Some(c));
                if let Some(cn) = singular_object_class(&obj_ty) {
                    if let Some(ci) = c.classes.get(cn) {
                        if let Some(msig) = ci.methods.get(method) {
                            return msig.return_type.clone();
                        }
                    }
                }
            }
            PhpType::Int
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            if let Some(c) = ctx {
                if let Some((cn, nullable)) = nullsafe_context_class(object, sig, c) {
                    if let Some(ci) = c.classes.get(&cn) {
                        if let Some(msig) = ci.methods.get(method) {
                            return if nullable {
                                merge_union_members(vec![msig.return_type.clone(), PhpType::Void])
                            } else {
                                msig.return_type.clone()
                            };
                        }
                    }
                }
            }
            PhpType::Void
        }
        ExprKind::StaticMethodCall {
            receiver, method, ..
        } => {
            if let Some(c) = ctx {
                let class_name = match receiver {
                    crate::parser::ast::StaticReceiver::Named(class_name) => {
                        class_name.as_str().to_string()
                    }
                    crate::parser::ast::StaticReceiver::Self_
                    | crate::parser::ast::StaticReceiver::Static => {
                        if let Some(current_class) = &c.current_class {
                            current_class.clone()
                        } else {
                            return PhpType::Int;
                        }
                    }
                    crate::parser::ast::StaticReceiver::Parent => {
                        if let Some(current_class) = &c.current_class {
                            if let Some(parent_name) = c
                                .classes
                                .get(current_class)
                                .and_then(|ci| ci.parent.as_ref())
                            {
                                parent_name.clone()
                            } else {
                                return PhpType::Int;
                            }
                        } else {
                            return PhpType::Int;
                        }
                    }
                };
                if let Some(ci) = c.classes.get(&class_name) {
                    if let Some(msig) = ci.static_methods.get(method) {
                        return msig.return_type.clone();
                    }
                }
            }
            PhpType::Int
        }
        ExprKind::This => {
            if let Some(c) = ctx {
                if let Some(cn) = &c.current_class {
                    return PhpType::Object(cn.clone());
                }
            }
            PhpType::Object(String::new())
        }
        ExprKind::PtrCast { target_type, .. } => PhpType::Pointer(Some(target_type.clone())),
        _ => PhpType::Int,
    }
}

fn nullsafe_context_class(
    object: &Expr,
    sig: &FunctionSig,
    ctx: &Context,
) -> Option<(String, bool)> {
    match infer_local_type(object, sig, Some(ctx)) {
        PhpType::Object(class_name) => Some((class_name, false)),
        PhpType::Void => None,
        PhpType::Union(members) => {
            let mut class_name = None;
            let mut nullable = false;
            for member in members {
                match member {
                    PhpType::Void => nullable = true,
                    PhpType::Object(candidate) => class_name = Some(candidate),
                    _ => return None,
                }
            }
            class_name.map(|name| (name, nullable))
        }
        _ => None,
    }
}
