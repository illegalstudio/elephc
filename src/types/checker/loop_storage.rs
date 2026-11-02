//! Purpose:
//! Computes fixed-point storage contracts for locals carried around loop back-edges.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::control_flow` before checking a loop body.
//!
//! Key details:
//! - The analysis iterates assignment and array-growth evidence until local types stop changing.
//! - Array payload changes retain their container-specific conversions, while nullable locals
//!   that receive a non-null whole value across a back-edge use one boxed `Mixed` contract.
//! - A local that enters holding `null` and is assigned something else inside the body gets boxed
//!   `Mixed`, so the header holds both the entering null and the value the back edge carries in
//!   (issue #562).
//! - EIR lowering consumes the checker-recorded contract instead of repeating expression
//!   inference, keeping non-literal RHSs and cascading promotions aligned across both layers.
//! - An `(array)` cast inside a literal uses the same `ArrayCast` classification as the checker,
//!   so a loop-carried array of those casts does not collapse to `array<int>`.

use std::collections::HashSet;

use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Stmt, StmtKind};
use crate::types::{
    array_key_contiguity_is_unproven, merge_array_key_types, normalized_array_key_type, PhpType,
    TypeEnv,
};

/// Source recorded for a local assignment that may affect a later loop-carried array rebind.
enum AssignedValue<'a> {
    /// An ordinary assignment whose RHS can be inferred under the evolving environment.
    Expr(&'a Expr),
    /// A binding whose semantic type is declared by its enclosing control-flow construct.
    Known(PhpType),
    /// A binding without a statically available RHS, such as a `foreach` value.
    Opaque,
    /// `$i++` and friends: the type is the variable's OWN, which only the environment knows.
    ///
    /// Distinct from `Opaque` because an increment is not an unknown value. Treating it as one
    /// made `$i` `Mixed` inside the storage fixed point, and a `$out[] = $i` then pinned `$out`
    /// to `array<mixed>` — so `for ($i = 0; …) { $out[] = $i; } return $out;` could not satisfy
    /// a declared `array<int>`, while appending a constant or a parameter always could.
    Increment(&'a str),
}

/// One local array write, including an optional explicit key expression.
struct ArrayWrite<'a> {
    /// Local receiving the element.
    name: &'a str,
    /// Explicit offset for `$array[$key] = value`, or `None` for append forms.
    index: Option<&'a Expr>,
    /// Value stored in the array.
    value: &'a Expr,
    /// Whether an iteration can reach the loop's end without performing this write.
    ///
    /// True under a branch or inside a nested loop. A counter only tracks an array's length while
    /// every iteration writes once, so a skippable write can leave a gap and cannot keep packed
    /// storage. Append forms carry `false` and never consult it — they have no key to gap.
    skippable: bool,
}

/// Computes stable storage types for locals already present at loop entry.
///
/// `infer_value` receives the evolving fixed-point environment, so a promotion can cascade
/// through intermediate locals and through later iterations. The result contains only locals
/// that require an up-front representation contract: indexed/associative arrays with boxed
/// `mixed` payloads, tagged nullable scalars, or whole-value `mixed` for a local crossing a
/// back-edge.
///
/// `ref_value_binding` names a `foreach ($source as &$value)` pair whose SOURCE is a plain local.
/// See [`apply_ref_value_binding_evidence`] for why what the body stores into `$value` is element
/// evidence against `$source`.
pub fn loop_carried_storage_types<'a>(
    body: &'a [Stmt],
    update: Option<&'a Stmt>,
    entry: &TypeEnv,
    packed_counter: Option<&str>,
    ref_value_binding: Option<(&'a str, &'a str)>,
    infer_value: &mut dyn FnMut(&Expr, &TypeEnv) -> Option<PhpType>,
) -> Vec<(String, PhpType)> {
    let mut assignments = Vec::new();
    collect_value_assignments(body, &mut assignments);
    if let Some(update) = update {
        collect_value_assignment_stmt(update, &mut assignments);
    }

    let mut writes = Vec::new();
    collect_array_writes(body, &mut writes, false);
    if let Some(update) = update {
        collect_array_write_stmt(update, &mut writes, false);
    }
    let mut foreach_keys = HashSet::new();
    collect_foreach_key_names(body, &mut foreach_keys);
    let mut foreach_values = HashSet::new();
    collect_foreach_value_names(body, &mut foreach_values);

    let mut fixed = entry.clone();
    let mut whole_representation_sources = HashSet::new();
    loop {
        let previous = fixed.clone();
        apply_assignment_evidence(
            &assignments,
            &mut fixed,
            infer_value,
            &mut whole_representation_sources,
        );
        apply_ref_value_binding_evidence(
            ref_value_binding,
            &assignments,
            &mut fixed,
            infer_value,
        );
        apply_array_write_evidence(
            &writes,
            &mut fixed,
            packed_counter,
            &foreach_keys,
            &foreach_values,
            infer_value,
        );
        if fixed == previous {
            break;
        }
    }

    let mut iterated_sources = HashSet::new();
    collect_foreach_sources(body, &mut iterated_sources);
    collect_element_read_sources(body, &mut iterated_sources);
    if let Some(update) = update {
        collect_foreach_sources(std::slice::from_ref(update), &mut iterated_sources);
        collect_element_read_sources(std::slice::from_ref(update), &mut iterated_sources);
    }

    let mut contracts = entry
        .iter()
        .filter_map(|(name, entry_ty)| {
            let fixed_ty = fixed.get(name)?;
            representation_contract(
                entry_ty,
                fixed_ty,
                whole_representation_sources.contains(name),
                iterated_sources.contains(name.as_str()),
            )
            .or_else(|| {
                if is_array_like(entry_ty) {
                    None
                } else {
                    null_entry_contract(entry_ty, fixed_ty)
                }
            })
            .map(|contract| (name.clone(), contract))
        })
        .collect::<Vec<_>>();
    contracts.sort_by(|left, right| left.0.cmp(&right.0));
    contracts
}

/// Carries a by-REFERENCE `foreach` value variable's fixed-point type into the iterated array's
/// element type.
///
/// `foreach ($chunks as &$chunk) { $chunk = new Chunk($chunk); }` writes THROUGH the reference into
/// `$chunks`, so the loop's element type has to accommodate whatever the body stores — exactly as
/// `$chunks[$k] = new Chunk(…)` would. Nothing else in this analysis knows that: the assignment is
/// syntactically a plain local store, and `collect_array_writes` only sees writes that NAME a
/// container. Without this the fixed point left `$chunks` at its entry `array<string>` while the
/// body stored an object into it, and the checker reported `cannot reassign $chunk from string to
/// Chunk` on code `php -n` runs (`Symfony\Component\String\ByteString::split` and its
/// `CodePointString` twin are the shape).
///
/// The stored types are re-derived here rather than read off the value variable's joined entry in
/// the fixed-point environment, because that entry cannot be trusted for this question:
/// `apply_assignment_evidence` ends its inference with `.unwrap_or(PhpType::Mixed)`, so a value it
/// has no answer for is indistinguishable from one that really is `mixed`, and boxing an array on
/// the strength of a fallback is exactly the wrong direction. An expression this analysis cannot
/// type is therefore NOT evidence at all — the same "only what can be typed exactly" discipline
/// `mixed_storage_scan` follows, and the safe one: missing a real widening costs the pre-existing
/// hard error, never a wrong representation.
///
/// It stays CONSERVATIVE where the checker is. `infer_type` answers `Mixed` for `int * int` —
/// PHP's arithmetic overflows to float — so `foreach ($nums as &$n) { $n = $n * 10; }` over an
/// `array<int>` really does box its payload. That is not a new answer: `$nums[$i] = $nums[$i] * 10`
/// in a loop has always boxed through `apply_array_write_evidence`, from the identical inference.
/// The two rules give the same answer to the same question by construction, which is the point of
/// routing this through the same join.
///
/// Applied per ITERATION, right after the assignment evidence, so a payload promotion can still
/// cascade — the array's new element type is what the next iteration's reads of `$chunk` see.
///
/// The join is the payload join, never the KEY one: a by-reference `foreach` visits the keys the
/// array already has, so it can neither turn a packed array into a hash nor renumber a hash.
/// A source that is not a plain local (`$this->items`) is not passed in at all — the caller can
/// neither read nor re-type such a place through this contract, and a target with no expression
/// behind it (a `RefAssign`, a `list()` element — [`AssignedValue::Opaque`]) has nothing to infer.
fn apply_ref_value_binding_evidence<'a>(
    ref_value_binding: Option<(&'a str, &'a str)>,
    assignments: &[(&'a str, AssignedValue<'a>)],
    env: &mut TypeEnv,
    infer_value: &mut dyn FnMut(&Expr, &TypeEnv) -> Option<PhpType>,
) {
    let Some((source_local, value_var)) = ref_value_binding else {
        return;
    };
    let mut stored: Option<PhpType> = None;
    for (name, source) in assignments {
        if *name != value_var {
            continue;
        }
        let AssignedValue::Expr(value) = source else {
            continue;
        };
        let Some(value_ty) = infer_storage_value_type(value, env, infer_value)
            .or_else(|| precise_scalar_expr_type(value))
        else {
            continue;
        };
        stored = Some(match stored {
            None => value_ty,
            Some(previous) => join_array_payload_type(previous, value_ty),
        });
    }
    let Some(stored) = stored else {
        return;
    };
    let updated = match env.get(source_local).cloned() {
        Some(PhpType::Array(element)) => {
            PhpType::Array(Box::new(join_array_payload_type(*element, stored)))
        }
        Some(PhpType::AssocArray { key, value }) => PhpType::AssocArray {
            key,
            value: Box::new(join_array_payload_type(*value, stored)),
        },
        _ => return,
    };
    env.insert(source_local.to_string(), updated);
}

/// Applies every collected local assignment monotonically to one fixed-point iteration.
fn apply_assignment_evidence(
    assignments: &[(&str, AssignedValue<'_>)],
    env: &mut TypeEnv,
    infer_value: &mut dyn FnMut(&Expr, &TypeEnv) -> Option<PhpType>,
    whole_representation_sources: &mut HashSet<String>,
) {
    for (name, source) in assignments {
        // `typed` is false when the value's type is only the `mixed` fallback: that is the
        // absence of an answer, not evidence of a gradual value (see the ref-binding rule below).
        let (incoming, typed) = match source {
            AssignedValue::Expr(expr) => match infer_storage_value_type(expr, env, infer_value)
                .or_else(|| precise_scalar_expr_type(expr))
            {
                Some(ty) => (ty, true),
                None => (PhpType::Mixed, false),
            },
            AssignedValue::Known(ty) => (ty.clone(), true),
            AssignedValue::Opaque => (PhpType::Mixed, false),
            // PHP's `++` keeps an int an int and a float a float. On anything else it is not
            // arithmetic at all — `++` on a string is a string increment, on null it yields 1 —
            // so those stay as unknown as they were.
            AssignedValue::Increment(variable) => match env.get(*variable) {
                Some(PhpType::Int) => (PhpType::Int, true),
                Some(PhpType::Float) => (PhpType::Float, true),
                _ => (PhpType::Mixed, false),
            },
        };
        let proves_whole_representation_change =
            env.get(*name).is_some_and(|existing| match source {
                AssignedValue::Opaque | AssignedValue::Increment(_) => false,
                AssignedValue::Expr(_) | AssignedValue::Known(_) if is_array_like(existing) => {
                    matches!(incoming.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
                }
                // A scalar local that the body rebinds to a gradual value changes its storage
                // from a raw word to a boxed cell. Without the contract the loop header kept the
                // scalar slot, so a read reached around the back edge took the box pointer for the
                // scalar: symfony/routing's `AttributeFileLoader::findClass()` starts
                // `$namespace = false`, stores a token's text into it, and then answered
                // `true === $namespace` for the text and built the class name "1\Kernel".
                AssignedValue::Expr(_) | AssignedValue::Known(_) => {
                    (matches!(existing, PhpType::Void)
                        && !matches!(&incoming, PhpType::Void | PhpType::Never))
                        || (typed
                            && matches!(
                                existing.codegen_repr(),
                                PhpType::Bool
                                    | PhpType::Int
                                    | PhpType::Float
                                    | PhpType::Str
                                    | PhpType::Object(_)
                            )
                            && matches!(incoming.codegen_repr(), PhpType::Mixed | PhpType::Union(_)))
                }
            });
        if proves_whole_representation_change {
            whole_representation_sources.insert((*name).to_string());
        }
        let merged = env
            .get(*name)
            .cloned()
            .map(|existing| join_loop_flow_type(existing, incoming.clone()))
            .unwrap_or(incoming);
        env.insert((*name).to_string(), merged);
    }
}

/// Applies indexed-array growth and element-write evidence to one fixed-point iteration.
fn apply_array_write_evidence(
    writes: &[ArrayWrite<'_>],
    env: &mut TypeEnv,
    packed_counter: Option<&str>,
    foreach_keys: &HashSet<&str>,
    foreach_values: &HashSet<&str>,
    infer_value: &mut dyn FnMut(&Expr, &TypeEnv) -> Option<PhpType>,
) {
    for write in writes {
        let incoming = infer_storage_value_type(write.value, env, infer_value)
            .or_else(|| precise_scalar_expr_type(write.value))
            .unwrap_or(PhpType::Mixed);
        let Some(current) = env.get(write.name).cloned() else {
            continue;
        };
        let key_type = write.index.map(|index| {
            let inferred = infer_storage_value_type(index, env, infer_value)
                .or_else(|| precise_scalar_expr_type(index))
                .unwrap_or(PhpType::Mixed);
            normalized_array_key_type(index, inferred)
        });
        let array_is_still_empty =
            matches!(&current, PhpType::Array(element) if matches!(element.as_ref(), PhpType::Never));
        let forces_hash = write.index.is_some_and(|index| {
            let literal_string_key = matches!(key_type.as_ref(), Some(PhpType::Str))
                && matches!(index.kind, ExprKind::StringLiteral(_) | ExprKind::Null);
            // The same rule `check_array_assign` applies, and it has to be applied HERE too: this
            // pass fixes the storage for the whole loop and emits the conversion, so a decision it
            // makes alone is the one the body then has to live with.
            // A `foreach` VALUE is any element of its iterable, so nothing bounds it against the
            // array's length either. This pass often has no type for it (`Mixed`), and the write
            // then reached `ArraySetMixedKey`, which keeps an integer key on packed storage and
            // zero-filled the gap: `foreach ([7] as $k) {} $a[$k] = $k;` in a loop built
            // `[null × 7, 7]` where php has `[7 => 7]`.
            let foreach_value_key = matches!(
                &index.kind,
                ExprKind::Variable(name) if foreach_values.contains(name.as_str())
            );
            // Not even a write the loop may skip escapes it: one inside the `foreach` body is
            // marked skippable, yet `foreach ([7] as $k) { $a[$k] = $k; }` still lands at 7.
            let unbounded_integer_key = ((matches!(key_type.as_ref(), Some(PhpType::Int))
                && !write.skippable)
                || foreach_value_key)
                && array_is_still_empty
                && !index_is_trusted_counter(index, packed_counter, foreach_keys)
                && array_key_contiguity_is_unproven(index);
            literal_string_key || unbounded_integer_key
        });
        let updated = match current {
            PhpType::Array(element) if forces_hash => {
                let was_empty = matches!(element.as_ref(), PhpType::Never);
                PhpType::AssocArray {
                    key: Box::new(if was_empty {
                        key_type.unwrap_or(PhpType::Mixed)
                    } else {
                        merge_array_key_types(
                            PhpType::Int,
                            key_type.unwrap_or(PhpType::Mixed),
                        )
                    }),
                    value: Box::new(if was_empty {
                        incoming
                    } else {
                        join_array_payload_type(*element, incoming)
                    }),
                }
            }
            PhpType::Array(element) => {
                PhpType::Array(Box::new(join_array_payload_type(*element, incoming)))
            }
            PhpType::AssocArray { key, value } => PhpType::AssocArray {
                key: Box::new(merge_array_key_types(
                    *key,
                    key_type.unwrap_or(PhpType::Int),
                )),
                value: Box::new(join_array_payload_type(*value, incoming)),
            },
            other => other,
        };
        env.insert(write.name.to_string(), updated);
    }
}

/// Infers the representation an expression materializes under the evolving loop environment.
///
/// Semantic checker types are authoritative for ordinary expressions, but array literals need
/// storage-aware element inference: direct variables and array reads can change representation
/// as the fixed point evolves, while the remaining literal elements follow the same syntactic
/// fallback used by EIR literal lowering.
fn infer_storage_value_type(
    expr: &Expr,
    env: &TypeEnv,
    infer_value: &mut dyn FnMut(&Expr, &TypeEnv) -> Option<PhpType>,
) -> Option<PhpType> {
    match &expr.kind {
        ExprKind::Variable(name) => env.get(name).cloned().or_else(|| infer_value(expr, env)),
        ExprKind::ArrayLiteral(items) => {
            let mut element = PhpType::Never;
            for item in items {
                let next = infer_array_literal_element_storage(item, env, infer_value);
                element = merge_materialized_array_element(element, next);
            }
            Some(PhpType::Array(Box::new(element)))
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            let mut key = PhpType::Never;
            let mut value = PhpType::Never;
            for (key_expr, value_expr) in entries {
                let next_key = infer_value(key_expr, env)
                    .unwrap_or_else(|| {
                        crate::types::checker::infer_expr_type_syntactic(key_expr)
                    })
                    .codegen_repr();
                let next_value =
                    infer_array_literal_element_storage(value_expr, env, infer_value);
                key = merge_materialized_array_element(key, next_key);
                value = merge_materialized_array_element(value, next_value);
            }
            Some(PhpType::AssocArray {
                key: Box::new(key),
                value: Box::new(value),
            })
        }
        ExprKind::ErrorSuppress(inner) => infer_storage_value_type(inner, env, infer_value),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_ty = infer_storage_value_type(then_expr, env, infer_value)?;
            let else_ty = infer_storage_value_type(else_expr, env, infer_value)?;
            Some(join_loop_flow_type(then_ty, else_ty))
        }
        _ => infer_value(expr, env),
    }
}

/// Infers one indexed/associative literal element's concrete runtime storage representation.
///
/// An `(array)` cast is classified with `ArrayCast` from the source's type in this
/// environment. The syntactic fallback answers `int` for a cast it does not recognise,
/// which would record the surrounding literal as an array of ints.
fn infer_array_literal_element_storage(
    item: &Expr,
    env: &TypeEnv,
    infer_value: &mut dyn FnMut(&Expr, &TypeEnv) -> Option<PhpType>,
) -> PhpType {
    match &item.kind {
        ExprKind::Variable(name) => env
            .get(name)
            .cloned()
            .or_else(|| infer_value(item, env))
            .unwrap_or(PhpType::Mixed)
            .codegen_repr(),
        ExprKind::ArrayAccess { array, .. } => {
            let source = infer_storage_value_type(array, env, infer_value)
                .or_else(|| infer_value(array, env))
                .unwrap_or(PhpType::Mixed)
                .codegen_repr();
            match source {
                PhpType::Array(element) => array_read_storage_type(*element),
                PhpType::AssocArray { value, .. } => array_read_storage_type(*value),
                PhpType::Str => PhpType::Str,
                _ => PhpType::Mixed,
            }
        }
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            infer_storage_value_type(item, env, infer_value)
                .unwrap_or(PhpType::Mixed)
                .codegen_repr()
        }
        ExprKind::Spread(inner) => match infer_storage_value_type(inner, env, infer_value)
            .unwrap_or(PhpType::Mixed)
            .codegen_repr()
        {
            PhpType::Array(element) => *element,
            PhpType::AssocArray { value, .. } => *value,
            _ => PhpType::Mixed,
        },
        ExprKind::Cast {
            target: CastType::Array,
            expr: inner,
        } => crate::types::array_cast_result_type(&array_cast_source_type_for_storage(
            inner, env, infer_value,
        ))
        .codegen_repr(),
        ExprKind::FunctionCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::PropertyAccess { .. }
        | ExprKind::ConstRef(_) => infer_value(item, env)
            .unwrap_or_else(|| crate::types::checker::infer_expr_type_syntactic(item))
            .codegen_repr(),
        _ => crate::types::checker::infer_expr_type_syntactic(item).codegen_repr(),
    }
}

/// Returns the PHP type an `(array)` cast inside a literal classifies.
///
/// Variables prefer the fixed-point environment, which is the storage the next iteration
/// will load. The literal `null` is `Void` so the cast is an empty array; the element
/// storage of a null literal is `Mixed` and would take the runtime-dispatch arm instead.
/// Every cast, including nested `(object)` and `(array)`, uses the same result type as
/// EIR lowering. A source the checker cannot type is `Mixed` rather than the syntactic
/// `int` guess.
fn array_cast_source_type_for_storage(
    expr: &Expr,
    env: &TypeEnv,
    infer_value: &mut dyn FnMut(&Expr, &TypeEnv) -> Option<PhpType>,
) -> PhpType {
    match &expr.kind {
        ExprKind::Null => PhpType::Void,
        ExprKind::Variable(name) => env
            .get(name)
            .cloned()
            .or_else(|| infer_value(expr, env))
            .unwrap_or(PhpType::Mixed),
        ExprKind::Cast { target, expr: inner } => crate::types::php_cast_result_type(
            target,
            &array_cast_source_type_for_storage(inner, env, infer_value),
        ),
        _ => infer_value(expr, env).unwrap_or(PhpType::Mixed),
    }
}

/// Adds the miss-capable tagged representation used by integer array reads on tagged targets.
fn array_read_storage_type(element: PhpType) -> PhpType {
    if crate::codegen::sentinels::null_repr_is_tagged()
        && matches!(element.codegen_repr(), PhpType::Int)
    {
        PhpType::TaggedScalar
    } else {
        element.codegen_repr()
    }
}

/// Merges two materialized literal element representations.
fn merge_materialized_array_element(left: PhpType, right: PhpType) -> PhpType {
    if matches!(left, PhpType::Never) {
        return right;
    }
    if matches!(right, PhpType::Never) {
        return left;
    }
    if left.codegen_repr() == right.codegen_repr() {
        left
    } else {
        PhpType::Mixed
    }
}

/// Joins two possible values of a local on the loop-storage lattice.
fn join_loop_flow_type(existing: PhpType, incoming: PhpType) -> PhpType {
    if existing == incoming {
        return existing;
    }
    if let Some(pointer) = join_pointer_flow_types(&existing, &incoming) {
        return pointer;
    }
    if existing.is_php_array() || incoming.is_php_array() {
        // A boxed representation need not erase a proven packed-or-hash array contract.
        return if is_array_like(&existing) && is_array_like(&incoming) {
            PhpType::php_array()
        } else {
            PhpType::Mixed
        };
    }
    match (existing, incoming) {
        (PhpType::Array(left), PhpType::Array(right)) => {
            PhpType::Array(Box::new(join_array_payload_type(*left, *right)))
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
        ) if left_key.codegen_repr() == right_key.codegen_repr() => PhpType::AssocArray {
            key: left_key,
            value: Box::new(join_array_payload_type(*left_value, *right_value)),
        },
        (PhpType::Array(element), PhpType::AssocArray { key, value })
        | (PhpType::AssocArray { key, value }, PhpType::Array(element)) => {
            let array_was_empty = matches!(element.as_ref(), PhpType::Never);
            PhpType::AssocArray {
                key: if array_was_empty {
                    key
                } else {
                    Box::new(merge_array_key_types(PhpType::Int, *key))
                },
                value: Box::new(if array_was_empty {
                    *value
                } else {
                    join_array_payload_type(*element, *value)
                }),
            }
        }
        (PhpType::Array(array), PhpType::Void | PhpType::Never)
        | (PhpType::Void | PhpType::Never, PhpType::Array(array)) => PhpType::Array(array),
        (PhpType::AssocArray { key, value }, PhpType::Void | PhpType::Never)
        | (PhpType::Void | PhpType::Never, PhpType::AssocArray { key, value }) => {
            PhpType::AssocArray { key, value }
        }
        (left, right) if left.codegen_repr() == right.codegen_repr() => left,
        (left, right) => merge_loop_union_members(left, right),
    }
}

/// Builds a stable semantic union for loop-carried whole-value representation changes.
///
/// Retaining members such as `null|ArrayAccessObject` lets flow narrowing recover the concrete
/// arm inside a truthy branch while `codegen_repr()` still selects one boxed `Mixed` frame slot.
/// Existing unions are flattened and duplicate members are removed so the fixed-point converges.
fn merge_loop_union_members(left: PhpType, right: PhpType) -> PhpType {
    let mut members = Vec::new();
    for ty in [left, right] {
        match ty {
            PhpType::Union(nested) => {
                for member in nested {
                    if !members.contains(&member) {
                        members.push(member);
                    }
                }
            }
            member if !matches!(member, PhpType::Never) && !members.contains(&member) => {
                members.push(member);
            }
            _ => {}
        }
    }
    match members.len() {
        0 => PhpType::Never,
        1 => members.pop().unwrap(),
        _ => PhpType::Union(members),
    }
}

/// Joins `null` and pointer values of one local into the pointer word they share.
///
/// A `?ptr<T>` is one pointer word where `null` is address 0, so `null`, pointers and `?ptr<T>`
/// values join to that pointer (`?ptr<T>` when either side can be `null`) instead of boxed
/// `mixed`. Two different targets, or a target and an untyped `ptr`, join to the untyped `ptr`,
/// which nothing narrows again: the fixed point replays every assignment until the environment
/// stops changing, so the join must only move up (`$p = $a; $p = $b; $p = $c;` over three
/// targets would otherwise flip between `ptr` and the last target forever). Returns `None`
/// unless both sides are one of these shapes and at least one of them holds a pointer.
fn join_pointer_flow_types(existing: &PhpType, incoming: &PhpType) -> Option<PhpType> {
    /// Splits a flow type into its pointer (none for `null`) and whether it can be `null`.
    fn split(ty: &PhpType) -> Option<(Option<PhpType>, bool)> {
        match ty {
            PhpType::Pointer(_) => Some((Some(ty.clone()), false)),
            PhpType::Void | PhpType::Never => Some((None, true)),
            PhpType::Union(members) => {
                crate::types::nullable_pointer_union_member(members).map(|p| (Some(p), true))
            }
            _ => None,
        }
    }
    let (left, left_null) = split(existing)?;
    let (right, right_null) = split(incoming)?;
    let pointer = match (left, right) {
        (Some(left), Some(right)) if left == right => left,
        (Some(_), Some(_)) => PhpType::Pointer(None),
        (Some(pointer), None) | (None, Some(pointer)) => pointer,
        (None, None) => return None,
    };
    Some(if left_null || right_null {
        PhpType::Union(vec![pointer, PhpType::Void])
    } else {
        pointer
    })
}

/// Joins array payload types so every live element has one stable runtime representation.
fn join_array_payload_type(existing: PhpType, incoming: PhpType) -> PhpType {
    if existing == incoming || existing.codegen_repr() == incoming.codegen_repr() {
        return existing;
    }
    if matches!(existing, PhpType::Never) {
        return incoming;
    }
    if matches!(incoming, PhpType::Never) {
        return existing;
    }
    PhpType::Mixed
}

/// Returns the concrete pre-loop storage contract required by a fixed-point result.
fn representation_contract(
    entry: &PhpType,
    fixed: &PhpType,
    has_whole_representation_source: bool,
    iterated_in_body: bool,
) -> Option<PhpType> {
    if fixed.is_php_array() {
        return (!entry.is_php_array()).then(|| fixed.clone());
    }
    match (entry.codegen_repr(), fixed.codegen_repr()) {
        (PhpType::Array(entry_element), PhpType::Array(fixed_element))
            if entry_element.codegen_repr() != PhpType::Mixed
                && fixed_element.codegen_repr() == PhpType::Mixed =>
        {
            Some(PhpType::Array(Box::new(PhpType::Mixed)))
        }
        (
            PhpType::AssocArray {
                value: entry_value,
                ..
            },
            PhpType::AssocArray {
                key: fixed_key,
                value: fixed_value,
            },
        ) if entry_value.codegen_repr() != PhpType::Mixed
            && fixed_value.codegen_repr() == PhpType::Mixed =>
        {
            Some(PhpType::AssocArray {
                key: fixed_key,
                value: Box::new(PhpType::Mixed),
            })
        }
        (PhpType::Array(_), fixed @ PhpType::AssocArray { .. }) => Some(fixed),
        // An empty-array local that the body both ITERATES and grows. `$seen = []` enters the
        // loop as `array<never>`, and the body's own `$seen[] = ...` is the only statement that
        // says what it holds; a `foreach ($seen as ...)` placed BEFORE that append textually is
        // still reached WITH elements on the second iteration. Leaving the element at `never`
        // made that read refuse code `php -n` runs — Symfony's
        // `CompiledUrlMatcherDumper::groupStaticRoutes` is the shape it was found on, where the
        // read destructures the element (`foreach ($dynamicRegex as [$hostRx, $rx, $prefix])`).
        //
        // The contract is the BOXED payload rather than the fixed point's own element type,
        // because that is the one promotion the lowering can materialize: the loop-storage
        // contract pass converts an indexed array in place with `ArrayToMixed`, which rewrites
        // the runtime element tag in the array header. Recording `array<array<mixed>>` instead
        // left the header saying `never` while every read was typed as an array, and the
        // destructuring read came back null.
        //
        // Gated on the iteration because a plain accumulator — `$out = []; foreach (…) { $out[] =
        // $n; }` — must keep its precise `array<int>` element: nothing in the loop reads it, so
        // boxing it would cost an allocation per element and buy nothing.
        (PhpType::Array(entry_element), PhpType::Array(fixed_element))
            if iterated_in_body
                && matches!(entry_element.as_ref(), PhpType::Never)
                && !matches!(fixed_element.as_ref(), PhpType::Never) =>
        {
            Some(PhpType::Array(Box::new(PhpType::Mixed)))
        }
        (PhpType::Void | PhpType::Never, PhpType::TaggedScalar)
            if has_whole_representation_source =>
        {
            Some(fixed.clone())
        }
        (entry_repr, PhpType::Mixed)
            if entry_repr != PhpType::Mixed && has_whole_representation_source =>
        {
            // A plain scalar entry takes the boxed representation itself: the joined fixed type is
            // a precise union, and re-typing the local with it made the body's index reads refuse
            // to compile ("String index must be integer") on values php only knows as mixed.
            //
            // An OBJECT entry is the same case: a typed parameter (`Definition $definition`) that
            // the body rebinds to a gradual value (`$definition = $factory;`). With the object slot
            // kept, the header's `$definition instanceof Definition` read the box of a null or a
            // class-name string as a live object -- Symfony's `PhpDumper::getClasses()` then
            // dispatched `getTag()` through a string's bytes.
            Some(
                if is_array_like(entry)
                    || matches!(
                        entry_repr,
                        PhpType::Bool
                            | PhpType::Int
                            | PhpType::Float
                            | PhpType::Str
                            | PhpType::Object(_)
                    )
                {
                    PhpType::Mixed
                } else {
                    fixed.clone()
                },
            )
        }
        _ => None,
    }
}

/// Recognizes concrete array storage and the boxed, non-null PHP array contract.
fn is_array_like(ty: &PhpType) -> bool {
    ty.is_php_array() || matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
}

/// The contract for a local that enters the loop holding `null` and is assigned something else
/// inside it.
///
/// The body is checked ONCE, with the entry environment, so without this the header keeps the
/// first iteration's `null` for the whole loop — and every later iteration re-enters holding
/// whatever the previous one stored. `$n = null; for (…) { if ($n === null) { $n = 0; } $n++;
/// echo $n; }` printed `111` where PHP prints `123`: the guard was folded as always-true
/// because the loaded value was typed `null` (issue #562).
///
/// Boxed `Mixed` rather than the fixed-point join: the join of `null` and `int` is already
/// `Mixed`, but the join of `null` and `array` is `array` — right for an array-typed entry that
/// is merely growing, wrong here, because the FIRST read still happens before the assignment
/// and still sees null. `Mixed` is the one representation that holds both on every path, which
/// is also what makes the `=== null` guard a real runtime tag test instead of a folded
/// constant.
///
/// A local assigned only pointers (packed record addresses read from a buffer included) gets
/// `?ptr<T>` instead: one pointer word holds both on every path, with `null` as address 0, so
/// `=== null` tests the address and field reads reach the record rather than a boxed cell.
fn null_entry_contract(entry_ty: &PhpType, fixed_ty: &PhpType) -> Option<PhpType> {
    if !matches!(entry_ty.codegen_repr(), PhpType::Void) {
        return None;
    }
    if matches!(fixed_ty.codegen_repr(), PhpType::Void) {
        return None;
    }
    if crate::types::is_nullable_pointer_type(fixed_ty) {
        return Some(fixed_ty.clone());
    }
    Some(PhpType::Mixed)
}

/// Returns a self-evident scalar type when semantic inference is unavailable.
fn precise_scalar_expr_type(value: &Expr) -> Option<PhpType> {
    match &value.kind {
        ExprKind::IntLiteral(_) => Some(PhpType::Int),
        ExprKind::FloatLiteral(_) => Some(PhpType::Float),
        ExprKind::StringLiteral(_) => Some(PhpType::Str),
        ExprKind::BoolLiteral(_) => Some(PhpType::Bool),
        ExprKind::Null => Some(PhpType::Void),
        ExprKind::Cast { target, .. } => match target {
            CastType::Int => Some(PhpType::Int),
            CastType::Float => Some(PhpType::Float),
            CastType::String => Some(PhpType::Str),
            CastType::Bool => Some(PhpType::Bool),
            CastType::Array | CastType::Object => None,
        },
        ExprKind::ErrorSuppress(inner) => precise_scalar_expr_type(inner),
        // `.` yields a string whatever its operands are. Left unknown, `$v = $v . $s[$i]` fell
        // back to `mixed`, which reads as a gradual rebind and boxed a plain string accumulator
        // for the whole loop.
        ExprKind::BinaryOp { op: BinOp::Concat, .. } => Some(PhpType::Str),
        _ => None,
    }
}

/// Collects local assignments from every statement in source traversal order.
fn collect_value_assignments<'a>(
    statements: &'a [Stmt],
    out: &mut Vec<(&'a str, AssignedValue<'a>)>,
) {
    for statement in statements {
        collect_value_assignment_stmt(statement, out);
    }
}

/// Collects local assignments from one statement and its executable nested bodies.
fn collect_value_assignment_stmt<'a>(
    statement: &'a Stmt,
    out: &mut Vec<(&'a str, AssignedValue<'a>)>,
) {
    match &statement.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            collect_value_assignments_from_expr(value, out);
            out.push((name, AssignedValue::Expr(value)));
        }
        StmtKind::RefAssign { target, source } => {
            collect_value_assignments_from_expr(source, out);
            out.push((target, AssignedValue::Opaque));
        }
        StmtKind::ListUnpack { vars, value } => {
            collect_value_assignments_from_expr(value, out);
            out.extend(
                vars.iter()
                    .map(|name| (name.as_str(), AssignedValue::Opaque)),
            );
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            collect_value_assignments_from_expr(index, out);
            collect_value_assignments_from_expr(value, out);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            collect_value_assignments_from_expr(target, out);
            collect_value_assignments_from_expr(value, out);
        }
        StmtKind::ArrayPush { value, .. } => {
            collect_value_assignments_from_expr(value, out);
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyRefAssign { object, source: value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            collect_value_assignments_from_expr(object, out);
            collect_value_assignments_from_expr(value, out);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            collect_value_assignments_from_expr(object, out);
            collect_value_assignments_from_expr(index, out);
            collect_value_assignments_from_expr(value, out);
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => {
            collect_value_assignments_from_expr(value, out);
        }
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            collect_value_assignments_from_expr(index, out);
            collect_value_assignments_from_expr(value, out);
        }
        StmtKind::StaticPropertyElementRefAssign { index, source, .. } => {
            collect_value_assignments_from_expr(index, out);
            collect_value_assignments_from_expr(source, out);
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
            ..
        } => {
            collect_value_assignments_from_expr(array, out);
            if let Some(key_var) = key_var {
                out.push((key_var, AssignedValue::Opaque));
            }
            out.push((value_var, AssignedValue::Opaque));
            collect_value_assignments(body, out);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_value_assignments_from_expr(condition, out);
            collect_value_assignments(then_body, out);
            for (condition, body) in elseif_clauses {
                collect_value_assignments_from_expr(condition, out);
                collect_value_assignments(body, out);
            }
            if let Some(else_body) = else_body {
                collect_value_assignments(else_body, out);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_value_assignments(then_body, out);
            if let Some(else_body) = else_body {
                collect_value_assignments(else_body, out);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_value_assignments_from_expr(condition, out);
            collect_value_assignments(body, out);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_value_assignment_stmt(init, out);
            }
            if let Some(condition) = condition {
                collect_value_assignments_from_expr(condition, out);
            }
            collect_value_assignments(body, out);
            if let Some(update) = update {
                collect_value_assignment_stmt(update, out);
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_value_assignments_from_expr(subject, out);
            for (values, body) in cases {
                for value in values {
                    collect_value_assignments_from_expr(value, out);
                }
                collect_value_assignments(body, out);
            }
            if let Some(default) = default {
                collect_value_assignments(default, out);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_value_assignments(try_body, out);
            for catch in catches {
                if let Some(variable) = &catch.variable {
                    let catch_type = if catch.exception_types.len() == 1 {
                        PhpType::Object(
                            catch.exception_types[0]
                                .as_str()
                                .trim_start_matches('\\')
                                .to_string(),
                        )
                    } else {
                        PhpType::Object("Throwable".to_string())
                    };
                    out.push((variable, AssignedValue::Known(catch_type)));
                }
                collect_value_assignments(&catch.body, out);
            }
            if let Some(finally_body) = finally_body {
                collect_value_assignments(finally_body, out);
            }
        }
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body) => collect_value_assignments(body, out),
        StmtKind::ExprStmt(expr)
        | StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::Include { path: expr, .. }
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. } => {
            collect_value_assignments_from_expr(expr, out);
        }
        StmtKind::Return(Some(expr)) => collect_value_assignments_from_expr(expr, out),
        _ => {}
    }
}

/// Collects assignment expressions and increment/decrement writes from an expression tree.
fn collect_value_assignments_from_expr<'a>(
    expr: &'a Expr,
    out: &mut Vec<(&'a str, AssignedValue<'a>)>,
) {
    match &expr.kind {
        ExprKind::Assignment {
            target,
            value,
            prelude,
            result_target,
            ..
        } => {
            collect_value_assignments(prelude, out);
            collect_value_assignments_from_expr(value, out);
            if let ExprKind::Variable(name) = &target.kind {
                out.push((name, AssignedValue::Expr(value)));
            }
            collect_value_assignments_from_expr(target, out);
            if let Some(result_target) = result_target {
                collect_value_assignments_from_expr(result_target, out);
            }
        }
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => out.push((name, AssignedValue::Increment(name))),
        _ => visit_child_expressions(expr, &mut |child| {
            collect_value_assignments_from_expr(child, out)
        }),
    }
}

/// Collects local indexed/associative array element writes from executable statements.
fn collect_array_writes<'a>(
    statements: &'a [Stmt],
    out: &mut Vec<ArrayWrite<'a>>,
    skippable: bool,
) {
    for statement in statements {
        collect_array_write_stmt(statement, out, skippable);
    }
}

/// Returns true when this index is the loop counter that provably tracks the array's length.
///
/// `packed_counter` is only ever `Some` for a `for` loop this pass is stabilizing, and only when
/// `packed_counter::packed_for_counter` proved the shape: initialized to `0`, advanced by `$i++`,
/// never reassigned, never jumped past by a `continue`. A `foreach` key is excluded even when it
/// matches: it is a runtime-tagged cell that may hold a string, so `Op::ArraySetMixedKey` decides
/// its storage at run time and this pass must not pin it.
fn index_is_trusted_counter(
    index: &Expr,
    packed_counter: Option<&str>,
    foreach_keys: &HashSet<&str>,
) -> bool {
    let ExprKind::Variable(name) = &index.kind else {
        return false;
    };
    foreach_keys.contains(name.as_str()) || packed_counter == Some(name.as_str())
}

/// Collects every local bound as a `foreach` VALUE anywhere in a loop body.
fn collect_foreach_value_names<'a>(statements: &'a [Stmt], out: &mut HashSet<&'a str>) {
    for statement in statements {
        if let StmtKind::Foreach {
            value_var, body, ..
        } = &statement.kind
        {
            out.insert(value_var.as_str());
            collect_foreach_value_names(body, out);
            continue;
        }
        for nested in nested_statement_bodies(statement) {
            collect_foreach_value_names(nested, out);
        }
    }
}

/// Collects every local bound as a `foreach` KEY anywhere in a loop body.
///
/// The checker's own set is built as each `foreach` is CHECKED, which is after this pass runs for
/// an enclosing loop, so the names have to be read back off the syntax here.
fn collect_foreach_key_names<'a>(statements: &'a [Stmt], out: &mut HashSet<&'a str>) {
    for statement in statements {
        if let StmtKind::Foreach { key_var, body, .. } = &statement.kind {
            out.extend(key_var.as_deref());
            collect_foreach_key_names(body, out);
            continue;
        }
        for nested in nested_statement_bodies(statement) {
            collect_foreach_key_names(nested, out);
        }
    }
}

/// Collects array growth/write sites from one statement and its nested bodies.
fn collect_array_write_stmt<'a>(
    statement: &'a Stmt,
    out: &mut Vec<ArrayWrite<'a>>,
    skippable: bool,
) {
    match &statement.kind {
        StmtKind::ArrayPush { array, value } => {
            out.push(ArrayWrite {
                name: array,
                index: None,
                value,
                skippable,
            });
            collect_growth_calls_from_expr(value, out);
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
            out.push(ArrayWrite {
                name: array,
                index: Some(index),
                value,
                skippable,
            });
            collect_growth_calls_from_expr(index, out);
            collect_growth_calls_from_expr(value, out);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            collect_growth_calls_from_expr(target, out);
            collect_growth_calls_from_expr(value, out);
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyRefAssign { object, source: value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            collect_growth_calls_from_expr(object, out);
            collect_growth_calls_from_expr(value, out);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            collect_growth_calls_from_expr(object, out);
            collect_growth_calls_from_expr(index, out);
            collect_growth_calls_from_expr(value, out);
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => {
            collect_growth_calls_from_expr(value, out);
        }
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            collect_growth_calls_from_expr(index, out);
            collect_growth_calls_from_expr(value, out);
        }
        StmtKind::StaticPropertyElementRefAssign { index, source, .. } => {
            collect_growth_calls_from_expr(index, out);
            collect_growth_calls_from_expr(source, out);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_growth_calls_from_expr(condition, out);
            collect_array_writes(then_body, out, true);
            for (condition, body) in elseif_clauses {
                collect_growth_calls_from_expr(condition, out);
                collect_array_writes(body, out, true);
            }
            if let Some(else_body) = else_body {
                collect_array_writes(else_body, out, true);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_array_writes(then_body, out, true);
            if let Some(else_body) = else_body {
                collect_array_writes(else_body, out, true);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_growth_calls_from_expr(condition, out);
            collect_array_writes(body, out, true);
        }
        StmtKind::Foreach { array, body, .. } => {
            collect_growth_calls_from_expr(array, out);
            collect_array_writes(body, out, true);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_array_write_stmt(init, out, skippable);
            }
            if let Some(condition) = condition {
                collect_growth_calls_from_expr(condition, out);
            }
            collect_array_writes(body, out, true);
            if let Some(update) = update {
                collect_array_write_stmt(update, out, true);
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_growth_calls_from_expr(subject, out);
            for (values, body) in cases {
                for value in values {
                    collect_growth_calls_from_expr(value, out);
                }
                collect_array_writes(body, out, true);
            }
            if let Some(default) = default {
                collect_array_writes(default, out, true);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_array_writes(try_body, out, true);
            for catch in catches {
                collect_array_writes(&catch.body, out, true);
            }
            if let Some(finally_body) = finally_body {
                collect_array_writes(finally_body, out, true);
            }
        }
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body) => collect_array_writes(body, out, skippable),
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ConstDecl { value, .. }
        | StmtKind::ListUnpack { value, .. }
        | StmtKind::StaticVar { init: value, .. }
        | StmtKind::Echo(value)
        | StmtKind::ExprStmt(value)
        | StmtKind::Throw(value)
        | StmtKind::Include { path: value, .. } => collect_growth_calls_from_expr(value, out),
        StmtKind::Return(Some(value)) => collect_growth_calls_from_expr(value, out),
        _ => {}
    }
}

/// Collects `array_push($local, ...)` and `array_unshift($local, ...)` sites from an expression
/// tree. Both add elements without changing the container's kind: Carbon's
/// `CarbonInterval::doCascade()` grows `$previous` with `array_unshift` below the
/// `foreach ($previous as [$subUnit, $subFactor])` that reads it.
fn collect_growth_calls_from_expr<'a>(expr: &'a Expr, out: &mut Vec<ArrayWrite<'a>>) {
    if let ExprKind::FunctionCall { name, args } = &expr.kind {
        let name = name.as_str().trim_start_matches('\\');
        if name.eq_ignore_ascii_case("array_push") || name.eq_ignore_ascii_case("array_unshift") {
            if let Some(array_name) = array_push_target_name(args) {
                out.extend(args.iter().skip(1).map(|argument| ArrayWrite {
                    name: array_name,
                    index: None,
                    value: call_arg_value(argument),
                    // An append has no key, so `skippable` is never consulted for it.
                    skippable: false,
                }));
            }
        }
    }
    visit_child_expressions(expr, &mut |child| {
        collect_growth_calls_from_expr(child, out)
    });
}

/// Visits direct executable child expressions without descending into closure bodies.
pub(crate) fn visit_child_expressions<'a>(expr: &'a Expr, visitor: &mut dyn FnMut(&'a Expr)) {
    match &expr.kind {
        ExprKind::BinaryOp { left, right, .. }
        | ExprKind::NullCoalesce {
            value: left,
            default: right,
        }
        | ExprKind::ShortTernary {
            value: left,
            default: right,
        }
        | ExprKind::Pipe {
            value: left,
            callable: right,
        } => {
            visitor(left);
            visitor(right);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            visitor(condition);
            visitor(then_expr);
            visitor(else_expr);
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            for argument in args {
                visitor(call_arg_value(argument));
            }
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            visitor(object);
            for argument in args {
                visitor(call_arg_value(argument));
            }
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            visitor(object);
            visitor(method);
            for argument in args {
                visitor(call_arg_value(argument));
            }
        }
        ExprKind::NewDynamic { name_expr, args } => {
            visitor(name_expr);
            for argument in args {
                visitor(call_arg_value(argument));
            }
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            visitor(class_name);
            for argument in args {
                visitor(call_arg_value(argument));
            }
        }
        ExprKind::ExprCall { callee, args } => {
            visitor(callee);
            for argument in args {
                visitor(call_arg_value(argument));
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                visitor(item);
            }
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries {
                visitor(key);
                visitor(value);
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries {
                for expr in entry.exprs() {
                    visitor(expr);
                }
            }
        }
        ExprKind::NamedArg { value, .. }
        | ExprKind::Spread(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Negate(value)
        | ExprKind::BitNot(value)
        | ExprKind::Not(value)
        | ExprKind::Clone(value)
        | ExprKind::Throw(value)
        | ExprKind::YieldFrom(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::IncludeValue { path: value, .. }
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::BufferNew { len: value, .. } => visitor(value),
        ExprKind::InstanceOf { value, target } => {
            visitor(value);
            if let crate::parser::ast::InstanceOfTarget::Expr(target) = target {
                visitor(target);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            visitor(array);
            visitor(index);
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. }
        | ExprKind::ObjectClassName { object } => visitor(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            visitor(object);
            visitor(property);
        }
        ExprKind::FirstClassCallable(crate::parser::ast::CallableTarget::Method {
            object,
            ..
        }) => visitor(object),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            visitor(subject);
            for (conditions, value) in arms {
                for condition in conditions {
                    visitor(condition);
                }
                visitor(value);
            }
            if let Some(default) = default {
                visitor(default);
            }
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            ..
        } => {
            visitor(target);
            visitor(value);
            if let Some(result_target) = result_target {
                visitor(result_target);
            }
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                visitor(key);
            }
            if let Some(value) = value {
                visitor(value);
            }
        }
        _ => {}
    }
}

/// Returns the local array name targeted by `array_push`'s first argument.
fn array_push_target_name(args: &[Expr]) -> Option<&str> {
    let first = args.first()?;
    match &call_arg_value(first).kind {
        ExprKind::Variable(name) => Some(name),
        _ => None,
    }
}

/// Unwraps a named call argument to its value expression.
fn call_arg_value(argument: &Expr) -> &Expr {
    match &argument.kind {
        ExprKind::NamedArg { value, .. } => value,
        _ => argument,
    }
}

/// Collects the plain locals a body ITERATES, so a growing empty array whose elements the loop
/// reads back can be told apart from one it only accumulates into.
///
/// Only `foreach` sources count. That is deliberately narrower than "every element read": a case
/// this misses keeps exactly today's behaviour, which is the same "only what can be answered
/// exactly" discipline the rest of this analysis follows, while a case it answers wrongly would
/// change a storage representation.
///
/// Nested function, closure and class bodies are separate scopes and are not descended into, the
/// same boundary [`visit_child_expressions`] observes.
fn collect_foreach_sources<'a>(statements: &'a [Stmt], out: &mut HashSet<&'a str>) {
    for statement in statements {
        if let StmtKind::Foreach { array, .. } = &statement.kind {
            if let ExprKind::Variable(name) = &array.kind {
                out.insert(name.as_str());
            }
        }
        for body in nested_statement_bodies(statement) {
            collect_foreach_sources(body, out);
        }
    }
}

/// Adds the locals the body READS as values, beyond the `foreach` sources: an `array_pop($stack)`,
/// a `$stack[$i]`, an `end($stack)` all reach the elements a previous iteration appended exactly
/// as a `foreach` does. Symfony's expression-language `Lexer::tokenize()` pushes onto `$brackets`
/// in one branch and pops it in another; with only `foreach` counted the stack stayed
/// `array<never>` at the loop header and every pop read null.
///
/// A write names its array by a plain local name (`$a[] = v`, `$a[k] = v`), not by a value
/// expression, so writes are not reads here. `count()`/`sizeof()` read only the length, which
/// does not depend on the element representation, so an accumulator measured inside its own loop
/// keeps its precise element type.
fn collect_element_read_sources<'a>(statements: &'a [Stmt], out: &mut HashSet<&'a str>) {
    use std::collections::HashMap;
    let mut reads: HashMap<&'a str, usize> = HashMap::new();
    let mut length_only: HashMap<&'a str, usize> = HashMap::new();
    crate::parser::ast::visit::walk_nodes(statements, &mut |node| {
        let crate::parser::ast::visit::Node::Expr(expr) = node else {
            return;
        };
        match &expr.kind {
            ExprKind::Variable(name) => *reads.entry(name.as_str()).or_default() += 1,
            ExprKind::FunctionCall { name, args }
                if name
                    .last_segment()
                    .is_some_and(|segment| {
                        segment.eq_ignore_ascii_case("count") || segment.eq_ignore_ascii_case("sizeof")
                    }) =>
            {
                if let Some(ExprKind::Variable(name)) = args.first().map(|arg| &arg.kind) {
                    *length_only.entry(name.as_str()).or_default() += 1;
                }
            }
            _ => {}
        }
    });
    for (name, count) in reads {
        if count > length_only.get(name).copied().unwrap_or(0) {
            out.insert(name);
        }
    }
}

/// Borrows every statement list nested directly in `statement`, stopping at scope boundaries.
fn nested_statement_bodies(statement: &Stmt) -> Vec<&[Stmt]> {
    let mut bodies: Vec<&[Stmt]> = Vec::new();
    match &statement.kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            bodies.push(then_body);
            bodies.extend(elseif_clauses.iter().map(|(_, body)| body.as_slice()));
            if let Some(body) = else_body {
                bodies.push(body);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            bodies.push(then_body);
            if let Some(body) = else_body {
                bodies.push(body);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body) => bodies.push(body),
        StmtKind::Switch { cases, default, .. } => {
            bodies.extend(cases.iter().map(|(_, body)| body.as_slice()));
            if let Some(body) = default {
                bodies.push(body);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            bodies.push(try_body);
            bodies.extend(catches.iter().map(|catch| catch.body.as_slice()));
            if let Some(body) = finally_body {
                bodies.push(body);
            }
        }
        _ => {}
    }
    bodies
}

#[cfg(test)]
mod tests {
    use super::{
        infer_storage_value_type, join_loop_flow_type, loop_carried_storage_types,
        representation_contract,
    };
    use crate::parser::ast::{CastType, Expr, ExprKind, Stmt, StmtKind};
    use crate::span::Span;
    use crate::types::{PhpType, TypeEnv};

    /// Builds an `(array)` cast expression for storage-inference fixtures.
    fn array_cast(inner: Expr) -> Expr {
        Expr::new(
            ExprKind::Cast {
                target: CastType::Array,
                expr: Box::new(inner),
            },
            Span::dummy(),
        )
    }

    /// Infers storage with the syntactic type standing in for the checker on literal sources.
    fn storage_type(expr: &Expr, env: &TypeEnv) -> PhpType {
        let mut infer = |expr: &Expr, _env: &TypeEnv| {
            Some(crate::types::checker::infer_expr_type_syntactic(expr))
        };
        infer_storage_value_type(expr, env, &mut infer).expect("literal storage")
    }

    /// An `(array)` cast used as a literal element keeps the cast's array, for both indexed
    /// and associative literals. The syntactic fallback used to answer `int` here.
    #[test]
    fn array_cast_literal_elements_keep_the_cast_result() {
        let string_cast = array_cast(Expr::string_lit("x"));
        let null_cast = array_cast(Expr::new(ExprKind::Null, Span::dummy()));
        let int_cast = array_cast(Expr::int_lit(1));
        assert_eq!(
            storage_type(
                &Expr::new(ExprKind::ArrayLiteral(vec![string_cast]), Span::dummy()),
                &TypeEnv::default(),
            ),
            PhpType::Array(Box::new(PhpType::Array(Box::new(PhpType::Str))))
        );
        assert_eq!(
            storage_type(
                &Expr::new(ExprKind::ArrayLiteral(vec![null_cast]), Span::dummy()),
                &TypeEnv::default(),
            ),
            PhpType::Array(Box::new(PhpType::Array(Box::new(PhpType::Never))))
        );
        let nested = Expr::new(
            ExprKind::ArrayLiteral(vec![
                array_cast(Expr::string_lit("x")),
                array_cast(Expr::new(ExprKind::Null, Span::dummy())),
            ]),
            Span::dummy(),
        );
        assert_eq!(
            storage_type(&nested, &TypeEnv::default()),
            PhpType::Array(Box::new(PhpType::Mixed))
        );
        assert_eq!(
            storage_type(
                &Expr::new(
                    ExprKind::ArrayLiteralAssoc(vec![(Expr::string_lit("k"), int_cast)]),
                    Span::dummy(),
                ),
                &TypeEnv::default(),
            ),
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Array(Box::new(PhpType::Int))),
            }
        );

        let mut env = TypeEnv::default();
        env.insert("arr".to_string(), PhpType::Array(Box::new(PhpType::Int)));
        env.insert("obj".to_string(), PhpType::Object("Box".to_string()));
        env.insert("mix".to_string(), PhpType::Mixed);
        assert_eq!(
            storage_type(
                &Expr::new(
                    ExprKind::ArrayLiteral(vec![array_cast(Expr::var("arr"))]),
                    Span::dummy(),
                ),
                &env,
            ),
            PhpType::Array(Box::new(PhpType::Array(Box::new(PhpType::Int))))
        );
        assert_eq!(
            storage_type(
                &Expr::new(
                    ExprKind::ArrayLiteralAssoc(vec![(
                        Expr::string_lit("k"),
                        array_cast(Expr::var("obj")),
                    )]),
                    Span::dummy(),
                ),
                &env,
            ),
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::AssocArray {
                    key: Box::new(PhpType::Str),
                    value: Box::new(PhpType::Mixed),
                }),
            }
        );
        assert_eq!(
            storage_type(
                &Expr::new(
                    ExprKind::ArrayLiteral(vec![array_cast(Expr::var("mix"))]),
                    Span::dummy(),
                ),
                &env,
            ),
            PhpType::Array(Box::new(PhpType::Mixed))
        );
        let object_cast = array_cast(Expr::new(
            ExprKind::Cast {
                target: CastType::Object,
                expr: Box::new(Expr::var("obj")),
            },
            Span::dummy(),
        ));
        assert_eq!(
            storage_type(
                &Expr::new(ExprKind::ArrayLiteral(vec![object_cast]), Span::dummy()),
                &env,
            ),
            PhpType::Array(Box::new(PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
            }))
        );
    }

    /// Joining concrete storage with a declared array preserves the logical array proof.
    #[test]
    fn declared_array_loop_joins_preserve_the_contract_in_both_orders() {
        for concrete in [
            PhpType::Array(Box::new(PhpType::Never)),
            PhpType::Array(Box::new(PhpType::Int)),
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Int),
            },
        ] {
            assert_eq!(
                join_loop_flow_type(concrete.clone(), PhpType::php_array()),
                PhpType::php_array(),
            );
            assert_eq!(
                join_loop_flow_type(PhpType::php_array(), concrete.clone()),
                PhpType::php_array(),
            );
            let contract = representation_contract(&concrete, &PhpType::php_array(), true, false)
                .expect("concrete entry storage must widen before the loop");
            assert!(contract.is_php_array());
            assert_eq!(contract.codegen_repr(), PhpType::Mixed);
        }
        assert_eq!(
            representation_contract(&PhpType::php_array(), &PhpType::php_array(), true, false),
            None,
        );
    }

    /// Boxed storage alone never proves that nullable, scalar or unknown values are arrays.
    #[test]
    fn declared_array_loop_joins_reject_non_array_evidence() {
        for non_array in [
            PhpType::Mixed,
            PhpType::Int,
            PhpType::Void,
            PhpType::Union(vec![PhpType::php_array(), PhpType::Void]),
        ] {
            assert_eq!(
                join_loop_flow_type(PhpType::php_array(), non_array.clone()),
                PhpType::Mixed,
            );
            assert_eq!(
                join_loop_flow_type(non_array, PhpType::php_array()),
                PhpType::Mixed,
            );
        }
    }

    /// `null` and pointers join to one pointer word on the loop lattice, and the join only moves
    /// up: different targets, or a target and an untyped `ptr`, stay the untyped `ptr`.
    #[test]
    fn null_and_pointer_loop_joins_keep_one_monotone_pointer_word() {
        let typed = |name: &str| PhpType::Pointer(Some(name.to_string()));
        let nullable = |pointer: PhpType| PhpType::Union(vec![pointer, PhpType::Void]);
        assert_eq!(join_loop_flow_type(PhpType::Void, typed("A")), nullable(typed("A")));
        assert_eq!(join_loop_flow_type(typed("A"), PhpType::Void), nullable(typed("A")));
        assert_eq!(join_loop_flow_type(nullable(typed("A")), typed("A")), nullable(typed("A")));
        assert_eq!(
            join_loop_flow_type(nullable(typed("A")), typed("B")),
            nullable(PhpType::Pointer(None)),
        );
        assert_eq!(
            join_loop_flow_type(nullable(PhpType::Pointer(None)), typed("A")),
            nullable(PhpType::Pointer(None)),
        );
        // A pointer and an int join to a semantic union (narrowing can recover each arm) whose
        // frame representation is still the one boxed Mixed slot.
        let mixed_join = join_loop_flow_type(typed("A"), PhpType::Int);
        assert_eq!(mixed_join, PhpType::Union(vec![typed("A"), PhpType::Int]));
        assert_eq!(mixed_join.codegen_repr(), PhpType::Mixed);
    }

    /// A local that enters a loop holding `null` and is assigned pointers to three different
    /// targets reaches a fixed point (a join that narrowed `ptr` back to a typed target made the
    /// analysis flip forever) and gets one untyped `?ptr` contract.
    #[test]
    fn null_entry_loop_over_three_pointer_targets_reaches_a_fixed_point() {
        let assign = |source: &str| {
            Stmt::new(
                StmtKind::Assign {
                    name: "p".to_string(),
                    value: Expr::var(source),
                },
                Span::dummy(),
            )
        };
        let body = vec![assign("a"), assign("b"), assign("c")];
        let mut entry = TypeEnv::default();
        entry.insert("p".to_string(), PhpType::Void);
        entry.insert("a".to_string(), PhpType::Pointer(Some("A".to_string())));
        entry.insert("b".to_string(), PhpType::Pointer(Some("B".to_string())));
        entry.insert("c".to_string(), PhpType::Pointer(Some("C".to_string())));
        let mut infer = |_expr: &Expr, _env: &TypeEnv| -> Option<PhpType> { None };
        let contracts = loop_carried_storage_types(&body, None, &entry, None, None, &mut infer);
        assert_eq!(
            contracts,
            vec![(
                "p".to_string(),
                PhpType::Union(vec![PhpType::Pointer(None), PhpType::Void]),
            )],
        );
    }
}
