//! Purpose:
//! Type-checks assignment arrays forms.
//! Updates type environments and validates storage-specific rules for locals, arrays, and properties.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::assignments`
//!
//! Key details:
//! - Assignment checking must distinguish value writes, by-reference mutation, nullable access, and declared property contracts.

use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind, NESTED_APPEND_TEMP_PREFIX};
use crate::span::Span;
use crate::types::{
    array_key_contiguity_is_unproven, merge_array_key_types, normalized_array_key_type,
    static_array_key_forces_hash_storage, PhpType, TypeEnv,
};

use super::super::super::Checker;

/// Validates and updates the type environment for `$array[$index] = $value` assignments.
///
/// Checks direct string offset writes and merges element types for arrays/assoc-arrays,
/// checks buffer index type and element type compatibility, and requires ArrayAccess for objects.
/// Updates `env` with the merged key/value types; returns an error for invalid targets or type mismatches.
///
/// Errors:
/// - Undefined variable
/// - Invalid direct string offset or unsupported update operator
/// - Buffer element type mismatch
/// - Object assignment without ArrayAccess
pub(super) fn check_array_assign(
    checker: &mut Checker,
    array: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    // PHP auto-vivifies an undefined variable when its first use is an offset write. There is
    // no undefined read to diagnose on this path; starting from an empty indexed array lets the
    // ordinary write path below decide between packed and associative storage from the key.
    let arr_ty = match env.get(array).cloned() {
        Some(ty) => ty,
        None => {
            let empty = PhpType::Array(Box::new(PhpType::Never));
            env.insert(array.to_string(), empty.clone());
            empty
        }
    };
    let idx_ty = checker.infer_type_with_assignment_effects(index, env)?;
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    super::locals::update_callable_assignment_metadata(checker, array, value, &val_ty, env)?;
    if arr_ty == PhpType::Str {
        return check_string_offset_assign(array, index, &idx_ty, value, span);
    }
    if let PhpType::Array(elem_ty) = &arr_ty {
        let normalized_idx_ty = normalized_array_key_type(index, idx_ty.clone());
        // A foreach loop key is a boxed `Mixed` cell at runtime (`Op::IterCurrentKey`)
        // even when the checker types it as `Int`/`Str` from the source array, so it
        // may hold either an integer or a string and the destination must stay indexed
        // `Array(Mixed)` with the indexed-vs-hash decision deferred to the runtime
        // write helper (`Op::ArraySetMixedKey`). A non-foreach string-typed key (a
        // literal string, or a string-valued expression like `"k" . $i` or a plain
        // string variable) always means associative hash storage in PHP, so it
        // promotes to `AssocArray` and stays usable by direct string-key reads. A
        // non-foreach `Mixed`-typed key (e.g. a `mixed` parameter) is likewise a
        // runtime-tagged cell, so it stays `Array(Mixed)` to match the lowering's
        // `ArraySetMixedKey` routing.
        let index_is_foreach_key = matches!(&index.kind, ExprKind::Variable(name) if checker.is_foreach_key(name));
        // An INTEGER key into a still-empty array decides that array's storage, because packed
        // storage has no keys: slot `n` is key `n`, and a write past the end zero-fills the gap.
        // php instead promotes to a hash the moment an integer key skips, so `$rows[$id] = …`
        // over ids 101/102/205 counts 3 there and counted 206 here. A literal key is decided
        // exactly; anything this pass cannot bound against the length takes hash storage, where
        // a gap costs nothing. A foreach key and a `Mixed`-typed key are excluded: both are
        // runtime-tagged cells that stay on `Array(Mixed)` for `Op::ArraySetMixedKey` to
        // tag-dispatch, and pinning them to an `Int` hash key here would be a lie. The counter of
        // the `for` loop this write runs directly in IS bounded — it starts at 0 and advances one
        // slot per iteration — so `for ($i = 0; …; $i++) { $a[$i] = … }` keeps packed storage.
        let forces_hash = matches!(normalized_idx_ty, PhpType::Str)
            || (matches!(idx_ty, PhpType::Str) && !index_is_foreach_key)
            || (matches!(elem_ty.as_ref(), PhpType::Never)
                && (static_array_key_forces_hash_storage(index)
                    || (matches!(normalized_idx_ty, PhpType::Int)
                        && !index_is_foreach_key
                        && !checker.index_is_packed_loop_counter(array, index)
                        && array_key_contiguity_is_unproven(index))));
        if forces_hash {
            let merged_key = if matches!(elem_ty.as_ref(), PhpType::Never) {
                normalized_idx_ty
            } else {
                merge_array_key_types(PhpType::Int, normalized_idx_ty)
            };
            let merged_value = if matches!(elem_ty.as_ref(), PhpType::Never) {
                val_ty
            } else if elem_ty.as_ref() == &val_ty {
                *elem_ty.clone()
            } else {
                checker
                    .merge_array_element_type(elem_ty, &val_ty)
                    .unwrap_or(PhpType::Mixed)
            };
            env.insert(
                array.to_string(),
                PhpType::AssocArray {
                    key: Box::new(merged_key),
                    value: Box::new(merged_value),
                },
            );
        } else if index_is_foreach_key || matches!(idx_ty, PhpType::Mixed) {
            env.insert(
                array.to_string(),
                PhpType::Array(Box::new(PhpType::Mixed)),
            );
        } else if **elem_ty != val_ty {
            let merged_ty = checker
                .merge_array_element_type(elem_ty, &val_ty)
                .unwrap_or(PhpType::Mixed);
            // A DECLARED element type (`array<int> $a = …`) is a contract, exactly like a
            // declared scalar local: widening it here would silently rewrite the array's
            // storage (`Op::ArrayToMixed`) and box every slot the declaration asked to keep
            // raw. An INFERRED element type has no such contract and still widens.
            //
            // The guard is the MERGE RESULT, not `elem_ty != val_ty`: that inequality is also
            // true when the merge changes nothing (a declared `array` is `Array(Mixed)`, and
            // `Mixed != Str` while `merge(Mixed, Str)` is still `Mixed`). Keying on the merge
            // is what keeps a bare `array` declaration writable.
            if merged_ty != **elem_ty {
                reject_declared_element_widening(checker, array, &arr_ty, &val_ty, span)?;
            }
            env.insert(array.to_string(), PhpType::Array(Box::new(merged_ty)));
        }
    } else if let PhpType::AssocArray {
        key,
        value: existing_value,
    } = &arr_ty
    {
        let merged_key = merge_array_key_types(
            *key.clone(),
            normalized_array_key_type(index, idx_ty),
        );
        let merged_value = if **existing_value == val_ty {
            *existing_value.clone()
        } else {
            // Pointers and `null` share the pointer word (`?ptr<T>` values).
            crate::types::merge_pointer_elements(existing_value, &val_ty).unwrap_or(PhpType::Mixed)
        };
        // Same contract as the indexed form, and guarded the same way — on the MERGE RESULT,
        // so a declared `array<string, mixed>` stays writable with anything while
        // `array<string, int>` rejects a string value instead of silently widening to
        // `array<string, mixed>` and boxing every slot.
        if merged_value != **existing_value {
            reject_declared_element_widening(checker, array, &arr_ty, &val_ty, span)?;
        }
        env.insert(
            array.to_string(),
            PhpType::AssocArray {
                key: Box::new(merged_key),
                value: Box::new(merged_value),
            },
        );
    } else if let PhpType::Buffer(elem_ty) = &arr_ty {
        validate_buffer_element_write(elem_ty, &idx_ty, &val_ty, span)?;
    } else if let PhpType::Object(class_name) = &arr_ty {
        if !checker.object_type_implements_interface(class_name, "ArrayAccess") {
            return Err(CompileError::new(
                span,
                "Object array assignment requires ArrayAccess",
            ));
        }
    }
    Ok(())
}

/// Diagnostic for a string offset write whose string lives in a property.
pub(super) const STRING_OFFSET_ON_PROPERTY_UNSUPPORTED: &str =
    "String offset assignment on a property is not supported; copy the property into a local \
     variable, assign the offset there, and store the local back";

/// Validates a string offset write `$s[$i] = $v` on a local typed `string`.
///
/// The offset must be one a string read also accepts (integer, float, `mixed`, or a numeric
/// string literal). PHP refuses a compound operator on a string offset
/// (`$s[0] .= "x"`) with a runtime `Error`; that shape always fails, so it is refused here
/// with PHP's wording. The local keeps its `string` type.
fn check_string_offset_assign(
    array: &str,
    index: &Expr,
    idx_ty: &PhpType,
    value: &Expr,
    span: Span,
) -> Result<(), CompileError> {
    if let Some(message) = string_offset_read_modify_write_error(array, index, value, span) {
        return Err(CompileError::new(span, message));
    }
    if !crate::types::checker::inference::is_valid_string_offset_index(index, idx_ty) {
        return Err(CompileError::new(span, "String index must be integer"));
    }
    Ok(())
}

/// Returns PHP's error for a desugared read-modify-write of a string offset, if `value` is one.
///
/// The parser rewrites `$s[$i] <op>= $v` and the statement form of `$s[$i]++` / `--$s[$i]`
/// into `$s[$i] = $s[$i] <op> <rhs>`, giving the synthesized value the statement's own span;
/// that separates it from a user-written `$s[0] = $s[0] . "x"`, which PHP allows. An
/// increment's synthesized `1` also carries the statement span, whereas the `1` of a written
/// `$s[0] += 1` carries its own. `??=` desugars to a null-coalesce and stays allowed, as in PHP.
fn string_offset_read_modify_write_error(
    array: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) -> Option<&'static str> {
    if value.span != span {
        return None;
    }
    if let ExprKind::Assignment { value: step, prelude, .. } = &value.kind {
        if prelude.iter().any(|stmt| matches!(&stmt.kind,
            crate::parser::ast::StmtKind::Assign { value: read, .. }
                if expr_is_string_offset_target(read, array, index)))
        {
            return Some("Cannot increment/decrement string offsets");
        }
        if prelude.is_empty() {
            return string_offset_read_modify_write_error(array, index, step, span);
        }
    }
    let ExprKind::BinaryOp { left, op, right } = &value.kind else {
        return None;
    };
    if !expr_is_string_offset_target(left, array, index) {
        return None;
    }
    let synthesized_step = right.span == span && matches!(right.kind, ExprKind::IntLiteral(1));
    if synthesized_step && matches!(op, BinOp::Add | BinOp::Sub) {
        Some("Cannot increment/decrement string offsets")
    } else {
        Some("Cannot use assign-op operators with string offsets")
    }
}

/// Returns whether `expr` is exactly the string offset target `$array[index]`.
pub(in crate::types::checker) fn expr_is_string_offset_target(
    expr: &Expr,
    array: &str,
    index: &Expr,
) -> bool {
    matches!(
        &expr.kind,
        ExprKind::ArrayAccess { array: receiver, index: read_index }
            if matches!(&receiver.kind, ExprKind::Variable(name) if name == array)
                && read_index.as_ref() == index
    )
}

/// Validates the index and value types for a whole-element buffer write.
///
/// A width-declared element (`buffer<u8>`, `buffer<f32>`) takes exactly what an `int` or
/// `float` element takes; the store narrows the value to the element width. A packed element
/// copies a record of the same class from a packed address or typed pointer.
pub(super) fn validate_buffer_element_write(
    elem_ty: &PhpType,
    idx_ty: &PhpType,
    val_ty: &PhpType,
    span: Span,
) -> Result<(), CompileError> {
    if !matches!(idx_ty, PhpType::Int | PhpType::Mixed) && !idx_ty.is_int_float_union() {
        return Err(CompileError::new(span, "Buffer index must be integer"));
    }
    match &elem_ty.storage_value_type() {
        PhpType::Packed(record) => {
            let same_record = matches!(val_ty,
                PhpType::Packed(source) | PhpType::Pointer(Some(source))
                    if source.trim_start_matches('\\')
                        .eq_ignore_ascii_case(record.trim_start_matches('\\'))
            );
            if same_record {
                Ok(())
            } else {
                Err(CompileError::new(span, &format!(
                    "Buffer element type mismatch: expected {} record, got {}",
                    record, val_ty,
                )))
            }
        },
        inner if !buffer_element_accepts_assignment(inner, val_ty) => Err(CompileError::new(
            span,
            &format!(
                "Buffer element type mismatch: expected {}, got {}",
                inner, val_ty
            ),
        )),
        _ => Ok(()),
    }
}

/// Returns whether a buffer element accepts an assignment value after runtime coercion.
fn buffer_element_accepts_assignment(expected: &PhpType, actual: &PhpType) -> bool {
    if expected == actual {
        return true;
    }
    // `int|float` belongs with `Mixed` here: a buffer element is a raw scalar slot and the
    // runtime narrows either one the same way. Refusing it while accepting `Mixed` made the
    // narrower type the stricter one, which is what a loop counter runs into —
    // `for (…; $i++) { $buf[$k] = $i; }`.
    if matches!(expected, PhpType::Float | PhpType::Int | PhpType::Bool)
        && actual.is_int_float_union()
    {
        return true;
    }
    matches!(
        (expected, actual),
        (PhpType::Bool, PhpType::False)
            | (PhpType::Float | PhpType::Int | PhpType::Bool, PhpType::Mixed)
    )
}

/// Validates a nested array assignment like `$arr[$i] = $value` where the target itself is an array access.
///
/// Type-checks the array, index, and value expressions, then validates that the array type supports
/// nested offset assignment. Selected scalar array elements use catchable runtime checks;
/// `Mixed` and objects implementing `ArrayAccess` retain their existing write paths.
///
/// Errors:
/// - Target is not an array access expression
/// - Direct string storage is not supported by the plain offset-assignment surface
/// - Target type does not support nested assignment (not `Mixed` or `ArrayAccess`)
pub(super) fn check_nested_array_assign(
    checker: &mut Checker,
    target: &Expr,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let ExprKind::ArrayAccess { array, index } = &target.kind else {
        return Err(CompileError::new(span, "Invalid assignment target"));
    };

    super::static_properties::promote_static_nested_write_root(checker, target, span)?;
    let arr_ty = checker.infer_nested_write_parent_with_effects(array, env)?;
    checker.infer_type_with_assignment_effects(index, env)?;
    if !nested_write_parent_can_promote(&arr_ty, array)
        || !check_promoted_nested_update_value(checker, target, value, span, env)?
    {
        checker.infer_type_with_assignment_effects(value, env)?;
    }
    match arr_ty {
        PhpType::Mixed => Ok(()),
        PhpType::Array(_) | PhpType::AssocArray { .. } => Ok(()),
        PhpType::Str | PhpType::Bool | PhpType::False | PhpType::Int | PhpType::Float
            | PhpType::Resource(_) | PhpType::Void
            if matches!(array.kind, ExprKind::ArrayAccess { .. }) => Ok(()),
        PhpType::Union(members)
            if matches!(array.kind, ExprKind::ArrayAccess { .. })
                && members.iter().all(|member| matches!(member,
                    PhpType::Bool | PhpType::False | PhpType::Void)) => Ok(()),
        PhpType::Str => Err(CompileError::new(
            span,
            "String offset assignment is not supported",
        )),
        PhpType::Object(class_name)
            if checker.object_type_implements_interface(&class_name, "ArrayAccess") =>
        {
            Ok(())
        }
        _ => Err(CompileError::new(
            span,
            "Nested array assignment requires a Mixed or ArrayAccess target",
        )),
    }
}

/// Returns whether a nested write parent may contain a promotable false or null.
fn nested_write_parent_can_promote(parent: &PhpType, array: &Expr) -> bool {
    if !matches!(array.kind, ExprKind::ArrayAccess { .. }) {
        return false;
    }
    match parent {
        // An earlier false/null ancestor is already inferred as Mixed in write context.
        // Only recognized generated updates skip their ordinary read below.
        PhpType::Mixed | PhpType::Bool | PhpType::False | PhpType::Void => true,
        PhpType::Union(members) => members.iter().all(|member| matches!(member,
            PhpType::Bool | PhpType::False | PhpType::Void)),
        _ => false,
    }
}

/// Checks only the user operand of a desugared update whose missing leaf reads as null.
fn check_promoted_nested_update_value(
    checker: &mut Checker,
    target: &Expr,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<bool, CompileError> {
    if value.span != span {
        return Ok(false);
    }
    match &value.kind {
        ExprKind::BinaryOp { left, right, .. } if left.as_ref() == target => {
            checker.infer_type_with_assignment_effects(right, env)?;
            Ok(true)
        }
        ExprKind::NullCoalesce { value: read, default } if read.as_ref() == target => {
            checker.infer_type_with_assignment_effects(default, env)?;
            Ok(true)
        }
        ExprKind::Assignment { target: result, value, prelude, result_target: None, .. }
            if matches!(prelude.len(), 1 | 2) =>
        {
            let crate::parser::ast::StmtKind::Assign { name, value: read } = &prelude[0].kind
                else { return Ok(false); };
            if read != target {
                return Ok(false);
            }
            let working = if prelude.len() == 2 {
                let crate::parser::ast::StmtKind::Assign { name: working, value: copy } = &prelude[1].kind
                    else { return Ok(false); };
                if !matches!(&copy.kind, ExprKind::Variable(source) if source == name) {
                    return Ok(false);
                }
                working
            } else {
                name
            };
            if !matches!(&value.kind,
                ExprKind::PreIncrement(local) | ExprKind::PreDecrement(local) if local == working)
            {
                return Ok(false);
            }
            let ExprKind::Variable(result) = &result.kind else { return Ok(false); };
            // The write-context lookup supplies a boxed null for the missing leaf.
            // Publish both captures without replaying an invalid read through false.
            env.insert(name.clone(), PhpType::Mixed);
            env.insert(working.clone(), PhpType::Mixed);
            let result_type = checker.infer_type_with_assignment_effects(value, env)?;
            env.insert(result.clone(), result_type);
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Validates and updates the type environment for `$array[] = $value` (push) assignments.
///
/// Type-checks the value, then merges it into the element type of the array.
/// For `PhpType::Array`, updates the element type in `env` to the merged type.
/// For `PhpType::AssocArray`, merges the pushed value type and adds integer keys.
/// For buffers, returns an error (buffers do not support push).
/// For objects implementing `ArrayAccess`, allows the push without element type merging.
///
/// Errors:
/// - Undefined variable
/// - Buffer push (buffers require `buffer_new<T>(len)` for allocation)
/// - Object push without `ArrayAccess`
pub(super) fn check_array_push(
    checker: &mut Checker,
    array: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    // PHP auto-vivifies an undefined variable on `$a[] = ...`. Starting from an empty indexed
    // array admits the write through the ordinary push path below, preserving callable-array
    // metadata and element-type merging exactly as a declared `[]` initializer would.
    let arr_ty = match env.get(array).cloned() {
        Some(ty) => ty,
        None => {
            let empty = PhpType::Array(Box::new(PhpType::Never));
            env.insert(array.to_string(), empty.clone());
            empty
        }
    };
    let val_ty = checker.infer_type_with_assignment_effects(value, env)?;
    super::locals::update_callable_assignment_metadata(checker, array, value, &val_ty, env)?;
    // Nested appends retain their catchable runtime Error through the generated temporary.
    if arr_ty == PhpType::Str && !array.starts_with(NESTED_APPEND_TEMP_PREFIX) {
        return Err(CompileError::new(span, "[] operator not supported for strings"));
    }
    if let PhpType::Array(elem_ty) = &arr_ty {
        if **elem_ty != val_ty {
            let merged_ty = checker
                .merge_array_element_type(elem_ty, &val_ty)
                .unwrap_or(PhpType::Mixed);
            env.insert(array.to_string(), PhpType::Array(Box::new(merged_ty)));
        }
    } else if let PhpType::AssocArray {
        key,
        value: existing_value,
    } = &arr_ty
    {
        let merged_key = merge_array_key_types(*key.clone(), PhpType::Int);
        let merged_value = if **existing_value == val_ty {
            *existing_value.clone()
        } else {
            // Pointers and `null` share the pointer word, as in the keyed write above.
            crate::types::merge_pointer_elements(existing_value, &val_ty).unwrap_or(PhpType::Mixed)
        };
        env.insert(
            array.to_string(),
            PhpType::AssocArray {
                key: Box::new(merged_key),
                value: Box::new(merged_value),
            },
        );
    } else if matches!(arr_ty, PhpType::Buffer(_)) {
        return Err(CompileError::new(
            span,
            "buffer<T> does not support push; allocate with buffer_new<T>(len)",
        ));
    } else if let PhpType::Object(class_name) = &arr_ty {
        if !checker.object_type_implements_interface(class_name, "ArrayAccess") {
            return Err(CompileError::new(
                span,
                "Object array push requires ArrayAccess",
            ));
        }
    }
    Ok(())
}

/// Rejects an element write that would widen a local's DECLARED element type.
///
/// `array<int> $a = …` and `array<string, int> $m = …` are programmer contracts in exactly
/// the sense `Checker::typed_local_names` already gives `int $x = …`: the declaration pins
/// the container's storage, and widening it is not a type refinement but a representation
/// rewrite (`Op::ArrayToMixed` boxes every slot). Accepting it silently is what made
/// `array<int> $a = [1,2,3]; $a[0] = "s";` compile to `array_to_mixed` with no diagnostic,
/// defeating the declaration.
///
/// `declared` is the whole container type rather than its element, so the diagnostic can name
/// the form the programmer actually wrote — `array<string, int>`, not `array<int>`.
///
/// Callers must have established that the write genuinely widens; a bare `array` declaration
/// is `Array(Mixed)` and absorbs every write without changing, so it never reaches here.
/// Inferred containers are absent from `typed_local_names` and keep widening as before.
fn reject_declared_element_widening(
    checker: &Checker,
    array: &str,
    declared: &PhpType,
    val_ty: &PhpType,
    span: Span,
) -> Result<(), CompileError> {
    if !checker.typed_local_names.contains_key(array) {
        return Ok(());
    }
    Err(CompileError::new(
        span,
        &format!(
            "cannot store {} into ${} declared as {}",
            val_ty, array, declared
        ),
    ))
}
