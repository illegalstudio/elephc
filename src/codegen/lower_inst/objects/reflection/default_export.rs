//! Purpose:
//! Renders a parameter default the way PHP's Reflection dump exports its written AST, for the
//! default shapes whose dump cannot be rebuilt from the evaluated value.
//!
//! Called from:
//! - `default_members::reflection_parameter_members_with_declaring_function()`.
//!
//! Key details:
//! - Object defaults and named class constants preserve their written expressions. Defaults
//!   stored as lazy helpers or without compact values also use the retained source expression.
//! - Top-level strings use the bytewise Reflection formatter; nested AST strings follow PHP's
//!   source export rules instead.
//! - PHP folds what it can at compile time and exports the rest as written, and the dump shows
//!   the difference: folded parameter arrays compact list keys, while a folded array inside
//!   an AST export prints every key. Null and non-integral float keys keep the written AST.
//! - Folded associative keys retain the parser/folder's profile-specific implicit numbering.
//! - Anything outside the supported subset answers `None`, and the dump then falls back to the
//!   evaluated value it printed before.
//! - Exported strings retain the compiler's private byte markers until dump property emission.
//!   Non-literal names are encoded separately to preserve genuine private-use Unicode text.

use super::*;

/// Exports a default whose dump PHP renders from its written AST, or `None` to keep the value.
pub(super) fn reflection_parameter_default_export(
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    default: &Expr,
    needs_source_export: bool,
) -> Option<String> {
    let exporter = DefaultExporter { current_class, current_info };
    if matches!(default.kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)) {
        return if exporter.is_literal(default) {
            exporter.export_value(default)
        } else { exporter.export(default) };
    }
    if matches!(default.kind, ExprKind::ArrayLiteralMixed(_)) {
        return exporter.export(default);
    }
    if needs_source_export {
        if let Some(value) = reflection_literal_parameter_default_value(default) {
            return Some(reflection_dump_default_value(&value));
        }
        return exporter.export(default);
    }
    match &default.kind {
        ExprKind::NewObject { .. } | ExprKind::NewScopedObject { .. } => exporter.export(default),
        ExprKind::ScopedConstantAccess { receiver: StaticReceiver::Named(_), .. } => {
            exporter.export(default)
        }
        ExprKind::ClassConstant { receiver } if exporter.class_name_of(receiver).is_none() => exporter.export(default),
        _ => None,
    }
}

/// The class scope a default is written in, which `self::class` and `parent::class` resolve against.
struct DefaultExporter<'a> {
    current_class: &'a str,
    current_info: Option<&'a crate::types::ClassInfo>,
}

/// One compile-time array key after PHP's key normalization.
#[derive(Clone, PartialEq)]
enum LiteralKey {
    Int(i64),
    /// Canonical literal-byte encoding, independent of how the source escaped the key.
    Str(String),
}

impl DefaultExporter<'_> {
    /// Exports one expression, choosing the folded literal form whenever PHP folds it.
    fn export(&self, expr: &Expr) -> Option<String> {
        if self.is_literal(expr) {
            return self.export_literal(expr);
        }
        match &expr.kind {
            // A dump exports syntax without evaluating it, even if resolving the constant
            // later would fail. Builtin and user references use the same resolved spelling.
            ExprKind::ConstRef(name) => Some(reflection_dump_text(name.as_str().trim_start_matches('\\'))),
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
                        let text = reflection_dump_text(class.as_str().trim_start_matches('\\'));
                        if text.contains('\\') {
                            format!("\\{text}")
                        } else {
                            text.to_string()
                        }
                    }
                    StaticReceiver::Self_ => "self".to_string(),
                    StaticReceiver::Parent => "parent".to_string(),
                    StaticReceiver::Static | StaticReceiver::Generic(_) => return None,
                };
                Some(format!("{receiver}::{}", reflection_dump_text(name)))
            }
            ExprKind::ClassConstant { receiver } => match receiver {
                StaticReceiver::Self_ => Some("self::class".to_string()),
                StaticReceiver::Parent => Some("parent::class".to_string()),
                _ => None,
            },
            ExprKind::NewObject { class_name, args } => {
                let args = args.iter().map(|arg| self.export(arg)).collect::<Option<Vec<_>>>()?;
                Some(format!(
                    "new \\{}({})",
                    reflection_dump_text(class_name.as_str().trim_start_matches('\\')),
                    args.join(", ")
                ))
            }
            ExprKind::NewScopedObject { receiver, args } => {
                let class = match receiver.written_class_receiver() {
                    StaticReceiver::Self_ => "self",
                    StaticReceiver::Parent => "parent",
                    _ => return None,
                };
                let args = args.iter().map(|arg| self.export(arg)).collect::<Option<Vec<_>>>()?;
                Some(format!("new {class}({})", args.join(", ")))
            }
            ExprKind::NamedArg { name, value } => Some(format!("{}: {}", reflection_dump_text(name), self.export(value)?)),
            ExprKind::ArrayLiteral(items) => {
                let items = items.iter().map(|item| self.export(item)).collect::<Option<Vec<_>>>()?;
                Some(format!("[{}]", items.join(", ")))
            }
            ExprKind::ArrayLiteralMixed(entries) => {
                let items = entries.iter().map(|entry| match entry {
                    crate::parser::ast::ArrayEntry::Keyed(key, value)
                        if key.span == value.span && matches!(key.kind, ExprKind::IntLiteral(_)) => self.export(value),
                    crate::parser::ast::ArrayEntry::Keyed(key, value) =>
                        Some(format!("{} => {}", self.export(key)?, self.export(value)?)),
                    crate::parser::ast::ArrayEntry::Value(value) => self.export(value),
                    crate::parser::ast::ArrayEntry::Spread(_) => None,
                }).collect::<Option<Vec<_>>>()?;
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
                .all(|(key, value)| self.literal_key(key).is_some() && self.is_literal(value))
                && self.literal_array_entries(expr).is_some(),
            _ => false,
        }
    }

    /// Prints an evaluated parameter array, compacting lists recursively and escaping value bytes.
    fn export_value(&self, expr: &Expr) -> Option<String> {
        if matches!(expr.kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)) {
            let entries = self.literal_array_entries(expr)?;
            let packed = entries.iter().enumerate().all(|(index, (key, _))|
                *key == LiteralKey::Int(index as i64));
            let rendered = entries.iter().map(|(key, value)| {
                let value = self.export_value(value)?;
                if packed { return Some(value); }
                let key = match key {
                    LiteralKey::Int(key) => key.to_string(),
                    LiteralKey::Str(key) => reflection_dump_default_value(
                        &ReflectionParameterDefaultValue::Str(key.clone())),
                };
                Some(format!("{key} => {value}"))
            }).collect::<Option<Vec<_>>>()?;
            return Some(format!("[{}]", rendered.join(", ")));
        }
        let value = if let ExprKind::ClassConstant { receiver } = &expr.kind {
            ReflectionParameterDefaultValue::Str(reflection_dump_text(&self.class_name_of(receiver)?))
        } else { reflection_literal_parameter_default_value(expr)? };
        Some(reflection_dump_default_value(&value))
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
                Some(export_php_string(&reflection_dump_text(&self.class_name_of(receiver)?)))
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
                // Parser and literal folding already assigned keys for the selected PHP profile.
                pairs.iter().map(|(key, value)| (Some(key), value)).collect()
            }
            _ => return None,
        };
        let mut entries: Vec<(LiteralKey, &Expr)> = Vec::with_capacity(written.len());
        let mut next_index: Option<i64> = None;
        let mut index_overflowed = false;
        for (key, value) in written {
            let key = match key {
                Some(key) => self.literal_key(key)?,
                None if !index_overflowed => LiteralKey::Int(next_index.unwrap_or(0)),
                None => return None,
            };
            if let LiteralKey::Int(index) = key {
                if let Some(following) = index.checked_add(1) {
                    next_index = Some(next_index.map_or(following, |next| next.max(following)));
                } else { index_overflowed = true; }
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
                ExprKind::FloatLiteral(value) => integral_float_key(-value).map(LiteralKey::Int),
                _ => None,
            },
            ExprKind::BoolLiteral(value) => Some(LiteralKey::Int(i64::from(*value))),
            ExprKind::FloatLiteral(value) => integral_float_key(*value).map(LiteralKey::Int),
            ExprKind::StringLiteral(value) => Some(literal_string_key(value)),
            ExprKind::ClassConstant { receiver } => {
                Some(literal_string_key(&reflection_dump_text(&self.class_name_of(receiver)?)))
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

}

/// Normalizes key identity from PHP bytes before comparing different source escape spellings.
fn literal_string_key(value: &str) -> LiteralKey {
    let bytes = elephc_builtin_contract::string_literal::literal_bytes(value);
    if let Some(index) = std::str::from_utf8(&bytes).ok().and_then(canonical_integer_key) {
        return LiteralKey::Int(index);
    }
    let mut canonical = String::with_capacity(value.len());
    for byte in bytes {
        elephc_builtin_contract::string_literal::push_escaped_byte(byte, &mut canonical);
    }
    LiteralKey::Str(canonical)
}

/// Returns a float key only when PHP can fold it losslessly into a signed integer key.
fn integral_float_key(value: f64) -> Option<i64> {
    (value.fract() == 0.0 && value >= i64::MIN as f64 && value < -(i64::MIN as f64))
        .then_some(value as i64)
}

/// Quotes a string as PHP's AST exporter does, switching to double quotes for control bytes.
fn export_php_string(value: &str) -> String {
    let bytes = elephc_builtin_contract::string_literal::literal_bytes(value);
    if bytes.iter().any(|byte| *byte < 0x20) {
        let mut quoted = String::from("\"");
        for byte in bytes {
            match byte {
                b'\n' => quoted.push_str("\\n"), b'\r' => quoted.push_str("\\r"),
                b'\t' => quoted.push_str("\\t"), 0x0b => quoted.push_str("\\v"),
                0x0c => quoted.push_str("\\f"), 0x1b => quoted.push_str("\\e"),
                b'\\' | b'"' | b'$' => { quoted.push('\\'); quoted.push(byte as char); }
                byte if byte < 0x20 => quoted.push_str(&format!("\\{byte:03o}")),
                byte => elephc_builtin_contract::string_literal::push_escaped_byte(byte, &mut quoted),
            }
        }
        quoted.push('"');
        return quoted;
    }
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for byte in bytes {
        if byte == b'\'' || byte == b'\\' {
            quoted.push('\\');
        }
        elephc_builtin_contract::string_literal::push_escaped_byte(byte, &mut quoted);
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
