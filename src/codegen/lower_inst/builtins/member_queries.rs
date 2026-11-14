//! Purpose:
//! Lowers class-like existence, callable, method, and property metadata queries.
//!
//! Called from:
//! - `super` runtime-function and language-construct dispatch.
//!
//! Key details:
//! - Preserves PHP case-insensitive lookup, visibility, inheritance, and eval-bridge fallbacks.

use super::*;

/// Lowers AOT class/interface/enum existence checks for literal or dynamic string names.
pub(crate) fn lower_class_like_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    ensure_arg_count_between(inst, name, 1, 2)?;
    let value = expect_operand(inst, 0)?;
    // An intrinsic class (`Closure`, `WeakReference`, ...) always exists and has no class-table
    // row for the bridge to find: once a program could reach the eval bridge,
    // `class_exists('Closure', false)` answered false.
    if name == "class_exists" {
        if let Some(symbol_name) = maybe_const_string_operand(ctx, value)? {
            let bare = symbol_name.trim_start_matches('\\');
            if crate::types::builtin_classes::intrinsic_class_names()
                .any(|intrinsic| intrinsic.eq_ignore_ascii_case(bare))
            {
                emit_static_bool(ctx, true);
                return store_if_result(ctx, inst);
            }
        }
    }
    if matches!(name, "class_exists" | "interface_exists" | "trait_exists" | "enum_exists")
        && ctx.module.required_runtime_features.eval_bridge
    {
        return lower_bridge_class_like_exists(ctx, inst, value, name);
    }
    // Compiled interfaces with activation events must consult their request cell
    // even when the queried name is a literal; immutable metadata alone only
    // means the compiler retained a possible declaration.
    if name == "interface_exists" && ctx.module_has_interface_activation_events() {
        return lower_runtime_interface_exists(ctx, inst, value);
    }
    if let Some(symbol_name) = maybe_const_string_operand(ctx, value)? {
        let exists = class_like_names(ctx, name)
            .folded
            .contains(&php_symbol_key(symbol_name.trim_start_matches('\\')));
        if exists && emit_deferred_class_exists(ctx, inst, &symbol_name)? {
            return store_if_result(ctx, inst);
        }
        // A name no compiled class carries may still be a run-time `class_alias()` of one.
        if !exists
            && name == "class_exists"
            && crate::codegen::frame::module_calls_runtime_fn(ctx.module, |id| {
                *id == crate::ir::RuntimeFnId::ClassAlias
            })
        {
            lower_dynamic_class_like_exists(ctx, name, value)?;
            return store_if_result(ctx, inst);
        }
        emit_static_bool(ctx, exists);
    } else {
        lower_dynamic_class_like_exists(ctx, name, value)?;
    }
    store_if_result(ctx, inst)
}

/// Lowers interface existence through the native activation overlay.
fn lower_runtime_interface_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    value: ValueId,
) -> Result<()> {
    if ctx.value_php_type(value)?.codegen_repr() != PhpType::Str {
        return Err(CodegenIrError::unsupported(
            "interface_exists with non-string dynamic name",
        ));
    }
    let (pointer, length) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(value, pointer, length)?;
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let length_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    if name_arg != pointer {
        ctx.emitter.instruction(&format!("mov {}, {}", name_arg, pointer));
    }
    if length_arg != length {
        ctx.emitter.instruction(&format!("mov {}, {}", length_arg, length));
    }
    abi::emit_call_label(ctx.emitter, "__rt_interface_exists");
    store_if_result(ctx, inst)
}

/// Lowers class-like existence through the eval bridge when AOT code can register SPL loaders.
///
/// A function compiled before an `eval` call does not own an eval-context local, yet PHP still
/// requires its later `class_exists()` call to invoke request-global SPL callbacks. Passing a null
/// context selects the bridge's temporary lookup context, which checks generated metadata and
/// then dispatches those callbacks without depending on any framework-specific loader.
fn lower_bridge_class_like_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    value: ValueId,
    name: &str,
) -> Result<()> {
    const BRIDGE_STACK_BYTES: usize = 16;

    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_reserve_temporary_stack(ctx.emitter, BRIDGE_STACK_BYTES);
    ctx.load_string_value_to_regs(value, ptr_reg, len_reg)?;
    let context_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let name_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    let autoload_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_load_int_immediate(ctx.emitter, context_arg, 0);
    if name_arg != ptr_reg {
        ctx.emitter
            .instruction(&format!("mov {}, {}", name_arg, ptr_reg));
    }
    if name_len_arg != len_reg {
        ctx.emitter
            .instruction(&format!("mov {}, {}", name_len_arg, len_reg));
    }
    if let Some(&autoload) = inst.operands.get(1) {
        ctx.load_value_to_reg(autoload, autoload_arg)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, autoload_arg, 1);
    }
    let symbol = ctx.emitter.target.extern_symbol(&format!(
        "__elephc_eval_dynamic_{}",
        name
    ));
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_rethrow_probe_throwable(ctx, BRIDGE_STACK_BYTES);
    abi::emit_release_temporary_stack(ctx.emitter, BRIDGE_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Rethrows the Throwable an autoloader raised inside a class-like probe.
///
/// The bridge answers status 2 and parks the owned Throwable box where
/// `__elephc_eval_take_probe_throwable` returns it (Symfony's
/// `ClassExistenceResource::throwOnRequiredClass` throws out of `class_exists()`). Any other
/// status falls through with the probe's answer still in the result register. The throw happens
/// with the bridge's temporary stack released, as the eval status handler does.
pub(in crate::codegen::lower_inst::builtins) fn emit_rethrow_probe_throwable(
    ctx: &mut FunctionContext<'_>,
    release_bytes: usize,
) {
    let no_throw = ctx.next_label("class_probe_no_throwable");
    let result = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #2", result));            // did an autoloader raise a Throwable inside the probe?
            ctx.emitter.instruction(&format!("b.ne {}", no_throw));             // no: keep the probe's boolean answer
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 2", result));             // did an autoloader raise a Throwable inside the probe?
            ctx.emitter.instruction(&format!("jne {}", no_throw));              // no: keep the probe's boolean answer
        }
    }
    let take = ctx.emitter.target.extern_symbol("__elephc_eval_take_probe_throwable");
    abi::emit_call_label(ctx.emitter, &take);
    abi::emit_release_temporary_stack(ctx.emitter, release_bytes);
    abi::emit_call_label(ctx.emitter, "__rt_throwable_take_boxed");
    abi::emit_store_reg_to_symbol(ctx.emitter, result, "_exc_value", 0);
    abi::emit_call_label(ctx.emitter, "__rt_throw_current");
    ctx.emitter.label(&no_throw);
}

/// Lowers a dynamic string `class_exists()`-family lookup against known AOT metadata.
pub(in crate::codegen::lower_inst) fn lower_dynamic_class_like_exists(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    value: ValueId,
) -> Result<()> {
    if ctx.value_php_type(value)?.codegen_repr() != PhpType::Str {
        return Err(CodegenIrError::unsupported(format!(
            "{} with non-string dynamic name",
            name
        )));
    }
    let names = class_like_names(ctx, name);
    let candidates = names.sorted.as_slice();
    if candidates.is_empty() {
        emit_static_bool(ctx, false);
        return Ok(());
    }

    let list = packed_name_list(&candidates);
    if name == "class_exists" {
        // A run-time class_alias() name exists as the class it aliases.
        let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
        ctx.load_string_value_to_regs(value, ptr_reg, len_reg)?;
        abi::emit_call_label(ctx.emitter, "__rt_class_alias_resolve");
        emit_find_regs_in_packed_name_list(ctx, ptr_reg, len_reg, &list);
    } else {
        emit_find_in_packed_name_list(ctx, value, &list)?;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // did the shared lookup find the name?
            ctx.emitter.instruction("cset x0, ge");                             // a found index means the class-like exists
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("not rax");                                 // -1 (missing) becomes 0, an index stays non-zero
            ctx.emitter.instruction("shr rax, 63");                             // keep the sign of the index as the boolean
        }
    }
    Ok(())
}

/// Packs names into the `[u16 little-endian length][bytes]` list `__rt_name_list_find_ci` walks.
///
/// Each name is stored bare: the routine drops the leading `\` of the runtime name itself.
pub(in crate::codegen::lower_inst) fn packed_name_list<S: AsRef<str>>(names: &[S]) -> Vec<u8> {
    let mut list = Vec::new();
    for name in names {
        let bytes = crate::string_bytes::literal_bytes(name.as_ref().trim_start_matches('\\'));
        let len = u16::try_from(bytes.len()).expect("a class-like name fits in 64 KiB");
        list.extend_from_slice(&len.to_le_bytes());
        list.extend_from_slice(&bytes);
    }
    list
}

/// Looks the runtime string `value` up in a packed name list through the shared
/// `__rt_name_list_find_ci` routine, leaving the entry index (or -1) in the integer result
/// register. The list is content-addressed data, so every site asking about the same names
/// shares one copy of it.
pub(in crate::codegen::lower_inst) fn emit_find_in_packed_name_list(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    list: &[u8],
) -> Result<()> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(value, ptr_reg, len_reg)?;
    emit_find_regs_in_packed_name_list(ctx, ptr_reg, len_reg, list);
    Ok(())
}

/// [`emit_find_in_packed_name_list`] for a name already held in `ptr_reg`/`len_reg`.
pub(in crate::codegen::lower_inst) fn emit_find_regs_in_packed_name_list(
    ctx: &mut FunctionContext<'_>,
    ptr_reg: &str,
    len_reg: &str,
    list: &[u8],
) {
    let (list_label, list_len) = ctx.data.add_string(list);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if ptr_reg != "x1" {
                ctx.emitter.instruction(&format!("mov x1, {}", ptr_reg));       // name pointer operand
            }
            if len_reg != "x2" {
                ctx.emitter.instruction(&format!("mov x2, {}", len_reg));       // name length operand
            }
            abi::emit_symbol_address(ctx.emitter, "x3", &list_label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", list_len as i64);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rsi, {}", len_reg));          // name length operand
            ctx.emitter.instruction(&format!("mov rdi, {}", ptr_reg));          // name pointer operand
            abi::emit_symbol_address(ctx.emitter, "rdx", &list_label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", list_len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_name_list_find_ci");
}

/// Loads entry `index` (in the integer result register, from a successful
/// [`emit_find_in_packed_name_list`]) of a packed name list into the string result registers.
pub(in crate::codegen::lower_inst) fn emit_packed_name_list_entry(
    ctx: &mut FunctionContext<'_>,
    list: &[u8],
) {
    let (list_label, _) = ctx.data.add_string(list);
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_symbol_address(ctx.emitter, "x3", &list_label),
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // entry index operand
            abi::emit_symbol_address(ctx.emitter, "rsi", &list_label);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_name_list_entry");
}

/// Collects deterministic metadata and intrinsic names for literal and dynamic existence probes.
/// The names one `class_exists()`-family builtin can find: sorted, and folded for a constant
/// name's membership test. Built once per builtin and codegen state; every site rebuilt and
/// sorted the whole class table, and a constant name folded every class again.
pub(crate) struct ClassLikeNames {
    sorted: std::sync::Arc<Vec<String>>,
    folded: crate::fast_hash::FastSet<String>,
}

/// [`ClassLikeNames`] of builtin `name`, shared by every site of the codegen state.
fn class_like_names(ctx: &FunctionContext<'_>, name: &str) -> std::sync::Arc<ClassLikeNames> {
    ctx.shared.class_like_names(name, || {
        #[cfg(test)]
        CLASS_LIKE_NAME_BUILDS.with(|count| count.set(count.get() + 1));
        let sorted = class_like_exists_candidates(ctx, name);
        let folded = sorted.iter().map(|candidate| php_symbol_key(candidate.trim_start_matches('\\'))).collect();
        ClassLikeNames { sorted: std::sync::Arc::new(sorted), folded }
    })
}

#[cfg(test)]
thread_local! {
    /// Builds of a builtin's class-like name candidates on this thread.
    pub(crate) static CLASS_LIKE_NAME_BUILDS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn class_like_exists_candidates(ctx: &FunctionContext<'_>, name: &str) -> Vec<String> {
    let mut candidates = BTreeSet::new();
    match name {
        "class_exists" => {
            candidates.extend(crate::types::builtin_classes::intrinsic_class_names().map(str::to_owned));
            candidates.extend(
                ctx.module
                    .class_infos
                    .keys()
                    .filter(|class_name| !is_internal_synthetic_class_name(class_name))
                    .cloned(),
            );
        }
        "interface_exists" => candidates.extend(ctx.module.interface_infos.keys().cloned()),
        "trait_exists" => candidates.extend(ctx.module.trait_table.names.iter().cloned()),
        "enum_exists" => candidates.extend(ctx.module.enum_infos.keys().cloned()),
        _ => {}
    }
    candidates.into_iter().collect()
}

/// Emits one case-insensitive comparison of the string saved in the current 16-byte temporary
/// stack slot (`[sp] = pointer`, `[sp + 8] = length`) against a baked candidate, branching to
/// `matched_label` on equality.
///
/// Shared by the dynamic `class_exists()`-family lookup and dynamic `extension_loaded()`. The
/// caller owns the temporary slot: it must push the runtime string with `emit_push_reg_pair`
/// before the first comparison and release 16 bytes once all comparisons are done. `__rt_strcasecmp`
/// is a leaf helper (no nested call, no SP adjustment) that reads its operands from
/// `x1/x2/x3/x4` on AArch64 and `rdi/rsi/rdx/rcx` on x86_64 and clobbers only caller-saved
/// scratch, so the slot and SP are intact across every emitted comparison.
pub(in crate::codegen::lower_inst) fn emit_branch_if_saved_string_matches_ci(
    ctx: &mut FunctionContext<'_>,
    candidate: &[u8],
    matched_label: &str,
) {
    let (candidate_label, candidate_len) = ctx.data.add_string(candidate);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
            abi::emit_symbol_address(ctx.emitter, "x3", &candidate_label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", candidate_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("cmp x0, #0");                              // did the dynamic class-like name match this metadata entry?
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // report existence when the runtime name matches case-insensitively
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 8);
            abi::emit_symbol_address(ctx.emitter, "rdx", &candidate_label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", candidate_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("test rax, rax");                           // did the dynamic class-like name match this metadata entry?
            ctx.emitter.instruction(&format!("je {}", matched_label));          // report existence when the runtime name matches case-insensitively
        }
    }
}

/// Lowers `is_callable(value)` through static lookup or runtime callable-shape helpers.
pub(crate) fn lower_is_callable(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_callable", 1)?;
    let strict_php = super::instruction_strict_php_profile(inst);
    set_is_callable_strict_profile(ctx, strict_php);
    let value = expect_operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    if has_eval_context(ctx) && value_ty != PhpType::Callable {
        return lower_eval_is_callable(ctx, inst, value);
    }
    match value_ty {
        PhpType::Callable => emit_static_bool(ctx, true),
        PhpType::Str => {
            if let Ok(function_name) = const_string_operand(ctx, value) {
                if let Some((class_name, method_name)) = function_name.rsplit_once("::") {
                    emit_static_bool(ctx, static_method_string_is_callable(ctx, class_name, method_name));
                } else {
                    emit_static_bool(
                        ctx,
                        callable_name_exists(ctx, &function_name, strict_php),
                    );
                }
            } else {
                ctx.load_value_to_result(value)?;
                emit_is_callable_dynamic_string_lookup(ctx);
            }
        }
        PhpType::Array(_) => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_array");
        }
        PhpType::AssocArray { .. } => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_assoc");
        }
        PhpType::Object(_) => {
            ctx.load_value_to_result(value)?;
            if ctx.module.required_runtime_features.eval_bridge {
                // The object may be an instance of an INTERPRETED subclass (`$this` inside a
                // compiled base class method) whose `__invoke` the native tables cannot see.
                // Box it and resolve its owning interpreter context by identity first, exactly as
                // the boxed-Mixed form does.
                let null_object = ctx.next_label("is_callable_null_object");
                let done = ctx.next_label("is_callable_object_done");
                abi::emit_branch_if_int_result_zero(ctx.emitter, &null_object);
                crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
                let result_reg = abi::int_result_reg(ctx.emitter).to_string();
                abi::emit_push_reg(ctx.emitter, &result_reg);
                emit_eval_owned_is_callable_from_result(ctx)?;
                crate::codegen::lower_inst::callables::release_invoker_arg_preserving_result(ctx);
                abi::emit_jump(ctx.emitter, &done);
                ctx.emitter.label(&null_object);
                emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_object");
                ctx.emitter.label(&done);
                return store_if_result(ctx, inst);
            }
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_object");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            if ctx.module.required_runtime_features.eval_bridge {
                emit_eval_owned_is_callable_or_runtime(ctx, value)?;
                return store_if_result(ctx, inst);
            }
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_mixed");
        }
        PhpType::Iterable => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_heap");
        }
        PhpType::Int
        | PhpType::Bool
        | PhpType::False
        | PhpType::Float
        | PhpType::Void
        | PhpType::Never
        | PhpType::Narrow(_)
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Resource(_)
        | PhpType::TaggedScalar => {
            emit_static_bool(ctx, false);
        }
    }
    store_if_result(ctx, inst)
}

/// Tests a boxed Mixed callback through its retained eval Closure owner before native fallback.
///
/// A Closure returned by runtime include/eval can outlive the AOT frame that created its context.
/// The native mixed predicate cannot inspect Magician's closure registry, so recover the owning
/// context by object identity first; ordinary runtime values retain the existing native path.
fn emit_eval_owned_is_callable_or_runtime(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    ctx.load_value_to_result(value)?;
    emit_eval_owned_is_callable_from_result(ctx)
}

/// Body of [`emit_eval_owned_is_callable_or_runtime`] for a boxed callback already in the result
/// register; the box stays borrowed.
fn emit_eval_owned_is_callable_from_result(ctx: &mut FunctionContext<'_>) -> Result<()> {
    const CONTEXT_OFFSET: usize = 0;
    const CALLBACK_OFFSET: usize = 16;
    const SCRATCH_BYTES: usize = 32;

    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    let owner_missing = ctx.next_label("eval_owned_is_callable_runtime");
    let done = ctx.next_label("eval_owned_is_callable_done");

    abi::emit_reserve_temporary_stack(ctx.emitter, SCRATCH_BYTES);
    abi::emit_store_to_sp(ctx.emitter, &result_reg, CALLBACK_OFFSET);
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, callback_arg, CALLBACK_OFFSET);
    let owner_symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_callable_owner_context");
    abi::emit_call_label(ctx.emitter, &owner_symbol);
    abi::emit_store_to_sp(ctx.emitter, &result_reg, CONTEXT_OFFSET);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &owner_missing);

    let context_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, context_arg, CONTEXT_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, callback_arg, CALLBACK_OFFSET);
    let is_callable_symbol = ctx.emitter.target.extern_symbol("__elephc_eval_is_callable");
    abi::emit_call_label(ctx.emitter, &is_callable_symbol);
    abi::emit_release_temporary_stack(ctx.emitter, SCRATCH_BYTES);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&owner_missing);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, CALLBACK_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, SCRATCH_BYTES);
    emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_mixed");
    ctx.emitter.label(&done);
    Ok(())
}

/// Calls the runtime `is_callable` helper for pointer-shaped values already in result regs.
pub(in crate::codegen::lower_inst) fn emit_is_callable_pointer_lookup(ctx: &mut FunctionContext<'_>, label: &str) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // move pointer-shaped value into helper argument 0
    }
    abi::emit_call_label(ctx.emitter, label);
}

/// Stores the current `is_callable()` call site's builtin visibility for nested runtime helpers.
pub(in crate::codegen::lower_inst) fn set_is_callable_strict_profile(ctx: &mut FunctionContext<'_>, strict_php: bool) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_callable_strict_profile");
            abi::emit_load_int_immediate(ctx.emitter, "x10", i64::from(strict_php));
            ctx.emitter.instruction("str x10, [x9]");                           // publish the call-site builtin visibility for nested string probes
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r10", "_callable_strict_profile");
            abi::emit_load_int_immediate(ctx.emitter, "r11", i64::from(strict_php));
            ctx.emitter.instruction("mov QWORD PTR [r10], r11");                // publish the call-site builtin visibility for nested string probes
        }
    }
}

/// Calls the runtime `is_callable` string-name helper for a loaded dynamic string value.
pub(in crate::codegen::lower_inst) fn emit_is_callable_dynamic_string_lookup(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // move string pointer into helper argument 0
            ctx.emitter.instruction("mov x1, x2");                              // move string length into helper argument 1
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // move string pointer into helper argument 0
            ctx.emitter.instruction("mov rsi, rdx");                            // move string length into helper argument 1
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_is_callable_string");
}

/// Lowers `method_exists()` and `property_exists()` through eval or static metadata.
pub(in crate::codegen::lower_inst) fn lower_member_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    ensure_arg_count(inst, name, 2)?;
    let target = expect_operand(inst, 0)?;
    let member = expect_operand(inst, 1)?;
    if has_eval_context(ctx) {
        return lower_eval_member_exists(ctx, inst, target, member, name);
    }
    // An object an eval context made carries methods and properties the native class tables
    // never saw (an interpreted subclass of a compiled class). Its owner context answers first.
    if ctx.module.required_runtime_features.eval_bridge
        && matches!(
            ctx.value_php_type(target)?.codegen_repr(),
            PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)
        )
    {
        let matched = ctx.next_label("member_exists_eval_owner");
        let done = ctx.next_label("member_exists_done");
        emit_eval_member_exists_owner_probe(ctx, target, member, name, &matched)?;
        lower_native_member_exists(ctx, inst, target, member, name)?;
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&matched);
        emit_static_bool(ctx, true);
        if inst.result.is_some() && inst.result_php_type.codegen_repr() == PhpType::Mixed {
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
        }
        store_if_result(ctx, inst)?;
        ctx.emitter.label(&done);
        return Ok(());
    }
    lower_native_member_exists(ctx, inst, target, member, name)
}

/// Answers `method_exists()` / `property_exists()` from the native class tables.
fn lower_native_member_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: ValueId,
    member: ValueId,
    name: &str,
) -> Result<()> {
    let Some(member_name) = maybe_const_string_operand(ctx, member)? else {
        return super::member_exists::lower_dynamic_member_exists(
            ctx, inst, target, member, name,
        );
    };
    let exists = match ctx.value_php_type(target)?.codegen_repr() {
        PhpType::Object(_) => {
            // The declared type does not determine the runtime instance's members.
            return super::member_exists::lower_dynamic_member_exists(
                ctx, inst, target, member, name,
            );
        }
        PhpType::Str => {
            let Some(class_name) = maybe_const_string_operand(ctx, target)? else {
                return super::member_exists::lower_dynamic_member_exists(
                    ctx, inst, target, member, name,
                );
            };
            static_member_exists_on_class(ctx, &class_name, &member_name, name, false)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            return super::member_exists::lower_dynamic_member_exists(
                ctx, inst, target, member, name,
            );
        }
        // A callable value is a `Closure` object: twig-bridge's `DebugCommand` asks
        // `method_exists($cb, '__invoke')` of every closure a Twig function wraps.
        PhpType::Callable => name == "method_exists" && closure_declares_method(&member_name),
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} target PHP type {:?}",
                name, other
            )))
        }
    };
    emit_static_bool(ctx, exists);
    store_if_result(ctx, inst)
}

/// Checks one static class-like target for `method_exists()` or `property_exists()`.
pub(in crate::codegen::lower_inst) fn static_member_exists_on_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    member_name: &str,
    name: &str,
    target_is_object: bool,
) -> bool {
    let Some((resolved_class, class_info)) = lookup_class_info(ctx, class_name) else {
        return false;
    };
    match name {
        "method_exists" => static_method_exists_on_class_info(
            ctx,
            &resolved_class,
            class_info,
            member_name,
            target_is_object,
        ),
        "property_exists" => static_property_exists_on_class_info(
            &resolved_class,
            class_info,
            member_name,
            target_is_object,
        ),
        _ => false,
    }
}

/// Looks up class metadata using PHP's case-insensitive class-name semantics.
pub(in crate::codegen::lower_inst) fn lookup_class_info<'a>(
    ctx: &'a FunctionContext<'_>,
    class_name: &str,
) -> Option<(String, &'a ClassInfo)> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.module
        .class_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == class_key)
        .map(|(candidate, class_info)| (candidate.clone(), class_info))
}

/// Checks static method metadata while hiding inherited private methods on class-string targets.
pub(in crate::codegen::lower_inst) fn static_method_exists_on_class_info(
    ctx: &FunctionContext<'_>,
    resolved_class: &str,
    class_info: &ClassInfo,
    method_name: &str,
    target_is_object: bool,
) -> bool {
    let method_key = php_symbol_key(method_name);
    if elephc_builtin_contract::lookup_class(resolved_class)
        .is_some_and(|class| !class.method_is_php_visible(&method_key))
    {
        return false;
    }
    if class_info.methods.contains_key(&method_key) {
        return target_is_object
            || method_visible_from_class_string(
                resolved_class,
                &method_key,
                &class_info.method_visibilities,
                &class_info.method_declaring_classes,
            );
    }
    if class_info.static_methods.contains_key(&method_key) {
        return target_is_object
            || method_visible_from_class_string(
                resolved_class,
                &method_key,
                &class_info.static_method_visibilities,
                &class_info.static_method_declaring_classes,
            );
    }
    if target_is_object {
        return static_parent_chain_method_exists(ctx, class_info, &method_key);
    }
    false
}

/// Checks parent class metadata for private methods visible to object-target `method_exists()`.
pub(in crate::codegen::lower_inst) fn static_parent_chain_method_exists(
    ctx: &FunctionContext<'_>,
    class_info: &ClassInfo,
    method_key: &str,
) -> bool {
    let mut visited = BTreeSet::new();
    let mut parent_name = class_info.parent.as_deref();
    while let Some(candidate) = parent_name {
        let parent_key = php_symbol_key(candidate.trim_start_matches('\\'));
        if !visited.insert(parent_key) {
            return false;
        }
        let Some((_resolved_class, parent_info)) = lookup_class_info(ctx, candidate) else {
            return false;
        };
        if parent_info.methods.contains_key(method_key)
            || parent_info.static_methods.contains_key(method_key)
        {
            return true;
        }
        parent_name = parent_info.parent.as_deref();
    }
    false
}

/// Returns whether a method should be visible for a class-string member probe.
pub(in crate::codegen::lower_inst) fn method_visible_from_class_string<S1, S2>(
    resolved_class: &str,
    method_key: &str,
    visibilities: &std::collections::HashMap<String, Visibility, S1>,
    declaring_classes: &std::collections::HashMap<String, String, S2>,
) -> bool
where
    S1: std::hash::BuildHasher,
    S2: std::hash::BuildHasher,
{
    visibilities.get(method_key) != Some(&Visibility::Private)
        || declaring_classes
            .get(method_key)
            .is_none_or(|declaring_class| {
                php_symbol_key(declaring_class.trim_start_matches('\\'))
                    == php_symbol_key(resolved_class.trim_start_matches('\\'))
            })
}

/// Checks static property metadata while hiding inherited private properties.
pub(in crate::codegen::lower_inst) fn static_property_exists_on_class_info(
    resolved_class: &str,
    class_info: &ClassInfo,
    property_name: &str,
    _target_is_object: bool,
) -> bool {
    if property_name.starts_with("__elephc_cursor_") {
        return false;
    }
    if elephc_builtin_contract::lookup_class(resolved_class)
        .is_some_and(|class| class.property_is_hidden(property_name))
    {
        return false;
    }
    property_visible_from_class_string(
        resolved_class,
        property_name,
        &class_info.property_visibilities,
        &class_info.property_declaring_classes,
    ) || property_visible_from_class_string(
        resolved_class,
        property_name,
        &class_info.static_property_visibilities,
        &class_info.static_property_declaring_classes,
    )
}

/// Returns whether a property exists for a class-string or ordinary object probe.
pub(in crate::codegen::lower_inst) fn property_visible_from_class_string<S1, S2>(
    resolved_class: &str,
    property_name: &str,
    visibilities: &std::collections::HashMap<String, Visibility, S1>,
    declaring_classes: &std::collections::HashMap<String, String, S2>,
) -> bool
where
    S1: std::hash::BuildHasher,
    S2: std::hash::BuildHasher,
{
    let Some(visibility) = visibilities.get(property_name) else {
        return false;
    };
    visibility != &Visibility::Private
        || declaring_classes
            .get(property_name)
            .is_none_or(|declaring_class| {
                php_symbol_key(declaring_class.trim_start_matches('\\'))
                    == php_symbol_key(resolved_class.trim_start_matches('\\'))
            })
}

/// Returns true when a static `Class::method` string names a public static method.
pub(in crate::codegen::lower_inst) fn static_method_string_is_callable(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
) -> bool {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    // The module's folded-name index answers the first class folding to the key, as the scan
    // over every class (one folded copy each) did.
    let Some((_, class_info)) = ctx.module.class_info_by_key(&class_key) else {
        return false;
    };
    let method_key = php_symbol_key(method_name);
    if !class_info.static_methods.contains_key(&method_key) {
        return false;
    }
    class_info.static_method_visibilities.get(&method_key) == Some(&Visibility::Public)
}

/// Answers a literal existence probe from the class's request-scoped LOAD flag, when it has one.
///
/// A class the autoload pass pulled in only so a probe could be answered is DECLARED from the
/// first instruction but was never LOADED, which is the distinction `class_exists($n, false)`
/// exists to report. Returns false when the name carries no flag, leaving the ordinary constant
/// fold in place.
///
/// A probe whose autoload argument is not a literal also keeps the constant fold: deciding it
/// needs a runtime branch, and every occurrence of this idiom spells the argument out.
fn emit_deferred_class_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    symbol_name: &str,
) -> Result<bool> {
    let folded = symbol_name.trim_start_matches('\\').to_ascii_lowercase();
    if !ctx.module.deferred_class_loads.contains(&folded) {
        return Ok(false);
    }
    let autoloads = match inst.operands.get(1) {
        None => true,
        Some(operand) => match const_bool_operand(ctx, *operand)? {
            Some(value) => value,
            None => return Ok(false),
        },
    };
    let flag = crate::codegen::source_units::deferred_class_symbol(&folded);
    ctx.data.add_comm(flag.clone(), 8);
    let result = abi::int_result_reg(ctx.emitter);
    if autoloads {
        abi::emit_load_int_immediate(ctx.emitter, result, 1);
        abi::emit_store_reg_to_symbol(ctx.emitter, result, &flag, 0);
    } else {
        abi::emit_load_symbol_to_reg(ctx.emitter, result, &flag, 0);
    }
    Ok(true)
}

/// Reads a `const_bool` operand's compile-time value, or None when it is not one.
fn const_bool_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<bool>> {
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
    if inst_ref.op != Op::ConstBool {
        return Ok(None);
    }
    match inst_ref.immediate {
        Some(Immediate::Bool(value)) => Ok(Some(value)),
        _ => Ok(None),
    }
}

/// Whether php's `Closure` class declares `method` (case-insensitively).
pub(super) fn closure_declares_method(method: &str) -> bool {
    ["__invoke", "bind", "bindto", "call", "fromcallable"]
        .contains(&method.to_ascii_lowercase().as_str())
}
