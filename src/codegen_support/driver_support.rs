//! Purpose:
//! Provides driver-level helpers that bridge generated user code with runtime conventions.
//! Emits runtime assembly fragments, deferred wrappers, and AST-owned hash-key normalization.
//!
//! Called from:
//! - `crate::codegen_support::generate()` and runtime-facing codegen helpers
//!
//! Key details:
//! - Runtime feature selection and deferred emission must stay deterministic for runtime caching.

use crate::parser::ast::Expr;
use crate::types::{ClassInfo, EnumInfo, PhpType};

use super::abi;
use super::context::{Context, HeapOwnership};
use super::data_section::DataSection;
use super::emit::Emitter;
use super::expr::{coerce_result_to_type, emit_expr, expr_result_heap_ownership};
use super::functions;
use super::platform::{Arch, Target};
use super::runtime;
use super::runtime_features::RuntimeFeatures;
use super::sentinels::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use super::value_boxing::{
    emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
};
use super::wrappers::{
    emit_callback_wrapper, emit_extern_callback_trampoline, emit_fiber_wrapper,
};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits a write syscall for a labeled literal string to stderr, using the given
/// label (from the data section) and its byte length. Handles target-specific
/// register conventions for the write syscall arguments.
pub(crate) fn emit_write_literal_stderr(emitter: &mut Emitter, label: &str, len: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            crate::codegen_support::abi::emit_symbol_address(emitter, "x1", label);     // load the page address of the stderr literal on AArch64
            emitter.instruction(&format!("mov x2, #{}", len));                  // materialize the stderr literal byte length in the AArch64 write-length register
            emitter.instruction("mov x0, #2");                                  // target the stderr file descriptor on AArch64
            emitter.syscall(4);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", label);
            emitter.instruction(&format!("mov edx, {}", len));                  // materialize the stderr literal byte length in the x86_64 write-length register
            emitter.instruction("mov edi, 2");                                  // target the stderr file descriptor on x86_64
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall number 1 = write
            emitter.instruction("syscall");                                     // write the requested literal bytes to stderr on x86_64
        }
    }
}

/// Emits a write syscall for the current string in result registers to stderr.
/// Loads pointer/length from the appropriate ABI registers for the target.
pub(crate) fn emit_write_current_string_stderr(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // target the stderr file descriptor on AArch64
            emitter.syscall(4);
        }
        Arch::X86_64 => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            emitter.instruction(&format!("mov rsi, {}", ptr_reg));              // move the current string pointer into the x86_64 write buffer register
            emitter.instruction(&format!("mov rdx, {}", len_reg));              // move the current string length into the x86_64 write length register
            emitter.instruction("mov edi, 2");                                  // target the stderr file descriptor on x86_64
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall number 1 = write
            emitter.instruction("syscall");                                     // write the current string payload to stderr on x86_64
        }
    }
}

/// Assembles the complete runtime assembly string for a given heap size and target.
#[allow(dead_code)]
pub fn generate_runtime(heap_size: usize, target: Target) -> String {
    generate_runtime_with_features(heap_size, target, RuntimeFeatures::all())
}

/// Assembles runtime assembly for the requested optional helper families.
pub fn generate_runtime_with_features(
    heap_size: usize,
    target: Target,
    features: RuntimeFeatures,
) -> String {
    generate_runtime_with_features_pic(heap_size, target, features, false)
}

/// Same as `generate_runtime_with_features` but emits position-independent
/// data references when `pic` is true. Required for the runtime object linked
/// into a `--emit cdylib` artifact, where cross-section symbol references must
/// resolve through the GOT instead of via direct PC-relative relocations.
pub fn generate_runtime_with_features_pic(
    heap_size: usize,
    target: Target,
    features: RuntimeFeatures,
    pic: bool,
) -> String {
    // macOS executables strip unreachable runtime helpers per-symbol: internal
    // labels are renamed to assembler-local (`L`-prefixed) labels and a
    // `.subsections_via_symbols` footer lets the linker's `-dead_strip` drop
    // whole unreferenced `__rt_*` helpers as single atoms. cdylibs (pic) never
    // strip, and Linux uses per-section `--gc-sections` instead, so both keep
    // the monolithic object.
    let dead_strip = !pic && target.platform == crate::codegen_support::platform::Platform::MacOS;
    let mut emitter = if pic {
        Emitter::new_pic(target)
    } else {
        Emitter::new(target)
    };
    emitter.dead_strip = dead_strip;
    emitter.emit_text_prelude();
    runtime::emit_runtime(&mut emitter, features);
    // Rename internal labels to `L`-locals in the runtime text only; the `.data`
    // below never references them, so it is appended unchanged.
    let internal_labels = emitter.take_internal_labels();
    let mut output = if dead_strip {
        crate::codegen_support::emit::localize_internal_labels(&emitter.output(), &internal_labels)
    } else {
        emitter.output()
    };
    output.push('\n');
    output.push_str(&runtime::emit_runtime_data_fixed(heap_size, target));
    // The PIC runtime object only ever links into an ELF cdylib, where every
    // runtime global must bind locally: hidden visibility prevents dynamic
    // preemption (two loaded elephc modules aliasing one runtime state) and
    // keeps the .so's dynamic symbol table down to the public ABI.
    if pic && target.platform == crate::codegen_support::platform::Platform::Linux {
        output = crate::codegen_support::visibility::append_hidden_directives(
            &output,
            &std::collections::HashSet::new(),
        );
    }
    // Footer that enables atom subdivision for `-dead_strip`. Emitted last so it
    // applies to the whole runtime object (text helpers and the `.data` table).
    if dead_strip {
        output.push_str(".subsections_via_symbols\n");
    }
    output
}

/// Emits global singleton initializers for all enum cases in sorted order.
pub(super) fn emit_enum_singleton_initializers(
    emitter: &mut Emitter,
    data: &mut DataSection,
    ctx: &Context,
    allowed_class_names: Option<&std::collections::HashSet<String>>,
) {
    let mut sorted_enums: Vec<(&String, &EnumInfo)> = ctx.enums.iter().collect();
    sorted_enums.sort_by_key(|(name, _)| name.as_str());
    for (enum_name, enum_info) in sorted_enums {
        if allowed_class_names.is_some_and(|allowed| !allowed.contains(enum_name)) {
            continue;
        }
        let Some(class_info) = ctx.classes.get(enum_name) else {
            continue;
        };
        for case in &enum_info.cases {
            emitter.comment(&format!("initialize enum singleton {}::{}", enum_name, case.name));
            let obj_size = 8 + class_info.properties.len() * 16;
            let result_reg = abi::int_result_reg(emitter);
            let object_reg = abi::symbol_scratch_reg(emitter);
            let temp_reg = abi::temp_int_reg(emitter.target);
            abi::emit_load_int_immediate(emitter, result_reg, obj_size as i64); // enum singleton object size in bytes in the heap allocator input register
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate enum singleton object storage
            abi::emit_load_int_immediate(emitter, temp_reg, 4);                 // heap kind 4 = object instance
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("str {}, [{}, #-8]", temp_reg, result_reg)); // store object kind in the uniform heap header just before the payload pointer
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("mov {}, 0x{:x}", temp_reg, (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize the x86_64 object heap kind word with the uniform heap marker
                    emitter.instruction(&format!("mov QWORD PTR [{} - 8], {}", result_reg, temp_reg)); // store object kind in the x86_64 uniform heap header just before the payload pointer
                }
            }
            abi::emit_load_int_immediate(emitter, temp_reg, class_info.class_id as i64); // load compile-time enum class id
            abi::emit_store_to_address(emitter, temp_reg, result_reg, 0);       // store enum class id at object header
            abi::emit_push_reg(emitter, result_reg);                            // save singleton object pointer while initializing properties

            for i in 0..class_info.properties.len() {
                let offset = 8 + i * 16;
                abi::emit_load_temporary_stack_slot(emitter, object_reg, 0);    // peek enum singleton pointer from the temporary stack slot
                abi::emit_store_zero_to_address(emitter, object_reg, offset);   // zero-initialize the low property word
                abi::emit_store_zero_to_address(emitter, object_reg, offset + 8); // zero-initialize the high property word
            }

            if let Some(case_value) = &case.value {
                abi::emit_load_temporary_stack_slot(emitter, object_reg, 0);    // reload enum singleton pointer for backing-value initialization
                match case_value {
                    crate::types::EnumCaseValue::Int(value) => {
                        load_immediate(emitter, temp_reg, *value);              // materialize the enum int backing value
                        abi::emit_store_to_address(emitter, temp_reg, object_reg, 8); // store the int backing value in the first property slot
                        abi::emit_store_zero_to_address(emitter, object_reg, 16); // clear the metadata/high word for the int property
                    }
                    crate::types::EnumCaseValue::Str(value) => {
                        let bytes = crate::string_bytes::literal_bytes(value);
                        let (label, len) = data.add_string(&bytes);
                        abi::emit_symbol_address(emitter, temp_reg, &label);    // materialize the enum string backing literal address
                        abi::emit_store_to_address(emitter, temp_reg, object_reg, 8); // store the string backing pointer in the first property slot
                        abi::emit_load_int_immediate(emitter, temp_reg, len as i64); // materialize the enum string backing length
                        abi::emit_store_to_address(emitter, temp_reg, object_reg, 16); // store the string backing length in the second property word
                    }
                }
            }

            abi::emit_pop_reg(emitter, result_reg);                             // pop initialized enum singleton pointer into the active integer result register
            let slot_label = crate::names::enum_case_symbol(enum_name, &case.name);
            abi::emit_store_reg_to_symbol(emitter, result_reg, &slot_label, 0); // publish the enum singleton pointer in its global slot
        }
    }
}

/// Emits initialization for static properties, including uninitialized sentinels.
///
/// `allowed_class_names` must match the filter used when emitting static-property *storage*
/// (`emit_runtime_data_user`): classes outside that set get no `.comm` slot, so initializing their
/// statics here would reference an undefined symbol. This matters for builtin/synthetic classes,
/// which are only emitted when actually used (unlike declared user classes); without the filter, a
/// declared-but-unused synthetic class carrying a static property (e.g. `DateTime`/`DateTimeImmutable`
/// sharing one) would emit an initializer for a slot that was never defined.
pub(super) fn emit_static_property_initializers(
    emitter: &mut Emitter,
    data: &mut DataSection,
    ctx: &mut Context,
    allowed_class_names: Option<&std::collections::HashSet<String>>,
) {
    let mut initializers = Vec::new();
    let mut uninitialized_static_properties = Vec::new();
    let mut sorted_classes: Vec<(&String, &ClassInfo)> = ctx.classes.iter().collect();
    sorted_classes.sort_by_key(|(class_name, _)| class_name.as_str());
    for (class_name, class_info) in sorted_classes {
        if allowed_class_names.is_some_and(|allowed| !allowed.contains(class_name.as_str())) {
            continue;
        }
        for (index, (property_name, prop_ty)) in class_info.static_properties.iter().enumerate() {
            let declaring_class = class_info
                .static_property_declaring_classes
                .get(property_name)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            if declaring_class != class_name {
                continue;
            }
            let default_expr = class_info.static_defaults.get(index).cloned().flatten();
            if default_expr.is_none() && class_info.declared_static_properties.contains(property_name) {
                uninitialized_static_properties.push((class_name.clone(), property_name.clone()));
            }
            let Some(default_expr) = default_expr else {
                continue;
            };
            let declared = class_info.declared_static_properties.contains(property_name);
            initializers.push((
                class_name.clone(),
                property_name.clone(),
                prop_ty.clone(),
                default_expr,
                declared,
            ));
        }
    }

    for (class_name, property_name) in uninitialized_static_properties {
        emitter.comment(&format!(
            "mark static property {}::${} uninitialized",
            class_name, property_name
        ));
        let marker_reg = abi::int_result_reg(emitter);
        abi::emit_load_int_immediate(emitter, marker_reg, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
        let symbol = crate::names::static_property_symbol(&class_name, &property_name);
        abi::emit_store_reg_to_symbol(emitter, marker_reg, &symbol, 8);
    }

    for (class_name, property_name, prop_ty, default_expr, declared) in initializers {
        emitter.comment(&format!(
            "initialize static property {}::${}",
            class_name, property_name
        ));
        let actual_ty = emit_expr(&default_expr, emitter, ctx, data);
        let store_ty = if declared {
            coerce_result_to_type(emitter, ctx, data, &actual_ty, &prop_ty);
            prop_ty
        } else {
            actual_ty
        };
        let symbol = crate::names::static_property_symbol(&class_name, &property_name);
        abi::emit_store_result_to_symbol(emitter, &symbol, &store_ty, false);
        if !matches!(store_ty.codegen_repr(), PhpType::Str) {
            abi::emit_store_zero_to_symbol(emitter, &symbol, 8);
        }
    }
}

/// Emits all deferred closures, fiber wrappers, and callback wrappers into the output.
pub(crate) fn emit_deferred_closures(
    emitter: &mut Emitter,
    data: &mut DataSection,
    ctx: &mut Context,
) {
    while !ctx.deferred_closures.is_empty()
        || !ctx.deferred_fiber_wrappers.is_empty()
        || !ctx.deferred_callback_wrappers.is_empty()
        || !ctx.deferred_extern_callback_trampolines.is_empty()
        || !ctx.deferred_runtime_callable_invokers.is_empty()
    {
        let closures: Vec<_> = ctx.deferred_closures.drain(..).collect();
        for closure in closures {
            if closure.needed {
                functions::emit_closure(
                    emitter,
                    data,
                    &closure.label,
                    &closure.sig,
                    &closure.hidden_params,
                    &closure.body,
                    closure.current_class.as_deref(),
                    &ctx.functions,
                    &ctx.callable_return_sigs,
                    &ctx.callable_array_return_sigs,
                    &ctx.fiber_return_sigs,
                    &ctx.function_variant_groups,
                    &ctx.constants,
                    &ctx.interfaces,
                    &ctx.traits,
                    &ctx.classes,
                    &ctx.enums,
                    &ctx.packed_classes,
                    &ctx.extern_functions,
                    &ctx.extern_classes,
                    &ctx.extern_globals,
                );
            } else {
                emitter.blank();
                emitter.comment(&format!("uninvoked FCC wrapper {} (stubbed)", closure.label));
                emitter.label_global(&closure.label);
                crate::codegen_support::abi::emit_load_int_immediate(
                    emitter,
                    crate::codegen_support::abi::int_result_reg(emitter),
                    0,
                );
                crate::codegen_support::abi::emit_return(emitter);
            }
        }
        let wrappers: Vec<_> = ctx.deferred_fiber_wrappers.drain(..).collect();
        for wrapper in wrappers {
            emit_fiber_wrapper(emitter, &wrapper);
        }
        let callback_wrappers: Vec<_> = ctx.deferred_callback_wrappers.drain(..).collect();
        for wrapper in callback_wrappers {
            emit_callback_wrapper(emitter, &wrapper);
        }
        let extern_trampolines: Vec<_> =
            ctx.deferred_extern_callback_trampolines.drain(..).collect();
        for trampoline in extern_trampolines {
            emit_extern_callback_trampoline(emitter, &trampoline);
        }
        let invokers: Vec<_> = ctx.deferred_runtime_callable_invokers.drain(..).collect();
        for invoker in invokers {
            crate::codegen_support::runtime_callable_invoker::emit_runtime_callable_invoker(
                emitter,
                data,
                ctx,
                &invoker,
            );
        }
    }
}

/// Emits code to push the main function's exception cleanup activation record.
pub(super) fn emit_main_activation_record_push(
    emitter: &mut Emitter,
    ctx: &Context,
    cleanup_label: &str,
) {
    let prev_offset = ctx
        .activation_prev_offset
        .expect("codegen bug: missing main activation prev slot");
    let cleanup_offset = ctx
        .activation_cleanup_offset
        .expect("codegen bug: missing main activation cleanup slot");
    let frame_base_offset = ctx
        .activation_frame_base_offset
        .expect("codegen bug: missing main activation frame-base slot");

    emitter.comment("register main exception cleanup frame");
    let scratch = abi::temp_int_reg(emitter.target);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_exc_call_frame_top", 0);
    abi::store_at_offset(emitter, scratch, prev_offset);                        // save the previous call-frame pointer in the main activation record
    abi::emit_symbol_address(emitter, scratch, cleanup_label);
    abi::store_at_offset(emitter, scratch, cleanup_offset);                     // save the main cleanup callback address in the activation record
    abi::emit_copy_frame_pointer(emitter, scratch);
    abi::store_at_offset(emitter, scratch, frame_base_offset);                  // save the main frame pointer in the activation record
    abi::emit_store_zero_to_local_slot(emitter, ctx.pending_action_offset.expect("codegen bug: missing main pending-action slot")); // clear any stale finally action before running main
    abi::emit_frame_slot_address(emitter, scratch, prev_offset);                // compute the address of the main activation record's first slot
    abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
}

/// Emits code to pop and restore the previous exception cleanup frame on main exit.
pub(super) fn emit_main_activation_record_pop(emitter: &mut Emitter, ctx: &Context) {
    let prev_offset = ctx
        .activation_prev_offset
        .expect("codegen bug: missing main activation prev slot");

    emitter.comment("unregister main exception cleanup frame");
    let scratch = abi::temp_int_reg(emitter.target);
    abi::load_at_offset(emitter, scratch, prev_offset);                         // reload the previous call-frame pointer from the main activation record
    abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
}

/// Emits the main cleanup callback label and body for exception unwinding.
pub(super) fn emit_main_cleanup_callback(
    emitter: &mut Emitter,
    cleanup_label: &str,
    ctx: &Context,
) {
    emitter.label(cleanup_label);
    abi::emit_cleanup_callback_prologue(emitter, abi::int_arg_reg_name(emitter.target, 0));
    functions::emit_owned_local_epilogue_cleanup(emitter, ctx, cleanup_label);
    abi::emit_cleanup_callback_epilogue(emitter);
    emitter.blank();
}

/// Boxes the current expression result as Mixed, applying ownership-aware handling for containers.
pub(crate) fn emit_box_current_expr_value_as_mixed_for_container(
    emitter: &mut Emitter,
    expr: &Expr,
    ty: &PhpType,
) {
    if !matches!(
        ty,
        PhpType::Str
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
            | PhpType::Callable
    ) || expr_result_heap_ownership(expr) != HeapOwnership::Owned
    {
        emit_box_current_value_as_mixed(emitter, ty);
        return;
    }

    emit_box_current_owned_value_as_mixed(emitter, ty);
}

/// Emits code to normalize an array key expression into the hash ABI (key_lo, key_hi registers).
pub(crate) fn emit_normalized_hash_key(
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let key_ty = emit_expr(expr, emitter, ctx, data).codegen_repr();
    match &key_ty {
        PhpType::Int | PhpType::Bool => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x1, x0");                              // move the integer array key payload into the normalized key low word
                emitter.instruction("mov x2, #-1");                             // key_hi sentinel marks the associative-array key as integer
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdx, -1");                             // key_hi sentinel marks the associative-array key as integer while rax keeps key_lo
            }
        },
        PhpType::Float => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("fcvtzs x1, d0");                           // PHP casts float array keys to integer keys
                emitter.instruction("mov x2, #-1");                             // key_hi sentinel marks the associative-array key as integer
            }
            Arch::X86_64 => {
                emitter.instruction("cvttsd2si rax, xmm0");                     // PHP casts float array keys to integer keys
                emitter.instruction("mov rdx, -1");                             // key_hi sentinel marks the associative-array key as integer
            }
        },
        PhpType::Str => {
            abi::emit_call_label(emitter, "__rt_hash_normalize_key");           // normalize numeric-string array keys to their integer PHP form
        }
        // PHP normalizes a null array key to the empty string "", so emit it as
        // a zero-length string key (key_hi = 0 signals a string key).
        PhpType::Void | PhpType::Never => {
            emit_empty_string_hash_key(emitter, data);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            let string_key = ctx.next_label("mixed_hash_key_string");
            let null_key = ctx.next_label("mixed_hash_key_null");
            let scalar_key = ctx.next_label("mixed_hash_key_scalar");
            let done = ctx.next_label("mixed_hash_key_done");
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("bl __rt_mixed_unbox");                 // decode the boxed key before normalizing it for hash storage
                    emitter.instruction("cmp x0, #1");                          // string mixed keys need PHP numeric-string normalization
                    emitter.instruction(&format!("b.eq {}", string_key));       // route string keys through the normal hash-key helper
                    emitter.instruction("cmp x0, #8");                          // null mixed keys normalize to the empty string like PHP
                    emitter.instruction(&format!("b.eq {}", null_key));          // route null keys to the empty-string key path
                    emitter.instruction("cmp x0, #0");                          // integer mixed keys are already scalar hash keys
                    emitter.instruction(&format!("b.eq {}", scalar_key));       // keep integer keys as integer hash keys
                    emitter.instruction("cmp x0, #3");                          // boolean mixed keys normalize like integer keys
                    emitter.instruction(&format!("b.eq {}", scalar_key));       // keep boolean keys as integer hash keys
                    emitter.instruction("mov x1, #0");                          // unsupported mixed key tags fall back to integer key zero
                    emitter.label(&scalar_key);
                    emitter.instruction("mov x2, #-1");                         // key_hi sentinel marks scalar mixed keys as integers
                    emitter.instruction(&format!("b {}", done));                // skip the string-key normalization path
                    emitter.label(&null_key);
                    emit_empty_string_hash_key_aarch64(emitter, data);          // null normalizes to the empty string "" hash key
                    emitter.instruction(&format!("b {}", done));               // skip the string-key normalization path
                    emitter.label(&string_key);
                    emitter.instruction("bl __rt_hash_normalize_key");          // normalize string mixed keys to PHP int/string hash keys
                    emitter.label(&done);
                }
                Arch::X86_64 => {
                    emitter.instruction("call __rt_mixed_unbox");               // decode the boxed key before normalizing it for hash storage
                    emitter.instruction("cmp rax, 1");                          // string mixed keys need PHP numeric-string normalization
                    emitter.instruction(&format!("je {}", string_key));         // route string keys through the normal hash-key helper
                    emitter.instruction("cmp rax, 8");                          // null mixed keys normalize to the empty string like PHP
                    emitter.instruction(&format!("je {}", null_key));           // route null keys to the empty-string key path
                    emitter.instruction("cmp rax, 0");                          // integer mixed keys are already scalar hash keys
                    emitter.instruction(&format!("je {}", scalar_key));         // keep integer keys as integer hash keys
                    emitter.instruction("cmp rax, 3");                          // boolean mixed keys normalize like integer keys
                    emitter.instruction(&format!("je {}", scalar_key));         // keep boolean keys as integer hash keys
                    emitter.instruction("xor eax, eax");                        // unsupported mixed key tags fall back to integer key zero
                    emitter.instruction("mov rdx, -1");                         // key_hi sentinel marks fallback mixed keys as integers
                    emitter.instruction(&format!("jmp {}", done));              // skip the string-key normalization path
                    emitter.label(&null_key);
                    emit_empty_string_hash_key_x86_64(emitter, data);           // null normalizes to the empty string "" hash key
                    emitter.instruction(&format!("jmp {}", done));             // skip the string-key normalization path
                    emitter.label(&scalar_key);
                    emitter.instruction("mov rax, rdi");                        // publish the unboxed scalar payload as key_lo
                    emitter.instruction("mov rdx, -1");                         // key_hi sentinel marks scalar mixed keys as integers
                    emitter.instruction(&format!("jmp {}", done));              // skip the string-key normalization path
                    emitter.label(&string_key);
                    emitter.instruction("mov rax, rdi");                        // move the unboxed string pointer into the hash normalizer input
                    emitter.instruction("call __rt_hash_normalize_key");        // normalize string mixed keys to PHP int/string hash keys
                    emitter.label(&done);
                }
            }
        }
        _ => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x1, x0");                              // treat unsupported key payloads as integer-like low words for the hash ABI
                emitter.instruction("mov x2, #-1");                             // key_hi sentinel marks the associative-array key as integer
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdx, -1");                             // treat unsupported key payloads as integer-like low words for the hash ABI
            }
        },
    }
    key_ty
}

/// Rounds `n` up to the nearest 16-byte boundary. Used to align stack frame sizes
/// and heap allocation sizes to the 16-byte ABI requirement on both AArch64 and x86_64.
pub(super) fn align16(n: usize) -> usize {
    (n + 15) & !15
}

/// Materializes an immediate i64 value into the given register via the target-aware
/// ABI helper (`emit_load_int_immediate`). Handles large immediates that may require
/// multiple instructions on the target architecture.
fn load_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    abi::emit_load_int_immediate(emitter, reg, value);                          // materialize the immediate through the shared target-aware helper
}

/// Emits the shared empty-string constant as a hash key pair for the active target.
///
/// PHP normalizes a null array key to the empty string `""`, which is a string
/// key: the low word holds the empty-string pointer and the high word is 0
/// (the string-key marker, distinct from the integer-key sentinel `-1`).
fn emit_empty_string_hash_key(emitter: &mut Emitter, data: &mut DataSection) {
    match emitter.target.arch {
        Arch::AArch64 => emit_empty_string_hash_key_aarch64(emitter, data),
        Arch::X86_64 => emit_empty_string_hash_key_x86_64(emitter, data),
    }
}

/// Emits the shared empty-string constant as the AArch64 hash key pair `x1`/`x2`.
fn emit_empty_string_hash_key_aarch64(emitter: &mut Emitter, data: &mut DataSection) {
    let (label, len) = data.add_string(b"");
    abi::emit_symbol_address(emitter, "x1", &label);
    abi::emit_load_int_immediate(emitter, "x2", len as i64);
}

/// Emits the shared empty-string constant as the x86_64 hash key pair `rax`/`rdx`.
fn emit_empty_string_hash_key_x86_64(emitter: &mut Emitter, data: &mut DataSection) {
    let (label, len) = data.add_string(b"");
    abi::emit_symbol_address(emitter, "rax", &label);
    abi::emit_load_int_immediate(emitter, "rdx", len as i64);
}
