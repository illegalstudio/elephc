//! Purpose:
//! Provides support emitters that bridge generated user code with runtime value conventions.
//! Boxes runtime payloads, emits runtime assembly fragments, and normalizes helper call results.
//!
//! Called from:
//! - `crate::codegen::generate()` and runtime-facing codegen helpers
//!
//! Key details:
//! - Mixed boxing and target register choices must match the runtime object layout exactly.

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

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits a write syscall for a labeled literal string to stderr, using the given
/// label (from the data section) and its byte length. Handles target-specific
/// register conventions for the write syscall arguments.
pub(crate) fn emit_write_literal_stderr(emitter: &mut Emitter, label: &str, len: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            crate::codegen::abi::emit_symbol_address(emitter, "x1", label);     // load the page address of the stderr literal on AArch64
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
    let mut emitter = if pic {
        Emitter::new_pic(target)
    } else {
        Emitter::new(target)
    };
    emitter.emit_text_prelude();
    runtime::emit_runtime(&mut emitter, features);
    let mut output = emitter.output();
    output.push('\n');
    output.push_str(&runtime::emit_runtime_data_fixed(heap_size));
    // The PIC runtime object only ever links into an ELF cdylib, where every
    // runtime global must bind locally: hidden visibility prevents dynamic
    // preemption (two loaded elephc modules aliasing one runtime state) and
    // keeps the .so's dynamic symbol table down to the public ABI.
    if pic && target.platform == crate::codegen::platform::Platform::Linux {
        output = crate::codegen::visibility::append_hidden_directives(
            &output,
            &std::collections::HashSet::new(),
        );
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
                crate::codegen::abi::emit_load_int_immediate(
                    emitter,
                    crate::codegen::abi::int_result_reg(emitter),
                    0,
                );
                crate::codegen::abi::emit_return(emitter);
            }
        }
        let wrappers: Vec<_> = ctx.deferred_fiber_wrappers.drain(..).collect();
        for wrapper in wrappers {
            functions::emit_fiber_wrapper(emitter, &wrapper);
        }
        let callback_wrappers: Vec<_> = ctx.deferred_callback_wrappers.drain(..).collect();
        for wrapper in callback_wrappers {
            functions::emit_callback_wrapper(emitter, &wrapper);
        }
        let extern_trampolines: Vec<_> =
            ctx.deferred_extern_callback_trampolines.drain(..).collect();
        for trampoline in extern_trampolines {
            functions::emit_extern_callback_trampoline(emitter, &trampoline);
        }
        let invokers: Vec<_> = ctx.deferred_runtime_callable_invokers.drain(..).collect();
        for invoker in invokers {
            crate::codegen::runtime_callable_invoker::emit_runtime_callable_invoker(
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

/// Returns the runtime value tag byte for a PhpType (used in heap header encoding).
pub(crate) fn runtime_value_tag(ty: &PhpType) -> u8 {
    match ty {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed => 7,
        PhpType::Union(_) => 7,
        PhpType::Iterable => 7,
        PhpType::Void => 8,
        PhpType::Resource(_) => 9,
        PhpType::Callable => 10,
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) | PhpType::Never => 0,
        PhpType::TaggedScalar => {
            unreachable!("TaggedScalar carries its runtime tag in the tag register, not a static tag")
        }
    }
}

/// Boxes raw register-based value components into a runtime Mixed cell via __rt_mixed_from_value.
pub(crate) fn emit_box_runtime_payload_as_mixed(
    emitter: &mut Emitter,
    value_tag_reg: &str,
    value_lo_reg: &str,
    value_hi_reg: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, {}", value_tag_reg));         // x0 = runtime value tag for the mixed boxing helper
            emitter.instruction(&format!("mov x1, {}", value_lo_reg));          // x1 = low payload word for the mixed boxing helper
            emitter.instruction(&format!("mov x2, {}", value_hi_reg));          // x2 = high payload word for the mixed boxing helper
            emitter.instruction("bl __rt_mixed_from_value");                    // retain/persist the payload as needed and return a boxed mixed cell
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", value_tag_reg));        // rax = runtime value tag for the mixed boxing helper
            emitter.instruction(&format!("mov rdi, {}", value_lo_reg));         // rdi = low payload word for the mixed boxing helper
            emitter.instruction(&format!("mov rsi, {}", value_hi_reg));         // rsi = high payload word for the mixed boxing helper
            emitter.instruction("call __rt_mixed_from_value");                  // box the payload into a temporary mixed cell on x86_64
        }
    }
}

/// Boxes the current expression result in the ABI result registers (x0/d0 or rax) into
/// a runtime Mixed cell, dispatching on the PHP type to emit the appropriate tag and
/// payload word setup. Ownership is not tracked here; callers must ensure the value
/// is safe to box (e.g., not a borrowed temporary that may be invalidated).
pub(crate) fn emit_box_current_value_as_mixed(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Mixed | PhpType::Union(_) => {}
        PhpType::Iterable => emit_box_iterable_as_mixed(emitter),
        PhpType::TaggedScalar => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x9, x0");                              // stage the tagged scalar payload while the tag moves into the helper tag register
                emitter.instruction("mov x0, x1");                              // pass the dynamic runtime tag as the mixed boxing helper tag argument
                emitter.instruction("mov x1, x9");                              // pass the tagged scalar payload as the mixed boxing helper low word
                emitter.instruction("mov x2, xzr");                             // tagged scalar payloads do not use a second word
                emitter.instruction("bl __rt_mixed_from_value");                // box the tagged scalar payload into a mixed cell
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, rax");                            // pass the tagged scalar payload as the mixed boxing helper low word
                emitter.instruction("mov rax, rdx");                            // pass the dynamic runtime tag as the mixed boxing helper tag argument
                emitter.instruction("xor rsi, rsi");                            // tagged scalar payloads do not use a second word
                emitter.instruction("call __rt_mixed_from_value");              // box the tagged scalar payload into a mixed cell
            }
        },
        PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never | PhpType::Resource(_) => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x1, x0");                              // move the current scalar payload into the mixed helper argument register
                emitter.instruction("mov x2, xzr");                             // scalar mixed payloads do not use a second word
                emitter.instruction(&format!("mov x0, #{}", runtime_value_tag(ty))); // materialize the static value tag for this scalar
                emitter.instruction("bl __rt_mixed_from_value");                // box the scalar payload into a mixed cell
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, rax");                            // move the current scalar payload into the mixed helper low-word register
                emitter.instruction("xor rsi, rsi");                            // scalar mixed payloads do not use a second word
                abi::emit_load_int_immediate(emitter, "rax", runtime_value_tag(ty) as i64);
                emitter.instruction("call __rt_mixed_from_value");              // box the scalar payload into a mixed cell
            }
        },
        PhpType::Float => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("fmov x1, d0");                             // move the current float bits into the mixed helper payload register
                emitter.instruction("mov x2, xzr");                             // float payloads only use the low word
                emitter.instruction("mov x0, #2");                              // runtime tag 2 = float
                emitter.instruction("bl __rt_mixed_from_value");                // box the float payload into a mixed cell
            }
            Arch::X86_64 => {
                emitter.instruction("movq rdi, xmm0");                          // move the current float bits into the mixed helper payload register
                emitter.instruction("xor rsi, rsi");                            // float payloads only use the low word
                abi::emit_load_int_immediate(emitter, "rax", 2);
                emitter.instruction("call __rt_mixed_from_value");              // box the float payload into a mixed cell
            }
        },
        PhpType::Str => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #1");                              // runtime tag 1 = string
                emitter.instruction("bl __rt_mixed_from_value");                // persist the string payload and box it into a mixed cell
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, rax");                            // move the current string pointer into the mixed helper low-word register
                emitter.instruction("mov rsi, rdx");                            // move the current string length into the mixed helper high-word register
                abi::emit_load_int_immediate(emitter, "rax", 1);
                emitter.instruction("call __rt_mixed_from_value");              // box the string payload into a mixed cell
            }
        },
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x1, x0");                          // move the current heap pointer into the mixed helper payload register
                    emitter.instruction("mov x2, xzr");                         // heap-backed payloads only use the low word
                    emitter.instruction(&format!("mov x0, #{}", runtime_value_tag(ty))); // materialize the heap payload tag for the mixed helper
                    emitter.instruction("bl __rt_mixed_from_value");            // retain the heap child and box it into a mixed cell
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rdi, rax");                        // move the current heap pointer into the mixed helper payload register
                    emitter.instruction("xor rsi, rsi");                        // heap-backed payloads only use the low word
                    abi::emit_load_int_immediate(emitter, "rax", runtime_value_tag(ty) as i64);
                    emitter.instruction("call __rt_mixed_from_value");          // box the heap child into a mixed cell
                }
            }
        }
        PhpType::Callable => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x1, x0");                              // move the callable descriptor into the mixed helper payload register
                emitter.instruction("mov x2, xzr");                             // callable descriptor payloads only use the low word
                emitter.instruction("mov x0, #10");                             // runtime tag 10 = callable descriptor
                emitter.instruction("bl __rt_mixed_from_value");                // retain the callable descriptor and box it into a mixed cell
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, rax");                            // move the callable descriptor into the mixed helper payload register
                emitter.instruction("xor rsi, rsi");                            // callable descriptor payloads only use the low word
                abi::emit_load_int_immediate(emitter, "rax", 10);
                emitter.instruction("call __rt_mixed_from_value");              // retain the callable descriptor and box it into a mixed cell
            }
        },
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x1, x0");                          // move the raw pointer into the mixed helper payload register
                    emitter.instruction("mov x2, xzr");                         // raw pointers only use the low word
                    emitter.instruction("mov x0, #0");                          // treat unsupported raw pointers as integer-like payloads for now
                    emitter.instruction("bl __rt_mixed_from_value");            // box the raw pointer bits into a mixed cell
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rdi, rax");                        // move the raw pointer into the mixed helper payload register
                    emitter.instruction("xor rsi, rsi");                        // raw pointers only use the low word
                    abi::emit_load_int_immediate(emitter, "rax", 0);
                    emitter.instruction("call __rt_mixed_from_value");          // box the raw pointer bits into a mixed cell
                }
            }
        }
    }
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

    match ty {
        PhpType::Str => emit_box_current_owned_string_as_mixed_for_container(emitter, ty),
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) | PhpType::Callable => {
            emit_box_current_owned_refcounted_as_mixed_for_container(emitter, ty);
        }
        _ => emit_box_current_value_as_mixed(emitter, ty),
    }
}

/// Releases the pushed temporary refcounted value after an array push operation.
pub(crate) fn emit_release_pushed_refcounted_temp_after_array_push(
    emitter: &mut Emitter,
    ty: &PhpType,
) {
    if !ty.is_refcounted() {
        return;
    }

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the updated array pointer while releasing the pushed temporary
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the pushed temporary pointer saved below the array result
            abi::emit_decref_if_refcounted(emitter, ty);
            emitter.instruction("ldr x0, [sp], #16");                           // restore the updated array pointer after releasing the pushed temporary
            emitter.instruction("add sp, sp, #16");                             // discard the saved pushed temporary pointer
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve a temporary slot for the updated array pointer
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // preserve the updated array pointer while releasing the pushed temporary
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // reload the pushed temporary pointer saved below the array result
            abi::emit_decref_if_refcounted(emitter, ty);
            emitter.instruction("mov rax, QWORD PTR [rsp]");                    // restore the updated array pointer after releasing the pushed temporary
            emitter.instruction("add rsp, 32");                                 // discard the array-result slot and the pushed temporary slot
        }
    }
}

/// Boxes an owned current result into Mixed and releases the original owner afterward.
pub(crate) fn emit_box_current_owned_value_as_mixed(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Str => emit_box_current_owned_string_as_mixed_for_container(emitter, ty),
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Iterable
        | PhpType::Object(_)
        | PhpType::Callable => {
            emit_box_current_owned_refcounted_as_mixed_for_container(emitter, ty);
        }
        _ => emit_box_current_value_as_mixed(emitter, ty),
    }
}

/// Boxes an owned string from x1/x2 (AArch64) or rax/rdx (x86_64) into a Mixed cell
/// while preserving and releasing the original string pointer/length after the Mixed
/// helper copies the payload. The original string is released via `__rt_heap_free_safe`
/// after the boxed copy is made.
fn emit_box_current_owned_string_as_mixed_for_container(emitter: &mut Emitter, ty: &PhpType) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the owned source string while boxing an owned copy into Mixed
            emit_box_current_value_as_mixed(emitter, ty);
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the boxed Mixed result while releasing the original string
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the original string pointer for safe heap release
            abi::emit_call_label(emitter, "__rt_heap_free_safe");               // release the original owned string after Mixed copied it
            emitter.instruction("ldr x0, [sp], #16");                           // restore the boxed Mixed result
            emitter.instruction("add sp, sp, #16");                             // discard the saved original string pointer and length
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");
            emit_box_current_value_as_mixed(emitter, ty);
            abi::emit_push_reg(emitter, "rax");
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // reload the original string pointer for safe heap release
            abi::emit_call_label(emitter, "__rt_heap_free_safe");               // release the original owned string after Mixed copied it
            abi::emit_pop_reg(emitter, "rax");
            emitter.instruction("add rsp, 16");                                 // discard the saved original string pointer and length
        }
    }
}

/// Boxes an owned refcounted value from the result register into a Mixed cell while
/// preserving the original heap pointer, boxing it, releasing the original via
/// decref, and restoring the boxed result. Used for owned arrays, iterables,
/// objects, and callables that must be transferred into a Mixed container without
/// double-freeing.
fn emit_box_current_owned_refcounted_as_mixed_for_container(emitter: &mut Emitter, ty: &PhpType) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the owned source heap value while boxing it into Mixed
            emit_box_current_value_as_mixed(emitter, ty);
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the boxed Mixed result while releasing the original owner
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the original heap value retained by the Mixed box
            abi::emit_decref_if_refcounted(emitter, ty);
            emitter.instruction("ldr x0, [sp], #16");                           // restore the boxed Mixed result
            emitter.instruction("add sp, sp, #16");                             // discard the saved original heap value pointer
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");
            emit_box_current_value_as_mixed(emitter, ty);
            abi::emit_push_reg(emitter, "rax");
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // reload the original heap value retained by the Mixed box
            abi::emit_decref_if_refcounted(emitter, ty);
            abi::emit_pop_reg(emitter, "rax");
            emitter.instruction("add rsp, 16");                                 // discard the saved original heap value pointer
        }
    }
}

/// Converts an Iterable to Mixed, returning true if the conversion was applied.
pub(crate) fn emit_box_iterable_value_for_mixed_container(
    emitter: &mut Emitter,
    ty: &mut PhpType,
) -> bool {
    if !matches!(ty, PhpType::Iterable) {
        return false;
    }
    emit_box_iterable_as_mixed(emitter);
    *ty = PhpType::Mixed;
    true
}

/// Probes the iterable's heap kind via `__rt_heap_kind`, maps it to the corresponding
/// Mixed tag (array→4, assoc→5, object→6), and boxes the iterable into a Mixed cell
/// via `__rt_mixed_from_value`. Preserves the iterable pointer across the kind probe
/// using a stack spill slot. Falls back to mixed tag 8 for unknown kinds.
fn emit_box_iterable_as_mixed(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the iterable heap pointer while probing its concrete heap kind
            emitter.instruction("bl __rt_heap_kind");                           // classify the raw iterable pointer by its heap-kind tag
            emitter.instruction("mov x9, x0");                                  // keep the heap kind available for tag normalization
            emitter.instruction("cmp x0, #2");                                  // is the heap kind at least the indexed-array tag?
            emitter.instruction("cset x10, hs");                                // record whether the iterable is in the supported heap-backed range lower bound
            emitter.instruction("cmp x0, #4");                                  // is the heap kind no greater than the object tag?
            emitter.instruction("cset x11, ls");                                // record whether the iterable is in the supported heap-backed range upper bound
            emitter.instruction("and x10, x10, x11");                           // combine the lower and upper bound checks into one predicate
            emitter.instruction("add x9, x9, #2");                              // map heap kind 2/3/4 to mixed tag 4/5/6
            emitter.instruction("mov x0, #8");                                  // default unknown iterable payloads to the null mixed tag
            emitter.instruction("cmp x10, #0");                                 // did the heap kind fall inside the supported iterable range?
            emitter.instruction("csel x0, x9, x0, ne");                         // choose the mapped concrete mixed tag when the range check succeeded
            emitter.instruction("ldr x1, [sp], #16");                           // restore the iterable heap pointer as the mixed payload low word
            emitter.instruction("mov x2, xzr");                                 // iterable payloads do not use a high payload word
            emitter.instruction("bl __rt_mixed_from_value");                    // retain the concrete heap payload and return an owned mixed cell
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");                                  // preserve the iterable heap pointer while probing its concrete heap kind
            emitter.instruction("call __rt_heap_kind");                         // classify the raw iterable pointer by its heap-kind tag
            emitter.instruction("mov r10, rax");                                // keep the heap kind available for tag normalization
            emitter.instruction("cmp rax, 2");                                  // is the heap kind at least the indexed-array tag?
            emitter.instruction("setae r11b");                                  // record whether the iterable is in the supported heap-backed range lower bound
            emitter.instruction("cmp rax, 4");                                  // is the heap kind no greater than the object tag?
            emitter.instruction("setbe dl");                                    // record whether the iterable is in the supported heap-backed range upper bound
            emitter.instruction("and dl, r11b");                                // combine the lower and upper bound checks into one predicate byte
            emitter.instruction("add r10, 2");                                  // map heap kind 2/3/4 to mixed tag 4/5/6
            emitter.instruction("mov rax, 8");                                  // default unknown iterable payloads to the null mixed tag
            emitter.instruction("test dl, dl");                                 // did the heap kind fall inside the supported iterable range?
            emitter.instruction("cmovne rax, r10");                             // choose the mapped concrete mixed tag when the range check succeeded
            abi::emit_pop_reg(emitter, "rdi");                                   // restore the iterable heap pointer as the mixed payload low word
            emitter.instruction("xor rsi, rsi");                                // iterable payloads do not use a high payload word
            emitter.instruction("call __rt_mixed_from_value");                  // retain the concrete heap payload and return an owned mixed cell
        }
    }
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
        PhpType::Mixed | PhpType::Union(_) => {
            let string_key = ctx.next_label("mixed_hash_key_string");
            let scalar_key = ctx.next_label("mixed_hash_key_scalar");
            let done = ctx.next_label("mixed_hash_key_done");
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("bl __rt_mixed_unbox");                 // decode the boxed key before normalizing it for hash storage
                    emitter.instruction("cmp x0, #1");                          // string mixed keys need PHP numeric-string normalization
                    emitter.instruction(&format!("b.eq {}", string_key));       // route string keys through the normal hash-key helper
                    emitter.instruction("cmp x0, #0");                          // integer mixed keys are already scalar hash keys
                    emitter.instruction(&format!("b.eq {}", scalar_key));       // keep integer keys as integer hash keys
                    emitter.instruction("cmp x0, #3");                          // boolean mixed keys normalize like integer keys
                    emitter.instruction(&format!("b.eq {}", scalar_key));       // keep boolean keys as integer hash keys
                    emitter.instruction("mov x1, #0");                          // unsupported mixed key tags fall back to integer key zero
                    emitter.label(&scalar_key);
                    emitter.instruction("mov x2, #-1");                         // key_hi sentinel marks scalar mixed keys as integers
                    emitter.instruction(&format!("b {}", done));                // skip the string-key normalization path
                    emitter.label(&string_key);
                    emitter.instruction("bl __rt_hash_normalize_key");          // normalize string mixed keys to PHP int/string hash keys
                    emitter.label(&done);
                }
                Arch::X86_64 => {
                    emitter.instruction("call __rt_mixed_unbox");               // decode the boxed key before normalizing it for hash storage
                    emitter.instruction("cmp rax, 1");                          // string mixed keys need PHP numeric-string normalization
                    emitter.instruction(&format!("je {}", string_key));         // route string keys through the normal hash-key helper
                    emitter.instruction("cmp rax, 0");                          // integer mixed keys are already scalar hash keys
                    emitter.instruction(&format!("je {}", scalar_key));         // keep integer keys as integer hash keys
                    emitter.instruction("cmp rax, 3");                          // boolean mixed keys normalize like integer keys
                    emitter.instruction(&format!("je {}", scalar_key));         // keep boolean keys as integer hash keys
                    emitter.instruction("xor eax, eax");                        // unsupported mixed key tags fall back to integer key zero
                    emitter.instruction("mov rdx, -1");                         // key_hi sentinel marks fallback mixed keys as integers
                    emitter.instruction(&format!("jmp {}", done));              // skip the string-key normalization path
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
