//! Purpose:
//! Renders a parameter default the way PHP's Reflection dump exports its written AST, for the
//! default shapes whose dump cannot be rebuilt from the evaluated value.
//!
//! Called from:
//! - `default_members::reflection_parameter_members_with_declaring_function()`.
//!
//! Key details:
//! - Only an object default (`new Foo(...)`) and a named class constant are exported. Every
//!   other top-level default already prints correctly from its evaluated value, and PHP prints
//!   a top-level string without escaping it, which this exporter would get wrong.
//! - PHP folds what it can at compile time and exports the rest as written, and the dump shows
//!   the difference: `new \N\Foo(N\LIMIT)` keeps a user constant's name, while an array made only
//!   of literals prints every key (`[0 => 1]`). The rules below are measured on PHP 8.5.10.
//! - Anything outside the supported subset answers `None`, and the dump then falls back to the
//!   evaluated value it printed before.

use super::*;

/// Exports a default whose dump PHP renders from its written AST, or `None` to keep the value.
pub(super) fn reflection_parameter_default_export(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    default: &Expr,
) -> Option<String> {
    let exporter = DefaultExporter { ctx, current_class, current_info };
    match &default.kind {
        ExprKind::NewObject { .. } => exporter.export(default),
        ExprKind::ScopedConstantAccess { receiver: StaticReceiver::Named(_), .. } => {
            exporter.export(default)
        }
        _ => None,
    }
}

/// The class scope a default is written in, which `self::class` and `parent::class` resolve against.
struct DefaultExporter<'a, 'ctx> {
    ctx: &'a FunctionContext<'ctx>,
    current_class: &'a str,
    current_info: Option<&'a crate::types::ClassInfo>,
}

/// One compile-time array key after PHP's key normalization.
#[derive(Clone, PartialEq)]
enum LiteralKey {
    Int(i64),
    Str(String),
}

impl DefaultExporter<'_, '_> {
    /// Exports one expression, choosing the folded literal form whenever PHP folds it.
    fn export(&self, expr: &Expr) -> Option<String> {
        if self.is_literal(expr) {
            return self.export_literal(expr);
        }
        match &expr.kind {
            ExprKind::ConstRef(name) => self.user_constant_name(name.as_str()),
            ExprKind::Negate(inner) => Some(format!("-{}", self.export(inner)?)),
            ExprKind::ScopedConstantAccess { receiver, name } => {
                let receiver = match receiver {
                    // PHP marks the class fully qualified only when resolving it changed the
                    // written name: `\N\Foo::VALUE` inside `namespace N`, but a global
                    // `Foo::VALUE` stays as written (`zend_compile_const_expr_class_const`).
                    // The resolver marks every name it resolves fully qualified, so a namespace
                    // separator is the only trace left; a global class WRITTEN as `\Foo::VALUE`,
                    // which PHP prints with its backslash, prints without it here.
                    StaticReceiver::Named(class) => {
                        let text = class.as_str().trim_start_matches('\\');
                        if text.contains('\\') {
                            format!("\\{text}")
                        } else {
                            text.to_string()
                        }
                    }
                    StaticReceiver::Self_ => "self".to_string(),
                    StaticReceiver::Parent => "parent".to_string(),
                    StaticReceiver::Static => return None,
                };
                Some(format!("{receiver}::{name}"))
            }
            ExprKind::NewObject { class_name, args } => {
                let args = args.iter().map(|arg| self.export(arg)).collect::<Option<Vec<_>>>()?;
                Some(format!(
                    "new \\{}({})",
                    class_name.as_str().trim_start_matches('\\'),
                    args.join(", ")
                ))
            }
            ExprKind::NamedArg { name, value } => Some(format!("{name}: {}", self.export(value)?)),
            ExprKind::ArrayLiteral(items) => {
                let items = items.iter().map(|item| self.export(item)).collect::<Option<Vec<_>>>()?;
                Some(format!("[{}]", items.join(", ")))
            }
            ExprKind::ArrayLiteralAssoc(pairs) => {
                let pairs = pairs
                    .iter()
                    .map(|(key, value)| {
                        // The parser gives a keyless item of a keyed array its auto key with the
                        // VALUE's span; PHP exports that item without a key, as written.
                        if key.span == value.span && matches!(key.kind, ExprKind::IntLiteral(_)) {
                            return self.export(value);
                        }
                        Some(format!("{} => {}", self.export(key)?, self.export(value)?))
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(format!("[{}]", pairs.join(", ")))
            }
            _ => None,
        }
    }

    /// Returns whether PHP folds this expression to a value at compile time.
    fn is_literal(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null => true,
            ExprKind::Negate(inner) => {
                matches!(inner.kind, ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_))
            }
            ExprKind::ClassConstant { receiver } => self.class_name_of(receiver).is_some(),
            ExprKind::ArrayLiteral(items) => items.iter().all(|item| self.is_literal(item)),
            ExprKind::ArrayLiteralAssoc(pairs) => pairs
                .iter()
                .all(|(key, value)| self.is_literal(key) && self.is_literal(value)),
            _ => false,
        }
    }

    /// Exports a folded value the way PHP prints a compile-time constant.
    fn export_literal(&self, expr: &Expr) -> Option<String> {
        match &expr.kind {
            ExprKind::IntLiteral(value) => Some(value.to_string()),
            ExprKind::FloatLiteral(value) => Some(reflection_dump_float(*value)),
            ExprKind::StringLiteral(value) => Some(export_php_string(value)),
            ExprKind::BoolLiteral(value) => Some(value.to_string()),
            ExprKind::Null => Some("null".to_string()),
            ExprKind::Negate(inner) => match inner.kind {
                ExprKind::IntLiteral(value) => Some(value.checked_neg()?.to_string()),
                ExprKind::FloatLiteral(value) => Some(reflection_dump_float(-value)),
                _ => None,
            },
            ExprKind::ClassConstant { receiver } => {
                Some(export_php_string(&self.class_name_of(receiver)?))
            }
            ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
                let entries = self.literal_array_entries(expr)?;
                let rendered = entries
                    .iter()
                    .map(|(key, value)| {
                        let key = match key {
                            LiteralKey::Int(key) => key.to_string(),
                            LiteralKey::Str(key) => export_php_string(key),
                        };
                        Some(format!("{key} => {}", self.export_literal(value)?))
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(format!("[{}]", rendered.join(", ")))
            }
            _ => None,
        }
    }

    /// Builds a folded array's entries with PHP's key normalization and overwrite order.
    fn literal_array_entries<'e>(&self, expr: &'e Expr) -> Option<Vec<(LiteralKey, &'e Expr)>> {
        let written: Vec<(Option<&Expr>, &Expr)> = match &expr.kind {
            ExprKind::ArrayLiteral(items) => items.iter().map(|item| (None, item)).collect(),
            ExprKind::ArrayLiteralAssoc(pairs) => {
                pairs.iter().map(|(key, value)| (Some(key), value)).collect()
            }
            _ => return None,
        };
        let mut entries: Vec<(LiteralKey, &Expr)> = Vec::with_capacity(written.len());
        let mut next_index: Option<i64> = None;
        for (key, value) in written {
            let key = match key {
                Some(key) => self.literal_key(key)?,
                None => LiteralKey::Int(next_index.unwrap_or(0)),
            };
            if let LiteralKey::Int(index) = key {
                let following = index.checked_add(1)?;
                next_index = Some(next_index.map_or(following, |next| next.max(following)));
            }
            match entries.iter_mut().find(|(existing, _)| *existing == key) {
                Some(entry) => entry.1 = value,
                None => entries.push((key, value)),
            }
        }
        Some(entries)
    }

    /// Normalizes one folded array key the way PHP stores it.
    fn literal_key(&self, key: &Expr) -> Option<LiteralKey> {
        match &key.kind {
            ExprKind::IntLiteral(value) => Some(LiteralKey::Int(*value)),
            ExprKind::Negate(inner) => match inner.kind {
                ExprKind::IntLiteral(value) => Some(LiteralKey::Int(value.checked_neg()?)),
                _ => None,
            },
            ExprKind::BoolLiteral(value) => Some(LiteralKey::Int(i64::from(*value))),
            ExprKind::Null => Some(LiteralKey::Str(String::new())),
            ExprKind::StringLiteral(value) => Some(match canonical_integer_key(value) {
                Some(index) => LiteralKey::Int(index),
                None => LiteralKey::Str(value.clone()),
            }),
            ExprKind::ClassConstant { receiver } => {
                Some(LiteralKey::Str(self.class_name_of(receiver)?))
            }
            _ => None,
        }
    }

    /// Resolves `X::class` for the receivers PHP resolves at compile time.
    fn class_name_of(&self, receiver: &StaticReceiver) -> Option<String> {
        match receiver {
            StaticReceiver::Named(class) => Some(class.as_str().trim_start_matches('\\').to_string()),
            StaticReceiver::Self_ if !self.current_class.is_empty() => {
                Some(self.current_class.trim_start_matches('\\').to_string())
            }
            StaticReceiver::Parent => self
                .current_info
                .and_then(|info| info.parent.as_deref())
                .map(|parent| parent.trim_start_matches('\\').to_string()),
            _ => None,
        }
    }

    /// Returns a user constant's name as PHP exports it. An internal constant is folded by PHP
    /// outside a namespace, so its name is not what the dump shows and the value is kept instead.
    fn user_constant_name(&self, name: &str) -> Option<String> {
        let key = name.trim_start_matches('\\');
        if elephc_builtin_contract::lookup_constant(key).is_some()
            || !self.ctx.module.global_constants.contains_key(key)
        {
            return None;
        }
        Some(key.to_string())
    }
}

/// Quotes a string the way PHP's AST export does, escaping the quote and the backslash.
fn export_php_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for ch in value.chars() {
        if ch == '\'' || ch == '\\' {
            quoted.push('\\');
        }
        quoted.push(ch);
    }
    quoted.push('\'');
    quoted
}

/// Returns the integer a string array key becomes in PHP (`"5"` does, `"05"` and `"5 "` do not).
fn canonical_integer_key(value: &str) -> Option<i64> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    let canonical = !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && value != "-0";
    if !canonical {
        return None;
    }
    value.parse().ok()
}
