//! Purpose:
//! Creates eval contexts and seeds top-level declared-symbol metadata.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Context creation also preserves regex-provider and PHP-profile registration.
//! - Core and user global constants are seeded through separate bridge registries so
//!   PHP's `Core` / `user` categories stay exact inside eval.

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
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_context_new");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::store_at_offset(ctx.emitter, result_reg, offset);
    register_eval_declared_symbols(ctx, offset);
    register_eval_native_global_constants(ctx, offset)?;
    register_eval_native_user_constants(ctx, offset);
    register_eval_native_functions(ctx, offset)?;
    register_eval_native_method_signatures(ctx, offset);
    // The runtime's `__rt_closure_bind` is shared by eval-free programs, so it reaches the
    // generated adapter wrapper through a slot the first eval context fills. Stored last: the
    // registration helpers above still expect the fresh context handle in the result register.
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_symbol_address(
        ctx.emitter,
        scratch,
        crate::codegen::eval_callable_helpers::EVAL_CALLBACK_WRAPPER_LABEL,
    );
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_elephc_eval_wrap_callback_fn", 0);
    ctx.emitter.label(&ready);
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_CONTEXT_HANDLE_OFFSET);
    Ok(())
}

/// Registers the AOT Core constant inventory with a newly allocated eval context.
pub(super) fn register_eval_native_global_constants(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
) -> Result<()> {
    let user_names = ctx
        .module
        .user_defined_constants
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut entries = ctx
        .module
        .global_constants
        .iter()
        .filter(|(name, _)| !user_names.contains(*name))
        .map(|(name, (value, ty))| (name.clone(), value.clone(), ty.clone()))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    for (name, value, ty) in entries {
        register_eval_native_global_constant(ctx, context_offset, &name, &value, &ty)?;
    }
    Ok(())
}

/// Emits one scalar AOT constant registration call with its exact PHP runtime type.
fn register_eval_native_global_constant(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    name: &str,
    value: &ExprKind,
    ty: &PhpType,
) -> Result<()> {
    let (kind, word, string_value) = eval_native_global_constant_abi_value(value, ty)?;
    register_eval_native_scalar_constant(
        ctx,
        context_offset,
        name,
        kind,
        word,
        string_value.as_deref(),
        "__elephc_eval_register_native_global_constant",
    );
    Ok(())
}

/// Registers the AOT user-declared constant inventory with a newly allocated eval context.
///
/// User constants stay in their own bridge registry so eval reports them under PHP's `user`
/// category instead of `Core`. A value AOT itself cannot materialize is skipped rather than
/// failing the compile; see `eval_native_user_constant_value`.
pub(super) fn register_eval_native_user_constants(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
) {
    let user_names = ctx
        .module
        .user_defined_constants
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut entries = ctx
        .module
        .global_constants
        .iter()
        .filter(|(name, _)| user_names.contains(*name))
        .map(|(name, (value, ty))| (name.clone(), value.clone(), ty.clone()))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    for (name, value, ty) in entries {
        let Some(encoded) = eval_native_user_constant_value(&value, &ty) else {
            continue;
        };
        register_eval_native_user_constant(ctx, context_offset, &name, &encoded);
    }
}

/// Emits one user-constant registration call for a scalar or array constant value.
fn register_eval_native_user_constant(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    name: &str,
    value: &EvalNativeUserConstantValue,
) {
    match value {
        EvalNativeUserConstantValue::Scalar {
            kind,
            word,
            string_value,
        } => register_eval_native_scalar_constant(
            ctx,
            context_offset,
            name,
            *kind,
            *word,
            string_value.as_deref(),
            "__elephc_eval_register_native_user_constant",
        ),
        EvalNativeUserConstantValue::Array(elements) => {
            register_eval_native_user_constant_array(ctx, context_offset, name, elements)
        }
    }
}

/// Emits one array-valued user-constant registration call with its encoded element spec.
fn register_eval_native_user_constant_array(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    name: &str,
    elements: &[EvalNativeCallableArrayDefaultElement],
) {
    let spec = encode_eval_native_array_default_elements(elements);
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    emit_eval_constant_name_args(ctx, name);
    let (spec_label, spec_len) = ctx.data.add_string(&spec);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &spec_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        spec_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_user_constant_array");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one scalar constant registration call against the requested bridge symbol.
///
/// The Core and user registries share this kind/payload ABI shape; only the callee differs.
fn register_eval_native_scalar_constant(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    name: &str,
    kind: i64,
    word: i64,
    string_value: Option<&str>,
    symbol_name: &str,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    emit_eval_constant_name_args(ctx, name);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        kind,
    );
    if let Some(string_value) = string_value {
        let (value_label, value_len) = ctx.data.add_string(string_value.as_bytes());
        abi::emit_symbol_address(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 4),
            &value_label,
        );
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 5),
            value_len as i64,
        );
    } else {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 4),
            word,
        );
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 5),
            0,
        );
    }
    let symbol = ctx.emitter.target.extern_symbol(symbol_name);
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Materializes one constant name into the shared name pointer/length argument pair.
fn emit_eval_constant_name_args(ctx: &mut FunctionContext<'_>, name: &str) {
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
}

/// Encodes one prescanned scalar constant for the eval registration ABI.
pub(super) fn eval_native_global_constant_abi_value(
    value: &ExprKind,
    ty: &PhpType,
) -> Result<(i64, i64, Option<String>)> {
    match value {
        ExprKind::Null => Ok((NATIVE_GLOBAL_CONSTANT_NULL, 0, None)),
        ExprKind::BoolLiteral(value) => Ok((
            NATIVE_GLOBAL_CONSTANT_BOOL,
            i64::from(*value),
            None,
        )),
        ExprKind::IntLiteral(value) if matches!(ty, PhpType::Resource(_)) => {
            Ok((NATIVE_GLOBAL_CONSTANT_RESOURCE, *value, None))
        }
        ExprKind::IntLiteral(value) => Ok((NATIVE_GLOBAL_CONSTANT_INT, *value, None)),
        ExprKind::FloatLiteral(value) => Ok((
            NATIVE_GLOBAL_CONSTANT_FLOAT,
            value.to_bits() as i64,
            None,
        )),
        ExprKind::StringLiteral(value) => Ok((
            NATIVE_GLOBAL_CONSTANT_STRING,
            0,
            Some(value.clone()),
        )),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => {
                Ok((NATIVE_GLOBAL_CONSTANT_INT, value.wrapping_neg(), None))
            }
            ExprKind::FloatLiteral(value) => Ok((
                NATIVE_GLOBAL_CONSTANT_FLOAT,
                (-value).to_bits() as i64,
                None,
            )),
            other => Err(CodegenIrError::unsupported(format!(
                "eval native global constant expression {:?}",
                other
            ))),
        },
        other => Err(CodegenIrError::unsupported(format!(
            "eval native global constant expression {:?}",
            other
        ))),
    }
}

/// Installs this binary's OPcache configuration in the eval bridge, which governs the
/// runtime script cache for dynamically included files.
///
/// The runtime cannot derive these values: `--ini` is a compile-time flag, so the
/// effective directive set exists only in the compiler. The bridge defaults to a
/// DISABLED cache, so a binary that never reaches this call — and every consumer
/// linking the archive without elephc's codegen — keeps the uncached behaviour.
///
/// CALLED FROM THE PROLOGUE, not from here. This used to run lazily at the first
/// `ensure_eval_context`, which made the cache's configuration depend on whether the
/// program had already executed an `eval()` — so `opcache_compile_file()` answered `false`
/// before the first one and `true` after, and `opcache_get_configuration()['blacklist']`
/// was empty then populated, for the same binary and the same directives. php-src
/// configures OPcache at startup, before a line of user code runs, and
/// `crate::codegen::frame` now does the same.
pub(crate) fn configure_eval_opcache(ctx: &mut FunctionContext<'_>) {
    let version_id = crate::codegen::compile_php_version().version_id();
    let overrides = crate::codegen_support::ini_overrides();
    let config = crate::opcache::runtime_cache::runtime_cache_config(
        version_id,
        crate::codegen_support::compile_is_web_sapi(),
        &overrides,
    );
    let arguments = [
        i64::from(config.enabled),
        i64::from(config.validate_timestamps),
        config.revalidate_freq as i64,
        config.max_file_size as i64,
        config.memory_consumption as i64,
        // THE RUNTIME TIER'S SHARE of the hash: the prime capacity minus the scripts compiled
        // into this binary, which php-src caches before any dynamic include and which hold a
        // slot each. MEASURED with `max_accelerated_files=200` (223 slots) and an absolute
        // entry path: reference admits 222 dynamic scripts beside the entry, 221 beside the
        // entry and one static `require`; elephc admitted 223 and reported more cached scripts
        // than `max_cached_keys`. (A RELATIVE entry path costs reference one more key — an
        // elephc binary has no invocation path, so the resolved form is the model.)
        config
            .max_accelerated_files
            .saturating_sub(crate::codegen_support::opcache_manifest_len() as u64) as i64,
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

/// Whether `opcache.restrict_api` denies this binary's OPcache API calls, as the bridge's
/// `swap_directive` addresses it. A compile-time verdict (see
/// `opcache_prelude::restrict_api_denies`), never an `ini_set()` target — reference makes the
/// directive `PHP_INI_SYSTEM`, and the prelude only maps the three `PHP_INI_ALL` ones to ids.
const OPCACHE_DIRECTIVE_API_RESTRICTED: i64 = 3;

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
        config.file_update_protection,
    );
    emit_opcache_directive_swap(
        ctx,
        OPCACHE_DIRECTIVE_API_RESTRICTED,
        i64::from(crate::codegen_support::opcache_api_restricted()),
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
