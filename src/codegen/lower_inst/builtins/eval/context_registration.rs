//! Purpose:
//! Creates eval contexts and seeds top-level declared-symbol metadata.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Context creation also preserves regex-provider and PHP-profile registration.

use super::*;

/// Ensures a persistent eval context exists and stores its handle in the scratch frame.
pub(super) fn ensure_eval_context(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let slot = eval_context_slot(ctx)?;
    let offset = ctx.local_offset(slot)?;
    let ready = ctx.next_label("eval_context_ready");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &ready);
    register_eval_regex_provider(ctx);
    configure_eval_opcache(ctx);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_context_new");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::store_at_offset(ctx.emitter, result_reg, offset);
    register_eval_declared_symbols(ctx, offset);
    register_eval_native_functions(ctx, offset)?;
    register_eval_native_method_signatures(ctx, offset);
    ctx.emitter.label(&ready);
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_CONTEXT_HANDLE_OFFSET);
    Ok(())
}

/// Installs this binary's OPcache configuration in the eval bridge, which governs the
/// runtime script cache for dynamically included files.
///
/// The runtime cannot derive these values: `--ini` is a compile-time flag, so the
/// effective directive set exists only in the compiler. The bridge defaults to a
/// DISABLED cache, so a binary that never reaches this call — and every consumer
/// linking the archive without elephc's codegen — keeps the uncached behaviour.
fn configure_eval_opcache(ctx: &mut FunctionContext<'_>) {
    let config = crate::opcache::runtime_cache::runtime_cache_config(
        crate::codegen::compile_php_version().version_id(),
        crate::codegen_support::compile_is_web_sapi(),
        &crate::codegen_support::ini_overrides(),
    );
    let arguments = [
        i64::from(config.enabled),
        i64::from(config.validate_timestamps),
        config.revalidate_freq as i64,
        config.max_file_size as i64,
        config.memory_consumption as i64,
        config.max_accelerated_files as i64,
    ];
    for (index, value) in arguments.into_iter().enumerate() {
        let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, index);
        abi::emit_load_int_immediate(ctx.emitter, arg_reg, value);
    }
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_configure_opcache");
    abi::emit_call_label(ctx.emitter, &symbol);
    configure_eval_opcache_file_cache(ctx, &config);
}

/// Installs the accelerator diagnostic channel and triggers php-src's startup validation
/// of `opcache.file_cache`.
///
/// A SECOND call, not four more arguments on the one above, because that one already
/// spends all six integer argument registers x86_64 provides — a seventh would need a
/// stack slot the emitter cannot express. It is emitted immediately after, because the
/// validation reads the enabled flag the first call installs.
///
/// The bridge fatals (and exits 254) on a bad directory exactly as reference PHP's
/// startup does, so this call can terminate the process before the program runs.
fn configure_eval_opcache_file_cache(
    ctx: &mut FunctionContext<'_>,
    config: &crate::opcache::runtime_cache::RuntimeCacheConfig,
) {
    let (file_cache_label, file_cache_len) = ctx.data.add_string(config.file_cache.as_bytes());
    let (error_log_label, error_log_len) = ctx.data.add_string(config.error_log.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        &file_cache_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        file_cache_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        i64::from(config.file_cache_read_only),
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        config.log_verbosity_level,
    );
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        &error_log_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        error_log_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_configure_opcache_file_cache");
    abi::emit_call_label(ctx.emitter, &symbol);
    load_eval_opcache_blacklist(ctx, config);
    configure_eval_opcache_swapped_directives(ctx, config);
}

/// Loads `opcache.blacklist_filename`, the paths that run but are never cached.
///
/// A THIRD call for the same reason there is a second one: the two above already spend
/// every integer argument register. Emitted AFTER the file-cache call because a directive
/// value matching no file logs through the accelerator channel that call installs.
///
/// Skipped entirely when the directive is unset, which is the overwhelmingly common case —
/// the bridge would return immediately, so the call would be pure code size.
fn load_eval_opcache_blacklist(
    ctx: &mut FunctionContext<'_>,
    config: &crate::opcache::runtime_cache::RuntimeCacheConfig,
) {
    if config.blacklist_filename.is_empty() {
        return;
    }
    let (label, len) = ctx.data.add_string(config.blacklist_filename.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        &label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_opcache_load_blacklist");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// `opcache.file_update_protection`, as the bridge's `swap_directive` addresses it.
///
/// The id is a wire contract with `elephc_magician::script_cache::config`; it is matched
/// by number across the C ABI, so it may never be reordered.
const OPCACHE_DIRECTIVE_FILE_UPDATE_PROTECTION: i64 = 2;

/// Carries the settings that did not fit either configure call's argument budget.
///
/// Both calls above spend all six integer argument registers x86_64 SysV provides, so
/// rather than a third fixed-shape call this reuses the same id/value setter `ini_set()`
/// uses at run time — one symbol serving the initial install and every later change.
fn configure_eval_opcache_swapped_directives(
    ctx: &mut FunctionContext<'_>,
    config: &crate::opcache::runtime_cache::RuntimeCacheConfig,
) {
    emit_opcache_directive_swap(
        ctx,
        OPCACHE_DIRECTIVE_FILE_UPDATE_PROTECTION,
        config.file_update_protection as i64,
    );
}

/// Emits one compiled-install `__elephc_eval_opcache_swap_directive` call.
///
/// Passes `as_override = 0`: this is the value `--ini` baked, not an `ini_set()`, so it
/// writes the base configuration and leaves any override in place.
fn emit_opcache_directive_swap(ctx: &mut FunctionContext<'_>, id: i64, value: i64) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), id);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        value,
    );
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_opcache_swap_directive");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Registers managed PCRE2 shim callbacks when regex is enabled for this binary.
pub(super) fn register_eval_regex_provider(ctx: &mut FunctionContext<'_>) {
    if !ctx.module.required_runtime_features.regex {
        return;
    }
    for (index, provider_symbol) in [
        "elephc_pcre2_v1_compile",
        "elephc_pcre2_v1_exec",
        "elephc_pcre2_v1_free",
    ]
    .into_iter()
    .enumerate()
    {
        let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, index);
        let symbol = ctx.emitter.target.extern_symbol(provider_symbol);
        abi::emit_symbol_address(ctx.emitter, arg_reg, &symbol);
    }
    let register = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_regex_provider");
    abi::emit_call_label(ctx.emitter, &register);
}

/// Writes the physical eval call site's strict profile before every runtime dispatch.
///
/// Writing both true and false prevents a strict eval from leaking its profile into
/// a later LFC eval that reuses the same persistent bridge context.
pub(super) fn mark_eval_strict_php(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    let strict_php = matches!(
        inst.immediate,
        Some(Immediate::ProfiledData {
            strict_php: true,
            ..
        })
    );
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_int_immediate(ctx.emitter, arg_reg, i64::from(strict_php));
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_set_strict_php");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Writes the compilation's PHP profile before every runtime dispatch.
///
/// Without this, `PHP_VERSION` and its siblings fork at the eval boundary: a binary compiled
/// `--php-version 8.2` would report `8.2.0` natively and `8.5.0` from inside `eval()`. The
/// bridge defaults to the newest profile, so this call is what makes the older ones true.
pub(super) fn mark_eval_php_version(ctx: &mut FunctionContext<'_>) {
    let version_id = i64::from(crate::codegen::compile_php_version().version_id());
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_int_immediate(ctx.emitter, arg_reg, version_id);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_set_php_version_id");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Returns the hidden frame slot that owns this function's persistent eval context.
pub(super) fn eval_context_slot(ctx: &FunctionContext<'_>) -> Result<LocalSlotId> {
    ctx.function
        .locals
        .iter()
        .find(|local| local.kind == LocalKind::EvalContext)
        .map(|local| local.id)
        .ok_or_else(|| CodegenIrError::invalid_module("eval call missing eval context local"))
}

/// Registers eligible AOT global functions with a newly allocated eval context.
pub(super) fn register_eval_native_functions(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
) -> Result<()> {
    let registrations = eval_native_function_registrations(ctx);
    for registration in registrations {
        register_eval_native_function(ctx, context_offset, &registration)?;
    }
    Ok(())
}

/// Registers eligible AOT method and constructor signatures with a newly allocated eval context.
pub(super) fn register_eval_native_method_signatures(ctx: &mut FunctionContext<'_>, context_offset: usize) {
    for registration in eval_native_method_registrations(ctx) {
        register_eval_native_method(ctx, context_offset, &registration);
    }
    for registration in eval_native_constructor_registrations(ctx) {
        register_eval_native_constructor(ctx, context_offset, &registration);
    }
    for registration in eval_native_property_type_registrations(ctx) {
        register_eval_native_property_type(ctx, context_offset, &registration);
    }
    for registration in eval_native_abstract_property_registrations(ctx) {
        register_eval_native_abstract_property(ctx, context_offset, &registration);
    }
    for registration in eval_native_interface_property_registrations(ctx) {
        register_eval_native_interface_property(ctx, context_offset, &registration);
    }
    for registration in eval_native_property_default_registrations(ctx) {
        register_eval_native_property_default(ctx, context_offset, &registration);
    }
    for registration in eval_native_member_attribute_registrations(ctx) {
        register_eval_native_member_attribute(ctx, context_offset, &registration);
    }
    register_eval_native_class_parents(ctx, context_offset);
}

/// Registers generated declared-name metadata with a newly allocated eval context.
pub(super) fn register_eval_declared_symbols(ctx: &mut FunctionContext<'_>, context_offset: usize) {
    let class_names = ctx.module.declared_class_names.clone();
    let interface_names = ctx.module.declared_interface_names.clone();
    let trait_names = ctx.module.declared_trait_names.clone();
    for name in class_names {
        register_eval_declared_symbol_name(
            ctx,
            context_offset,
            "__elephc_eval_register_declared_class_name",
            &name,
        );
    }
    for name in interface_names {
        register_eval_declared_symbol_name(
            ctx,
            context_offset,
            "__elephc_eval_register_declared_interface_name",
            &name,
        );
    }
    for name in trait_names {
        register_eval_declared_symbol_name(
            ctx,
            context_offset,
            "__elephc_eval_register_declared_trait_name",
            &name,
        );
    }
}

/// Emits one declared-name metadata registration call into the eval context.
pub(super) fn register_eval_declared_symbol_name(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    symbol_name: &str,
    name: &str,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let symbol = ctx.emitter.target.extern_symbol(symbol_name);
    abi::emit_call_label(ctx.emitter, &symbol);
}
