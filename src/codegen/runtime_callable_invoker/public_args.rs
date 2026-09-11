//! Purpose:
//! Describes how a descriptor invoker binds a boxed argument container onto a physical
//! `FunctionSig`, including the hidden `func_num_args()` / `func_get_args()` slots added by
//! `crate::func_args`.
//!
//! Called from:
//! - The indexed and associative argument builders in `super`.
//!
//! Key details:
//! - `InvokerArgMode` is a compile-time selector and is independent of the native-throw
//!   boundary flag. It adds no word to the two-word descriptor invoker ABI.
//! - `PublicRaw` containers hold exactly the arguments PHP supplied. Visible regulars are
//!   bound from the container; `__elephc_func_argc` is synthesized; a hidden collector that
//!   needs optional-count metadata receives that count as its first element.
//! - `EvalPrebound` keeps the previous physical layout, so eval-registered native free
//!   functions keep today's binding until their separate metadata task.
//! - Receiver and capture values stay on the descriptor capture list; they are not mixed
//!   into the public argument-shape counters.

use super::{abi, Arch, DataSection, Emitter, FunctionSig, InvokerEmitContext, PhpType};

/// How an invoker must interpret the boxed argument container it receives.
///
/// This is a compile-time selector only: it adds no word to the runtime descriptor ABI.
/// It is deliberately independent of the native-throw boundary flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InvokerArgMode {
    /// Public first-class-callable / `call_user_func` / `call_user_func_array` containers.
    PublicRaw,
    /// Eval-registered native free-function descriptors, whose containers are packed against
    /// physical parameters by Magician before the invoker runs.
    EvalPrebound,
}

/// One callable signature's binding layout for a given container mode.
pub(super) struct InvokerParamShape {
    /// Regular parameters a public caller may bind, positionally or by name.
    pub(super) visible_regular: usize,
    /// Physical parameters preceding the variadic slot, including hidden ones.
    physical_regular: usize,
    /// A hidden `__elephc_func_argc` slot must be synthesized after the visible regulars.
    pub(super) hidden_argc: bool,
    /// The hidden collector must begin with the actual PHP argument count.
    pub(super) collector_needs_count: bool,
}

impl InvokerParamShape {
    /// Describes how one signature binds a container of the given mode.
    ///
    /// `EvalPrebound` returns the physical layout used before the public/eval split.
    /// `PublicRaw` uses `FunctionSig` / `func_args` helpers for the visible prefix and
    /// hidden slots. A malformed hidden-argc placement does not fall back to treating
    /// hidden fields as public; the visible prefix from `regular_param_count` still
    /// governs container binding, and the expected post-visible slot is synthesized.
    pub(super) fn of(sig: &FunctionSig, mode: InvokerArgMode) -> Self {
        let physical_regular = if sig.variadic.is_some() {
            sig.params.len().saturating_sub(1)
        } else {
            sig.params.len()
        };
        if mode == InvokerArgMode::EvalPrebound {
            return Self {
                visible_regular: physical_regular,
                physical_regular,
                hidden_argc: false,
                collector_needs_count: false,
            };
        }
        let visible_regular = crate::types::call_args::regular_param_count(sig);
        let hidden_argc = crate::func_args::sig_has_hidden_argc_param(sig);
        if hidden_argc {
            debug_assert_eq!(
                sig.params.get(visible_regular).map(|(name, _)| name.as_str()),
                Some(crate::func_args::HIDDEN_ARGC_PARAM),
                "hidden argc must follow the visible regular parameters"
            );
            debug_assert!(
                sig.variadic.is_some(),
                "hidden argc is only appended beside a source variadic"
            );
        }
        Self {
            visible_regular,
            physical_regular,
            hidden_argc,
            collector_needs_count: crate::func_args::sig_collects_optional_arg_count(sig),
        }
    }

    /// Returns the PHYSICAL owner index of the variadic slot, which hidden slots still shift.
    pub(super) fn variadic_owner_index(&self) -> usize {
        self.physical_regular
    }

    /// Returns how many collector entries precede the copied argument tail.
    pub(super) fn collector_prefix(&self) -> usize {
        usize::from(self.collector_needs_count)
    }

    /// Returns whether this container's actual PHP argument count must be materialized.
    pub(super) fn needs_actual_count(&self) -> bool {
        self.hidden_argc || self.collector_needs_count
    }
}

/// Returns the callee-saved register holding an associative container's actual argument count.
pub(super) fn assoc_arg_count_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x21",
        Arch::X86_64 => "r14",
    }
}

/// Pushes an already computed actual argument count as the hidden `__elephc_func_argc` argument.
pub(super) fn push_arg_count_from_reg(emitter: &mut Emitter, count_reg: &str) {
    abi::emit_reg_move(emitter, abi::int_result_reg(emitter), count_reg);
    abi::emit_push_result_value(emitter, &PhpType::Int);
}

/// Clamps a computed variadic tail count to zero when fewer arguments than regulars arrived.
///
/// Only the hidden collector reaches this: it is allocated unconditionally because it must hold
/// the count even when every optional parameter was omitted, so its tail count can go negative.
pub(super) fn clamp_tail_count_to_zero(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    count_reg: &str,
) {
    let done_label = ctx.next_label("invoker_tail_count_ok");
    super::emit_compare_len_ge(emitter, count_reg, 0, &done_label);
    abi::emit_load_int_immediate(emitter, count_reg, 0);
    emitter.label(&done_label);
}

/// Writes the actual argument count into an indexed hidden collector's first slot.
///
/// The array pointer is reloaded from the pushed temporary slot because boxing the count calls a
/// runtime helper that clobbers the caller-saved scratch register holding it.
#[allow(clippy::too_many_arguments)]
pub(super) fn store_indexed_count_prefix(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
    count_reg: &str,
    array_reg: &str,
    len_store_reg: &str,
    offset_reg: &str,
    index_reg: &str,
    elem_ty: &PhpType,
) {
    abi::emit_reg_move(emitter, abi::int_result_reg(emitter), count_reg);
    // A boxed count transfers its fresh owner into the collector, exactly like the tail loop.
    let (stored_ty, _) =
        super::coerce_current_value_to_target(emitter, ctx, data, &PhpType::Int, Some(elem_ty));
    abi::emit_load_temporary_stack_slot(emitter, array_reg, 0);
    abi::emit_load_int_immediate(emitter, index_reg, 0);
    super::emit_store_current_value_to_array_slot(
        emitter,
        &stored_ty,
        array_reg,
        len_store_reg,
        offset_reg,
        index_reg,
    );
    abi::emit_load_int_immediate(emitter, index_reg, 1);
    abi::emit_store_to_address(emitter, index_reg, array_reg, 0);
}

/// Inserts the actual argument count as the associative collector's first entry, at key zero.
pub(super) fn insert_assoc_count_prefix(
    emitter: &mut Emitter,
    count_reg: &str,
    elem_ty: &PhpType,
) {
    let value_lo = abi::int_arg_reg_name(emitter.target, 3);
    let value_hi = abi::int_arg_reg_name(emitter.target, 4);
    let value_tag = abi::int_arg_reg_name(emitter.target, 5);
    if matches!(elem_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_reg_move(emitter, abi::int_result_reg(emitter), count_reg);
        crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Int);
        abi::emit_reg_move(emitter, value_lo, abi::int_result_reg(emitter));
        abi::emit_load_int_immediate(emitter, value_hi, 0);
        abi::emit_load_int_immediate(
            emitter,
            value_tag,
            crate::codegen::runtime_value_tag(&PhpType::Mixed) as i64,
        );
    } else {
        abi::emit_reg_move(emitter, value_lo, count_reg);
        abi::emit_load_int_immediate(emitter, value_hi, 0);
        abi::emit_load_int_immediate(
            emitter,
            value_tag,
            crate::codegen::runtime_value_tag(&PhpType::Int) as i64,
        );
    }
    abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 0), 0);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 1), 0);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 2), -1);
    abi::emit_call_label(emitter, "__rt_hash_set");
    match emitter.target.arch {
        Arch::AArch64 => abi::emit_store_to_address(emitter, "x0", "sp", 0),
        Arch::X86_64 => abi::emit_store_to_address(emitter, "rax", "rsp", 0),
    }
}

/// Computes the PHP argument count an associative public container represents.
///
/// PHP fills the gap before the greatest supplied regular parameter with defaults and reports
/// that slot's index plus one, then adds the numeric surplus. Unknown named entries, which only
/// a source-declared variadic can absorb, are not arguments for `func_num_args()` purposes.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_assoc_actual_arg_count(
    hash_reg: &str,
    sig: &FunctionSig,
    shape: &InvokerParamShape,
    count_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) {
    abi::emit_load_int_immediate(emitter, count_reg, 0);
    for index in 0..shape.visible_regular {
        let missing_label = ctx.next_label("invoker_assoc_count_missing");
        super::emit_hash_lookup_for_param_or_index(
            hash_reg,
            sig.params.get(index).map(|(name, _)| name.as_str()),
            index,
            emitter,
            ctx,
            data,
        );
        abi::emit_branch_if_int_result_zero(emitter, &missing_label);
        // Indices rise monotonically, so the last supplied one is the greatest one.
        abi::emit_load_int_immediate(emitter, count_reg, (index + 1) as i64);
        emitter.label(&missing_label);
    }
    emit_add_numeric_surplus_count(hash_reg, shape.visible_regular, count_reg, emitter, ctx);
}

/// Adds every numeric container key past the visible regulars to the running argument count.
fn emit_add_numeric_surplus_count(
    hash_reg: &str,
    skip_numeric_before: usize,
    count_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
) {
    const SCRATCH_BYTES: usize = 16;
    const CURSOR_OFF: usize = 0;
    const SOURCE_HASH_OFF: usize = 8;

    let loop_label = ctx.next_label("invoker_assoc_surplus_loop");
    let numeric_label = ctx.next_label("invoker_assoc_surplus_numeric");
    let count_label = ctx.next_label("invoker_assoc_surplus_count");
    let next_label = ctx.next_label("invoker_assoc_surplus_next");
    let done_label = ctx.next_label("invoker_assoc_surplus_done");
    let (cursor_reg, key_reg, key_len_reg, stack_reg) = match emitter.target.arch {
        Arch::AArch64 => ("x0", "x1", "x2", "sp"),
        Arch::X86_64 => ("rax", "rdi", "rdx", "rsp"),
    };

    // -- seed a private iterator over the public argument hash --
    abi::emit_reserve_temporary_stack(emitter, SCRATCH_BYTES);
    abi::emit_store_to_address(emitter, hash_reg, stack_reg, SOURCE_HASH_OFF);
    abi::emit_store_zero_to_address(emitter, stack_reg, CURSOR_OFF);

    // -- count numeric keys past the visible regular parameters --
    emitter.label(&loop_label);
    abi::emit_load_temporary_stack_slot(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        SOURCE_HASH_OFF,
    );
    abi::emit_load_temporary_stack_slot(
        emitter,
        abi::int_arg_reg_name(emitter.target, 1),
        CURSOR_OFF,
    );
    abi::emit_call_label(emitter, "__rt_hash_iter_next");
    emit_branch_if_reg_is_minus_one(emitter, cursor_reg, &done_label);
    abi::emit_store_to_address(emitter, cursor_reg, stack_reg, CURSOR_OFF);
    emit_branch_if_reg_is_minus_one(emitter, key_len_reg, &numeric_label);
    abi::emit_jump(emitter, &next_label);
    emitter.label(&numeric_label);
    super::emit_compare_len_ge(emitter, key_reg, skip_numeric_before, &count_label);
    abi::emit_jump(emitter, &next_label);
    emitter.label(&count_label);
    super::emit_increment_reg(emitter, count_reg);
    emitter.label(&next_label);
    abi::emit_jump(emitter, &loop_label);
    emitter.label(&done_label);
    abi::emit_release_temporary_stack(emitter, SCRATCH_BYTES);
}

/// Branches when a register holds the -1 sentinel used for numeric keys and iterator ends.
fn emit_branch_if_reg_is_minus_one(emitter: &mut Emitter, reg: &str, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmn {}, #1", reg));                   // compare the register against the -1 sentinel
            emitter.instruction(&format!("b.eq {}", label));                    // take the sentinel path when it matches
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, -1", reg));                   // compare the register against the -1 sentinel
            emitter.instruction(&format!("je {}", label));                      // take the sentinel path when it matches
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a signature whose hidden slots mirror what `crate::func_args` appends.
    fn sig(
        params: Vec<(&str, PhpType)>,
        defaults: Vec<Option<i64>>,
        variadic: Option<&str>,
    ) -> FunctionSig {
        let count = params.len();
        FunctionSig {
            params: params
                .into_iter()
                .map(|(name, ty)| (name.to_string(), ty))
                .collect(),
            param_type_exprs: vec![None; count],
            param_attributes: vec![Vec::new(); count],
            defaults: defaults
                .into_iter()
                .map(|value| {
                    value.map(|value| {
                        crate::parser::ast::Expr::new(
                            crate::parser::ast::ExprKind::IntLiteral(value),
                            crate::span::Span::dummy(),
                        )
                    })
                })
                .collect(),
            return_type: PhpType::Mixed,
            declared_return: false,
            by_ref_return: false,
            ref_params: vec![false; count],
            declared_params: vec![false; count],
            variadic: variadic.map(str::to_string),
            deprecation: None,
        }
    }

    /// A hidden count slot is synthesized for public containers and bound for eval containers.
    #[test]
    fn hidden_argc_slot_is_public_only() {
        let sig = sig(
            vec![
                ("a", PhpType::Int),
                (crate::func_args::HIDDEN_ARGC_PARAM, PhpType::Int),
                ("rest", PhpType::Array(Box::new(PhpType::Mixed))),
            ],
            vec![Some(10), Some(0), None],
            Some("rest"),
        );
        let public = InvokerParamShape::of(&sig, InvokerArgMode::PublicRaw);
        assert_eq!(public.visible_regular, 1);
        assert!(public.hidden_argc);
        assert!(!public.collector_needs_count);
        assert_eq!(public.variadic_owner_index(), 2);
        let eval = InvokerParamShape::of(&sig, InvokerArgMode::EvalPrebound);
        assert_eq!(eval.visible_regular, 2);
        assert!(!eval.hidden_argc);
        assert_eq!(eval.variadic_owner_index(), 2);
    }

    /// An optional-parameter hidden collector reports its mandatory count prefix.
    #[test]
    fn hidden_collector_with_optional_regulars_needs_a_count_prefix() {
        let sig = sig(
            vec![
                ("a", PhpType::Int),
                (
                    crate::func_args::HIDDEN_ARGS_PARAM,
                    PhpType::Array(Box::new(PhpType::Mixed)),
                ),
            ],
            vec![Some(10), None],
            Some(crate::func_args::HIDDEN_ARGS_PARAM),
        );
        let public = InvokerParamShape::of(&sig, InvokerArgMode::PublicRaw);
        assert_eq!(public.visible_regular, 1);
        assert!(public.collector_needs_count);
        assert_eq!(public.collector_prefix(), 1);
        assert!(!InvokerParamShape::of(&sig, InvokerArgMode::EvalPrebound).collector_needs_count);
    }

    /// A plain signature binds identically in both modes, so ordinary callables cannot regress.
    #[test]
    fn ordinary_signatures_are_mode_independent() {
        let plain = sig(
            vec![("a", PhpType::Int), ("b", PhpType::Str)],
            vec![None, None],
            None,
        );
        for mode in [InvokerArgMode::PublicRaw, InvokerArgMode::EvalPrebound] {
            let shape = InvokerParamShape::of(&plain, mode);
            assert_eq!(shape.visible_regular, 2);
            assert!(!shape.needs_actual_count());
            assert_eq!(shape.collector_prefix(), 0);
        }
    }

}
