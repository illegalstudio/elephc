//! Purpose:
//! Table-driven `new $class(...)`: one runtime name lookup and one shared tail replace the
//! per-class compare-and-construct arm of the generic dynamic-new ladder.
//!
//! Called from:
//! - `lower_dynamic_object_new_mixed()` (runtime argument container form) and
//!   `lower_dynamic_object_new_without_constructor_mixed()` in `fiber_dynamic_entry`.
//!
//! Key details:
//! - The generic ladder cost sites x classes: eight shared helpers each carried ~850 arms of
//!   ~470 instructions on the Symfony app (about 3.2 M instructions, 8% of the user code), and the
//!   constructorless helper carried more. An arm needed only per-class FACTS: the allocation
//!   template, the constructor's static descriptor, the class id and two flags. They are now
//!   one 64-byte record per class in a static table: name pointer, name length, template,
//!   descriptor (0 without a constructor), class id, property-hash offset (0 for none), flags
//!   (bit 0: allocating defaults left to the property-init thunk; bit 1: abstract; bit 2: enum;
//!   bit 3: Throwable, whose creation site the tail writes for THIS `new`), padding.
//! - `__rt_dynamic_new_lookup` finds the record by case-insensitive name. The tail allocates
//!   from the template (`__rt_object_new_template`), creates the property hash when the class has
//!   one, runs the property-init thunk for the allocating defaults, and then constructs through the
//!   generic uniform invoker with the site's argument container, exactly as the per-class arm did
//!   with its own descriptor.
//! - Classes this cannot describe stay explicit arms: object-owned reference cells, hidden
//!   ancestor slots, `stdClass` and the SPL storage classes. Throwable subclasses ARE records: the
//!   tail is emitted at the site, so it knows the line and file of the `new` and writes them when
//!   the record's Throwable flag is set (the constructorless form never recorded a creation site
//!   and still does not).
//! - Errors are unchanged: a name the table does not hold falls through to the same registry
//!   fallback, eval fallback and class-not-found fatal; argument errors come from the invoker.
//! - `ELEPHC_NO_DYNAMIC_NEW_TABLE=1` keeps the per-class arms.

use super::*;
use crate::codegen_support::data_section::DataWord;

/// Fewest table-eligible classes worth the lookup.
const MIN_TABLE_CLASSES: usize = 4;

/// Whether the environment switches the table off.
fn disabled() -> bool {
    std::env::var_os("ELEPHC_NO_DYNAMIC_NEW_TABLE").is_some()
}

/// Whether one candidate can be described by a table record.
fn eligible(ctx: &FunctionContext<'_>, candidate: &DynamicNewCandidate, _container_form: bool) -> bool {
    if !candidate.owned_reference_property_offsets.is_empty() || candidate.class_name == "stdClass" {
        return false;
    }
    // A record describes literal defaults only; a class whose defaults need its EIR initializer
    // thunk keeps its explicit arm, which runs the thunk.
    if candidate.property_initialization.uses_thunk() {
        return false;
    }
    // A Throwable records the file and line of the `new` that built it, which the tail knows (it
    // is emitted at the site): the record's Throwable flag makes the constructor form write them.
    // The constructorless form never recorded a creation site and still does not.
    // The SPL storage classes take the ordinary allocation in both forms (the container form
    // routes them through the generic arm, and the constructorless arm treats them like any class).
    let hidden = ctx
        .module
        .class_infos
        .get(&candidate.class_name)
        .is_some_and(|info| (0..info.properties.len()).any(|index| info.property_slot_is_hidden(index)));
    !hidden
}

/// Splits the candidates into the table-described ones and the explicit arms.
///
/// Returns `None` (everything stays an arm) when the switch is off or too few classes qualify.
pub(super) fn split_table_candidates(
    ctx: &FunctionContext<'_>,
    candidates: Vec<DynamicNewCandidate>,
    container_form: bool,
) -> (Vec<DynamicNewCandidate>, Vec<DynamicNewCandidate>) {
    if disabled() {
        return (Vec::new(), candidates);
    }
    let eligible_count = candidates
        .iter()
        .filter(|candidate| eligible(ctx, candidate, container_form))
        .count();
    if eligible_count < MIN_TABLE_CLASSES {
        return (Vec::new(), candidates);
    }
    candidates
        .into_iter()
        .partition(|candidate| eligible(ctx, candidate, container_form))
}

/// Builds the record table and returns its label.
pub(super) fn build_table(
    ctx: &mut FunctionContext<'_>,
    table_candidates: &[DynamicNewCandidate],
    with_constructor: bool,
) -> Result<String> {
    let mut words = Vec::with_capacity(table_candidates.len() * 8);
    for candidate in table_candidates {
        let (folded, rest) = partition_template_defaults(ctx, &candidate.property_initialization.inline_defaults());
        let template = object_template_label(
            ctx,
            candidate.class_id,
            candidate.property_count,
            candidate.allow_dynamic_properties,
            &candidate.uninitialized_marker_offsets,
            &folded,
        );
        let (name_label, name_len) = ctx.data.add_string(candidate.class_name.as_bytes());
        let descriptor = match (&candidate.constructor_impl, with_constructor) {
            (Some(constructor), true) => {
                DataWord::Symbol(constructor_container_descriptor(ctx, candidate, constructor)?)
            }
            _ => DataWord::U64(0),
        };
        let hash_offset = if candidate.allow_dynamic_properties {
            dynamic_property_hash_offset(candidate.property_count) as u64
        } else {
            0
        };
        words.extend([
            DataWord::Symbol(name_label),
            DataWord::U64(name_len as u64),
            DataWord::Symbol(template),
            descriptor,
            DataWord::U64(candidate.class_id),
            DataWord::U64(hash_offset),
            DataWord::U64(
                u64::from(!rest.is_empty())
                    | (u64::from(
                        ctx.module
                            .class_infos
                            .get(&candidate.class_name)
                            .is_some_and(|info| info.is_abstract),
                    ) << 1)
                    | (u64::from(ctx.module.enum_infos.contains_key(&candidate.class_name)) << 2)
                    | (u64::from(super::super::is_throwable_like_class(ctx, &candidate.class_name)) << 3),
            ),
            DataWord::U64(0),
        ]);
    }
    Ok(ctx.data.add_words(words))
}

/// Emits the lookup: with the class name staged at the top of the temporary stack, branches to
/// `hit_label` with the record in the result register, or falls through on a miss.
pub(super) fn emit_table_lookup(
    ctx: &mut FunctionContext<'_>,
    table_label: &str,
    count: usize,
    hit_label: &str,
) {
    let target = ctx.emitter.target;
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 0), 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 1), 8);
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(target, 2), table_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 3), count as i64);
    abi::emit_call_label(ctx.emitter, "__rt_dynamic_new_lookup");
    let miss = ctx.next_label("dynamic_new_table_miss");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &miss);
    abi::emit_jump(ctx.emitter, hit_label);
    ctx.emitter.label(&miss);
}

/// Emits the shared tail, entered with the record in the result register and the class name still
/// staged on the temporary stack. Leaves the owned boxed object in `result` and jumps to `done`.
pub(super) fn emit_table_tail(
    ctx: &mut FunctionContext<'_>,
    constructor_arg_container: Option<ValueId>,
    result: ValueId,
    done_label: &str,
    creation_line: u32,
) -> Result<()> {
    let target = ctx.emitter.target;
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_push_reg(ctx.emitter, result_reg); // [record]
    // -- an abstract class cannot be instantiated --
    let abstract_label = ctx.next_label("dynamic_new_table_abstract");
    match target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x9, [{result_reg}, #48]"));  // flags
            ctx.emitter.instruction("and x9, x9, #6");                          // abstract (2) or enum (4)
            ctx.emitter.instruction(&format!("cbnz x9, {abstract_label}"));     // refuse
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov r9, QWORD PTR [{result_reg} + 48]")); // flags
            ctx.emitter.instruction("test r9, 6");                              // abstract (2) or enum (4)?
            ctx.emitter.instruction(&format!("jnz {abstract_label}"));          // refuse
        }
    }
    abi::emit_load_from_address(ctx.emitter, abi::int_arg_reg_name(target, 0), result_reg, 16);
    abi::emit_call_label(ctx.emitter, "__rt_object_new_template");
    abi::emit_push_reg(ctx.emitter, result_reg); // [object][record]

    // -- property hash, for a class that has one --
    let no_hash = ctx.next_label("dynamic_new_table_no_hash");
    match target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #16]");                       // the record
            ctx.emitter.instruction("ldr x10, [x9, #40]");                      // property-hash offset (0 = none)
            ctx.emitter.instruction(&format!("cbz x10, {no_hash}"));            // no hash for this class
            abi::emit_load_int_immediate(ctx.emitter, "x0", 4);
            abi::emit_load_int_immediate(ctx.emitter, "x1", runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.instruction("mov x11, x0");                             // the new property hash
            ctx.emitter.instruction("ldr x9, [sp, #16]");                       // the record again
            ctx.emitter.instruction("ldr x10, [x9, #40]");                      // property-hash offset
            ctx.emitter.instruction("ldr x9, [sp]");                            // the object
            ctx.emitter.instruction("str x11, [x9, x10]");                      // publish the hash in the object
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 16]");            // the record
            ctx.emitter.instruction("mov r10, QWORD PTR [r9 + 40]");            // property-hash offset (0 = none)
            ctx.emitter.instruction("test r10, r10");                           // does the class have a property hash?
            ctx.emitter.instruction(&format!("jz {no_hash}"));                  // no hash for this class
            abi::emit_load_int_immediate(ctx.emitter, "rdi", 4);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.instruction("mov r11, rax");                            // the new property hash
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 16]");            // the record again
            ctx.emitter.instruction("mov r10, QWORD PTR [r9 + 40]");            // property-hash offset
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp]");                 // the object
            ctx.emitter.instruction("mov QWORD PTR [r9 + r10], r11");           // publish the hash in the object
        }
    }
    ctx.emitter.label(&no_hash);

    // -- allocating defaults through the property-init thunk --
    let no_thunk = ctx.next_label("dynamic_new_table_no_thunk");
    match target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #16]");                       // the record
            ctx.emitter.instruction("ldr x10, [x9, #48]");                      // flags
            ctx.emitter.instruction(&format!("tbz x10, #0, {no_thunk}"));       // no allocating defaults
            ctx.emitter.instruction("ldr x11, [x9, #32]");                      // class id
            abi::emit_symbol_address(ctx.emitter, "x10", "_class_propinit_ptrs");
            ctx.emitter.instruction("ldr x10, [x10, x11, lsl #3]");             // the class's thunk (0 = none)
            ctx.emitter.instruction(&format!("cbz x10, {no_thunk}"));           // nothing to run
            ctx.emitter.instruction("ldr x0, [sp]");                            // this = the new object
            ctx.emitter.instruction("blr x10");                                 // assign the allocating defaults
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 16]");            // the record
            ctx.emitter.instruction("mov r10, QWORD PTR [r9 + 48]");            // flags
            ctx.emitter.instruction("test r10, 1");                             // allocating defaults left to the thunk?
            ctx.emitter.instruction(&format!("jz {no_thunk}"));                 // none
            ctx.emitter.instruction("mov r11, QWORD PTR [r9 + 32]");            // class id
            abi::emit_symbol_address(ctx.emitter, "r10", "_class_propinit_ptrs");
            ctx.emitter.instruction("mov r10, QWORD PTR [r10 + r11 * 8]");      // the class's thunk (0 = none)
            ctx.emitter.instruction("test r10, r10");                           // is there one?
            ctx.emitter.instruction(&format!("jz {no_thunk}"));                 // nothing to run
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // this = the new object
            ctx.emitter.instruction("call r10");                                // assign the allocating defaults
        }
    }
    ctx.emitter.label(&no_thunk);

    // -- a Throwable records the file and line of this `new` --
    if constructor_arg_container.is_some() {
        let no_throwable = ctx.next_label("dynamic_new_table_no_throwable");
        match target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x9, [sp, #16]");                   // the record
                ctx.emitter.instruction("ldr x10, [x9, #48]");                  // flags
                ctx.emitter.instruction(&format!("tbz x10, #3, {no_throwable}")); // not a Throwable
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 16]");        // the record
                ctx.emitter.instruction("mov r10, QWORD PTR [r9 + 48]");        // flags
                ctx.emitter.instruction("test r10, 8");                         // a Throwable?
                ctx.emitter.instruction(&format!("jz {no_throwable}"));         // no
            }
        }
        let object_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
        super::throwable_new::emit_throwable_creation_site(ctx, object_reg, creation_line);
        ctx.emitter.label(&no_throwable);
    }

    // -- constructor, through the uniform invoker --
    if let Some(arg_container) = constructor_arg_container {
        let no_ctor = ctx.next_label("dynamic_new_table_no_ctor");
        let descriptor_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, descriptor_reg, 16);
        abi::emit_load_from_address(ctx.emitter, descriptor_reg, descriptor_reg, 24);
        match target.arch {
            Arch::AArch64 => ctx.emitter.instruction(&format!("cbz {descriptor_reg}, {no_ctor}")), // no constructor
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("test {descriptor_reg}, {descriptor_reg}")); // is there a constructor?
                ctx.emitter.instruction(&format!("jz {no_ctor}"));              // none
            }
        }
        let guard_bytes = super::super::callable_guards::begin_unconstructed_object(ctx.emitter);
        emit_table_constructor_call(ctx, arg_container, guard_bytes, guard_bytes + 16)?;
        super::super::callable_guards::end_unconstructed_object(ctx.emitter, guard_bytes);
        ctx.emitter.label(&no_ctor);
    }

    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Object(String::new()));
    ctx.store_result_value(result)?;
    abi::emit_jump(ctx.emitter, done_label);

    // -- `Cannot instantiate abstract class|enum <name>` (the record is the only staged slot) --
    ctx.emitter.label(&abstract_label);
    let enum_label = ctx.next_label("dynamic_new_table_enum");
    let record_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, record_reg, 0);
    match target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x9, [{record_reg}, #48]"));  // flags
            ctx.emitter.instruction(&format!("tbnz x9, #2, {enum_label}"));     // an enum
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov r9, QWORD PTR [{record_reg} + 48]")); // flags
            ctx.emitter.instruction("test r9, 4");                              // an enum?
            ctx.emitter.instruction(&format!("jnz {enum_label}"));              // yes
        }
    }
    emit_instantiation_refusal(ctx, "Cannot instantiate abstract class ");
    ctx.emitter.label(&enum_label);
    emit_instantiation_refusal(ctx, "Cannot instantiate enum ");
    Ok(())
}

/// Throws `<prefix><class name of the staged record>` as a catchable `Error`.
fn emit_instantiation_refusal(ctx: &mut FunctionContext<'_>, prefix: &str) {
    let (text_ptr, text_len) = abi::string_result_regs(ctx.emitter);
    let record_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, record_reg, 0);
    abi::emit_load_from_address(ctx.emitter, text_ptr, record_reg, 0);
    abi::emit_load_from_address(ctx.emitter, text_len, record_reg, 8);
    super::super::runtime_class_messages::emit_concat_static_prefix(ctx, prefix);
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    super::super::exceptions::emit_error_from_string_result(ctx);
}

/// Calls the selected constructor through a runtime copy of its static descriptor, the way the
/// per-class container arm did, with the descriptor chosen from the record at run time.
fn emit_table_constructor_call(
    ctx: &mut FunctionContext<'_>,
    arg_container: ValueId,
    object_stack_offset: usize,
    record_stack_offset: usize,
) -> Result<()> {
    let receiver_ty = PhpType::Object(String::new());
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    let descriptor_reg = abi::nested_call_reg(ctx.emitter).to_string();
    let total_bytes = callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 16;
    abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, object_stack_offset);
    abi::emit_incref_if_refcounted(ctx.emitter, &receiver_ty);
    abi::emit_push_reg(ctx.emitter, &result_reg);
    abi::emit_load_int_immediate(ctx.emitter, &result_reg, total_bytes as i64);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter
        .instruction(&format!("mov {}, {}", descriptor_reg, result_reg)); // preserve the runtime constructor descriptor while copying its header
    let source_reg = abi::symbol_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &source_reg, record_stack_offset + 16);
    abi::emit_load_from_address(ctx.emitter, &source_reg, &source_reg, 24);
    callable_descriptor::emit_copy_descriptor_template_to_runtime(ctx.emitter, &descriptor_reg, &source_reg);
    abi::emit_pop_reg(ctx.emitter, &result_reg);
    callable_descriptor::emit_store_current_result_to_runtime_capture(
        ctx.emitter,
        &descriptor_reg,
        0,
        &receiver_ty,
    );
    callables::emit_descriptor_reg_invoker_mixed_result_with_arg_container(
        ctx,
        &descriptor_reg,
        arg_container,
        "dynamic_constructor",
        true,
        true,
    )?;
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    Ok(())
}

/// The constructorless tail: allocation, hash and thunk only.
pub(super) fn emit_table_tail_without_constructor(
    ctx: &mut FunctionContext<'_>,
    result: ValueId,
    done_label: &str,
) -> Result<()> {
    emit_table_tail(ctx, None, result, done_label, 0)
}
