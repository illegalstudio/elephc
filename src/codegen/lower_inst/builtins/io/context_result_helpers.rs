//! Purpose:
//! Context storage, static arrays, literal paths, and scalar results.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Verifies that a builtin call has a lowered operand count within an inclusive range.
pub(super) fn ensure_arg_count_between(inst: &Instruction, name: &str, min: usize, max: usize) -> Result<()> {
    let actual = inst.operands.len();
    if (min..=max).contains(&actual) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {}..={} args, got {}",
        name, min, max, actual
    )))
}

/// Loads the four-argument `stream_context_set_option` form into the runtime helper ABI.
pub(super) fn lower_stream_context_set_option_4(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let wrapper = expect_operand(inst, 1)?;
    let option = expect_operand(inst, 2)?;
    let value = expect_operand(inst, 3)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, wrapper, "stream_context_set_option wrapper")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, option, "stream_context_set_option option")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            materialize_owned_stream_context_option_mixed(ctx, value)?;
            ctx.emitter.instruction("mov x4, x0");                              // transfer the owned boxed Mixed option value
            ctx.emitter.instruction("mov x5, xzr");                             // boxed Mixed values use no high payload word
            abi::emit_pop_reg_pair(ctx.emitter, "x2", "x3");
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x1");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, wrapper, "stream_context_set_option wrapper")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, option, "stream_context_set_option option")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            materialize_owned_stream_context_option_mixed(ctx, value)?;
            ctx.emitter.instruction("mov r8, rax");                             // transfer the owned boxed Mixed option value
            ctx.emitter.instruction("xor r9, r9");                              // boxed Mixed values use no high payload word
            abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_context_set_option_4");
    Ok(())
}

/// Materializes one stream option as an owned boxed Mixed cell.
///
/// Concrete values are boxed with child retention. Values already represented
/// by a Mixed cell are unboxed and reboxed so the context owns a distinct PHP
/// value cell while retaining the same underlying COW payload. The runtime
/// option setter consumes the newly created owner.
fn materialize_owned_stream_context_option_mixed(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    let raw_value_ty = ctx.raw_value_php_type(value)?;
    let value_ty = match raw_value_ty {
        PhpType::Resource(kind) => PhpType::Resource(kind),
        other => other.codegen_repr(),
    };
    ctx.load_value_to_result(value)?;
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        if ctx.emitter.target.arch == Arch::X86_64 {
            ctx.emitter.instruction("mov rsi, rdx");                            // move the unboxed high word into mixed_from_value's SysV input register
        }
        abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    }
    Ok(())
}

/// Merges one EIR options hash into the owned construction scratch.
pub(super) fn merge_stream_context_options_into_scratch(
    ctx: &mut FunctionContext<'_>,
    options: ValueId,
) -> Result<()> {
    ctx.load_value_to_result(options)?;
    emit_merge_loaded_stream_context_options_into_scratch(ctx);
    Ok(())
}

/// What the two-argument form's caller must still do once this guard has spoken.
pub(super) enum StreamContextTwoArgOptions {
    /// php refused the argument; the call is over.
    Refused,
    /// Nothing left to do: the guard merged what was legal, or there was nothing legal to merge.
    Handled,
}

/// Refuses and merges a two-argument `$options` / `$wrapper_or_options`.
///
/// The merge helper walks its argument as a HASH, and this path reached it with no check of any
/// kind. Two ways to die, both measured:
///
/// ```text
/// stream_context_set_options($c, json_decode("1"));        SIGBUS
/// stream_context_set_options($c, json_decode("[1]", true)) Fatal error: heap memory exhausted
/// ```
///
/// php raises for both — a `TypeError` for the first and its options-FORM `ValueError` for the
/// second — and the two spellings word the type refusal differently, which is what `style`
/// carries. The order is php's: judge the type, then the shape, then merge.
pub(super) fn emit_two_argument_options_guard(
    ctx: &mut FunctionContext<'_>,
    options: ValueId,
    style: StreamContextOptionsRefusal,
) -> Result<StreamContextTwoArgOptions> {
    if matches!(
        ctx.raw_value_php_type(options)?.codegen_repr(),
        PhpType::Void | PhpType::Never
    ) {
        emit_stream_context_options_refusal(ctx, style, "null");
        return Ok(StreamContextTwoArgOptions::Refused);
    }
    if emit_declared_stream_context_options_refusal(ctx, options, style)? {
        return Ok(StreamContextTwoArgOptions::Refused);
    }
    let boxed = matches!(
        ctx.raw_value_php_type(options)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    );
    ctx.load_value_to_result(options)?;
    let packed = ctx.next_label("sctx_two_arg_packed");
    let done = ctx.next_label("sctx_two_arg_done");
    if boxed {
        // A boxed operand is a CELL, and both the shape guard and the merge read a container
        // header. The tag also decides whether a survivor may be merged at all.
        emit_unbox_two_argument_options(ctx, &packed, style);
    } else if matches!(
        ctx.raw_value_php_type(options)?.codegen_repr(),
        PhpType::Array(_)
    ) {
        // A DECLARED list takes the same route: only an empty one is legal, and the shape guard
        // below is what says so.
        emit_two_argument_options_shape_guard(ctx);
        return Ok(StreamContextTwoArgOptions::Handled);
    }
    emit_two_argument_options_shape_guard(ctx);
    emit_merge_loaded_stream_context_options_into_scratch(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&packed);
    // A packed array that survived the guard was EMPTY, and an empty list carries no options —
    // merging it is what walked an indexed header as a bucket table.
    emit_two_argument_options_shape_guard(ctx);
    ctx.emitter.label(&done);
    Ok(StreamContextTwoArgOptions::Handled)
}

/// Replaces a boxed two-argument options value with the container it holds, refusing the rest.
///
/// Branches to `packed` when the container is an indexed array, which the caller must not merge.
/// Anything that is NOT a container is php's refusal, raised here: the shape guard downstream
/// reads a container HEADER, so an integer reaching it segfaults.
fn emit_unbox_two_argument_options(
    ctx: &mut FunctionContext<'_>,
    packed: &str,
    style: StreamContextOptionsRefusal,
) {
    let hashed = ctx.next_label("sctx_two_arg_hash");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");                      // tag, lo, hi
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, x0");                              // the runtime tag
            ctx.emitter.instruction("mov x0, x1");                              // the payload becomes the candidate
            ctx.emitter.instruction("cmp x9, #4");                              // runtime tag 4 = packed array
            ctx.emitter.instruction(&format!("b.eq {}", packed));
            ctx.emitter.instruction("cmp x9, #5");                              // runtime tag 5 = hash
            ctx.emitter.instruction(&format!("b.eq {}", hashed));
            emit_stream_context_options_type_error_ladder(ctx, "x9", "x1", style);
            ctx.emitter.label(&hashed);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // the runtime tag
            ctx.emitter.instruction("mov rax, rdi");                            // the payload becomes the candidate
            ctx.emitter.instruction("cmp r10, 4");                              // runtime tag 4 = packed array
            ctx.emitter.instruction(&format!("je {}", packed));
            ctx.emitter.instruction("cmp r10, 5");                              // runtime tag 5 = hash
            ctx.emitter.instruction(&format!("je {}", hashed));
            emit_stream_context_options_type_error_ladder(ctx, "r10", "rdi", style);
            ctx.emitter.label(&hashed);
        }
    }
}

/// Raises php's options-FORM `ValueError` unless the loaded container is a legal options map.
fn emit_two_argument_options_shape_guard(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_stream_context_options_shape_ok");
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedAtLeast(result_reg, 1),
        STREAM_CONTEXT_OPTIONS_SHAPE_MESSAGE,
    );
    abi::emit_pop_reg(ctx.emitter, result_reg);
}

/// Merges a present runtime `params['options']` map into options scratch.
///
/// The lookup accepts direct associative hashes and hashes boxed behind Mixed storage. An ABSENT
/// entry leaves scratch unchanged and says nothing; an entry that is PRESENT but is not a map
/// throws php's `TypeError: Invalid stream/context parameter`.
///
/// Those two used to share one exit, so `["options" => 1]` was silently ignored where php-src's
/// own `streams/bug44712.phpt` expects the throw. MEASURED on `php -n` 8.5.6: an int, a string and
/// a null all throw, an empty array does not, and an absent key does not — through
/// `stream_context_set_params()` and `stream_context_create()`'s second argument alike, which is
/// why the refusal lives in this shared helper rather than at either call site.
pub(super) fn merge_stream_context_params_options_into_scratch(
    ctx: &mut FunctionContext<'_>,
    params: ValueId,
    location: Option<(String, u32)>,
) -> Result<()> {
    let hashed = ctx.next_label("sctx_params_options_hashed");
    let tagged = ctx.next_label("sctx_params_options_tagged");
    let packed = ctx.next_label("sctx_params_options_packed");
    let bad = ctx.next_label("sctx_params_options_bad");
    let done = ctx.next_label("sctx_params_options_done");
    let (key, key_len) = ctx.data.add_string(b"options");
    ctx.load_value_to_result(params)?;
    super::stream_context::emit_skip_unless_assoc_array(ctx, &done);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &key);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction(&format!("cbz x0, {}", done));              // absent params options leave the current context options unchanged
            ctx.emitter.instruction("mov x9, x3");                              // hold the tag: unboxing overwrites x3
            ctx.emitter.instruction("cmp x9, #7");                              // is the params entry boxed behind Mixed storage?
            ctx.emitter.instruction(&format!("b.ne {}", tagged));                 // a direct entry already carries its own tag
            ctx.emitter.instruction("mov x0, x1");                              // pass the boxed params entry to Mixed unboxing
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("mov x9, x0");                              // the concrete tag behind the box
            ctx.emitter.label(&tagged);
            ctx.emitter.instruction("mov x0, x1");                              // the container itself, boxed or not
            ctx.emitter.instruction("cmp x9, #5");     // an associative wrapper map is merged
            ctx.emitter.instruction(&format!("b.eq {}", hashed));
            ctx.emitter.instruction("cmp x9, #4");   // `[]` arrives PACKED and is valid php
            ctx.emitter.instruction(&format!("b.eq {}", packed));
            ctx.emitter.instruction(&format!("b {}", bad));                     // a present non-array entry is php's TypeError
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the runtime params hash to lookup
            abi::emit_symbol_address(ctx.emitter, "rsi", &key);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction("test rax, rax");                           // was a params options key present?
            ctx.emitter.instruction(&format!("jz {}", done));                   // absent params options preserve the current context options
            ctx.emitter.instruction("mov r10, rcx");                            // hold the tag: unboxing overwrites rcx
            ctx.emitter.instruction("cmp r10, 7");                              // is the entry boxed behind Mixed storage?
            ctx.emitter.instruction(&format!("jne {}", tagged));                  // a direct entry already carries its own tag
            ctx.emitter.instruction("mov rax, rdi");                            // pass the boxed params entry to Mixed unboxing
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("mov r10, rax");                            // the concrete tag behind the box
            ctx.emitter.label(&tagged);
            ctx.emitter.instruction("mov rax, rdi");                            // the container itself, boxed or not
            ctx.emitter.instruction("cmp r10, 5");     // an associative wrapper map is merged
            ctx.emitter.instruction(&format!("je {}", hashed));
            ctx.emitter.instruction("cmp r10, 4");   // `[]` arrives PACKED and is valid php
            ctx.emitter.instruction(&format!("je {}", packed));
            ctx.emitter.instruction(&format!("jmp {}", bad));                   // a present non-array entry is php's TypeError
        }
    }
    ctx.emitter.label(&packed);
    // A packed array is `[]` or a list with integer keys. The form guard refuses the second and
    // passes the first, which has nothing to merge — and this keeps a packed header away from
    // `__rt_stream_context_merge_options`, which reads a hash header.
    emit_two_argument_options_shape_guard(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {}", done)),
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {}", done)),
    }
    ctx.emitter.label(&hashed);
    // The same FORM check the direct `stream_context_create($options)` door runs: php refuses
    // `["http" => 1]` with a ValueError through BOTH doors, and this one merged it unexamined.
    emit_two_argument_options_shape_guard(ctx);
    emit_merge_loaded_stream_context_options_into_scratch(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {}", done)),
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {}", done)),
    }
    ctx.emitter.label(&bad);
    // `emit_type_error_at` never returns, so `done` below is reached only by the branches above.
    super::super::exceptions::emit_type_error_at(
        ctx,
        STREAM_CONTEXT_PARAMS_OPTIONS_MESSAGE,
        location,
    );
    ctx.emitter.label(&done);
    Ok(())
}

/// Merges the loaded incoming wrapper map into the owned options scratch.
///
/// The dedicated runtime helper returns a fresh COW root. This emitter releases
/// the previous scratch owner only after the replacement is complete, then
/// publishes the returned owner for later transfer into `ContextState`.
fn emit_merge_loaded_stream_context_options_into_scratch(
    ctx: &mut FunctionContext<'_>,
) {
    let old_released = ctx.next_label("sctx_merge_old_released");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("ldr x0, [x9]");                            // pass the current owned wrapper map as the left merge input
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_call_label(ctx.emitter, "__rt_stream_context_merge_options");
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("ldr x0, [x9]");                            // load the previous scratch owner for release
            ctx.emitter.instruction(&format!("cbz x0, {}", old_released));      // an empty scratch has no owner to release
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.label(&old_released);
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("str x0, [x9]");                            // publish the newly owned merged wrapper map
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov rdi, QWORD PTR [r9]");                 // pass the current owned wrapper map as the left merge input
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_call_label(ctx.emitter, "__rt_stream_context_merge_options");
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov rax, QWORD PTR [r9]");                 // load the previous scratch owner for release
            ctx.emitter.instruction("test rax, rax");                           // does the previous scratch own a wrapper map?
            ctx.emitter.instruction(&format!("jz {}", old_released));           // an empty scratch has no owner to release
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.label(&old_released);
            abi::emit_pop_reg(ctx.emitter, "rax");
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov QWORD PTR [r9], rax");                 // publish the newly owned merged wrapper map
        }
    }
}

/// How php refuses an argument that should have been an options array.
///
/// The family does not agree, MEASURED on `php -n` 8.5.6:
///
/// ```text
/// stream_context_set_options($c, 1)  TypeError: Argument #2 ($options) must be of type array, int given
/// stream_context_set_option($c, 1)   ValueError: Argument #3 ($option_name) cannot be null when
///                                    argument #2 ($wrapper_or_options) is a string
/// stream_context_set_option($c, $o)  TypeError: Argument #2 ($wrapper_or_options) must be of type
///                                    array|string, stdClass given
/// ```
///
/// The singular declares `array|string`, so a SCALAR is coerced into a wrapper name and the
/// complaint moves to the argument that was not supplied. Only a value with no string form —
/// an object or a closure — is a type error there.
#[derive(Clone, Copy)]
pub(super) enum StreamContextOptionsRefusal {
    /// `array $options` / `?array $options`: every non-array names its own type.
    ArrayOnly(StreamContextOptionsParam),
    /// `array|string $wrapper_or_options`: a scalar becomes a wrapper name instead.
    ArrayOrString,
}

impl StreamContextOptionsRefusal {
    /// Whether a null argument is accepted rather than refused.
    fn accepts_null(self) -> bool {
        match self {
            Self::ArrayOnly(param) => param.accepts_null,
            // `array|string` has no null form: php coerces it to "" and objects about the name.
            Self::ArrayOrString => false,
        }
    }
}

/// Which php function's `$options` argument is being published, and how php words its refusal.
///
/// php composes the message from the FUNCTION NAME, the ARGUMENT NUMBER and the declared type, and
/// all three vary across the family — MEASURED on `php -n` 8.5.6:
///
/// ```text
/// stream_context_create(1)          ... Argument #1 ($options) must be of type ?array, int given
/// stream_context_get_default(1)     ... Argument #1 ($options) must be of type ?array, int given
/// stream_context_set_default(1)     ... Argument #1 ($options) must be of type array, int given
/// stream_context_set_default(null)  ... Argument #1 ($options) must be of type array, null given
/// stream_context_get_default(null)  accepted
/// ```
#[derive(Clone, Copy)]
pub(super) struct StreamContextOptionsParam {
    /// The php function name, as it appears at the head of the message.
    function: &'static str,
    /// The 1-based argument position php prints.
    argument: u32,
    /// Whether php declares `?array` rather than `array`, which decides null ALONE.
    accepts_null: bool,
}

impl StreamContextOptionsParam {
    /// `stream_context_create(?array $options = null)`.
    pub(super) const CREATE: Self = Self {
        function: "stream_context_create",
        argument: 1,
        accepts_null: true,
    };
    /// `stream_context_get_default(?array $options = null)`.
    pub(super) const GET_DEFAULT: Self = Self {
        function: "stream_context_get_default",
        argument: 1,
        accepts_null: true,
    };
    /// `stream_context_set_default(array $options)` — the one that refuses null.
    pub(super) const SET_DEFAULT: Self = Self {
        function: "stream_context_set_default",
        argument: 1,
        accepts_null: false,
    };

    /// `stream_context_set_options(StreamContext $context, array $options)` — argument TWO.
    pub(super) const SET_OPTIONS: Self = Self {
        function: "stream_context_set_options",
        argument: 2,
        accepts_null: false,
    };

    /// Returns everything php prints before the offending value's own spelling.
    fn type_error_prefix(self) -> String {
        format!(
            "{}(): Argument #{} ($options) must be of type {}array, ",
            self.function,
            self.argument,
            if self.accepts_null { "?" } else { "" }
        )
    }
}

/// Stores an options heap pointer in the runtime's single stream-context slot.
pub(super) fn store_stream_context_options(
    ctx: &mut FunctionContext<'_>,
    options: ValueId,
    clear_on_null: bool,
    param: StreamContextOptionsParam,
) -> Result<()> {
    if matches!(
        ctx.raw_value_php_type(options)?.codegen_repr(),
        PhpType::Void | PhpType::Never
    ) {
        if !param.accepts_null {
            // `array $options`, not `?array`: php refuses the literal null too.
            emit_stream_context_options_refusal(
                ctx,
                StreamContextOptionsRefusal::ArrayOnly(param),
                "null",
            );
            return Ok(());
        }
        if clear_on_null {
            clear_stream_context_options(ctx);
        }
        return Ok(());
    }
    if emit_declared_stream_context_options_refusal(
        ctx,
        options,
        StreamContextOptionsRefusal::ArrayOnly(param),
    )? {
        return Ok(());
    }
    ctx.load_value_to_result(options)?;
    // A `Mixed` operand is a BOXED cell, and the guard below reads a container header. Left boxed
    // it read the box's header, concluded "not a packed array", and walked the box as a hash —
    // measured as a ValueError on `stream_context_create(json_decode("[]", true))`, which php
    // accepts. The payload is put in front of the guard here; whether it may be PUBLISHED is
    // decided after the guard has spoken, because only the guard refuses a non-empty list.
    let boxed_mixed = matches!(
        ctx.raw_value_php_type(options)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    );
    if boxed_mixed {
        emit_unbox_stream_context_options(ctx, param);
    }
    // php keeps only entries whose key is a STRING and whose value is an ARRAY, and raises a
    // catchable `ValueError` for anything else — `['ssl' => "abc"]`, `['ssl' => 1]`,
    // `['ssl' => null]` and `[0 => [...]]` all measured on `php -n` 8.5.6. elephc stored the
    // malformed map in silence, so a typo in a context array produced a context carrying nothing.
    // The guard runs on the LOADED pointer, before it is published, so a refused map never
    // reaches the runtime slot.
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_stream_context_options_shape_ok");
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedAtLeast(result_reg, 1),
        STREAM_CONTEXT_OPTIONS_SHAPE_MESSAGE,
    );
    abi::emit_pop_reg(ctx.emitter, result_reg);
    if boxed_mixed {
        // The guard has spoken. A packed array that survived it was EMPTY, and an empty list
        // carries no options — publishing it is what made the next `set_option()` hash a key out
        // of an indexed array's header.
        emit_drop_packed_stream_context_options(ctx);
    }
    // An INDEXED array is never an options map. php's shape is
    // `["wrappername"]["optionname"] = $value`, so a list can only be legal when EMPTY — measured,
    // `stream_context_create([])` is a valid context and `stream_context_create([1])` is a
    // ValueError, which the guard above already raises. What remains is not to PUBLISH the list:
    // every reader of this slot treats it as a hash, and `stream_context_set_option()` on a
    // context built from `[]` used to segfault hashing a key out of an indexed array's header.
    if matches!(
        ctx.raw_value_php_type(options)?.codegen_repr(),
        PhpType::Array(_)
    ) {
        clear_stream_context_options(ctx);
        return Ok(());
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => store_stream_context_options_aarch64(ctx, clear_on_null),
        Arch::X86_64 => store_stream_context_options_x86_64(ctx, clear_on_null),
    }
    Ok(())
}

/// Replaces a boxed `Mixed` options value with the container it holds, and remembers whether that
/// container was a packed array.
///
/// The tag rides on the stack across the shape guard because the guard is what decides whether a
/// packed array is legal at all: an empty one is, a non-empty one is php's `ValueError`.
fn emit_unbox_stream_context_options(
    ctx: &mut FunctionContext<'_>,
    param: StreamContextOptionsParam,
) {
    let hashed = ctx.next_label("sctx_mixed_hash");
    let done = ctx.next_label("sctx_mixed_done");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");                      // tag, lo, hi
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, x0");                              // the runtime tag
            ctx.emitter.instruction("mov x0, x1");                              // the payload becomes the candidate
            ctx.emitter.instruction("cmp x9, #4");                              // runtime tag 4 = packed array
            ctx.emitter.instruction("cset x10, eq");                            // remember it for after the guard
            ctx.emitter.instruction(&format!("b.eq {}", done));
            ctx.emitter.instruction("cmp x9, #5");                              // runtime tag 5 = hash
            ctx.emitter.instruction(&format!("b.eq {}", hashed));
            emit_stream_context_options_type_error_ladder(
                ctx,
                "x9",
                "x1",
                StreamContextOptionsRefusal::ArrayOnly(param),
            );                                                                  // php refuses a scalar here
            ctx.emitter.instruction("mov x0, xzr");                             // runtime tag 8 = null, the one shape `?array` accepts besides an array
            ctx.emitter.label(&hashed);
            ctx.emitter.label(&done);
            abi::emit_push_reg(ctx.emitter, "x10");                             // the packed flag, across the guard
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // the runtime tag
            ctx.emitter.instruction("mov rax, rdi");                            // the payload becomes the candidate
            ctx.emitter.instruction("xor r11d, r11d");
            ctx.emitter.instruction("cmp r10, 4");                              // runtime tag 4 = packed array
            ctx.emitter.instruction("sete r11b");                               // remember it for after the guard
            ctx.emitter.instruction(&format!("je {}", done));
            ctx.emitter.instruction("cmp r10, 5");                              // runtime tag 5 = hash
            ctx.emitter.instruction(&format!("je {}", hashed));
            emit_stream_context_options_type_error_ladder(
                ctx,
                "r10",
                "rdi",
                StreamContextOptionsRefusal::ArrayOnly(param),
            );                                                                  // php refuses a scalar here
            ctx.emitter.instruction("xor eax, eax");                            // runtime tag 8 = null, the one shape `?array` accepts besides an array
            ctx.emitter.label(&hashed);
            ctx.emitter.label(&done);
            abi::emit_push_reg(ctx.emitter, "r11");                             // the packed flag, across the guard
        }
    }
}

/// Raises the refusal `style` calls for, naming `given` as the offending value.
///
/// `given` is php's own spelling of the VALUE, not its type — `true` rather than `bool`, and a
/// class name for an object — which is why every caller composes it rather than passing a type.
fn emit_stream_context_options_refusal(
    ctx: &mut FunctionContext<'_>,
    style: StreamContextOptionsRefusal,
    given: &str,
) {
    match style {
        StreamContextOptionsRefusal::ArrayOnly(param) => {
            let prefix = param.type_error_prefix();
            super::super::exceptions::emit_type_error(ctx, &format!("{prefix}{given} given"));
        }
        StreamContextOptionsRefusal::ArrayOrString => {
            if given == "null" {
                // php 8.1 deprecated passing null to a non-nullable INTERNAL parameter, and this
                // one is `array|string`. The notice precedes the refusal, and the coercion still
                // happens — null becomes "", which is why the refusal names a STRING wrapper.
                super::fopen_core::emit_static_diag_warning(
                    ctx,
                    "Deprecated: stream_context_set_option(): Passing null to parameter #2 \
                     ($wrapper_or_options) of type array|string is deprecated\n",
                );
            }
            super::super::exceptions::emit_value_error(
                ctx,
                super::stream_context::STREAM_CONTEXT_SET_OPTION_NAME_CANNOT_BE_NULL,
            );
        }
    }
}

/// Raises the refusal for a value php cannot turn into a wrapper name — an object or a closure.
fn emit_stream_context_options_class_refusal(
    ctx: &mut FunctionContext<'_>,
    style: StreamContextOptionsRefusal,
    given: &str,
) {
    match style {
        StreamContextOptionsRefusal::ArrayOnly(param) => {
            let prefix = param.type_error_prefix();
            super::super::exceptions::emit_type_error(ctx, &format!("{prefix}{given} given"));
        }
        StreamContextOptionsRefusal::ArrayOrString => {
            super::super::exceptions::emit_type_error(
                ctx,
                &format!(
                    "stream_context_set_option(): Argument #2 ($wrapper_or_options) must be of \
                     type array|string, {given} given"
                ),
            );
        }
    }
}

/// php's `?array` signature refuses everything but an array and null, and names the offending
/// value in the message.
///
/// MEASURED on `php -n` 8.5.6 — the tail is composed from the VALUE, not merely its type:
///
/// ```text
/// 1        → int given          "x"          → string given
/// 1.5      → float given        true         → true given
/// $handle  → resource given     new Thing()  → Thing given
/// null     → accepted (`?array`)
/// ```
///
/// The scalar words are static, so each is a whole pre-baked message selected by tag. An OBJECT
/// names its CLASS, which is only known at run time, so that arm reads the name out of the dense
/// `_class_name_entries` table `get_class()` reads and composes the message around it.
fn emit_stream_context_options_type_error_ladder(
    ctx: &mut FunctionContext<'_>,
    tag_reg: &str,
    payload_reg: &str,
    style: StreamContextOptionsRefusal,
) {
    // Each arm below is emitted INLINE, with no branch over it: `emit_type_error` never returns
    // (it ends in a jump to `__rt_throw_current`), so the instruction after a raise is only ever
    // reached when that arm's tag did NOT match. The ladder reads as fall-through and is not.
    //
    // (runtime tag, the word php prints). Tag 3 is split by the payload below.
    let scalars: &[(i64, &str)] = &[(0, "int"), (1, "string"), (2, "float"), (9, "resource")];
    for (tag, word) in scalars {
        let skip = ctx.next_label("sctx_opt_not_scalar");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {}, #{}", tag_reg, tag));
                ctx.emitter.instruction(&format!("b.ne {}", skip));
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", tag_reg, tag));
                ctx.emitter.instruction(&format!("jne {}", skip));
            }
        }
        emit_stream_context_options_refusal(ctx, style, word);
        ctx.emitter.label(&skip);
    }
    // A bool names its own value.
    let not_bool = ctx.next_label("sctx_opt_not_bool");
    let is_false = ctx.next_label("sctx_opt_false");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #3", tag_reg));           // runtime tag 3 = bool
            ctx.emitter.instruction(&format!("b.ne {}", not_bool));
            ctx.emitter.instruction(&format!("cbz {}, {}", payload_reg, is_false));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 3", tag_reg));            // runtime tag 3 = bool
            ctx.emitter.instruction(&format!("jne {}", not_bool));
            ctx.emitter.instruction(&format!("test {}, {}", payload_reg, payload_reg));
            ctx.emitter.instruction(&format!("jz {}", is_false));
        }
    }
    emit_stream_context_options_refusal(ctx, style, "true");
    ctx.emitter.label(&is_false);
    emit_stream_context_options_refusal(ctx, style, "false");
    ctx.emitter.label(&not_bool);

    if !style.accepts_null() {
        // `array $options`: a boxed null is refused here exactly like a declared one above.
        let not_null = ctx.next_label("sctx_opt_not_null");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {}, #8", tag_reg));       // runtime tag 8 = null
                ctx.emitter.instruction(&format!("b.ne {}", not_null));
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {}, 8", tag_reg));        // runtime tag 8 = null
                ctx.emitter.instruction(&format!("jne {}", not_null));
            }
        }
        emit_stream_context_options_refusal(ctx, style, "null");
        ctx.emitter.label(&not_null);
    }

    // A closure is tag 10, NOT the object tag, so the class-name arm below never sees it. php
    // prints `Closure` for a closure and for a first-class callable alike, and that name is fixed.
    let not_closure = ctx.next_label("sctx_opt_not_closure");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #10", tag_reg));          // runtime tag 10 = callable descriptor
            ctx.emitter.instruction(&format!("b.ne {}", not_closure));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 10", tag_reg));           // runtime tag 10 = callable descriptor
            ctx.emitter.instruction(&format!("jne {}", not_closure));
        }
    }
    emit_stream_context_options_class_refusal(ctx, style, "Closure");
    ctx.emitter.label(&not_closure);

    // An object names its class, which no pre-baked message can carry.
    let not_object = ctx.next_label("sctx_opt_not_object");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #6", tag_reg));           // runtime tag 6 = object
            ctx.emitter.instruction(&format!("b.ne {}", not_object));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 6", tag_reg));            // runtime tag 6 = object
            ctx.emitter.instruction(&format!("jne {}", not_object));
        }
    }
    emit_class_name_to_string_result(ctx, payload_reg);
    super::super::exceptions::emit_message_concat_prefix(ctx, &class_refusal_prefix(style));
    super::super::exceptions::emit_message_concat_suffix(ctx, " given");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");                      // the Throwable owns its message
    super::super::exceptions::emit_type_error_from_string_result(ctx);
    ctx.emitter.label(&not_object);
}

/// Refuses an `$options` operand whose DECLARED type can never be an options array.
///
/// The runtime ladder beside this one reads a boxed `Mixed`'s tag; a declared type is known while
/// lowering, so each of these is one whole pre-baked message and no tag is read at all. Before it
/// existed, `stream_context_set_default(new stdClass())` reached the SHAPE guard and came back as
/// php's `ValueError` about the options FORM — a different error, for a different mistake.
///
/// A declared `Bool` is the one shape that still needs a runtime test: php prints the VALUE,
/// `true` or `false`, and only the value knows which.
///
/// Returns `true` when it emitted a refusal, in which case the caller must emit nothing more.
fn emit_declared_stream_context_options_refusal(
    ctx: &mut FunctionContext<'_>,
    options: ValueId,
    style: StreamContextOptionsRefusal,
) -> Result<bool> {
    let word = match ctx.raw_value_php_type(options)?.codegen_repr() {
        // The shapes that can legitimately hold an options map, or that carry a runtime tag the
        // ladder reads instead.
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Iterable
        | PhpType::TaggedScalar => return Ok(false),
        PhpType::Int => "int".to_string(),
        PhpType::Float => "float".to_string(),
        PhpType::Str => "string".to_string(),
        PhpType::Resource(_) => "resource".to_string(),
        PhpType::Callable => {
            emit_stream_context_options_class_refusal(ctx, style, "Closure");
            return Ok(true);
        }
        PhpType::False => "false".to_string(),
        PhpType::Object(class) => {
            emit_stream_context_options_class_refusal(ctx, style, &class);
            return Ok(true);
        }
        PhpType::Bool => {
            let is_false = ctx.next_label("sctx_opt_declared_false");
            let result_reg = abi::int_result_reg(ctx.emitter);
            ctx.load_value_to_result(options)?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter
                        .instruction(&format!("cbz {}, {}", result_reg, is_false));
                }
                Arch::X86_64 => {
                    ctx.emitter
                        .instruction(&format!("test {}, {}", result_reg, result_reg));
                    ctx.emitter.instruction(&format!("jz {}", is_false));
                }
            }
            emit_stream_context_options_refusal(ctx, style, "true");
            ctx.emitter.label(&is_false);
            emit_stream_context_options_refusal(ctx, style, "false");
            return Ok(true);
        }
        // Anything else keeps the behaviour it had: this guard exists to REPLACE a wrong error
        // with php's, not to turn an unlisted shape into a refusal it never had.
        _ => return Ok(false),
    };
    emit_stream_context_options_refusal(ctx, style, &word);
    Ok(true)
}

/// Everything php prints before a CLASS NAME, per refusal style.
fn class_refusal_prefix(style: StreamContextOptionsRefusal) -> String {
    match style {
        StreamContextOptionsRefusal::ArrayOnly(param) => param.type_error_prefix(),
        StreamContextOptionsRefusal::ArrayOrString => {
            "stream_context_set_option(): Argument #2 ($wrapper_or_options) must be of type \
             array|string, "
                .to_string()
        }
    }
}

/// Loads an object's class name into the string-result pair, as `get_class()` would spell it.
///
/// Reads the dense `_class_name_entries` (ptr, len) rows by class id, bounded by
/// `_class_name_count`. Only the throwing path reaches here, so the registers it borrows are
/// already dead — but the bound is still checked, because a malformed id would otherwise turn an
/// out-of-range row into a wild pointer inside an error message.
fn emit_class_name_to_string_result(ctx: &mut FunctionContext<'_>, object_reg: &str) {
    let (name_ptr, name_len) = abi::string_result_regs(ctx.emitter);
    let fallback = ctx.next_label("sctx_opt_class_unnamed");
    let ready = ctx.next_label("sctx_opt_class_named");
    let (unnamed_label, unnamed_len) = ctx.data.add_string(b"object");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x13, [{}]", object_reg));     // the receiver's class id
            abi::emit_load_symbol_to_reg(ctx.emitter, "x14", "_class_name_count", 0);
            ctx.emitter.instruction("cmp x13, x14");
            ctx.emitter.instruction(&format!("b.hs {}", fallback));             // an id past the table names no class
            abi::emit_symbol_address(ctx.emitter, "x14", "_class_name_entries");
            ctx.emitter.instruction("add x14, x14, x13, lsl #4");               // select the 16-byte class-name row
            ctx.emitter.instruction(&format!("ldp {}, {}, [x14]", name_ptr, name_len));
            ctx.emitter.instruction(&format!("cbnz {}, {}", name_len, ready));  // a non-empty name is what php prints
            ctx.emitter.label(&fallback);
            abi::emit_symbol_address(ctx.emitter, name_ptr, &unnamed_label);
            abi::emit_load_int_immediate(ctx.emitter, name_len, unnamed_len as i64);
            ctx.emitter.label(&ready);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov r9, QWORD PTR [{}]", object_reg)); // the receiver's class id
            ctx.emitter.instruction("cmp r9, QWORD PTR [rip + _class_name_count]");
            ctx.emitter.instruction(&format!("jae {}", fallback));              // an id past the table names no class
            ctx.emitter.instruction("lea r8, [rip + _class_name_entries]");
            ctx.emitter.instruction("shl r9, 4");                               // scale the class id to the 16-byte row
            ctx.emitter
                .instruction(&format!("mov {}, QWORD PTR [r8 + r9]", name_ptr));
            ctx.emitter
                .instruction(&format!("mov {}, QWORD PTR [r8 + r9 + 8]", name_len));
            ctx.emitter.instruction(&format!("test {}, {}", name_len, name_len));
            ctx.emitter.instruction(&format!("jnz {}", ready));                 // a non-empty name is what php prints
            ctx.emitter.label(&fallback);
            abi::emit_symbol_address(ctx.emitter, name_ptr, &unnamed_label);
            abi::emit_load_int_immediate(ctx.emitter, name_len, unnamed_len as i64);
            ctx.emitter.label(&ready);
        }
    }
}

/// Drops an options value the unboxing marked as a packed array, once the guard has accepted it.
fn emit_drop_packed_stream_context_options(ctx: &mut FunctionContext<'_>) {
    let keep = ctx.next_label("sctx_mixed_keep");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x10");
            ctx.emitter.instruction(&format!("cbz x10, {}", keep));
            ctx.emitter.instruction("mov x0, xzr");                             // an empty list carries no options
            ctx.emitter.label(&keep);
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "r11");
            ctx.emitter.instruction("test r11, r11");
            ctx.emitter.instruction(&format!("jz {}", keep));
            ctx.emitter.instruction("xor eax, eax");                            // an empty list carries no options
            ctx.emitter.label(&keep);
        }
    }
}

/// php-src's verbatim `TypeError` for a `params['options']` entry that is not an array.
///
/// `zend_type_error("Invalid stream/context parameter")` in `ext/standard/streamsfuncs.c` — a bare
/// `zend_type_error`, so it carries no `funcname(): Argument #n` prefix the way the argument
/// errors around it do. MEASURED on `php -n` 8.5.6 through `getMessage()`.
pub(super) const STREAM_CONTEXT_PARAMS_OPTIONS_MESSAGE: &str = "Invalid stream/context parameter";

/// php-src's verbatim `ValueError` for a stream-context options array of the wrong shape.
pub(super) const STREAM_CONTEXT_OPTIONS_SHAPE_MESSAGE: &str =
    "Options should have the form [\"wrappername\"][\"optionname\"] = $value";

/// Stores the loaded AArch64 options pointer into `_stream_context_options`.
pub(super) fn store_stream_context_options_aarch64(ctx: &mut FunctionContext<'_>, clear_on_null: bool) {
    let skip_label = ctx.next_label("sctx_store_done");
    if clear_on_null {
        let zero_label = ctx.next_label("sctx_store_zero");
        ctx.emitter.instruction(&format!("cbz x0, {}", zero_label));            // clear the context slot when a null options value is passed
        abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
        ctx.emitter.instruction("str x0, [x9]");                                // persist the options heap pointer globally
        abi::emit_call_label(ctx.emitter, "__rt_incref");
        ctx.emitter.instruction(&format!("b {}", skip_label));                  // skip the null-clearing fallback after retaining options
        ctx.emitter.label(&zero_label);
        clear_stream_context_options(ctx);
        ctx.emitter.label(&skip_label);
        return;
    }
    ctx.emitter.instruction(&format!("cbz x0, {}", skip_label));                // leave the context slot unchanged for null options
    abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
    ctx.emitter.instruction("str x0, [x9]");                                    // persist the options heap pointer globally
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&skip_label);
}

/// Stores the loaded x86_64 options pointer into `_stream_context_options`.
pub(super) fn store_stream_context_options_x86_64(ctx: &mut FunctionContext<'_>, clear_on_null: bool) {
    let skip_label = ctx.next_label("sctx_store_done_x86");
    if clear_on_null {
        let zero_label = ctx.next_label("sctx_store_zero_x86");
        ctx.emitter.instruction("test rax, rax");                               // check whether the options pointer is null
        ctx.emitter.instruction(&format!("jz {}", zero_label));                 // clear the context slot when a null options value is passed
        abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
        ctx.emitter.instruction("mov QWORD PTR [r9], rax");                     // persist the options heap pointer globally
        ctx.emitter.instruction("mov rdi, rax");                                // pass the options pointer to incref
        abi::emit_call_label(ctx.emitter, "__rt_incref");
        ctx.emitter.instruction(&format!("jmp {}", skip_label));                // skip the null-clearing fallback after retaining options
        ctx.emitter.label(&zero_label);
        clear_stream_context_options(ctx);
        ctx.emitter.label(&skip_label);
        return;
    }
    ctx.emitter.instruction("test rax, rax");                                   // check whether the options pointer is null
    ctx.emitter.instruction(&format!("jz {}", skip_label));                     // leave the context slot unchanged for null options
    abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
    ctx.emitter.instruction("mov QWORD PTR [r9], rax");                         // persist the options heap pointer globally
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the options pointer to incref
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&skip_label);
}

/// Transfers the retained construction-slot options into one `ContextState`.
///
/// The global slot is only scratch storage while lowering the options value.
/// Each context owns its own hash reference, and replacement releases the
/// previous state child after the new pointer has been installed.
pub(super) fn update_stream_context_state_from_handle(
    ctx: &mut FunctionContext<'_>,
    context: ValueId,
) -> Result<()> {
    load_context_state_to_result(ctx, context, "stream_context_set_option")?;
    emit_transfer_stream_context_options_to_loaded_state(ctx);
    Ok(())
}

/// Transfers retained options scratch into the `ContextState` in the result register.
pub(super) fn emit_transfer_stream_context_options_to_loaded_state(ctx: &mut FunctionContext<'_>) {
    let skip_label = ctx.next_label("sctx_update_skip");
    let done_label = ctx.next_label("sctx_update_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", skip_label));        // ignore an invalid or stale context handle
            ctx.emitter.instruction("mov x10, x0");                             // preserve ContextState while loading construction scratch
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("ldr x11, [x9]");                           // take the retained replacement options pointer
            ctx.emitter.instruction("str xzr, [x9]");                           // transfer scratch ownership into ContextState
            ctx.emitter.instruction("ldr x0, [x10]");                           // load the previously owned options hash
            ctx.emitter.instruction("str x11, [x10]");                          // publish the replacement before releasing the old hash
            ctx.emitter.instruction(&format!("cbz x0, {}", done_label));        // skip release when the context had no options
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction(&format!("b {}", done_label));              // join the state-update epilogue
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the context handle resolve?
            ctx.emitter.instruction(&format!("jz {}", skip_label));
            ctx.emitter.instruction("mov r10, rax");                            // preserve ContextState while loading construction scratch
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov r11, QWORD PTR [r9]");                 // take the retained replacement options pointer
            ctx.emitter.instruction("mov QWORD PTR [r9], 0");                   // transfer scratch ownership into ContextState
            ctx.emitter.instruction("mov rax, QWORD PTR [r10]");                // load the previously owned options hash
            ctx.emitter.instruction("mov QWORD PTR [r10], r11");                // publish the replacement before releasing the old hash
            ctx.emitter.instruction("test rax, rax");                           // did the context own an earlier hash?
            ctx.emitter.instruction(&format!("jz {}", done_label));             // skip release for an empty state
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // join the state-update epilogue
        }
    }
    ctx.emitter.label(&skip_label);
    ctx.emitter.label(&done_label);
}

/// Restores stream-context options from the handle's stable `ContextState`.
///
/// The global slot remains transient scratch for legacy open helpers; it never
/// owns this borrowed pointer. Context identity and ownership stay in the
/// registry state.
pub(super) fn restore_stream_context_from_handle(
    ctx: &mut FunctionContext<'_>,
    context: ValueId,
) -> Result<()> {
    let skip_label = ctx.next_label("sctx_restore_skip");
    load_context_state_to_result(ctx, context, "fopen")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", skip_label));        // invalid or stale context handles carry no options
            ctx.emitter.instruction("ldr x1, [x0]");                            // load the borrowed ContextState.options pointer
            ctx.emitter.instruction(&format!("cbz x1, {}", skip_label));        // an empty context leaves construction scratch unchanged
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("str x1, [x9]");                            // expose the borrowed hash to the legacy open helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test rax, rax"));
            ctx.emitter.instruction(&format!("jz {}", skip_label));             // invalid or stale context handles carry no options
            ctx.emitter.instruction("mov rcx, QWORD PTR [rax]");                // load the borrowed ContextState.options pointer
            ctx.emitter.instruction(&format!("test rcx, rcx"));
            ctx.emitter.instruction(&format!("jz {}", skip_label));             // an empty context leaves construction scratch unchanged
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov QWORD PTR [r9], rcx");                 // expose the borrowed hash to the legacy open helper
        }
    }
    ctx.emitter.label(&skip_label);
    Ok(())
}

/// Clears the runtime's single stream-context options slot.
pub(super) fn clear_stream_context_options(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("str xzr, [x9]");                           // clear the persisted stream-context options pointer
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov QWORD PTR [r9], 0");                   // clear the persisted stream-context options pointer
        }
    }
}

/// Retains the transient options scratch before a helper may replace it through COW.
pub(super) fn retain_stream_context_options_scratch(ctx: &mut FunctionContext<'_>) {
    let done_label = ctx.next_label("sctx_scratch_retain_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("ldr x0, [x9]");                            // load the borrowed ContextState.options pointer
            ctx.emitter.instruction(&format!("cbz x0, {}", done_label));        // empty scratch has no ownership to acquire
            abi::emit_call_label(ctx.emitter, "__rt_incref");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov rax, QWORD PTR [r9]");                 // load the borrowed ContextState.options pointer
            ctx.emitter.instruction("test rax, rax");                           // is there an options hash to retain?
            ctx.emitter.instruction(&format!("jz {}", done_label));             // empty scratch has no ownership to acquire
            ctx.emitter.instruction("mov rdi, rax");                            // pass the options hash to heap incref
            abi::emit_call_label(ctx.emitter, "__rt_incref");
        }
    }
    ctx.emitter.label(&done_label);
}

/// Emits an empty associative hash with Mixed values as the current result.
pub(super) fn emit_empty_mixed_hash(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #1");                              // pass the empty hash's initial capacity
            ctx.emitter.instruction("mov x1, #7");                              // select Mixed values for the empty hash
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 1");                              // pass the empty hash's initial capacity
            ctx.emitter.instruction("mov esi, 7");                              // select Mixed values for the empty hash
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
}

/// Emits an indexed string array from static names as the current result.
pub(super) fn emit_static_string_array(ctx: &mut FunctionContext<'_>, names: &[&str]) {
    let capacity = names.len().max(1);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 16);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 16);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_static_string_array_fill_aarch64(ctx, names),
        Arch::X86_64 => emit_static_string_array_fill_x86_64(ctx, names),
    }
}

/// Appends static strings to the current result array on AArch64.
pub(super) fn emit_static_string_array_fill_aarch64(ctx: &mut FunctionContext<'_>, names: &[&str]) {
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // park the string array while appending entries
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("ldr x0, [sp]");                                // reload the string array for this append
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("str x0, [sp]");                                // preserve the possibly-grown string array
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the final string array as the result
}

/// Appends static strings to the current result array on x86_64.
pub(super) fn emit_static_string_array_fill_x86_64(ctx: &mut FunctionContext<'_>, names: &[&str]) {
    ctx.emitter.instruction("push rax");                                        // park the string array while appending entries
    ctx.emitter.instruction("sub rsp, 8");                                      // keep stack alignment stable across append helper calls
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // reload the string array for this append
        abi::emit_symbol_address(ctx.emitter, "rsi", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // preserve the possibly-grown string array
    }
    ctx.emitter.instruction("add rsp, 8");                                      // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax");                                         // restore the final string array as the result
}

/// Emits a stream descriptor as the current integer/resource result.
pub(super) fn emit_fd_result(ctx: &mut FunctionContext<'_>, fd: i64) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), fd);
}

/// Emits `dup(fd)` and leaves the FRESH descriptor as the current integer result.
///
/// A `php://stdout` handle must not be the process's own descriptor. php-src's
/// `php://` wrapper duplicates it (`ext/standard/php_fopen_wrapper.c`), so closing the
/// stream closes only the copy. Handing out descriptor 1 directly made
/// `fclose(fopen('php://stdout', 'w'))` close the program's real standard output: every
/// later `echo` was written to a closed descriptor and silently discarded, while the
/// program still exited 0.
///
/// `dup` returning `-1` flows into the caller's `false` boxing like any other failed
/// open. The call is safe here because the descriptor is boxed through
/// `__rt_mixed_from_value` immediately afterwards, so this path already contains a call
/// and the enclosing frame already preserves the link register.
pub(super) fn emit_dup_fd_result(ctx: &mut FunctionContext<'_>, fd: i64) {
    let arg_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x0",
        Arch::X86_64 => "rdi",
    };
    abi::emit_load_int_immediate(ctx.emitter, arg_reg, fd);
    ctx.emitter.bl_c("dup");                                                    // hand out a copy, never the process's own descriptor
}

/// Emits the `php://fd/N` open, which is a `dup()` that SAYS why it failed.
///
/// The three standard streams reach [`emit_dup_fd_result`] instead: their descriptors always
/// exist, so php has no refusal to word for them. A descriptor the URL names is different —
/// php checks it against `getdtablesize()` and reports a failed `dup()` with the errno NUMBER
/// as well as its text — and neither the bound nor the errno is known at compile time, so the
/// whole open goes through the runtime helper and the URL travels with it for the message.
pub(super) fn emit_php_fd_open_result(ctx: &mut FunctionContext<'_>, fd: i64, path: &str) {
    let (path_symbol, path_len) = ctx.data.add_string(path.as_bytes());
    let (fd_reg, url_reg, len_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x0", "x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi", "rdx"),
    };
    abi::emit_load_int_immediate(ctx.emitter, fd_reg, fd);
    abi::emit_symbol_address(ctx.emitter, url_reg, &path_symbol);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, path_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_php_fd_open");
}

/// Emits a boolean scalar as the current integer result.
pub(super) fn emit_bool_result(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        i64::from(value),
    );
}

/// Returns a literal string operand when the value was produced by `ConstStr`.
pub(super) fn optional_const_string_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<String>> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "string literal operand has no data id",
        ));
    };
    Ok(Some(
        ctx.module
            .data
            .strings
            .get(data.as_raw() as usize)
            .cloned()
            .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?,
    ))
}

/// Maps statically-known `php://` standard-stream URLs to native descriptors.
pub(super) fn php_standard_stream_fd(path: &str) -> Option<i64> {
    match path {
        "php://stdin" | "php://input" => Some(0),
        "php://stdout" | "php://output" => Some(1),
        "php://stderr" => Some(2),
        _ => None,
    }
}

/// Recognizes `php://fd/N` URLs and returns the descriptor embedded in the URL.
pub(super) fn php_fd_stream(path: &str) -> Option<i64> {
    php_fd_number(path.strip_prefix("php://fd/")?)
}

/// Reads the descriptor php-src's `php_stream_url_wrap_php` reads out of a `php://fd/` URL.
///
/// php-src runs `ZEND_STRTOL(start, &end, 10)` and refuses the URL with its own FORM sentence
/// when `end == start` or `*end != '\0'`; anything that DOES parse — including a negative number
/// and a leading-zero spelling — goes on to the range check, which words its refusal differently.
/// Measured on `php -n` 8.5.6: `php://fd/abc` and `php://fd/12abc` get the form sentence,
/// `php://fd/-1` gets the range sentence, and `php://fd/099` is descriptor 99.
///
/// `strtol` also skips leading whitespace, so php opens `php://fd/ 1`; this reader does not, and
/// answers the FORM sentence for it. The run-time dispatch in `__rt_php_fd_open`'s caller reads
/// the same shape, so the two agree with each other — which is the property that matters more
/// here than a space inside a URL.
///
/// The accumulation wraps rather than saturating, for the same reason: the assembly parser
/// multiplies and adds without an overflow check, and a URL spelling thirty digits should not
/// mean two different things depending on which dispatch saw it.
pub(super) fn php_fd_number(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    let (negative, digits) = match bytes.first() {
        Some(b'-') => (true, &bytes[1..]),
        Some(b'+') => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut value: i64 = 0;
    for byte in digits {
        value = value
            .wrapping_mul(10)
            .wrapping_add(i64::from(byte - b'0'));
    }
    Some(if negative { value.wrapping_neg() } else { value })
}

/// Recognizes in-memory `php://` stream URLs backed by the temp-file helper.
pub(super) fn is_php_memory_stream(path: &str) -> bool {
    path == "php://memory" || is_php_temp_stream(path)
}

/// Whether a literal path names `php://temp`, with or without its `/maxmemory:N` suffix.
///
/// Told apart from `php://memory` for ONE reason: php's temp stream wraps a memory stream and
/// registers both, so it consumes two resource ids where memory consumes one.
pub(super) fn is_php_temp_stream(path: &str) -> bool {
    path == "php://temp" || path.starts_with("php://temp/")
}

