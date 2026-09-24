//! Purpose:
//! Class-like names, constant evaluation, and empty metadata construction.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Returns the `__construct` member object metadata when the reflected class-like symbol has one.
pub(super) fn reflection_constructor_member(
    method_members: &[ReflectionListedMember],
) -> Option<ReflectionListedMember> {
    method_members
        .iter()
        .find(|member| php_symbol_key(&member.name) == "__construct")
        .cloned()
}

/// Returns the `__construct` member a class inherits from an ancestor that declares it PRIVATE.
///
/// A private method is not inherited, so the descendant's own method list — which is what
/// `getMethods()` and `method_exists()` answer from, and which PHP also reports as empty here —
/// carries no `__construct`. PHP's `getConstructor()` nevertheless returns a `ReflectionMethod`
/// on the DECLARING class, and `isInstantiable()` is `false` because that constructor is not
/// public. Both were wrong without this fallback: `getConstructor()` answered `null` and
/// `isInstantiable()` answered `true` (issue #869).
///
/// Built from the OWNER's `ClassInfo`, so the member's declaring class, visibility flags,
/// parameters and source lines are the ancestor's, exactly as PHP reports them. Returns `None`
/// for a class with no constructor anywhere in its chain, and for one whose own list already
/// carried the entry — that caller checks first.
pub(super) fn inherited_private_constructor_member(
    ctx: &FunctionContext<'_>,
    class_name: &str,
) -> Result<Option<ReflectionListedMember>> {
    let Some((owner_name, owner_info)) =
        crate::types::constructor_owner(&ctx.module.class_infos, class_name)
    else {
        return Ok(None);
    };
    if php_symbol_key(owner_name) == php_symbol_key(class_name) {
        return Ok(None);
    }
    // The failure is PROPAGATED, not swallowed into `None`. Building a member resolves the
    // owner's prototype and parameter defaults, and either can fail; answering "no constructor"
    // instead would turn a real metadata error into a silently wrong `isInstantiable() == true`,
    // where the ordinary method-member path reports it. `?` here matches that path.
    reflection_class_method_member(ctx, owner_name, owner_info, "__construct")
}

/// Builds common ReflectionMethod/ReflectionProperty predicate flags.
pub(super) fn reflection_member_flags(
    is_static: bool,
    visibility: &Visibility,
    is_final: bool,
    is_abstract: bool,
    is_readonly: bool,
    is_promoted: bool,
) -> ReflectionMemberFlags {
    ReflectionMemberFlags {
        is_static,
        is_public: visibility == &Visibility::Public,
        is_protected: visibility == &Visibility::Protected,
        is_private: visibility == &Visibility::Private,
        is_final,
        is_abstract,
        is_readonly,
        is_promoted,
        is_virtual: false,
    }
}

/// Returns PHP case-insensitive method names declared by an interface and its parents.
pub(super) fn reflection_interface_method_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Vec<String> {
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return Vec::new();
    };
    let Some(info) = ctx.module.interface_infos.get(interface_name) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    push_unique_method_names(info.methods.keys(), &mut names, &mut seen);
    push_unique_method_names(info.static_methods.keys(), &mut names, &mut seen);
    names
}

/// Returns PHP case-sensitive property names declared by an interface and its parents.
pub(super) fn reflection_interface_property_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Vec<String> {
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return Vec::new();
    };
    let Some(info) = ctx.module.interface_infos.get(interface_name) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for property in info.properties.keys() {
        push_unique_property_name(property, &mut names, &mut seen);
    }
    names
}

/// Returns PHP case-sensitive constant names declared by an interface and its parents.
pub(super) fn reflection_interface_constant_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Vec<String> {
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return Vec::new();
    };
    let Some(info) = ctx.module.interface_infos.get(interface_name) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for constant in info.constants.keys() {
        push_unique_constant_name(constant, &mut names, &mut seen);
    }
    names
}

/// Returns PHP case-insensitive direct method names declared by a trait.
pub(super) fn reflection_trait_method_names(ctx: &FunctionContext<'_>, trait_name: &str) -> Vec<String> {
    ctx.module
        .declared_trait_method_names
        .get(trait_name)
        .cloned()
        .unwrap_or_default()
}

/// Returns PHP case-sensitive direct property names declared by a trait.
pub(super) fn reflection_trait_property_names(ctx: &FunctionContext<'_>, trait_name: &str) -> Vec<String> {
    ctx.module
        .declared_trait_property_names
        .get(trait_name)
        .cloned()
        .unwrap_or_default()
}

/// Returns PHP case-sensitive direct constant names declared by a trait.
pub(super) fn reflection_trait_constant_names(ctx: &FunctionContext<'_>, trait_name: &str) -> Vec<String> {
    ctx.module
        .declared_trait_constant_names
        .get(trait_name)
        .cloned()
        .unwrap_or_default()
}

/// Appends lower-case method names while preserving first-seen order.
pub(super) fn push_unique_method_names<'a>(
    method_names: impl Iterator<Item = &'a String>,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    for method_name in method_names {
        let key = php_symbol_key(method_name);
        if seen.insert(key.clone()) {
            names.push(key);
        }
    }
}

/// Appends one case-sensitive property name while preserving first-seen order.
pub(super) fn push_unique_property_name(
    property_name: &str,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(property_name.to_string()) {
        names.push(property_name.to_string());
    }
}

/// Appends one case-sensitive class constant name while preserving first-seen order.
pub(super) fn push_unique_constant_name(
    constant_name: &str,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(constant_name.to_string()) {
        names.push(constant_name.to_string());
    }
}

/// Appends one constant metadata member while preserving first-seen order.
pub(super) fn push_unique_constant_member(
    constant_name: &str,
    value: ReflectionConstantValue,
    members: &mut Vec<ReflectionConstantMember>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(constant_name.to_string()) {
        members.push(ReflectionConstantMember {
            name: constant_name.to_string(),
            value,
        });
    }
}

/// Evaluates one class/interface/trait constant expression for static Reflection metadata.
pub(super) fn reflection_constant_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    expr: &Expr,
    depth: usize,
) -> Result<ReflectionConstantValue> {
    if depth > 16 {
        return Err(CodegenIrError::unsupported(
            "deep recursive ReflectionClass constant metadata",
        ));
    }
    match &expr.kind {
        ExprKind::IntLiteral(value) => Ok(ReflectionConstantValue::Int(*value)),
        ExprKind::BoolLiteral(value) => Ok(ReflectionConstantValue::Bool(*value)),
        ExprKind::FloatLiteral(value) => Ok(ReflectionConstantValue::Float(*value)),
        ExprKind::StringLiteral(value) => Ok(ReflectionConstantValue::Str(value.clone())),
        ExprKind::Null => Ok(ReflectionConstantValue::Null),
        ExprKind::Negate(inner) => {
            match reflection_constant_value(ctx, current_class, current_info, inner, depth + 1)? {
                ReflectionConstantValue::Int(value) => Ok(ReflectionConstantValue::Int(-value)),
                ReflectionConstantValue::Float(value) => Ok(ReflectionConstantValue::Float(-value)),
                other => Err(unsupported_reflection_constant_value(other)),
            }
        }
        ExprKind::BinaryOp { left, op, right } => reflection_binary_constant_value(
            ctx,
            current_class,
            current_info,
            left,
            op,
            right,
            depth + 1,
        ),
        ExprKind::ClassConstant { receiver } => {
            let class_name =
                reflection_static_receiver_name(current_class, current_info, receiver)?;
            Ok(ReflectionConstantValue::Str(class_name))
        }
        ExprKind::ScopedConstantAccess { receiver, name } => reflection_scoped_constant_value(
            ctx,
            current_class,
            current_info,
            receiver,
            name,
            depth + 1,
        ),
        ExprKind::ConstRef(name) => {
            reflection_global_constant_value(ctx, name, expr.span, depth)
        }
        ExprKind::ArrayLiteral(elements) => {
            // A bare list may carry spread elements, which the parser leaves as `...source`
            // expressions; normalize to the entry list and share the entry fold.
            let entries: Vec<crate::parser::ast::ArrayEntry> = elements
                .iter()
                .map(|element| match &element.kind {
                    ExprKind::Spread(_) => {
                        crate::parser::ast::ArrayEntry::Spread(element.clone())
                    }
                    _ => crate::parser::ast::ArrayEntry::Value(element.clone()),
                })
                .collect();
            reflection_constant_array_entries_fold(
                ctx,
                current_class,
                current_info,
                &entries,
                depth,
            )
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            let entries: Vec<crate::parser::ast::ArrayEntry> = pairs
                .iter()
                .map(|(key, value)| {
                    crate::parser::ast::ArrayEntry::Keyed(key.clone(), value.clone())
                })
                .collect();
            reflection_constant_array_entries_fold(
                ctx,
                current_class,
                current_info,
                &entries,
                depth,
            )
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            reflection_constant_array_entries_fold(
                ctx,
                current_class,
                current_info,
                entries,
                depth,
            )
        }
        other => Err(CodegenIrError::unsupported(format!(
            "ReflectionClass constant metadata expression {:?}",
            other
        ))),
    }
}

/// Folds one constant array literal's entries in PHP evaluation order into a list or hash
/// constant value.
///
/// Bare elements claim the next free integer slot, explicit keys normalize with PHP's key
/// rules, and a spread's integer keys renumber into a contiguous block starting after the
/// destination's highest integer key (0 when it has none) while its string keys are kept in
/// place. Duplicate keys collapse the way PHP's hash does (first position, last value wins).
/// The result is a packed `Array` only when the final keys are exactly `0..n-1` in order.
fn reflection_constant_array_entries_fold(
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
                let key = next_reflection_constant_int_key(max_int)?;
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
                                start = Some(next_reflection_constant_int_key(max_int)?);
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

/// Returns the next free integer slot — `max + 1` for some highest key, `0` when the
/// destination holds no integer key yet — or a compile error on overflow.
fn next_reflection_constant_int_key(max_int: Option<i64>) -> Result<i64> {
    match max_int {
        None => Ok(0),
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
            ) => existing == candidate,
            _ => false,
        }
    });
    if let Some(index) = duplicate {
        values[index].value = value;
    } else {
        values.push(ReflectionConstantAssocEntry { key, value });
    }
}

/// Evaluates one supported binary operator in a static Reflection constant expression.
pub(super) fn reflection_binary_constant_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    depth: usize,
) -> Result<ReflectionConstantValue> {
    let left = reflection_constant_value(ctx, current_class, current_info, left, depth)?;
    let right = reflection_constant_value(ctx, current_class, current_info, right, depth)?;
    match (&left, op, &right) {
        (
            ReflectionConstantValue::Int(left),
            BinOp::Add,
            ReflectionConstantValue::Int(right),
        ) => {
            Ok(ReflectionConstantValue::Int(*left + *right))
        }
        (
            ReflectionConstantValue::Int(left),
            BinOp::Sub,
            ReflectionConstantValue::Int(right),
        ) => {
            Ok(ReflectionConstantValue::Int(*left - *right))
        }
        (
            ReflectionConstantValue::Int(left),
            BinOp::Mul,
            ReflectionConstantValue::Int(right),
        ) => {
            Ok(ReflectionConstantValue::Int(*left * *right))
        }
        (
            ReflectionConstantValue::Int(left),
            BinOp::Mod,
            ReflectionConstantValue::Int(right),
        ) if *right != 0 => {
            Ok(ReflectionConstantValue::Int(*left % *right))
        }
        (
            ReflectionConstantValue::Int(left),
            BinOp::Pow,
            ReflectionConstantValue::Int(right),
        ) if *right >= 0 =>
        {
            Ok(ReflectionConstantValue::Int((*left).pow(*right as u32)))
        }
        (
            ReflectionConstantValue::Str(left),
            BinOp::Concat,
            ReflectionConstantValue::Str(right),
        ) => Ok(ReflectionConstantValue::Str(format!("{}{}", left, right))),
        (
            left,
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow,
            right,
        ) => reflection_float_binary_constant_value(left, op, right).ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "ReflectionClass constant metadata binary value {:?} {:?}",
                reflection_constant_value_kind(left),
                reflection_constant_value_kind(right)
            ))
        }),
        (left, _, right) => Err(CodegenIrError::unsupported(format!(
            "ReflectionClass constant metadata binary value {:?} {:?}",
            reflection_constant_value_kind(left),
            reflection_constant_value_kind(right)
        ))),
    }
}

/// Evaluates a numeric binary operator that must produce a float Reflection value.
pub(super) fn reflection_float_binary_constant_value(
    left: &ReflectionConstantValue,
    op: &BinOp,
    right: &ReflectionConstantValue,
) -> Option<ReflectionConstantValue> {
    let left = reflection_constant_value_as_float(left)?;
    let right = reflection_constant_value_as_float(right)?;
    let value = match op {
        BinOp::Add => left + right,
        BinOp::Sub => left - right,
        BinOp::Mul => left * right,
        BinOp::Div if right != 0.0 => left / right,
        BinOp::Pow => left.powf(right),
        _ => return None,
    };
    Some(ReflectionConstantValue::Float(value))
}

/// Returns the float representation of numeric Reflection constant metadata.
pub(super) fn reflection_constant_value_as_float(value: &ReflectionConstantValue) -> Option<f64> {
    match value {
        ReflectionConstantValue::Int(value) => Some(*value as f64),
        ReflectionConstantValue::Float(value) => Some(*value),
        _ => None,
    }
}

/// Resolves and evaluates one scoped class/interface/trait constant value.
pub(super) fn reflection_scoped_constant_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    receiver: &StaticReceiver,
    constant_name: &str,
    depth: usize,
) -> Result<ReflectionConstantValue> {
    let class_name = reflection_static_receiver_name(current_class, current_info, receiver)?;
    if let Some((resolved_name, info)) = resolve_reflection_class(ctx, &class_name) {
        if let Some(value_expr) = info.constants.get(constant_name) {
            return reflection_constant_value(ctx, resolved_name, Some(info), value_expr, depth);
        }
        for interface_name in &info.interfaces {
            if let Some(value_expr) =
                reflection_interface_constant_expr(ctx, interface_name, constant_name)
            {
                return reflection_constant_value(ctx, interface_name, None, &value_expr, depth);
            }
        }
    }
    if let Some(interface_name) = resolve_reflection_interface(ctx, &class_name) {
        if let Some(value_expr) =
            reflection_interface_constant_expr(ctx, interface_name, constant_name)
        {
            return reflection_constant_value(ctx, interface_name, None, &value_expr, depth);
        }
    }
    if let Some(trait_name) = resolve_reflection_trait(ctx, &class_name) {
        if let Some(value_expr) = ctx
            .module
            .declared_trait_constants
            .get(trait_name)
            .and_then(|constants| constants.get(constant_name))
        {
            return reflection_constant_value(ctx, trait_name, None, value_expr, depth);
        }
    }
    if ctx
        .module
        .enum_infos
        .get(&class_name)
        .is_some_and(|info| info.cases.iter().any(|case| case.name == constant_name))
    {
        return Ok(ReflectionConstantValue::EnumCase {
            enum_name: class_name,
            case_name: constant_name.to_string(),
        });
    }
    Err(CodegenIrError::unsupported(format!(
        "ReflectionClass constant metadata for {}::{}",
        current_class, constant_name
    )))
}

/// Resolves and evaluates one global constant reference for static Reflection metadata.
///
/// The stored table carries no span of its own, so the synthesized expression takes
/// `span` — the reference site. Nested expressions inside the stored kind keep their
/// definition-site spans; only failures attributed to the top-level expression point at
/// the reference.
pub(super) fn reflection_global_constant_value(
    ctx: &FunctionContext<'_>,
    name: &crate::names::Name,
    span: crate::span::Span,
    depth: usize,
) -> Result<ReflectionConstantValue> {
    let expr_kind = ctx
        .module
        .global_constants
        .get(name.as_str())
        .or_else(|| ctx.module.global_constants.get(name.as_str().trim_start_matches('\\')))
        .map(|(expr_kind, _)| expr_kind.clone())
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "ReflectionClass constant metadata for global constant {}",
                name.as_str().trim_start_matches('\\')
            ))
        })?;
    let expr = crate::parser::ast::Expr::new(expr_kind, span);
    reflection_constant_value(ctx, "", None, &expr, depth + 1)
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
            // PHP 8.5 casts NAN and the infinities to 0; Rust `as` saturates infinities
            // to ±i64::MAX, so only finite floats take the ordinary truncating cast.
            let key = if value.is_finite() { *value as i64 } else { 0 };
            Some(ReflectionDefaultArrayKey::Int(key))
        }
        ReflectionConstantValue::Str(value) => reflection_default_string_array_key(value),
        ReflectionConstantValue::Null => Some(ReflectionDefaultArrayKey::Str(String::new())),
        _ => None,
    }
}

/// Returns an interface constant expression, including inherited parent interfaces.
pub(super) fn reflection_interface_constant_expr(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    constant_name: &str,
) -> Option<Expr> {
    let mut visited = std::collections::HashSet::new();
    let mut queue = vec![interface_name.to_string()];
    while let Some(name) = queue.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        if let Some(info) = ctx.module.interface_infos.get(&name) {
            if let Some(value) = info.constants.get(constant_name) {
                return Some(value.clone());
            }
            queue.extend(info.parents.iter().cloned());
        }
    }
    None
}

/// Resolves a static receiver against the current reflected declaration.
pub(super) fn reflection_static_receiver_name(
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    receiver: &StaticReceiver,
) -> Result<String> {
    match receiver {
        StaticReceiver::Named(name) => Ok(name.as_str().trim_start_matches('\\').to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static => Ok(current_class.to_string()),
        StaticReceiver::Parent => current_info
            .and_then(|info| info.parent.clone())
            .ok_or_else(|| {
                CodegenIrError::unsupported(format!(
                    "ReflectionClass constant metadata parent receiver in {}",
                    current_class
                ))
            }),
    }
}

/// Returns a small label for unsupported constant-value diagnostics.
pub(super) fn reflection_constant_value_kind(value: &ReflectionConstantValue) -> &'static str {
    match value {
        ReflectionConstantValue::Int(_) => "int",
        ReflectionConstantValue::Bool(_) => "bool",
        ReflectionConstantValue::Float(_) => "float",
        ReflectionConstantValue::Str(_) => "string",
        ReflectionConstantValue::Null => "null",
        ReflectionConstantValue::EnumCase { .. } => "enum-case",
        ReflectionConstantValue::Array(_) => "array",
        ReflectionConstantValue::AssocArray(_) => "assoc-array",
    }
}

/// Reports an unsupported unary constant value while avoiding large debug output.
pub(super) fn unsupported_reflection_constant_value(value: ReflectionConstantValue) -> CodegenIrError {
    CodegenIrError::unsupported(format!(
        "ReflectionClass constant metadata unary value {}",
        reflection_constant_value_kind(&value)
    ))
}

/// Looks up class-constant metadata by PHP-style class name and case-sensitive constant name.
pub(super) fn resolve_reflection_class_constant<'a>(
    ctx: &'a FunctionContext<'_>,
    class_name: &str,
    constant_name: &str,
) -> Option<(&'a str, &'a crate::types::ClassInfo)> {
    let (resolved_name, info) = resolve_reflection_class(ctx, class_name)?;
    if info.constants.contains_key(constant_name) {
        return Some((resolved_name, info));
    }
    let parent = info.parent.as_deref()?;
    resolve_reflection_class_constant(ctx, parent, constant_name)
}

/// Looks up enum-case metadata by PHP-style enum name and case-sensitive case name.
pub(super) fn resolve_reflection_enum_case<'a>(
    ctx: &'a FunctionContext<'_>,
    enum_name: &str,
    case_name: &str,
) -> Option<(&'a str, &'a crate::types::EnumCaseInfo)> {
    let enum_key = php_symbol_key(enum_name.trim_start_matches('\\'));
    ctx.module
        .enum_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == enum_key)
        .and_then(|(name, info)| {
            info.cases
                .iter()
                .find(|case| case.name == case_name)
                .map(|case| (name.as_str(), case))
        })
}

/// Returns a static Reflection value for a backed enum case, when present.
pub(super) fn reflection_enum_case_backing_value(case: &EnumCaseInfo) -> Option<ReflectionConstantValue> {
    match case.value.as_ref()? {
        EnumCaseValue::Int(value) => Some(ReflectionConstantValue::Int(*value)),
        EnumCaseValue::Str(value) => Some(ReflectionConstantValue::Str(value.clone())),
    }
}

