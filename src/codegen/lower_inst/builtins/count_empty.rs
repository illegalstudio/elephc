//! Purpose:
//! Lowers count, closure binding, and PHP empty semantics for concrete runtime representations.
//!
//! Called from:
//! - `super` builtin and language-construct dispatch.
//!
//! Key details:
//! - Handles sentinel null markers and target-specific float truthiness without changing representation contracts.

use super::*;

/// PHP's `count()` TypeError, which names the offending type — and a boolean by its VALUE.
const COUNT_TYPE_ERROR_PREFIX: &str =
    "count(): Argument #1 ($value) must be of type Countable|array, ";

/// Raises PHP's `count()` TypeError unless the boxed value in the int result register is
/// countable, and returns to its caller when it is.
///
/// `__rt_mixed_count` answered 0 for every non-countable tag and let execution continue, where
/// PHP 8 raises a TypeError and stops — measured, `count(false)` is fatal there and was `0`
/// here. The quiet return dates from PHP 7.2's warning. The checker hid most of it by refusing
/// a union unless EVERY member is countable, which is why that rule could not be relaxed to
/// give `file()` its `array|false` return type: relaxing it without this would have spread the
/// silent zero rather than removed it.
///
/// A tag 6 (object) is left to `__rt_mixed_count`. PHP also throws for an object that does not
/// implement `Countable`; deciding that needs the interface check at run time and is not done
/// here, so objects keep exactly the behaviour they had.
///
/// This reads its argument from the result register rather than an operand so the SAME body
/// serves the inline site and the shared helper — see `crate::codegen::shared_count_guard`,
/// which exists because the first version of this guard was inlined and cost 292 lines of
/// assembly at every site.
pub(in crate::codegen) fn emit_count_countable_guard_from_result(
    ctx: &mut FunctionContext<'_>,
) -> Result<()> {
    let countable = ctx.next_label("count_countable");
    let int_case = ctx.next_label("count_type_int");
    let string_case = ctx.next_label("count_type_string");
    let float_case = ctx.next_label("count_type_float");
    let bool_case = ctx.next_label("count_type_bool");
    let true_case = ctx.next_label("count_type_true");
    let resource_case = ctx.next_label("count_type_resource");

    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    for tag in [4u8, 5, 6] {
        super::scalar_metadata::emit_branch_on_gettype_mixed_tag(ctx, tag, &countable);
    }
    super::scalar_metadata::emit_branch_on_gettype_mixed_tag(ctx, 0, &int_case);
    super::scalar_metadata::emit_branch_on_gettype_mixed_tag(ctx, 1, &string_case);
    super::scalar_metadata::emit_branch_on_gettype_mixed_tag(ctx, 2, &float_case);
    super::scalar_metadata::emit_branch_on_gettype_mixed_tag(ctx, 3, &bool_case);
    super::scalar_metadata::emit_branch_on_gettype_mixed_tag(ctx, 9, &resource_case);
    // Every remaining tag is PHP's null.
    emit_count_type_error(ctx, "null");

    ctx.emitter.label(&int_case);
    emit_count_type_error(ctx, "int");
    ctx.emitter.label(&string_case);
    emit_count_type_error(ctx, "string");
    ctx.emitter.label(&float_case);
    emit_count_type_error(ctx, "float");
    ctx.emitter.label(&resource_case);
    emit_count_type_error(ctx, "resource");

    // PHP prints a boolean by value: "false given" / "true given", never "bool given".
    ctx.emitter.label(&bool_case);
    let payload = match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    };
    abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), payload); // unbox left value_lo here
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &true_case);
    emit_count_type_error(ctx, "false");
    ctx.emitter.label(&true_case);
    emit_count_type_error(ctx, "true");

    ctx.emitter.label(&countable);
    Ok(())
}

/// Raises the `count()` TypeError naming `type_name`, exactly as php-src words it.
fn emit_count_type_error(ctx: &mut FunctionContext<'_>, type_name: &str) {
    super::exceptions::emit_type_error(ctx, &format!("{COUNT_TYPE_ERROR_PREFIX}{type_name} given"));
}

/// Guards one `count()` site, through the shared helper when the module emits one.
fn emit_count_countable_guard(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    ctx.load_value_to_result(value)?;
    match crate::codegen::shared_count_guard::shared_guard_label(ctx) {
        Some(label) => {
            abi::emit_call_label(ctx.emitter, label);
            Ok(())
        }
        None => emit_count_countable_guard_from_result(ctx),
    }
}

/// Lowers `count(array)` for concrete array values by reading the runtime length header.
///
/// Called from `crate::builtins::array::count` (the registry home) via a thin wrapper.
/// Handles Array/AssocArray (reads length directly from the runtime header), Mixed/Union
/// (delegates to `__rt_mixed_count`), and Countable Object (calls the object's `count`
/// method via intrinsic or dynamic dispatch).
pub(crate) fn lower_count(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "count", 1, 2)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.value_php_type(value)?.codegen_repr();
    if inst.operands.len() == 2 {
        require_recursive_count_is_flat(&ty)?;
        emit_count_mode_guard(ctx, expect_operand(inst, 1)?)?;
    }
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            ctx.load_value_to_result(value)?;
            let result_reg = abi::int_result_reg(ctx.emitter);
            let null_label = ctx.next_label("count_null_container");
            let done_label = ctx.next_label("count_done");
            let scratch_reg = abi::secondary_scratch_reg(ctx.emitter);
            crate::codegen::sentinels::emit_branch_if_null_container(
                ctx.emitter,
                result_reg,
                scratch_reg,
                &null_label,
            );
            abi::emit_load_from_address(ctx.emitter, result_reg, result_reg, 0);
            abi::emit_jump(ctx.emitter, &done_label);
            ctx.emitter.label(&null_label);
            super::exceptions::emit_type_error(
                ctx,
                "count(): Argument #1 ($value) must be of type Countable|array, null given",
            );
            ctx.emitter.label(&done_label);
            store_if_result(ctx, inst)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_count_countable_guard(ctx, value)?;
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_count");
            store_if_result(ctx, inst)
        }
        PhpType::Object(class_name)
            if super::class_implements_interface(ctx, &class_name, "Countable") =>
        {
            if let Some(intrinsic) = super::runtime_backed_instance_intrinsic(&class_name, "count") {
                super::lower_instance_runtime_intrinsic(ctx, inst, &class_name, "count", intrinsic)
            } else {
                super::lower_runtime_object_method_call(ctx, inst, &class_name, "count")
            }
        }
        other => Err(CodegenIrError::unsupported(format!(
            "count for PHP type {:?}",
            other
        ))),
    }
}

/// Lowers the synthetic `closure_bind` call: rebinds a closure's captured
/// `$this` to a new receiver via `__rt_closure_bind(descriptor, new_this)`,
/// returning the rebound closure descriptor.
pub(in crate::codegen::lower_inst) fn lower_closure_bind(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "closure_bind", 2)?;
    let descriptor = expect_operand(inst, 0)?;
    let new_this = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(descriptor, "x0")?;
            ctx.load_value_to_reg(new_this, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(descriptor, "rdi")?;
            ctx.load_value_to_reg(new_this, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_closure_bind");
    store_if_result(ctx, inst)
}

/// Lowers `empty()` for concrete scalar and array-like operands.
pub(in crate::codegen::lower_inst) fn lower_empty(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "empty", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.raw_value_php_type(value)? {
        PhpType::Int | PhpType::Pointer(_) => {
            ctx.load_value_to_result(value)?;
            emit_int_result_zero_bool(ctx);
        }
        // Sentinel-marked slots; see `emit_sentinel_null_or_zero_empty_bool`.
        ty @ (PhpType::Bool | PhpType::False | PhpType::Float) => {
            emit_sentinel_null_or_zero_empty_bool(ctx, value, ty == PhpType::Float)?;
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            emit_string_length_zero_bool(ctx);
        }
        PhpType::TaggedScalar => {
            emit_tagged_scalar_empty_bool(ctx, value)?;
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            predicates::emit_array_truthiness(ctx, value)?;
            invert_bool_result(ctx);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_is_empty");
        }
        PhpType::Callable | PhpType::Object(_) | PhpType::Resource(_) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "empty for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Emits `empty()` for an unboxed scalar slot whose miss marker is the in-band
/// `NULL_SENTINEL`: true when the payload carries the sentinel (PHP null is empty), otherwise
/// the ordinary zero test for the slot's own type.
///
/// `bool` and `float` element slots have no tagged representation, so a silent missed read
/// leaves the sentinel word behind (`emit_float_null_sentinel` for floats). Testing the
/// payload alone answered `false` for a missing `bool` element — the sentinel is a non-zero
/// integer — and would answer `false` for a missing `float` element too, since its marker is a
/// NaN rather than `0.0`. A genuine `bool` is only ever 0 or 1 and the float marker's bit
/// pattern is unreachable by arithmetic, so neither check misfires on a real value.
///
/// `is_float` selects where the payload lives: the float result register (compared on raw
/// bits, because a float compare reports the sentinel NaN as unordered rather than equal) or
/// the integer result register.
pub(in crate::codegen::lower_inst) fn emit_sentinel_null_or_zero_empty_bool(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    is_float: bool,
) -> Result<()> {
    let empty_label = ctx.next_label("empty_sentinel_true");
    let done_label = ctx.next_label("empty_sentinel_done");
    ctx.load_value_to_result(value)?;
    if is_float {
        crate::codegen::sentinels::emit_float_result_bits_to_int_result(ctx.emitter);
    }
    let sentinel_reg = abi::secondary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, sentinel_reg, crate::codegen::NULL_SENTINEL);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", result_reg, sentinel_reg)); // does the scalar payload carry the in-band null sentinel?
            ctx.emitter
                .instruction(&format!("b.eq {}", empty_label));                 // PHP null is empty()
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", result_reg, sentinel_reg)); // does the scalar payload carry the in-band null sentinel?
            ctx.emitter
                .instruction(&format!("je {}", empty_label));                   // PHP null is empty()
        }
    }
    if is_float {
        emit_float_result_zero_bool(ctx);
    } else {
        emit_int_result_zero_bool(ctx);
    }
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&empty_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits true for a tagged scalar that is null or an integer zero.
pub(in crate::codegen::lower_inst) fn emit_tagged_scalar_empty_bool(ctx: &mut FunctionContext<'_>, value: crate::ir::ValueId) -> Result<()> {
    let empty_label = ctx.next_label("empty_tagged_true");
    let done_label = ctx.next_label("empty_tagged_done");
    ctx.load_value_to_result(value)?;
    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &empty_label);
    emit_int_result_zero_bool(ctx);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&empty_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits true when the canonical integer result register is zero.
pub(in crate::codegen::lower_inst) fn emit_int_result_zero_bool(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", result_reg));        // compare the empty() integer operand against zero
            ctx.emitter.instruction(&format!("cset {}, eq", result_reg));       // return true when the integer operand is zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 0", result_reg));         // compare the empty() integer operand against zero
            ctx.emitter.instruction("sete al");                                 // materialize true when the integer operand is zero
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

/// Emits true when the canonical float result register is zero.
///
/// `empty($x)` is `!(bool)$x`, so a NAN operand must answer FALSE — `var_dump(empty($n))` on a
/// genuine NAN prints `bool(false)`. Saying so on x86_64 takes an explicit parity fixup; see
/// `emit_x86_64_float_zero_from_flags`.
///
/// The in-band `NULL_SENTINEL` miss marker is filtered by the caller
/// (`emit_sentinel_null_or_zero_empty_bool`) BEFORE this runs, so the NAN reaching here is
/// always a real program value.
pub(in crate::codegen::lower_inst) fn emit_float_result_zero_bool(ctx: &mut FunctionContext<'_>) {
    predicates::emit_nan_bool_coercion_probe(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // AArch64 `fcmp` reports unordered as `N=0 Z=0 C=1 V=1`, so `eq` (Z==1) already
            // answers false for a NAN. No parity fixup is needed on this arch.
            ctx.emitter.instruction("fcmp d0, #0.0");                           // compare the empty() float operand against zero
            ctx.emitter.instruction("cset x0, eq");                             // return true when the float operand is zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xorpd xmm1, xmm1");                        // materialize a zero float register for empty() comparison
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // compare the empty() float operand against zero
            emit_x86_64_float_zero_from_flags(ctx);
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

/// Materializes "this float is zero" from x86_64 `ucomisd` flags, counting NAN as NOT zero.
///
/// `ucomisd` sets `ZF=PF=CF=1` for an UNORDERED compare, so a bare `sete al` reads a NAN as
/// EQUAL to zero and makes `empty(NAN)` answer `true` — the inverse of PHP, and of what the
/// AArch64 arm produces from the same source. Masking with `PF` clear restores
/// `empty(NAN) === false` and leaves every ordered comparison untouched (`PF=0` there). This
/// mirrors the `==` branch of `comparisons::emit_x86_64_float_equality_result`.
pub(in crate::codegen::lower_inst) fn emit_x86_64_float_zero_from_flags(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.instruction("sete al");                                         // materialize true when the float operand is ordered-equal to zero
    ctx.emitter.instruction("setnp r10b");                                      // materialize whether the comparison was ordered
    ctx.emitter.instruction("and al, r10b");                                    // a NAN is not empty, so clear the unordered case
}

/// Emits true when the loaded string length register is zero.
pub(in crate::codegen::lower_inst) fn emit_string_length_zero_bool(ctx: &mut FunctionContext<'_>) {
    let len_reg = abi::string_result_regs(ctx.emitter).1;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", len_reg));           // compare the empty() string length against zero
            ctx.emitter.instruction("cset x0, eq");                             // return true when the string length is zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 0", len_reg));            // compare the empty() string length against zero
            ctx.emitter.instruction("sete al");                                 // materialize true when the string length is zero
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

/// Inverts a canonical 0/1 boolean result in the integer result register.
pub(in crate::codegen::lower_inst) fn invert_bool_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("eor x0, x0, #1");                          // invert the canonical boolean result for empty()
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor rax, 1");                              // invert the canonical boolean result for empty()
        }
    }
}

/// php-src's verbatim `ValueError` wording for an unknown `count()` mode.
const COUNT_MODE_MESSAGE: &str =
    "count(): Argument #2 ($mode) must be either COUNT_NORMAL or COUNT_RECURSIVE";

/// Materializes `count()`'s `$mode` and raises PHP's `ValueError` for anything else.
///
/// PHP accepts only `COUNT_NORMAL` (`0`) and `COUNT_RECURSIVE` (`1`) and raises a catchable
/// `ValueError` otherwise, so the guard runs before the receiver is even loaded. `$mode` can be
/// a runtime value, which is why the check is emitted here instead of in the checker.
fn emit_count_mode_guard(ctx: &mut FunctionContext<'_>, mode: ValueId) -> Result<()> {
    match ctx.load_value_to_result(mode)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => {}
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => abi::emit_float_result_to_int_result(ctx.emitter),
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, mode)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "count mode for PHP type {:?}",
                other
            )))
        }
    }
    let mode_reg = abi::int_result_reg(ctx.emitter);
    super::exceptions::emit_value_error_unless(
        ctx,
        super::exceptions::ValueGuard::SignedInRange(mode_reg, 0, 1),
        COUNT_MODE_MESSAGE,
    );
    Ok(())
}

/// Rejects a `count($value, $mode)` receiver whose `COUNT_RECURSIVE` total is not the flat count.
///
/// `COUNT_RECURSIVE` adds the size of every nested array, so it only equals the flat count when
/// the receiver provably cannot hold one. elephc's INDEXED arrays store their payload untagged
/// (`[length][capacity][elem_size][elements...]`), so a runtime walk cannot tell an
/// `array<int>` slot from an `array<array<int>>` slot; recursing anyway would either read
/// integers as pointers or silently undercount. Until the array header carries an element tag,
/// a receiver that CAN nest is refused with an explicit diagnostic instead.
fn require_recursive_count_is_flat(ty: &PhpType) -> Result<()> {
    let element = match ty {
        PhpType::Array(elem) => elem.codegen_repr(),
        PhpType::AssocArray { value, .. } => value.codegen_repr(),
        // php-src ignores `$mode` entirely for Countable objects: it calls `count()` and
        // returns that value, so every object receiver is already exact.
        PhpType::Object(_) => return Ok(()),
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "count() with an explicit $mode for PHP type {:?} (COUNT_RECURSIVE needs a \
                 statically known element type)",
                other
            )))
        }
    };
    if matches!(
        element,
        PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Iterable
            | PhpType::Object(_)
    ) {
        return Err(CodegenIrError::unsupported(format!(
            "count() with an explicit $mode over an array of {:?} (COUNT_RECURSIVE over nested \
             containers needs a runtime element tag in the array header)",
            element
        )));
    }
    Ok(())
}
