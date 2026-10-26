//! Purpose:
//! Evaluates array-valued Reflection constants with PHP key and spread semantics.
//!
//! Called from:
//! - `super::names_constants::reflection_constant_value()`.
//!
//! Key details:
//! - Duplicate keys keep their first position and last value; integer spreads are renumbered.
//! - Recursive values share the constant evaluator's depth bound and target-neutral metadata.

use super::*;

/// Folds one constant array literal's entries in PHP evaluation order into a list or hash
/// constant value.
///
/// Bare elements claim the next free integer slot, explicit keys normalize with PHP's key
/// rules, and a spread's integer keys renumber into a contiguous block starting after the
/// destination's highest integer key (0 when it has none) while its string keys are kept in
/// place. Duplicate keys collapse the way PHP's hash does (first position, last value wins).
/// The result is a packed `Array` only when the final keys are exactly `0..n-1` in order.
pub(super) fn reflection_constant_array_entries_fold(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    entries: &[crate::parser::ast::ArrayEntry],
    depth: usize,
) -> Result<ReflectionConstantValue> {
    let mut values: Vec<ReflectionConstantAssocEntry> = Vec::with_capacity(entries.len());
    // The highest integer key seen so far; `None` means no integer key yet, so the next
    // free slot is 0. PHP advances from the maximum (even a negative one), not from the
    // lowest free slot.
    let mut max_int: Option<i64> = None;
    for entry in entries {
        match entry {
            crate::parser::ast::ArrayEntry::Value(expr) => {
                let value = reflection_constant_value(
                    ctx,
                    current_class,
                    current_info,
                    expr,
                    depth + 1,
                )?;
                let key = next_reflection_constant_int_key(
                    max_int,
                    crate::codegen::compile_php_version(),
                )?;
                max_int = Some(key);
                values.push(ReflectionConstantAssocEntry {
                    key: ReflectionDefaultArrayKey::Int(key),
                    value,
                });
            }
            crate::parser::ast::ArrayEntry::Keyed(key_expr, value_expr) => {
                let key = reflection_constant_array_key_expr(
                    ctx,
                    current_class,
                    current_info,
                    key_expr,
                    depth + 1,
                )?;
                let value = reflection_constant_value(
                    ctx,
                    current_class,
                    current_info,
                    value_expr,
                    depth + 1,
                )?;
                if let ReflectionDefaultArrayKey::Int(candidate) = &key {
                    let candidate = *candidate;
                    max_int = Some(std::cmp::max(max_int.unwrap_or(candidate), candidate));
                }
                reflection_constant_insert_entry(&mut values, key, value);
            }
            crate::parser::ast::ArrayEntry::Spread(entry_expr) => {
                // The parser stores the whole `...source` expression, so the source
                // node itself is one level down.
                let source = if let ExprKind::Spread(inner) = &entry_expr.kind {
                    &**inner
                } else {
                    entry_expr
                };
                let source_value = reflection_constant_value(
                    ctx,
                    current_class,
                    current_info,
                    source,
                    depth + 1,
                )?;
                let source_entries = match source_value {
                    ReflectionConstantValue::Array(items) => items
                        .into_iter()
                        .enumerate()
                        .map(|(index, value)| {
                            (ReflectionDefaultArrayKey::Int(index as i64), value)
                        })
                        .collect::<Vec<_>>(),
                    ReflectionConstantValue::AssocArray(source_entries) => source_entries
                        .into_iter()
                        .map(|entry| (entry.key, entry.value))
                        .collect(),
                    other => {
                        return Err(CodegenIrError::unsupported(format!(
                            "ReflectionClass constant metadata spread of a non-array constant ({})",
                            reflection_constant_value_kind(&other)
                        )))
                    }
                };
                // PHP renumbers spread integer keys into a contiguous block starting
                // after the destination's highest integer key (0 when it has none);
                // string keys are kept in place. The start is computed lazily so a spread
                // whose keys need no slot (all strings, or empty) cannot trigger a spurious
                // overflow when the destination's highest key is i64::MAX.
                let mut start: Option<i64> = None;
                let mut slot: i64 = 0;
                for (key, value) in source_entries {
                    let key = match key {
                        ReflectionDefaultArrayKey::Int(_) => {
                            if start.is_none() {
                                start = Some(next_reflection_constant_int_key(
                                    max_int,
                                    crate::codegen::compile_php_version(),
                                )?);
                            }
                            let key = start
                                .unwrap()
                                .checked_add(slot)
                                .ok_or_else(|| {
                                    CodegenIrError::unsupported(
                                        "ReflectionClass constant metadata integer key overflow",
                                    )
                                })?;
                            slot += 1;
                            max_int = Some(std::cmp::max(max_int.unwrap_or(key), key));
                            ReflectionDefaultArrayKey::Int(key)
                        }
                        ReflectionDefaultArrayKey::Str(name) => {
                            ReflectionDefaultArrayKey::Str(name)
                        }
                    };
                    reflection_constant_insert_entry(&mut values, key, value);
                }
            }
        }
    }
    // PHP treats an array whose keys are exactly 0..n-1 in order as a list; the packed
    // `Array` shape keeps the metadata consistent with the parser's `ArrayLiteral`.
    let is_list = values
        .iter()
        .enumerate()
        .all(|(index, entry)| {
            matches!(&entry.key, ReflectionDefaultArrayKey::Int(key) if *key == index as i64)
        });
    if is_list {
        Ok(ReflectionConstantValue::Array(
            values
                .into_iter()
                .map(|entry| entry.value)
                .collect(),
        ))
    } else {
        Ok(ReflectionConstantValue::AssocArray(values))
    }
}

/// Returns the next free integer slot, `max + 1` for some highest key, `0` when the
/// destination holds no integer key yet, or a compile error on overflow.
fn next_reflection_constant_int_key(
    max_int: Option<i64>,
    php_version: crate::php_version::PhpVersion,
) -> Result<i64> {
    match max_int {
        None => Ok(0),
        Some(max_int) if max_int < 0 && php_version < crate::php_version::PhpVersion::Php83 => {
            Ok(0)
        }
        Some(max_int) => max_int
            .checked_add(1)
            .ok_or_else(|| {
                CodegenIrError::unsupported(
                    "ReflectionClass constant metadata integer key overflow",
                )
            }),
    }
}

/// Inserts one folded constant-array entry, collapsing duplicate keys the way PHP's hash
/// does (first insertion position, last value wins).
fn reflection_constant_insert_entry(
    values: &mut Vec<ReflectionConstantAssocEntry>,
    key: ReflectionDefaultArrayKey,
    value: ReflectionConstantValue,
) {
    let duplicate = values.iter().position(|existing| {
        match (&existing.key, &key) {
            (
                ReflectionDefaultArrayKey::Int(existing),
                ReflectionDefaultArrayKey::Int(candidate),
            ) => existing == candidate,
            (
                ReflectionDefaultArrayKey::Str(existing),
                ReflectionDefaultArrayKey::Str(candidate),
            ) => elephc_builtin_contract::string_literal::literal_bytes(existing)
                == elephc_builtin_contract::string_literal::literal_bytes(candidate),
            _ => false,
        }
    });
    if let Some(index) = duplicate {
        values[index].value = value;
    } else {
        values.push(ReflectionConstantAssocEntry { key, value });
    }
}

/// Normalizes one evaluated constant expression into a PHP array key form.
pub(super) fn reflection_constant_array_key_expr(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    key: &Expr,
    depth: usize,
) -> Result<ReflectionDefaultArrayKey> {
    if let Some(key) = reflection_default_array_key(key) {
        return Ok(key);
    }
    let value = reflection_constant_value(ctx, current_class, current_info, key, depth)?;
    reflection_constant_array_key(&value).ok_or_else(|| {
        CodegenIrError::unsupported(
            "ReflectionClass constant metadata array key that is not a scalar",
        )
    })
}

/// Returns the PHP array key form of one evaluated constant value.
pub(super) fn reflection_constant_array_key(
    value: &ReflectionConstantValue,
) -> Option<ReflectionDefaultArrayKey> {
    match value {
        ReflectionConstantValue::Int(value) => Some(ReflectionDefaultArrayKey::Int(*value)),
        ReflectionConstantValue::Bool(value) => {
            Some(ReflectionDefaultArrayKey::Int(i64::from(*value)))
        }
        ReflectionConstantValue::Float(value) => {
            let key = crate::types::php_float_array_key(*value);
            Some(ReflectionDefaultArrayKey::Int(key))
        }
        ReflectionConstantValue::Str(value) => reflection_default_string_array_key(value),
        ReflectionConstantValue::Null => Some(ReflectionDefaultArrayKey::Str(String::new())),
        _ => None,
    }
}
