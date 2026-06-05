//! Purpose:
//! Lowers associative array literals with normalized keys and runtime hash insertion.
//! Builds heap array values and leaves the resulting handle in expression result registers.
//!
//! Called from:
//! - `crate::codegen::expr::arrays`
//!
//! Key details:
//! - Literal emission must evaluate elements in source order and retain heap elements inserted into arrays.

use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::{abi, platform::Arch};
use super::super::{emit_expr, retain_borrowed_heap_arg, Expr, ExprKind, PhpType};

/// Emits an empty associative array literal with the given key/value types.
///
/// Allocates a hash table with initial capacity 16 via `__rt_hash_new`, using the
/// provided `key_ty` and `value_ty` to set the runtime value tag. Leaves the hash
/// handle in the result register. Returns `PhpType::AssocArray` with the given
/// key and value types boxed.
pub(crate) fn emit_empty_assoc_array_literal(
    key_ty: PhpType,
    value_ty: PhpType,
    emitter: &mut Emitter,
) -> PhpType {
    emitter.comment("empty assoc array literal");
    let capacity_reg = abi::int_arg_reg_name(emitter.target, 0);
    let value_tag_reg = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_int_immediate(emitter, capacity_reg, 16);
    abi::emit_load_int_immediate(
        emitter,
        value_tag_reg,
        super::super::super::runtime_value_tag(&value_ty.codegen_repr()) as i64,
    );
    abi::emit_call_label(emitter, "__rt_hash_new");
    PhpType::AssocArray {
        key: Box::new(key_ty),
        value: Box::new(value_ty),
    }
}

/// Rewrites a bare array literal as an `AssocArray` value when the target type is
/// associative (`T[string]:V`). An empty `[]` becomes an empty hash; a positional
/// literal `[a, b, ...]` becomes explicit `0 => a, 1 => b, ...` pairs so the
/// associative emitter normalizes the keys into hash storage. Returns `None` when
/// `value` is not an array literal or `target_ty` is not associative, so the caller
/// falls back to ordinary expression emission. Shared by property assignment and
/// object-allocation default initialization so an `[]` default whose refined type is
/// associative is stored as hash storage rather than an indexed-list array.
pub(crate) fn emit_array_literal_as_assoc_target(
    value: &Expr,
    target_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let ExprKind::ArrayLiteral(elems) = &value.kind else {
        return None;
    };
    let PhpType::AssocArray {
        key: target_key_ty,
        value: target_value_ty,
    } = target_ty
    else {
        return None;
    };
    if elems.is_empty() {
        return Some(emit_empty_assoc_array_literal(
            *target_key_ty.clone(),
            *target_value_ty.clone(),
            emitter,
        ));
    }
    let pairs: Vec<(Expr, Expr)> = elems
        .iter()
        .enumerate()
        .map(|(idx, elem)| {
            (
                Expr::new(ExprKind::IntLiteral(idx as i64), elem.span),
                elem.clone(),
            )
        })
        .collect();
    Some(emit_assoc_array_literal(&pairs, emitter, ctx, data))
}

/// Emits a non-empty associative array literal with key/value expression pairs.
///
/// Allocates a hash table via `__rt_hash_new`, then inserts each key/value pair
/// in source order via `__rt_hash_set`. Keys are emitted as normalized hash-key
/// payloads before each value expression is evaluated and inserted. Persists the
/// updated hash table pointer after each insertion (allowing the table to grow).
/// Returns `PhpType::AssocArray` with the normalized key type and the merged value
/// type (uses `PhpType::Mixed` when value types differ across pairs).
pub(crate) fn emit_assoc_array_literal(
    pairs: &[(Expr, Expr)],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("assoc array literal");
    let result_reg = abi::int_result_reg(emitter);
    let stack_reg = match emitter.target.arch {
        Arch::AArch64 => "sp",
        Arch::X86_64 => "rsp",
    };
    let hash_capacity_reg = abi::int_arg_reg_name(emitter.target, 0);
    let key_ptr_reg = abi::int_arg_reg_name(emitter.target, 1);
    let key_len_reg = abi::int_arg_reg_name(emitter.target, 2);
    let value_lo_reg = abi::int_arg_reg_name(emitter.target, 3);
    let value_hi_reg = abi::int_arg_reg_name(emitter.target, 4);
    let value_tag_reg = abi::int_arg_reg_name(emitter.target, 5);
    let tag_reg = abi::int_arg_reg_name(emitter.target, 1);
    let float_bits_reg = abi::temp_int_reg(emitter.target);
    let zero_reg = match emitter.target.arch {
        Arch::AArch64 => "xzr",
        Arch::X86_64 => "0",
    };
    let (string_ptr_reg, string_len_reg) = abi::string_result_regs(emitter);

    let first_value_ty = super::super::super::functions::infer_contextual_type(&pairs[0].1, ctx);
    let header_value_ty = if matches!(first_value_ty, PhpType::Iterable) {
        PhpType::Mixed
    } else {
        first_value_ty
    };
    let value_type_tag = super::super::super::runtime_value_tag(&header_value_ty);

    abi::emit_load_int_immediate(
        emitter,
        hash_capacity_reg,
        std::cmp::max(pairs.len() * 2, 16) as i64,
    );
    abi::emit_load_int_immediate(emitter, tag_reg, value_type_tag as i64);
    abi::emit_call_label(emitter, "__rt_hash_new");
    abi::emit_push_reg(emitter, result_reg);                                    // save the hash table pointer while key/value pairs are inserted

    let mut val_ty = PhpType::Int;
    for (i, pair) in pairs.iter().enumerate() {
        super::super::super::emit_normalized_hash_key(&pair.0, emitter, ctx, data);
        abi::emit_push_reg_pair(emitter, string_ptr_reg, string_len_reg);        // save the assoc-array key payload while the value expression is emitted
        let mut ty = emit_expr(&pair.1, emitter, ctx, data);
        let boxed_iterable =
            crate::codegen::emit_box_iterable_value_for_mixed_container(emitter, &mut ty);
        if !boxed_iterable {
            retain_borrowed_heap_arg(emitter, &pair.1, &ty);
        }
        if i == 0 {
            val_ty = ty.clone();
        } else if ty != val_ty {
            val_ty = PhpType::Mixed;
        }
        let (val_lo, val_hi) = match &ty {
            PhpType::Int | PhpType::Bool => (result_reg, zero_reg),
            PhpType::Str => {
                abi::emit_call_label(emitter, "__rt_str_persist");              // copy the borrowed string result into owned heap storage
                (string_ptr_reg, string_len_reg)
            }
            PhpType::Float => {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction(&format!("fmov {}, {}", float_bits_reg, abi::float_result_reg(emitter))); // move the float bits into an integer scratch register for hash insertion
                    }
                    Arch::X86_64 => {
                        emitter.instruction(&format!("movq {}, {}", float_bits_reg, abi::float_result_reg(emitter))); // move the float bits into an integer scratch register for hash insertion
                    }
                }
                (float_bits_reg, zero_reg)
            }
            _ => (result_reg, zero_reg),
        };
        emitter.instruction(&format!("mov {}, {}", value_lo_reg, val_lo));      // move the low payload word into the hash-set value register
        emitter.instruction(&format!("mov {}, {}", value_hi_reg, val_hi));      // move the high payload word into the hash-set value register
        abi::emit_load_int_immediate(
            emitter,
            value_tag_reg,
            super::super::super::runtime_value_tag(&ty) as i64,
        );
        abi::emit_pop_reg_pair(emitter, key_ptr_reg, key_len_reg);              // restore the assoc-array key payload into the hash-set argument registers
        abi::emit_load_from_address(emitter, hash_capacity_reg, stack_reg, 0);  // reload the current hash table pointer before insertion
        abi::emit_call_label(emitter, "__rt_hash_set");
        abi::emit_store_to_address(emitter, result_reg, stack_reg, 0);          // persist the updated hash table pointer after possible growth
    }

    abi::emit_pop_reg(emitter, result_reg);                                     // restore the completed hash table pointer as the expression result

    let mut key_ty = normalized_assoc_key_type(&pairs[0].0, ctx);
    for (key, _) in pairs.iter().skip(1) {
        let next_ty = normalized_assoc_key_type(key, ctx);
        if next_ty != key_ty {
            key_ty = PhpType::Mixed;
            break;
        }
    }

    PhpType::AssocArray {
        key: Box::new(key_ty),
        value: Box::new(val_ty),
    }
}

/// Emits an associative array literal with spread elements from other arrays.
///
/// Creates an empty hash table with `PhpType::Mixed` keys, then merges each spread
/// operand in source order using `__rt_hash_union` (for assoc arrays) or
/// `__rt_hash_array_union` (for indexed arrays). The merged hash handle is left in
/// the result register. Returns `PhpType::AssocArray` with mixed keys and the
/// inferred value type from the spread elements.
pub(crate) fn emit_array_literal_with_assoc_spread(
    elems: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("assoc array literal with spread");
    let result_reg = abi::int_result_reg(emitter);
    let value_ty = assoc_spread_literal_value_type(elems, ctx);
    emit_empty_assoc_array_literal(PhpType::Mixed, value_ty.clone(), emitter);
    abi::emit_push_reg(emitter, result_reg);                                    // save the merged hash while source-order spread operands are evaluated

    for elem in elems {
        let elem_ty = match &elem.kind {
            ExprKind::Spread(inner) => emit_expr(inner, emitter, ctx, data),
            _ => continue,
        };
        let helper = match elem_ty {
            PhpType::AssocArray { .. } => "__rt_hash_union",
            PhpType::Array(_) => "__rt_hash_array_union",
            _ => continue,
        };
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x1, x0");                              // pass the next spread array as the right merge operand
                abi::emit_pop_reg(emitter, "x0");                               // restore the accumulated named-prefix hash as the left operand
            }
            Arch::X86_64 => {
                emitter.instruction("mov rsi, rax");                            // pass the next spread array as the right merge operand
                abi::emit_pop_reg(emitter, "rdi");                              // restore the accumulated named-prefix hash as the left operand
            }
        }
        abi::emit_call_label(emitter, helper);                                  // merge this spread operand into the named-prefix hash
        abi::emit_push_reg(emitter, result_reg);                                // keep the updated hash available for the next spread operand
    }

    abi::emit_pop_reg(emitter, result_reg);                                     // restore the completed named-prefix hash as the expression result
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(value_ty),
    }
}

/// Infers the value type for an associative array literal with spread elements.
///
/// Iterates over spread expressions in `elems`, extracting the value type from
/// each `PhpType::Array` or `PhpType::AssocArray`. Returns the common value type
/// if all spreads agree; otherwise returns `PhpType::Mixed`. Falls back to
/// `PhpType::Mixed` when no spread elements are found.
fn assoc_spread_literal_value_type(elems: &[Expr], ctx: &Context) -> PhpType {
    let mut value_ty = PhpType::Never;
    for elem in elems {
        let ExprKind::Spread(inner) = &elem.kind else {
            continue;
        };
        let next = match super::super::super::functions::infer_contextual_type(inner, ctx) {
            PhpType::Array(elem) => *elem,
            PhpType::AssocArray { value, .. } => *value,
            _ => PhpType::Mixed,
        };
        if matches!(value_ty, PhpType::Never) {
            value_ty = next;
        } else if value_ty != next {
            value_ty = PhpType::Mixed;
        }
    }
    if matches!(value_ty, PhpType::Never) {
        PhpType::Mixed
    } else {
        value_ty
    }
}

/// Normalizes the key expression type for an associative array element.
///
/// Infers the raw type of `key` from `ctx`, then applies array-key normalization
/// via `crate::types::normalized_array_key_type` to produce a canonical key type.
fn normalized_assoc_key_type(key: &Expr, ctx: &Context) -> PhpType {
    let raw_ty = super::super::super::functions::infer_contextual_type(key, ctx);
    crate::types::normalized_array_key_type(key, raw_ty)
}
