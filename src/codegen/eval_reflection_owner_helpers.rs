//! Purpose:
//! Emits user-assembly helpers that let libelephc-eval materialize
//! ReflectionClass, ReflectionMethod, ReflectionParameter, ReflectionProperty,
//! ReflectionClassConstant, ReflectionEnum*, and ReflectionType objects
//! with private metadata slots populated from runtime eval declarations.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` when an EIR module uses eval.
//!
//! Key details:
//! - Reflection owner objects store private metadata slots such as `__attrs`,
//!   `__name`, `__parameters`, and the ReflectionClass metadata-name arrays.
//! - The helper retains supplied array payloads for object ownership.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::ir::{Function, LocalKind, Module};
use crate::types::ClassInfo;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Fixed object layout for one synthetic Reflection owner class.
struct ReflectionOwnerLayout {
    class_id: u64,
    property_count: usize,
    name_lo: Option<usize>,
    name_hi: Option<usize>,
    short_name_lo: Option<usize>,
    short_name_hi: Option<usize>,
    namespace_name_lo: Option<usize>,
    namespace_name_hi: Option<usize>,
    interface_names_lo: Option<usize>,
    interface_names_hi: Option<usize>,
    trait_names_lo: Option<usize>,
    trait_names_hi: Option<usize>,
    method_names_lo: Option<usize>,
    method_names_hi: Option<usize>,
    property_names_lo: Option<usize>,
    property_names_hi: Option<usize>,
    method_objects_lo: Option<usize>,
    method_objects_hi: Option<usize>,
    property_objects_lo: Option<usize>,
    property_objects_hi: Option<usize>,
    constructor_lo: Option<usize>,
    constructor_hi: Option<usize>,
    parent_class_lo: Option<usize>,
    parent_class_hi: Option<usize>,
    value_lo: Option<usize>,
    value_hi: Option<usize>,
    backing_value_lo: Option<usize>,
    backing_value_hi: Option<usize>,
    attrs_lo: usize,
    attrs_hi: usize,
    is_final_lo: Option<usize>,
    is_final_hi: Option<usize>,
    is_abstract_lo: Option<usize>,
    is_abstract_hi: Option<usize>,
    is_interface_lo: Option<usize>,
    is_interface_hi: Option<usize>,
    is_trait_lo: Option<usize>,
    is_trait_hi: Option<usize>,
    is_enum_lo: Option<usize>,
    is_enum_hi: Option<usize>,
    is_readonly_lo: Option<usize>,
    is_readonly_hi: Option<usize>,
    is_instantiable_lo: Option<usize>,
    is_instantiable_hi: Option<usize>,
    is_cloneable_lo: Option<usize>,
    is_cloneable_hi: Option<usize>,
    is_iterable_lo: Option<usize>,
    is_iterable_hi: Option<usize>,
    is_internal_lo: Option<usize>,
    is_internal_hi: Option<usize>,
    is_user_defined_lo: Option<usize>,
    is_user_defined_hi: Option<usize>,
    modifiers_lo: Option<usize>,
    modifiers_hi: Option<usize>,
    in_namespace_lo: Option<usize>,
    in_namespace_hi: Option<usize>,
    is_static_lo: Option<usize>,
    is_static_hi: Option<usize>,
    is_public_lo: Option<usize>,
    is_public_hi: Option<usize>,
    is_protected_lo: Option<usize>,
    is_protected_hi: Option<usize>,
    is_private_lo: Option<usize>,
    is_private_hi: Option<usize>,
    is_enum_case_lo: Option<usize>,
    is_enum_case_hi: Option<usize>,
    required_parameter_count_lo: Option<usize>,
    required_parameter_count_hi: Option<usize>,
    position_lo: Option<usize>,
    position_hi: Option<usize>,
    is_optional_lo: Option<usize>,
    is_optional_hi: Option<usize>,
    is_variadic_lo: Option<usize>,
    is_variadic_hi: Option<usize>,
    is_passed_by_reference_lo: Option<usize>,
    is_passed_by_reference_hi: Option<usize>,
    has_type_lo: Option<usize>,
    has_type_hi: Option<usize>,
    parameter_type_lo: Option<usize>,
    parameter_type_hi: Option<usize>,
    has_default_value_lo: Option<usize>,
    has_default_value_hi: Option<usize>,
    default_value_lo: Option<usize>,
    default_value_hi: Option<usize>,
    declaring_function_lo: Option<usize>,
    declaring_function_hi: Option<usize>,
    allows_null_lo: Option<usize>,
    allows_null_hi: Option<usize>,
    is_builtin_lo: Option<usize>,
    is_builtin_hi: Option<usize>,
}

/// Layouts for the Reflection owner classes eval can materialize.
struct ReflectionOwnerLayouts {
    class: ReflectionOwnerLayout,
    method: ReflectionOwnerLayout,
    property: ReflectionOwnerLayout,
    class_constant: ReflectionOwnerLayout,
    enum_unit_case: ReflectionOwnerLayout,
    enum_backed_case: ReflectionOwnerLayout,
    parameter: ReflectionOwnerLayout,
    named_type: ReflectionOwnerLayout,
    union_type: ReflectionOwnerLayout,
    intersection_type: ReflectionOwnerLayout,
}

/// Emits eval Reflection owner helpers when any lowered function owns an eval context.
pub(super) fn emit_eval_reflection_owner_helpers(module: &Module, emitter: &mut Emitter) {
    if !module_uses_eval(module) {
        return;
    }
    emitter.blank();
    emitter.comment("--- eval bridge: reflection owner helpers ---");
    label_c_global(module, emitter, "__elephc_eval_reflection_owner_new");
    let Some(layouts) = reflection_owner_layouts(module) else {
        emit_reflection_owner_new_stub(emitter);
        return;
    };
    match module.target.arch {
        Arch::AArch64 => emit_reflection_owner_new_aarch64(emitter, &layouts),
        Arch::X86_64 => emit_reflection_owner_new_x86_64(emitter, &layouts),
    }
}

/// Returns true when the EIR module contains a function that can call eval.
fn module_uses_eval(module: &Module) -> bool {
    all_module_functions(module).any(function_uses_eval)
}

/// Iterates every EIR function body emitted or inspected by the backend.
fn all_module_functions(module: &Module) -> impl Iterator<Item = &Function> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
}

/// Returns true when a function has hidden eval state locals.
fn function_uses_eval(function: &Function) -> bool {
    function.locals.iter().any(|local| {
        matches!(
            local.kind,
            LocalKind::EvalContext | LocalKind::EvalScope | LocalKind::EvalGlobalScope
        )
    })
}

/// Returns the Reflection owner object layouts from class metadata.
fn reflection_owner_layouts(module: &Module) -> Option<ReflectionOwnerLayouts> {
    Some(ReflectionOwnerLayouts {
        class: reflection_owner_layout(module.class_infos.get("ReflectionClass")?, true)?,
        method: reflection_owner_layout(module.class_infos.get("ReflectionMethod")?, true)?,
        property: reflection_owner_layout(module.class_infos.get("ReflectionProperty")?, true)?,
        class_constant: reflection_owner_layout(
            module.class_infos.get("ReflectionClassConstant")?,
            true,
        )?,
        enum_unit_case: reflection_owner_layout(
            module.class_infos.get("ReflectionEnumUnitCase")?,
            true,
        )?,
        enum_backed_case: reflection_owner_layout(
            module.class_infos.get("ReflectionEnumBackedCase")?,
            true,
        )?,
        parameter: reflection_owner_layout(module.class_infos.get("ReflectionParameter")?, true)?,
        named_type: reflection_owner_layout(module.class_infos.get("ReflectionNamedType")?, true)?,
        union_type: reflection_owner_layout(module.class_infos.get("ReflectionUnionType")?, false)?,
        intersection_type: reflection_owner_layout(
            module.class_infos.get("ReflectionIntersectionType")?,
            false,
        )?,
    })
}

/// Returns one Reflection owner layout from class metadata.
fn reflection_owner_layout(info: &ClassInfo, has_name: bool) -> Option<ReflectionOwnerLayout> {
    let attrs_lo = reflection_property_offset(info, "__attrs")?;
    let name_lo = has_name
        .then(|| reflection_property_offset(info, "__name"))
        .flatten();
    let short_name_lo = reflection_property_offset(info, "__short_name");
    let namespace_name_lo = reflection_property_offset(info, "__namespace_name");
    let interface_names_lo = reflection_property_offset(info, "__interface_names");
    let trait_names_lo = reflection_property_offset(info, "__trait_names");
    let method_names_lo = reflection_property_offset(info, "__method_names");
    let property_names_lo = reflection_property_offset(info, "__property_names");
    let method_objects_lo = reflection_property_offset(info, "__methods")
        .or_else(|| reflection_property_offset(info, "__parameters"))
        .or_else(|| reflection_property_offset(info, "__types"));
    let property_objects_lo = reflection_property_offset(info, "__properties");
    let constructor_lo = reflection_property_offset(info, "__constructor");
    let parent_class_lo = reflection_property_offset(info, "__parent_class")
        .or_else(|| reflection_property_offset(info, "__declaring_class"));
    let value_lo = reflection_property_offset(info, "__value");
    let backing_value_lo = reflection_property_offset(info, "__backing_value");
    let is_final_lo = reflection_property_offset(info, "__is_final");
    let is_abstract_lo = reflection_property_offset(info, "__is_abstract");
    let is_interface_lo = reflection_property_offset(info, "__is_interface");
    let is_trait_lo = reflection_property_offset(info, "__is_trait");
    let is_enum_lo = reflection_property_offset(info, "__is_enum");
    let is_readonly_lo = reflection_property_offset(info, "__is_readonly");
    let is_instantiable_lo = reflection_property_offset(info, "__is_instantiable");
    let is_cloneable_lo = reflection_property_offset(info, "__is_cloneable");
    let is_iterable_lo = reflection_property_offset(info, "__is_iterable");
    let is_internal_lo = reflection_property_offset(info, "__is_internal");
    let is_user_defined_lo = reflection_property_offset(info, "__is_user_defined");
    let modifiers_lo = reflection_property_offset(info, "__modifiers");
    let in_namespace_lo = reflection_property_offset(info, "__in_namespace");
    let is_static_lo = reflection_property_offset(info, "__is_static");
    let is_public_lo = reflection_property_offset(info, "__is_public");
    let is_protected_lo = reflection_property_offset(info, "__is_protected");
    let is_private_lo = reflection_property_offset(info, "__is_private");
    let is_enum_case_lo = reflection_property_offset(info, "__is_enum_case");
    let required_parameter_count_lo =
        reflection_property_offset(info, "__required_parameter_count");
    let position_lo = reflection_property_offset(info, "__position");
    let is_optional_lo = reflection_property_offset(info, "__is_optional")
        .or_else(|| reflection_property_offset(info, "__optional"));
    let is_variadic_lo = reflection_property_offset(info, "__is_variadic")
        .or_else(|| reflection_property_offset(info, "__variadic"));
    let is_passed_by_reference_lo = reflection_property_offset(info, "__is_passed_by_reference");
    let has_type_lo = reflection_property_offset(info, "__has_type");
    let parameter_type_lo = reflection_property_offset(info, "__type");
    let has_default_value_lo = reflection_property_offset(info, "__has_default_value");
    let default_value_lo = reflection_property_offset(info, "__default_value");
    let declaring_function_lo = reflection_property_offset(info, "__declaring_function");
    let allows_null_lo = reflection_property_offset(info, "__allows_null");
    let is_builtin_lo = reflection_property_offset(info, "__is_builtin")
        .or_else(|| reflection_property_offset(info, "__builtin"));
    Some(ReflectionOwnerLayout {
        class_id: info.class_id,
        property_count: info.properties.len(),
        name_lo,
        name_hi: name_lo.map(|offset| offset + 8),
        short_name_lo,
        short_name_hi: short_name_lo.map(|offset| offset + 8),
        namespace_name_lo,
        namespace_name_hi: namespace_name_lo.map(|offset| offset + 8),
        interface_names_lo,
        interface_names_hi: interface_names_lo.map(|offset| offset + 8),
        trait_names_lo,
        trait_names_hi: trait_names_lo.map(|offset| offset + 8),
        method_names_lo,
        method_names_hi: method_names_lo.map(|offset| offset + 8),
        property_names_lo,
        property_names_hi: property_names_lo.map(|offset| offset + 8),
        method_objects_lo,
        method_objects_hi: method_objects_lo.map(|offset| offset + 8),
        property_objects_lo,
        property_objects_hi: property_objects_lo.map(|offset| offset + 8),
        constructor_lo,
        constructor_hi: constructor_lo.map(|offset| offset + 8),
        parent_class_lo,
        parent_class_hi: parent_class_lo.map(|offset| offset + 8),
        value_lo,
        value_hi: value_lo.map(|offset| offset + 8),
        backing_value_lo,
        backing_value_hi: backing_value_lo.map(|offset| offset + 8),
        attrs_lo,
        attrs_hi: attrs_lo + 8,
        is_final_lo,
        is_final_hi: is_final_lo.map(|offset| offset + 8),
        is_abstract_lo,
        is_abstract_hi: is_abstract_lo.map(|offset| offset + 8),
        is_interface_lo,
        is_interface_hi: is_interface_lo.map(|offset| offset + 8),
        is_trait_lo,
        is_trait_hi: is_trait_lo.map(|offset| offset + 8),
        is_enum_lo,
        is_enum_hi: is_enum_lo.map(|offset| offset + 8),
        is_readonly_lo,
        is_readonly_hi: is_readonly_lo.map(|offset| offset + 8),
        is_instantiable_lo,
        is_instantiable_hi: is_instantiable_lo.map(|offset| offset + 8),
        is_cloneable_lo,
        is_cloneable_hi: is_cloneable_lo.map(|offset| offset + 8),
        is_iterable_lo,
        is_iterable_hi: is_iterable_lo.map(|offset| offset + 8),
        is_internal_lo,
        is_internal_hi: is_internal_lo.map(|offset| offset + 8),
        is_user_defined_lo,
        is_user_defined_hi: is_user_defined_lo.map(|offset| offset + 8),
        modifiers_lo,
        modifiers_hi: modifiers_lo.map(|offset| offset + 8),
        in_namespace_lo,
        in_namespace_hi: in_namespace_lo.map(|offset| offset + 8),
        is_static_lo,
        is_static_hi: is_static_lo.map(|offset| offset + 8),
        is_public_lo,
        is_public_hi: is_public_lo.map(|offset| offset + 8),
        is_protected_lo,
        is_protected_hi: is_protected_lo.map(|offset| offset + 8),
        is_private_lo,
        is_private_hi: is_private_lo.map(|offset| offset + 8),
        is_enum_case_lo,
        is_enum_case_hi: is_enum_case_lo.map(|offset| offset + 8),
        required_parameter_count_lo,
        required_parameter_count_hi: required_parameter_count_lo.map(|offset| offset + 8),
        position_lo,
        position_hi: position_lo.map(|offset| offset + 8),
        is_optional_lo,
        is_optional_hi: is_optional_lo.map(|offset| offset + 8),
        is_variadic_lo,
        is_variadic_hi: is_variadic_lo.map(|offset| offset + 8),
        is_passed_by_reference_lo,
        is_passed_by_reference_hi: is_passed_by_reference_lo.map(|offset| offset + 8),
        has_type_lo,
        has_type_hi: has_type_lo.map(|offset| offset + 8),
        parameter_type_lo,
        parameter_type_hi: parameter_type_lo.map(|offset| offset + 8),
        has_default_value_lo,
        has_default_value_hi: has_default_value_lo.map(|offset| offset + 8),
        default_value_lo,
        default_value_hi: default_value_lo.map(|offset| offset + 8),
        declaring_function_lo,
        declaring_function_hi: declaring_function_lo.map(|offset| offset + 8),
        allows_null_lo,
        allows_null_hi: allows_null_lo.map(|offset| offset + 8),
        is_builtin_lo,
        is_builtin_hi: is_builtin_lo.map(|offset| offset + 8),
    })
}

/// Returns one declared property offset from the synthetic reflection class layout.
fn reflection_property_offset(info: &ClassInfo, property: &str) -> Option<usize> {
    info.property_offsets.get(property).copied()
}

/// Emits a fail-closed helper when Reflection owner metadata is unavailable.
fn emit_reflection_owner_new_stub(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, xzr");                                 // report helper failure when Reflection owner metadata is missing
            emitter.instruction("ret");                                         // return the null pointer to Rust
        }
        Arch::X86_64 => {
            emitter.instruction("xor eax, eax");                                // report helper failure when Reflection owner metadata is missing
            emitter.instruction("ret");                                         // return the null pointer to Rust
        }
    }
}

/// Emits the ARM64 Reflection owner materializer helper body.
fn emit_reflection_owner_new_aarch64(emitter: &mut Emitter, layouts: &ReflectionOwnerLayouts) {
    let fail_label = "__elephc_eval_reflection_owner_new_fail";
    let done_label = "__elephc_eval_reflection_owner_new_done";
    let box_label = "__elephc_eval_reflection_owner_new_box";
    let class_label = "__elephc_eval_reflection_owner_new_class";
    let method_label = "__elephc_eval_reflection_owner_new_method";
    let property_label = "__elephc_eval_reflection_owner_new_property";
    let class_constant_label = "__elephc_eval_reflection_owner_new_class_constant";
    let enum_unit_case_label = "__elephc_eval_reflection_owner_new_enum_unit_case";
    let enum_backed_case_label = "__elephc_eval_reflection_owner_new_enum_backed_case";
    let parameter_label = "__elephc_eval_reflection_owner_new_parameter";
    let named_type_label = "__elephc_eval_reflection_owner_new_named_type";
    let union_type_label = "__elephc_eval_reflection_owner_new_union_type";
    let intersection_type_label = "__elephc_eval_reflection_owner_new_intersection_type";
    emitter.instruction("sub sp, sp, #160");                                    // reserve helper frame for inputs, object, arrays, scratch, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #144]");                            // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #144");                                   // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the Reflection owner kind
    emitter.instruction("str x1, [sp, #8]");                                    // save the reflected-name pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the reflected-name length
    emitter.instruction("str x3, [sp, #24]");                                   // save the boxed ReflectionAttribute array
    emitter.instruction("str x4, [sp, #80]");                                   // save the boxed ReflectionClass interface-name array
    emitter.instruction("str x5, [sp, #88]");                                   // save the boxed ReflectionClass trait-name array
    emitter.instruction("str x6, [sp, #104]");                                  // save the boxed ReflectionClass method-name array
    emitter.instruction("str x7, [sp, #112]");                                  // save the boxed ReflectionClass property-name array
    emitter.instruction("ldr x8, [sp, #160]");                                  // load the boxed ReflectionClass method objects array from the first stack argument
    emitter.instruction("str x8, [sp, #120]");                                  // save the boxed ReflectionClass method objects array
    emitter.instruction("ldr x8, [sp, #168]");                                  // load the boxed ReflectionClass property objects array from the second stack argument
    emitter.instruction("str x8, [sp, #128]");                                  // save the boxed ReflectionClass property objects array
    emitter.instruction("ldr x8, [sp, #176]");                                  // load the boxed ReflectionClass parent value from the third stack argument
    emitter.instruction("str x8, [sp, #136]");                                  // save the boxed ReflectionClass parent value
    emitter.instruction("ldr x8, [sp, #184]");                                  // load ReflectionClass modifier flags from the fourth stack argument
    emitter.instruction("str x8, [sp, #48]");                                   // save ReflectionClass modifier flags
    emitter.instruction("ldr x8, [sp, #192]");                                  // load owner modifier/count metadata from the fifth stack argument
    emitter.instruction("str x8, [sp, #96]");                                   // save owner modifier/count metadata
    emitter.instruction("ldr x8, [sp, #200]");                                  // load ReflectionMethod getModifiers bitmask from the sixth stack argument
    emitter.instruction("str x8, [sp, #72]");                                   // save ReflectionMethod getModifiers bitmask
    emitter.instruction("ldr x8, [sp, #208]");                                  // load boxed ReflectionClassConstant value from the seventh stack argument
    emitter.instruction("str x8, [sp, #56]");                                   // save boxed ReflectionClassConstant value
    emitter.instruction("ldr x8, [sp, #216]");                                  // load boxed ReflectionEnumBackedCase backing value from the eighth stack argument
    emitter.instruction("str x8, [sp, #64]");                                   // save boxed ReflectionEnumBackedCase backing value
    emitter.instruction("cmp x0, #0");                                          // owner kind 0 means ReflectionClass
    emitter.instruction(&format!("b.eq {}", class_label));                      // allocate a ReflectionClass owner
    emitter.instruction("cmp x0, #1");                                          // owner kind 1 means ReflectionMethod
    emitter.instruction(&format!("b.eq {}", method_label));                     // allocate a ReflectionMethod owner
    emitter.instruction("cmp x0, #2");                                          // owner kind 2 means ReflectionProperty
    emitter.instruction(&format!("b.eq {}", property_label));                   // allocate a ReflectionProperty owner
    emitter.instruction("cmp x0, #3");                                          // owner kind 3 means ReflectionClassConstant
    emitter.instruction(&format!("b.eq {}", class_constant_label));             // allocate a ReflectionClassConstant owner
    emitter.instruction("cmp x0, #4");                                          // owner kind 4 means ReflectionEnumUnitCase
    emitter.instruction(&format!("b.eq {}", enum_unit_case_label));             // allocate a ReflectionEnumUnitCase owner
    emitter.instruction("cmp x0, #5");                                          // owner kind 5 means ReflectionEnumBackedCase
    emitter.instruction(&format!("b.eq {}", enum_backed_case_label));           // allocate a ReflectionEnumBackedCase owner
    emitter.instruction("cmp x0, #6");                                          // owner kind 6 means ReflectionParameter
    emitter.instruction(&format!("b.eq {}", parameter_label));                  // allocate a ReflectionParameter owner
    emitter.instruction("cmp x0, #7");                                          // owner kind 7 means ReflectionNamedType
    emitter.instruction(&format!("b.eq {}", named_type_label));                 // allocate a ReflectionNamedType owner
    emitter.instruction("cmp x0, #8");                                          // owner kind 8 means ReflectionUnionType
    emitter.instruction(&format!("b.eq {}", union_type_label));                 // allocate a ReflectionUnionType owner
    emitter.instruction("cmp x0, #9");                                          // owner kind 9 means ReflectionIntersectionType
    emitter.instruction(&format!("b.eq {}", intersection_type_label));          // allocate a ReflectionIntersectionType owner
    emitter.instruction(&format!("b {}", fail_label));                          // reject unknown owner kinds
    emit_aarch64_owner_kind_body(
        emitter,
        class_label,
        &layouts.class,
        true,
        fail_label,
        box_label,
    );
    emit_aarch64_owner_kind_body(
        emitter,
        method_label,
        &layouts.method,
        true,
        fail_label,
        box_label,
    );
    emit_aarch64_owner_kind_body(
        emitter,
        property_label,
        &layouts.property,
        true,
        fail_label,
        box_label,
    );
    emit_aarch64_owner_kind_body(
        emitter,
        class_constant_label,
        &layouts.class_constant,
        true,
        fail_label,
        box_label,
    );
    emit_aarch64_owner_kind_body(
        emitter,
        enum_unit_case_label,
        &layouts.enum_unit_case,
        true,
        fail_label,
        box_label,
    );
    emit_aarch64_owner_kind_body(
        emitter,
        enum_backed_case_label,
        &layouts.enum_backed_case,
        true,
        fail_label,
        box_label,
    );
    emit_aarch64_owner_kind_body(
        emitter,
        parameter_label,
        &layouts.parameter,
        true,
        fail_label,
        box_label,
    );
    emit_aarch64_owner_kind_body(
        emitter,
        named_type_label,
        &layouts.named_type,
        true,
        fail_label,
        box_label,
    );
    emit_aarch64_owner_kind_body(
        emitter,
        union_type_label,
        &layouts.union_type,
        false,
        fail_label,
        box_label,
    );
    emit_aarch64_owner_kind_body(
        emitter,
        intersection_type_label,
        &layouts.intersection_type,
        false,
        fail_label,
        box_label,
    );
    emitter.label(box_label);
    emitter.instruction("mov x0, #6");                                          // runtime tag 6 = object
    emitter.instruction("ldr x1, [sp, #32]");                                   // move the Reflection owner object pointer into the Mixed payload
    emitter.instruction("mov x2, xzr");                                         // object payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the Reflection owner object for eval
    emitter.instruction(&format!("b {}", done_label));                          // skip the fail-closed return path after boxing
    emitter.label(fail_label);
    emitter.instruction("mov x0, xzr");                                         // return a null pointer so Rust reports runtime failure
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #144]");                            // restore the Rust caller frame
    emitter.instruction("add sp, sp, #160");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed reflection owner to Rust
}

/// Emits the x86_64 Reflection owner materializer helper body.
fn emit_reflection_owner_new_x86_64(emitter: &mut Emitter, layouts: &ReflectionOwnerLayouts) {
    let fail_label = "__elephc_eval_reflection_owner_new_fail_x";
    let done_label = "__elephc_eval_reflection_owner_new_done_x";
    let box_label = "__elephc_eval_reflection_owner_new_box_x";
    let class_label = "__elephc_eval_reflection_owner_new_class_x";
    let method_label = "__elephc_eval_reflection_owner_new_method_x";
    let property_label = "__elephc_eval_reflection_owner_new_property_x";
    let class_constant_label = "__elephc_eval_reflection_owner_new_class_constant_x";
    let enum_unit_case_label = "__elephc_eval_reflection_owner_new_enum_unit_case_x";
    let enum_backed_case_label = "__elephc_eval_reflection_owner_new_enum_backed_case_x";
    let parameter_label = "__elephc_eval_reflection_owner_new_parameter_x";
    let named_type_label = "__elephc_eval_reflection_owner_new_named_type_x";
    let union_type_label = "__elephc_eval_reflection_owner_new_union_type_x";
    let intersection_type_label = "__elephc_eval_reflection_owner_new_intersection_type_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 144");                                        // reserve slots for inputs, object, metadata arrays, and name parts
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the Reflection owner kind
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the reflected-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the reflected-name length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the boxed ReflectionAttribute array
    emitter.instruction("mov QWORD PTR [rbp - 88], r8");                        // save the boxed ReflectionClass interface-name array
    emitter.instruction("mov QWORD PTR [rbp - 96], r9");                        // save the boxed ReflectionClass trait-name array
    emitter.instruction("mov rax, QWORD PTR [rbp + 16]");                       // load the boxed ReflectionClass method-name array from the first stack argument
    emitter.instruction("mov QWORD PTR [rbp - 112], rax");                      // save the boxed ReflectionClass method-name array
    emitter.instruction("mov rax, QWORD PTR [rbp + 24]");                       // load the boxed ReflectionClass property-name array from the second stack argument
    emitter.instruction("mov QWORD PTR [rbp - 120], rax");                      // save the boxed ReflectionClass property-name array
    emitter.instruction("mov rax, QWORD PTR [rbp + 32]");                       // load the boxed ReflectionClass method objects array from the third stack argument
    emitter.instruction("mov QWORD PTR [rbp - 128], rax");                      // save the boxed ReflectionClass method objects array
    emitter.instruction("mov rax, QWORD PTR [rbp + 40]");                       // load the boxed ReflectionClass property objects array from the fourth stack argument
    emitter.instruction("mov QWORD PTR [rbp - 136], rax");                      // save the boxed ReflectionClass property objects array
    emitter.instruction("mov rax, QWORD PTR [rbp + 48]");                       // load the boxed ReflectionClass parent value from the fifth stack argument
    emitter.instruction("mov QWORD PTR [rbp - 144], rax");                      // save the boxed ReflectionClass parent value
    emitter.instruction("mov rax, QWORD PTR [rbp + 56]");                       // load ReflectionClass modifier flags from the sixth stack argument
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save ReflectionClass modifier flags
    emitter.instruction("mov rax, QWORD PTR [rbp + 64]");                       // load owner modifier/count metadata from the seventh stack argument
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // save owner modifier/count metadata
    emitter.instruction("mov rax, QWORD PTR [rbp + 72]");                       // load ReflectionMethod getModifiers bitmask from the eighth stack argument
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save ReflectionMethod getModifiers bitmask
    emitter.instruction("mov rax, QWORD PTR [rbp + 80]");                       // load boxed ReflectionClassConstant value from the ninth stack argument
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save boxed ReflectionClassConstant value
    emitter.instruction("mov rax, QWORD PTR [rbp + 88]");                       // load boxed ReflectionEnumBackedCase backing value from the tenth stack argument
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save boxed ReflectionEnumBackedCase backing value
    emitter.instruction("cmp rdi, 0");                                          // owner kind 0 means ReflectionClass
    emitter.instruction(&format!("je {}", class_label));                        // allocate a ReflectionClass owner
    emitter.instruction("cmp rdi, 1");                                          // owner kind 1 means ReflectionMethod
    emitter.instruction(&format!("je {}", method_label));                       // allocate a ReflectionMethod owner
    emitter.instruction("cmp rdi, 2");                                          // owner kind 2 means ReflectionProperty
    emitter.instruction(&format!("je {}", property_label));                     // allocate a ReflectionProperty owner
    emitter.instruction("cmp rdi, 3");                                          // owner kind 3 means ReflectionClassConstant
    emitter.instruction(&format!("je {}", class_constant_label));               // allocate a ReflectionClassConstant owner
    emitter.instruction("cmp rdi, 4");                                          // owner kind 4 means ReflectionEnumUnitCase
    emitter.instruction(&format!("je {}", enum_unit_case_label));               // allocate a ReflectionEnumUnitCase owner
    emitter.instruction("cmp rdi, 5");                                          // owner kind 5 means ReflectionEnumBackedCase
    emitter.instruction(&format!("je {}", enum_backed_case_label));             // allocate a ReflectionEnumBackedCase owner
    emitter.instruction("cmp rdi, 6");                                          // owner kind 6 means ReflectionParameter
    emitter.instruction(&format!("je {}", parameter_label));                    // allocate a ReflectionParameter owner
    emitter.instruction("cmp rdi, 7");                                          // owner kind 7 means ReflectionNamedType
    emitter.instruction(&format!("je {}", named_type_label));                   // allocate a ReflectionNamedType owner
    emitter.instruction("cmp rdi, 8");                                          // owner kind 8 means ReflectionUnionType
    emitter.instruction(&format!("je {}", union_type_label));                   // allocate a ReflectionUnionType owner
    emitter.instruction("cmp rdi, 9");                                          // owner kind 9 means ReflectionIntersectionType
    emitter.instruction(&format!("je {}", intersection_type_label));            // allocate a ReflectionIntersectionType owner
    emitter.instruction(&format!("jmp {}", fail_label));                        // reject unknown owner kinds
    emit_x86_64_owner_kind_body(
        emitter,
        class_label,
        &layouts.class,
        true,
        fail_label,
        box_label,
    );
    emit_x86_64_owner_kind_body(
        emitter,
        method_label,
        &layouts.method,
        true,
        fail_label,
        box_label,
    );
    emit_x86_64_owner_kind_body(
        emitter,
        property_label,
        &layouts.property,
        true,
        fail_label,
        box_label,
    );
    emit_x86_64_owner_kind_body(
        emitter,
        class_constant_label,
        &layouts.class_constant,
        true,
        fail_label,
        box_label,
    );
    emit_x86_64_owner_kind_body(
        emitter,
        enum_unit_case_label,
        &layouts.enum_unit_case,
        true,
        fail_label,
        box_label,
    );
    emit_x86_64_owner_kind_body(
        emitter,
        enum_backed_case_label,
        &layouts.enum_backed_case,
        true,
        fail_label,
        box_label,
    );
    emit_x86_64_owner_kind_body(
        emitter,
        parameter_label,
        &layouts.parameter,
        true,
        fail_label,
        box_label,
    );
    emit_x86_64_owner_kind_body(
        emitter,
        named_type_label,
        &layouts.named_type,
        true,
        fail_label,
        box_label,
    );
    emit_x86_64_owner_kind_body(
        emitter,
        union_type_label,
        &layouts.union_type,
        false,
        fail_label,
        box_label,
    );
    emit_x86_64_owner_kind_body(
        emitter,
        intersection_type_label,
        &layouts.intersection_type,
        false,
        fail_label,
        box_label,
    );
    emitter.label(box_label);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // move the Reflection owner object pointer into the Mixed payload
    emitter.instruction("xor esi, esi");                                        // object payloads do not use a high word
    emitter.instruction("mov eax, 6");                                          // runtime tag 6 = object
    emitter.instruction("call __rt_mixed_from_value");                          // box the Reflection owner object for eval
    emitter.instruction(&format!("jmp {}", done_label));                        // skip the fail-closed return path after boxing
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax");                                        // return a null pointer so Rust reports runtime failure
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed reflection owner to Rust
}

/// Emits one ARM64 owner-kind allocation and slot-population body.
fn emit_aarch64_owner_kind_body(
    emitter: &mut Emitter,
    label: &str,
    layout: &ReflectionOwnerLayout,
    set_name: bool,
    fail_label: &str,
    box_label: &str,
) {
    emitter.label(label);
    emit_alloc_reflection_owner_object_aarch64(emitter, layout);
    emitter.instruction("str x0, [sp, #32]");                                   // save the unboxed Reflection owner object pointer
    if set_name {
        emit_set_owner_name_property_aarch64(emitter, layout);
    }
    emit_set_owner_class_flags_property_aarch64(emitter, layout);
    emit_set_owner_member_flags_property_aarch64(emitter, layout);
    emit_set_owner_constant_value_property_aarch64(emitter, layout, fail_label);
    emit_set_owner_backing_value_property_aarch64(emitter, layout, fail_label);
    emit_set_owner_required_parameter_count_property_aarch64(emitter, layout);
    emit_set_owner_parameter_property_aarch64(emitter, layout);
    emit_set_owner_parameter_type_property_aarch64(emitter, layout, fail_label);
    emit_set_owner_parameter_default_property_aarch64(emitter, layout, fail_label);
    emit_set_owner_named_type_flags_property_aarch64(emitter, layout);
    emit_set_owner_metadata_arrays_property_aarch64(emitter, layout, fail_label);
    emit_set_owner_constructor_property_aarch64(emitter, layout, fail_label);
    emit_set_owner_parent_class_property_aarch64(emitter, layout, fail_label);
    emit_set_owner_declaring_function_property_aarch64(emitter, layout, fail_label);
    emit_set_owner_attrs_property_aarch64(emitter, layout, fail_label);
    emitter.instruction(&format!("b {}", box_label));                           // box this populated Reflection owner object
}

/// Emits one x86_64 owner-kind allocation and slot-population body.
fn emit_x86_64_owner_kind_body(
    emitter: &mut Emitter,
    label: &str,
    layout: &ReflectionOwnerLayout,
    set_name: bool,
    fail_label: &str,
    box_label: &str,
) {
    emitter.label(label);
    emit_alloc_reflection_owner_object_x86_64(emitter, layout);
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the unboxed Reflection owner object pointer
    if set_name {
        emit_set_owner_name_property_x86_64(emitter, layout);
    }
    emit_set_owner_class_flags_property_x86_64(emitter, layout);
    emit_set_owner_member_flags_property_x86_64(emitter, layout);
    emit_set_owner_constant_value_property_x86_64(emitter, layout, fail_label);
    emit_set_owner_backing_value_property_x86_64(emitter, layout, fail_label);
    emit_set_owner_required_parameter_count_property_x86_64(emitter, layout);
    emit_set_owner_parameter_property_x86_64(emitter, layout);
    emit_set_owner_parameter_type_property_x86_64(emitter, layout, fail_label);
    emit_set_owner_parameter_default_property_x86_64(emitter, layout, fail_label);
    emit_set_owner_named_type_flags_property_x86_64(emitter, layout);
    emit_set_owner_metadata_arrays_property_x86_64(emitter, layout, fail_label);
    emit_set_owner_constructor_property_x86_64(emitter, layout, fail_label);
    emit_set_owner_parent_class_property_x86_64(emitter, layout, fail_label);
    emit_set_owner_declaring_function_property_x86_64(emitter, layout, fail_label);
    emit_set_owner_attrs_property_x86_64(emitter, layout, fail_label);
    emitter.instruction(&format!("jmp {}", box_label));                         // box this populated Reflection owner object
}

/// Allocates a zero-initialized ARM64 Reflection owner object payload.
fn emit_alloc_reflection_owner_object_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let payload_size = 8 + layout.property_count * 16;
    emitter.instruction(&format!("mov x0, #{}", payload_size));                 // request Reflection owner object payload storage
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction("mov x9, #4");                                          // heap kind 4 marks the payload as an object
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the object heap header before the payload
    emitter.instruction(&format!("mov x10, #{}", layout.class_id));             // materialize the Reflection owner class id
    emitter.instruction("str x10, [x0]");                                       // store the class id at object payload offset zero
    for index in 0..layout.property_count {
        let offset = 8 + index * 16;
        abi::emit_store_zero_to_address(emitter, "x0", offset);
        abi::emit_store_zero_to_address(emitter, "x0", offset + 8);
    }
}

/// Allocates a zero-initialized x86_64 Reflection owner object payload.
fn emit_alloc_reflection_owner_object_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let payload_size = 8 + layout.property_count * 16;
    emitter.instruction(&format!("mov rax, {}", payload_size));                 // request Reflection owner object payload storage
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction(&format!(
        "mov r10, 0x{:x}",
        (X86_64_HEAP_MAGIC_HI32 << 32) | 4
    )); // materialize the x86_64 object heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the object heap header before the payload
    emitter.instruction(&format!("mov r10, {}", layout.class_id));              // materialize the Reflection owner class id
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the class id at object payload offset zero
    for index in 0..layout.property_count {
        let offset = 8 + index * 16;
        abi::emit_store_zero_to_address(emitter, "rax", offset);
        abi::emit_store_zero_to_address(emitter, "rax", offset + 8);
    }
}

/// Stores the incoming ARM64 reflected class name into ReflectionClass.
fn emit_set_owner_name_property_aarch64(emitter: &mut Emitter, layout: &ReflectionOwnerLayout) {
    let Some(name_lo) = layout.name_lo else {
        return;
    };
    let Some(name_hi) = layout.name_hi else {
        return;
    };
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the reflected-name pointer for persistence
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the reflected-name length for persistence
    emitter.instruction("bl __rt_str_persist");                                 // copy the eval-owned name bytes for object ownership
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", name_lo);
    abi::emit_store_to_address(emitter, "x2", "x9", name_hi);
    let (
        Some(short_name_lo),
        Some(short_name_hi),
        Some(namespace_name_lo),
        Some(namespace_name_hi),
        Some(in_namespace_lo),
        Some(in_namespace_hi),
    ) = (
        layout.short_name_lo,
        layout.short_name_hi,
        layout.namespace_name_lo,
        layout.namespace_name_hi,
        layout.in_namespace_lo,
        layout.in_namespace_hi,
    )
    else {
        emit_set_owner_low_bit_final_property_aarch64(emitter, layout);
        return;
    };
    let scan_loop_label = "__elephc_eval_reflection_owner_name_scan_loop";
    let found_label = "__elephc_eval_reflection_owner_name_scan_found";
    let no_namespace_label = "__elephc_eval_reflection_owner_name_scan_none";
    let store_parts_label = "__elephc_eval_reflection_owner_name_store_parts";
    emitter.instruction("ldr x3, [sp, #8]");                                    // reload the original reflected-name pointer for splitting
    emitter.instruction("ldr x4, [sp, #16]");                                   // reload the original reflected-name length for splitting
    emitter.instruction("mov x5, x4");                                          // start scanning from one byte past the final name byte
    emitter.instruction(&format!("cbz x5, {}", no_namespace_label));            // empty names have no namespace component
    emitter.label(scan_loop_label);
    emitter.instruction("sub x5, x5, #1");                                      // move the scan cursor to the previous byte
    emitter.instruction("ldrb w6, [x3, x5]");                                   // read one reflected-name byte from the scan cursor
    emitter.instruction("cmp w6, #92");                                         // compare against PHP namespace separator '\\'
    emitter.instruction(&format!("b.eq {}", found_label));                      // split at the final namespace separator
    emitter.instruction(&format!("cbnz x5, {}", scan_loop_label));              // keep scanning until the first byte has been checked
    emitter.label(no_namespace_label);
    emitter.instruction("str x3, [sp, #56]");                                   // short-name pointer is the original name pointer
    emitter.instruction("str x4, [sp, #64]");                                   // short-name length is the full name length
    emitter.instruction("str xzr, [sp, #72]");                                  // namespace length is zero for global names
    emitter.instruction(&format!("b {}", store_parts_label));                   // skip the namespaced split path
    emitter.label(found_label);
    emitter.instruction("add x6, x5, #1");                                      // compute the short-name byte offset after the separator
    emitter.instruction("add x7, x3, x6");                                      // compute the short-name pointer
    emitter.instruction("sub x8, x4, x6");                                      // compute the short-name length
    emitter.instruction("str x7, [sp, #56]");                                   // save the short-name pointer across persistence calls
    emitter.instruction("str x8, [sp, #64]");                                   // save the short-name length across persistence calls
    emitter.instruction("str x5, [sp, #72]");                                   // namespace length is the separator offset
    emitter.label(store_parts_label);
    emitter.instruction("ldr x1, [sp, #8]");                                    // use the original name pointer for namespace persistence
    emitter.instruction("ldr x2, [sp, #72]");                                   // reload the namespace byte length
    emitter.instruction("bl __rt_str_persist");                                 // copy the namespace bytes for ReflectionClass storage
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", namespace_name_lo);
    abi::emit_store_to_address(emitter, "x2", "x9", namespace_name_hi);
    emitter.instruction("cmp x2, #0");                                          // detect whether a namespace component was present
    emitter.instruction("cset x10, ne");                                        // materialize ReflectionClass::inNamespace()
    abi::emit_store_to_address(emitter, "x10", "x9", in_namespace_lo);
    abi::emit_store_zero_to_address(emitter, "x9", in_namespace_hi);
    emitter.instruction("ldr x1, [sp, #56]");                                   // reload the short-name pointer
    emitter.instruction("ldr x2, [sp, #64]");                                   // reload the short-name byte length
    emitter.instruction("bl __rt_str_persist");                                 // copy the short-name bytes for ReflectionClass storage
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", short_name_lo);
    abi::emit_store_to_address(emitter, "x2", "x9", short_name_hi);
}

/// Stores the incoming x86_64 reflected class name into ReflectionClass.
fn emit_set_owner_name_property_x86_64(emitter: &mut Emitter, layout: &ReflectionOwnerLayout) {
    let Some(name_lo) = layout.name_lo else {
        return;
    };
    let Some(name_hi) = layout.name_hi else {
        return;
    };
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the reflected-name pointer for persistence
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the reflected-name length for persistence
    emitter.instruction("call __rt_str_persist");                               // copy the eval-owned name bytes for object ownership
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "rax", "r10", name_lo);
    abi::emit_store_to_address(emitter, "rdx", "r10", name_hi);
    let (
        Some(short_name_lo),
        Some(short_name_hi),
        Some(namespace_name_lo),
        Some(namespace_name_hi),
        Some(in_namespace_lo),
        Some(in_namespace_hi),
    ) = (
        layout.short_name_lo,
        layout.short_name_hi,
        layout.namespace_name_lo,
        layout.namespace_name_hi,
        layout.in_namespace_lo,
        layout.in_namespace_hi,
    )
    else {
        return;
    };
    let scan_loop_label = "__elephc_eval_reflection_owner_name_scan_loop_x";
    let found_label = "__elephc_eval_reflection_owner_name_scan_found_x";
    let no_namespace_label = "__elephc_eval_reflection_owner_name_scan_none_x";
    let store_parts_label = "__elephc_eval_reflection_owner_name_store_parts_x";
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the original reflected-name pointer for splitting
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the original reflected-name length for splitting
    emitter.instruction("mov r11, r9");                                         // start scanning from one byte past the final name byte
    emitter.instruction("test r11, r11");                                       // check whether the reflected name is empty
    emitter.instruction(&format!("jz {}", no_namespace_label));                 // empty names have no namespace component
    emitter.label(scan_loop_label);
    emitter.instruction("sub r11, 1");                                          // move the scan cursor to the previous byte
    emitter.instruction("movzx eax, BYTE PTR [r8 + r11]");                      // read one reflected-name byte from the scan cursor
    emitter.instruction("cmp eax, 92");                                         // compare against PHP namespace separator '\\'
    emitter.instruction(&format!("je {}", found_label));                        // split at the final namespace separator
    emitter.instruction("test r11, r11");                                       // check whether the first byte has been examined
    emitter.instruction(&format!("jnz {}", scan_loop_label));                   // keep scanning until the first byte has been checked
    emitter.label(no_namespace_label);
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // short-name pointer is the original name pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // short-name length is the full name length
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // namespace length is zero for global names
    emitter.instruction(&format!("jmp {}", store_parts_label));                 // skip the namespaced split path
    emitter.label(found_label);
    emitter.instruction("lea rax, [r11 + 1]");                                  // compute the short-name byte offset after the separator
    emitter.instruction("lea r10, [r8 + rax]");                                 // compute the short-name pointer
    emitter.instruction("mov rcx, r9");                                         // copy the full name length before subtracting the prefix
    emitter.instruction("sub rcx, rax");                                        // compute the short-name length
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // save the short-name pointer across persistence calls
    emitter.instruction("mov QWORD PTR [rbp - 72], rcx");                       // save the short-name length across persistence calls
    emitter.instruction("mov QWORD PTR [rbp - 80], r11");                       // namespace length is the separator offset
    emitter.label(store_parts_label);
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // use the original name pointer for namespace persistence
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // reload the namespace byte length
    emitter.instruction("call __rt_str_persist");                               // copy the namespace bytes for ReflectionClass storage
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "rax", "r10", namespace_name_lo);
    abi::emit_store_to_address(emitter, "rdx", "r10", namespace_name_hi);
    emitter.instruction("test rdx, rdx");                                       // detect whether a namespace component was present
    emitter.instruction("setne al");                                            // materialize ReflectionClass::inNamespace()
    emitter.instruction("movzx eax, al");                                       // widen the namespace boolean to a full word
    abi::emit_store_to_address(emitter, "rax", "r10", in_namespace_lo);
    abi::emit_store_zero_to_address(emitter, "r10", in_namespace_hi);
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the short-name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // reload the short-name byte length
    emitter.instruction("call __rt_str_persist");                               // copy the short-name bytes for ReflectionClass storage
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "rax", "r10", short_name_lo);
    abi::emit_store_to_address(emitter, "rdx", "r10", short_name_hi);
}

/// Stores incoming ARM64 ReflectionClass boolean modifier flags.
fn emit_set_owner_class_flags_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let (
        Some(is_final_lo),
        Some(is_final_hi),
        Some(is_abstract_lo),
        Some(is_abstract_hi),
        Some(is_interface_lo),
        Some(is_interface_hi),
        Some(is_trait_lo),
        Some(is_trait_hi),
        Some(is_enum_lo),
        Some(is_enum_hi),
        Some(is_readonly_lo),
        Some(is_readonly_hi),
        Some(is_instantiable_lo),
        Some(is_instantiable_hi),
        Some(is_cloneable_lo),
        Some(is_cloneable_hi),
        Some(is_iterable_lo),
        Some(is_iterable_hi),
        Some(is_internal_lo),
        Some(is_internal_hi),
        Some(is_user_defined_lo),
        Some(is_user_defined_hi),
        Some(modifiers_lo),
        Some(modifiers_hi),
    ) = (
        layout.is_final_lo,
        layout.is_final_hi,
        layout.is_abstract_lo,
        layout.is_abstract_hi,
        layout.is_interface_lo,
        layout.is_interface_hi,
        layout.is_trait_lo,
        layout.is_trait_hi,
        layout.is_enum_lo,
        layout.is_enum_hi,
        layout.is_readonly_lo,
        layout.is_readonly_hi,
        layout.is_instantiable_lo,
        layout.is_instantiable_hi,
        layout.is_cloneable_lo,
        layout.is_cloneable_hi,
        layout.is_iterable_lo,
        layout.is_iterable_hi,
        layout.is_internal_lo,
        layout.is_internal_hi,
        layout.is_user_defined_lo,
        layout.is_user_defined_hi,
        layout.modifiers_lo,
        layout.modifiers_hi,
    )
    else {
        return;
    };
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload ReflectionClass modifier flags
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    emitter.instruction("and x10, x11, #1");                                    // extract the final-class flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_final_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_final_hi);
    emitter.instruction("lsr x10, x11, #1");                                    // move the abstract-class bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the abstract-class flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_abstract_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_abstract_hi);
    emitter.instruction("lsr x10, x11, #2");                                    // move the interface bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the interface flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_interface_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_interface_hi);
    emitter.instruction("lsr x10, x11, #3");                                    // move the trait bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the trait flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_trait_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_trait_hi);
    emitter.instruction("lsr x10, x11, #4");                                    // move the enum bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the enum flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_enum_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_enum_hi);
    emitter.instruction("lsr x10, x11, #5");                                    // move the readonly-class bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the readonly-class flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_readonly_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_readonly_hi);
    emitter.instruction("lsr x10, x11, #6");                                    // move the instantiable-class bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the instantiable-class flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_instantiable_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_instantiable_hi);
    emitter.instruction("lsr x10, x11, #7");                                    // move the cloneable-class bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the cloneable-class flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_cloneable_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_cloneable_hi);
    emitter.instruction("lsr x10, x11, #8");                                    // move the internal-class bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the internal-class flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_internal_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_internal_hi);
    emitter.instruction("lsr x10, x11, #9");                                    // move the user-defined-class bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the user-defined-class flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_user_defined_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_user_defined_hi);
    emitter.instruction("lsr x10, x11, #10");                                   // move the iterable-class bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the iterable-class flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_iterable_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_iterable_hi);
    emitter.instruction("ldr x10, [sp, #96]");                                  // reload PHP ReflectionClass::getModifiers() bitmask
    abi::emit_store_to_address(emitter, "x10", "x9", modifiers_lo);
    abi::emit_store_zero_to_address(emitter, "x9", modifiers_hi);
}

/// Stores incoming x86_64 ReflectionClass boolean modifier flags.
fn emit_set_owner_class_flags_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let (
        Some(is_final_lo),
        Some(is_final_hi),
        Some(is_abstract_lo),
        Some(is_abstract_hi),
        Some(is_interface_lo),
        Some(is_interface_hi),
        Some(is_trait_lo),
        Some(is_trait_hi),
        Some(is_enum_lo),
        Some(is_enum_hi),
        Some(is_readonly_lo),
        Some(is_readonly_hi),
        Some(is_instantiable_lo),
        Some(is_instantiable_hi),
        Some(is_cloneable_lo),
        Some(is_cloneable_hi),
        Some(is_iterable_lo),
        Some(is_iterable_hi),
        Some(is_internal_lo),
        Some(is_internal_hi),
        Some(is_user_defined_lo),
        Some(is_user_defined_hi),
        Some(modifiers_lo),
        Some(modifiers_hi),
    ) = (
        layout.is_final_lo,
        layout.is_final_hi,
        layout.is_abstract_lo,
        layout.is_abstract_hi,
        layout.is_interface_lo,
        layout.is_interface_hi,
        layout.is_trait_lo,
        layout.is_trait_hi,
        layout.is_enum_lo,
        layout.is_enum_hi,
        layout.is_readonly_lo,
        layout.is_readonly_hi,
        layout.is_instantiable_lo,
        layout.is_instantiable_hi,
        layout.is_cloneable_lo,
        layout.is_cloneable_hi,
        layout.is_iterable_lo,
        layout.is_iterable_hi,
        layout.is_internal_lo,
        layout.is_internal_hi,
        layout.is_user_defined_lo,
        layout.is_user_defined_hi,
        layout.modifiers_lo,
        layout.modifiers_hi,
    )
    else {
        return;
    };
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload ReflectionClass modifier flags
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the final bit
    emitter.instruction("and rax, 1");                                          // extract the final-class flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_final_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_final_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the abstract bit
    emitter.instruction("shr rax, 1");                                          // move the abstract-class bit into position
    emitter.instruction("and rax, 1");                                          // extract the abstract-class flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_abstract_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_abstract_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the interface bit
    emitter.instruction("shr rax, 2");                                          // move the interface bit into position
    emitter.instruction("and rax, 1");                                          // extract the interface flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_interface_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_interface_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the trait bit
    emitter.instruction("shr rax, 3");                                          // move the trait bit into position
    emitter.instruction("and rax, 1");                                          // extract the trait flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_trait_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_trait_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the enum bit
    emitter.instruction("shr rax, 4");                                          // move the enum bit into position
    emitter.instruction("and rax, 1");                                          // extract the enum flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_enum_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_enum_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the readonly-class bit
    emitter.instruction("shr rax, 5");                                          // move the readonly-class bit into position
    emitter.instruction("and rax, 1");                                          // extract the readonly-class flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_readonly_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_readonly_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the instantiable bit
    emitter.instruction("shr rax, 6");                                          // move the instantiable-class bit into position
    emitter.instruction("and rax, 1");                                          // extract the instantiable-class flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_instantiable_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_instantiable_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the cloneable bit
    emitter.instruction("shr rax, 7");                                          // move the cloneable-class bit into position
    emitter.instruction("and rax, 1");                                          // extract the cloneable-class flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_cloneable_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_cloneable_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the internal bit
    emitter.instruction("shr rax, 8");                                          // move the internal-class bit into position
    emitter.instruction("and rax, 1");                                          // extract the internal-class flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_internal_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_internal_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the user-defined bit
    emitter.instruction("shr rax, 9");                                          // move the user-defined-class bit into position
    emitter.instruction("and rax, 1");                                          // extract the user-defined-class flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_user_defined_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_user_defined_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the iterable bit
    emitter.instruction("shr rax, 10");                                         // move the iterable-class bit into position
    emitter.instruction("and rax, 1");                                          // extract the iterable-class flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_iterable_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_iterable_hi);
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // reload PHP ReflectionClass::getModifiers() bitmask
    abi::emit_store_to_address(emitter, "rax", "r10", modifiers_lo);
    abi::emit_store_zero_to_address(emitter, "r10", modifiers_hi);
}

/// Stores incoming ARM64 ReflectionMethod/ReflectionProperty boolean flags.
fn emit_set_owner_member_flags_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let (
        Some(is_public_lo),
        Some(is_public_hi),
        Some(is_protected_lo),
        Some(is_protected_hi),
        Some(is_private_lo),
        Some(is_private_hi),
    ) = (
        layout.is_public_lo,
        layout.is_public_hi,
        layout.is_protected_lo,
        layout.is_protected_hi,
        layout.is_private_lo,
        layout.is_private_hi,
    )
    else {
        return;
    };
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload Reflection member predicate flags
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    if let (Some(is_static_lo), Some(is_static_hi)) = (layout.is_static_lo, layout.is_static_hi) {
        emitter.instruction("and x10, x11, #1");                                // extract the static-member flag as a boolean
        abi::emit_store_to_address(emitter, "x10", "x9", is_static_lo);
        abi::emit_store_zero_to_address(emitter, "x9", is_static_hi);
    }
    emitter.instruction("lsr x10, x11, #1");                                    // move the public-member bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the public-member flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_public_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_public_hi);
    emitter.instruction("lsr x10, x11, #2");                                    // move the protected-member bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the protected-member flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_protected_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_protected_hi);
    emitter.instruction("lsr x10, x11, #3");                                    // move the private-member bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the private-member flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_private_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_private_hi);
    if let (Some(is_enum_case_lo), Some(is_enum_case_hi)) =
        (layout.is_enum_case_lo, layout.is_enum_case_hi)
    {
        emitter.instruction("lsr x10, x11, #7");                                // move the enum-case flag into position
        emitter.instruction("and x10, x10, #1");                                // extract the enum-case flag as a boolean
        abi::emit_store_to_address(emitter, "x10", "x9", is_enum_case_lo);
        abi::emit_store_zero_to_address(emitter, "x9", is_enum_case_hi);
    }
    if let (Some(is_readonly_lo), Some(is_readonly_hi)) =
        (layout.is_readonly_lo, layout.is_readonly_hi)
    {
        emitter.instruction("lsr x10, x11, #6");                                // move the readonly-property bit into position
        emitter.instruction("and x10, x10, #1");                                // extract the readonly-property flag as a boolean
        abi::emit_store_to_address(emitter, "x10", "x9", is_readonly_lo);
        abi::emit_store_zero_to_address(emitter, "x9", is_readonly_hi);
    }
    if let (Some(has_default_value_lo), Some(has_default_value_hi)) =
        (layout.has_default_value_lo, layout.has_default_value_hi)
    {
        emitter.instruction("lsr x10, x11, #8");                                // move the default-value bit into position
        emitter.instruction("and x10, x10, #1");                                // extract the default-value flag as a boolean
        abi::emit_store_to_address(emitter, "x10", "x9", has_default_value_lo);
        abi::emit_store_zero_to_address(emitter, "x9", has_default_value_hi);
    }
    if let (Some(modifiers_lo), Some(modifiers_hi)) = (layout.modifiers_lo, layout.modifiers_hi) {
        if layout.required_parameter_count_lo.is_some() {
            emitter.instruction("ldr x10, [sp, #72]");                          // reload PHP ReflectionMethod::getModifiers() bitmask
        } else {
            emitter.instruction("ldr x10, [sp, #96]");                          // reload PHP Reflection member getModifiers() bitmask
        }
        abi::emit_store_to_address(emitter, "x10", "x9", modifiers_lo);
        abi::emit_store_zero_to_address(emitter, "x9", modifiers_hi);
    }
    if let (Some(is_final_lo), Some(is_final_hi)) = (layout.is_final_lo, layout.is_final_hi) {
        emitter.instruction("lsr x10, x11, #4");                                // move the final-member bit into position
        emitter.instruction("and x10, x10, #1");                                // extract the final-member flag as a boolean
        abi::emit_store_to_address(emitter, "x10", "x9", is_final_lo);
        abi::emit_store_zero_to_address(emitter, "x9", is_final_hi);
    }
    if let (Some(is_abstract_lo), Some(is_abstract_hi)) =
        (layout.is_abstract_lo, layout.is_abstract_hi)
    {
        emitter.instruction("lsr x10, x11, #5");                                // move the abstract-member bit into position
        emitter.instruction("and x10, x10, #1");                                // extract the abstract-member flag as a boolean
        abi::emit_store_to_address(emitter, "x10", "x9", is_abstract_lo);
        abi::emit_store_zero_to_address(emitter, "x9", is_abstract_hi);
    }
}

/// Stores incoming bit-zero finality for ARM64 owners without full member flags.
fn emit_set_owner_low_bit_final_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let (Some(is_final_lo), Some(is_final_hi)) = (layout.is_final_lo, layout.is_final_hi) else {
        return;
    };
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload Reflection owner predicate flags
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    emitter.instruction("and x10, x11, #1");                                    // extract bit-zero finality as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_final_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_final_hi);
}

/// Stores incoming x86_64 ReflectionMethod/ReflectionProperty boolean flags.
fn emit_set_owner_member_flags_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let (
        Some(is_public_lo),
        Some(is_public_hi),
        Some(is_protected_lo),
        Some(is_protected_hi),
        Some(is_private_lo),
        Some(is_private_hi),
    ) = (
        layout.is_public_lo,
        layout.is_public_hi,
        layout.is_protected_lo,
        layout.is_protected_hi,
        layout.is_private_lo,
        layout.is_private_hi,
    )
    else {
        emit_set_owner_low_bit_final_property_x86_64(emitter, layout);
        return;
    };
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload Reflection member predicate flags
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    if let (Some(is_static_lo), Some(is_static_hi)) = (layout.is_static_lo, layout.is_static_hi) {
        emitter.instruction("mov rax, r11");                                    // copy flags before extracting the static bit
        emitter.instruction("and rax, 1");                                      // extract the static-member flag as a boolean
        abi::emit_store_to_address(emitter, "rax", "r10", is_static_lo);
        abi::emit_store_zero_to_address(emitter, "r10", is_static_hi);
    }
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the public bit
    emitter.instruction("shr rax, 1");                                          // move the public-member bit into position
    emitter.instruction("and rax, 1");                                          // extract the public-member flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_public_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_public_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the protected bit
    emitter.instruction("shr rax, 2");                                          // move the protected-member bit into position
    emitter.instruction("and rax, 1");                                          // extract the protected-member flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_protected_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_protected_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the private bit
    emitter.instruction("shr rax, 3");                                          // move the private-member bit into position
    emitter.instruction("and rax, 1");                                          // extract the private-member flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_private_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_private_hi);
    if let (Some(is_enum_case_lo), Some(is_enum_case_hi)) =
        (layout.is_enum_case_lo, layout.is_enum_case_hi)
    {
        emitter.instruction("mov rax, r11");                                    // copy flags before extracting the enum-case bit
        emitter.instruction("shr rax, 7");                                      // move the enum-case bit into position
        emitter.instruction("and rax, 1");                                      // extract the enum-case flag as a boolean
        abi::emit_store_to_address(emitter, "rax", "r10", is_enum_case_lo);
        abi::emit_store_zero_to_address(emitter, "r10", is_enum_case_hi);
    }
    if let (Some(is_readonly_lo), Some(is_readonly_hi)) =
        (layout.is_readonly_lo, layout.is_readonly_hi)
    {
        emitter.instruction("mov rax, r11");                                    // copy flags before extracting the readonly bit
        emitter.instruction("shr rax, 6");                                      // move the readonly-property bit into position
        emitter.instruction("and rax, 1");                                      // extract the readonly-property flag as a boolean
        abi::emit_store_to_address(emitter, "rax", "r10", is_readonly_lo);
        abi::emit_store_zero_to_address(emitter, "r10", is_readonly_hi);
    }
    if let (Some(has_default_value_lo), Some(has_default_value_hi)) =
        (layout.has_default_value_lo, layout.has_default_value_hi)
    {
        emitter.instruction("mov rax, r11");                                    // copy flags before extracting the default-value bit
        emitter.instruction("shr rax, 8");                                      // move the default-value bit into position
        emitter.instruction("and rax, 1");                                      // extract the default-value flag as a boolean
        abi::emit_store_to_address(emitter, "rax", "r10", has_default_value_lo);
        abi::emit_store_zero_to_address(emitter, "r10", has_default_value_hi);
    }
    if let (Some(modifiers_lo), Some(modifiers_hi)) = (layout.modifiers_lo, layout.modifiers_hi) {
        if layout.required_parameter_count_lo.is_some() {
            emitter.instruction("mov rax, QWORD PTR [rbp - 80]");               // reload PHP ReflectionMethod::getModifiers() bitmask
        } else {
            emitter.instruction("mov rax, QWORD PTR [rbp - 104]");              // reload PHP Reflection member getModifiers() bitmask
        }
        abi::emit_store_to_address(emitter, "rax", "r10", modifiers_lo);
        abi::emit_store_zero_to_address(emitter, "r10", modifiers_hi);
    }
    if let (Some(is_final_lo), Some(is_final_hi)) = (layout.is_final_lo, layout.is_final_hi) {
        emitter.instruction("mov rax, r11");                                    // copy flags before extracting the final bit
        emitter.instruction("shr rax, 4");                                      // move the final-member bit into position
        emitter.instruction("and rax, 1");                                      // extract the final-member flag as a boolean
        abi::emit_store_to_address(emitter, "rax", "r10", is_final_lo);
        abi::emit_store_zero_to_address(emitter, "r10", is_final_hi);
    }
    if let (Some(is_abstract_lo), Some(is_abstract_hi)) =
        (layout.is_abstract_lo, layout.is_abstract_hi)
    {
        emitter.instruction("mov rax, r11");                                    // copy flags before extracting the abstract bit
        emitter.instruction("shr rax, 5");                                      // move the abstract-member bit into position
        emitter.instruction("and rax, 1");                                      // extract the abstract-member flag as a boolean
        abi::emit_store_to_address(emitter, "rax", "r10", is_abstract_lo);
        abi::emit_store_zero_to_address(emitter, "r10", is_abstract_hi);
    }
}

/// Stores incoming bit-zero finality for x86_64 owners without full member flags.
fn emit_set_owner_low_bit_final_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let (Some(is_final_lo), Some(is_final_hi)) = (layout.is_final_lo, layout.is_final_hi) else {
        return;
    };
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload Reflection owner predicate flags
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting bit-zero finality
    emitter.instruction("and rax, 1");                                          // extract bit-zero finality as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_final_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_final_hi);
}

/// Stores incoming ARM64 ReflectionMethod required-parameter count.
fn emit_set_owner_required_parameter_count_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let (Some(required_parameter_count_lo), Some(required_parameter_count_hi)) = (
        layout.required_parameter_count_lo,
        layout.required_parameter_count_hi,
    ) else {
        return;
    };
    emitter.instruction("ldr x10, [sp, #96]");                                  // reload ReflectionMethod required-parameter count
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "x10", "x9", required_parameter_count_lo);
    abi::emit_store_zero_to_address(emitter, "x9", required_parameter_count_hi);
}

/// Stores incoming x86_64 ReflectionMethod required-parameter count.
fn emit_set_owner_required_parameter_count_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let (Some(required_parameter_count_lo), Some(required_parameter_count_hi)) = (
        layout.required_parameter_count_lo,
        layout.required_parameter_count_hi,
    ) else {
        return;
    };
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // reload ReflectionMethod required-parameter count
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "rax", "r10", required_parameter_count_lo);
    abi::emit_store_zero_to_address(emitter, "r10", required_parameter_count_hi);
}

/// Stores incoming ARM64 ReflectionParameter position and predicate flags.
fn emit_set_owner_parameter_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let (
        Some(position_lo),
        Some(position_hi),
        Some(is_optional_lo),
        Some(is_optional_hi),
        Some(is_variadic_lo),
        Some(is_variadic_hi),
        Some(is_passed_by_reference_lo),
        Some(is_passed_by_reference_hi),
        Some(has_type_lo),
        Some(has_type_hi),
        Some(has_default_value_lo),
        Some(has_default_value_hi),
    ) = (
        layout.position_lo,
        layout.position_hi,
        layout.is_optional_lo,
        layout.is_optional_hi,
        layout.is_variadic_lo,
        layout.is_variadic_hi,
        layout.is_passed_by_reference_lo,
        layout.is_passed_by_reference_hi,
        layout.has_type_lo,
        layout.has_type_hi,
        layout.has_default_value_lo,
        layout.has_default_value_hi,
    )
    else {
        return;
    };
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload ReflectionParameter predicate flags
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the ReflectionParameter object pointer
    emitter.instruction("ldr x10, [sp, #96]");                                  // reload the zero-based parameter position
    abi::emit_store_to_address(emitter, "x10", "x9", position_lo);
    abi::emit_store_zero_to_address(emitter, "x9", position_hi);
    emitter.instruction("and x10, x11, #1");                                    // extract the optional-parameter flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_optional_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_optional_hi);
    emitter.instruction("lsr x10, x11, #1");                                    // move the variadic-parameter bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the variadic-parameter flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_variadic_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_variadic_hi);
    emitter.instruction("lsr x10, x11, #2");                                    // move the by-reference-parameter bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the by-reference-parameter flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_passed_by_reference_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_passed_by_reference_hi);
    emitter.instruction("lsr x10, x11, #3");                                    // move the typed-parameter bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the typed-parameter flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", has_type_lo);
    abi::emit_store_zero_to_address(emitter, "x9", has_type_hi);
    emitter.instruction("lsr x10, x11, #4");                                    // move the default-value bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the default-value flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", has_default_value_lo);
    abi::emit_store_zero_to_address(emitter, "x9", has_default_value_hi);
}

/// Stores incoming ARM64 ReflectionParameter type metadata.
fn emit_set_owner_parameter_type_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (Some(type_lo), Some(type_hi)) = (layout.parameter_type_lo, layout.parameter_type_hi)
    else {
        return;
    };
    emitter.instruction("ldr x0, [sp, #120]");                                  // reload the boxed ReflectionParameter type value
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // reject malformed null type metadata
    emitter.instruction("str x0, [sp, #40]");                                   // save the boxed type value across incref
    emitter.instruction("bl __rt_incref");                                      // retain the boxed type value for ReflectionParameter storage
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the retained boxed type value
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the ReflectionParameter object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", type_lo);
    abi::emit_store_zero_to_address(emitter, "x9", type_hi);
}

/// Stores incoming ARM64 ReflectionParameter default-value metadata.
fn emit_set_owner_parameter_default_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (Some(default_lo), Some(default_hi)) = (layout.default_value_lo, layout.default_value_hi)
    else {
        return;
    };
    emitter.instruction("ldr x0, [sp, #128]");                                  // reload the boxed ReflectionParameter default value
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // reject malformed null default metadata
    emitter.instruction("str x0, [sp, #40]");                                   // save the boxed default value across incref
    emitter.instruction("bl __rt_incref");                                      // retain the boxed default value for ReflectionParameter storage
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the retained boxed default value
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the ReflectionParameter object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", default_lo);
    abi::emit_store_zero_to_address(emitter, "x9", default_hi);
}

/// Stores incoming ARM64 ReflectionNamedType predicate flags.
fn emit_set_owner_named_type_flags_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let (Some(allows_null_lo), Some(allows_null_hi)) =
        (layout.allows_null_lo, layout.allows_null_hi)
    else {
        return;
    };
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload ReflectionNamedType predicate flags
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the ReflectionNamedType object pointer
    emitter.instruction("and x10, x11, #1");                                    // extract the nullable-type flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", allows_null_lo);
    abi::emit_store_zero_to_address(emitter, "x9", allows_null_hi);
    let (Some(is_builtin_lo), Some(is_builtin_hi)) = (layout.is_builtin_lo, layout.is_builtin_hi)
    else {
        return;
    };
    emitter.instruction("lsr x10, x11, #1");                                    // move the builtin-type bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the builtin-type flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_builtin_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_builtin_hi);
}

/// Stores incoming x86_64 ReflectionParameter position and predicate flags.
fn emit_set_owner_parameter_property_x86_64(emitter: &mut Emitter, layout: &ReflectionOwnerLayout) {
    let (
        Some(position_lo),
        Some(position_hi),
        Some(is_optional_lo),
        Some(is_optional_hi),
        Some(is_variadic_lo),
        Some(is_variadic_hi),
        Some(is_passed_by_reference_lo),
        Some(is_passed_by_reference_hi),
        Some(has_type_lo),
        Some(has_type_hi),
        Some(has_default_value_lo),
        Some(has_default_value_hi),
    ) = (
        layout.position_lo,
        layout.position_hi,
        layout.is_optional_lo,
        layout.is_optional_hi,
        layout.is_variadic_lo,
        layout.is_variadic_hi,
        layout.is_passed_by_reference_lo,
        layout.is_passed_by_reference_hi,
        layout.has_type_lo,
        layout.has_type_hi,
        layout.has_default_value_lo,
        layout.has_default_value_hi,
    )
    else {
        return;
    };
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload ReflectionParameter predicate flags
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the ReflectionParameter object pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // reload the zero-based parameter position
    abi::emit_store_to_address(emitter, "rax", "r10", position_lo);
    abi::emit_store_zero_to_address(emitter, "r10", position_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the optional bit
    emitter.instruction("and rax, 1");                                          // extract the optional-parameter flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_optional_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_optional_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the variadic bit
    emitter.instruction("shr rax, 1");                                          // move the variadic-parameter bit into position
    emitter.instruction("and rax, 1");                                          // extract the variadic-parameter flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_variadic_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_variadic_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the by-reference bit
    emitter.instruction("shr rax, 2");                                          // move the by-reference-parameter bit into position
    emitter.instruction("and rax, 1");                                          // extract the by-reference-parameter flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_passed_by_reference_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_passed_by_reference_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the typed bit
    emitter.instruction("shr rax, 3");                                          // move the typed-parameter bit into position
    emitter.instruction("and rax, 1");                                          // extract the typed-parameter flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", has_type_lo);
    abi::emit_store_zero_to_address(emitter, "r10", has_type_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the default-value bit
    emitter.instruction("shr rax, 4");                                          // move the default-value bit into position
    emitter.instruction("and rax, 1");                                          // extract the default-value flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", has_default_value_lo);
    abi::emit_store_zero_to_address(emitter, "r10", has_default_value_hi);
}

/// Stores incoming x86_64 ReflectionParameter type metadata.
fn emit_set_owner_parameter_type_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (Some(type_lo), Some(type_hi)) = (layout.parameter_type_lo, layout.parameter_type_hi)
    else {
        return;
    };
    emitter.instruction("mov rax, QWORD PTR [rbp - 128]");                      // reload the boxed ReflectionParameter type value
    emitter.instruction("test rax, rax");                                       // check whether the boxed type value is null
    emitter.instruction(&format!("jz {}", fail_label));                         // reject malformed null type metadata
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the boxed type value across incref
    emitter.instruction("call __rt_incref");                                    // retain the boxed type value for ReflectionParameter storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the retained boxed type value
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the ReflectionParameter object pointer
    abi::emit_store_to_address(emitter, "rdi", "r10", type_lo);
    abi::emit_store_zero_to_address(emitter, "r10", type_hi);
}

/// Stores incoming x86_64 ReflectionParameter default-value metadata.
fn emit_set_owner_parameter_default_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (Some(default_lo), Some(default_hi)) = (layout.default_value_lo, layout.default_value_hi)
    else {
        return;
    };
    emitter.instruction("mov rax, QWORD PTR [rbp - 136]");                      // reload the boxed ReflectionParameter default value
    emitter.instruction("test rax, rax");                                       // check whether the boxed default value is null
    emitter.instruction(&format!("jz {}", fail_label));                         // reject malformed null default metadata
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the boxed default value across incref
    emitter.instruction("call __rt_incref");                                    // retain the boxed default value for ReflectionParameter storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the retained boxed default value
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the ReflectionParameter object pointer
    abi::emit_store_to_address(emitter, "rdi", "r10", default_lo);
    abi::emit_store_zero_to_address(emitter, "r10", default_hi);
}

/// Stores incoming x86_64 ReflectionNamedType predicate flags.
fn emit_set_owner_named_type_flags_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let (Some(allows_null_lo), Some(allows_null_hi)) =
        (layout.allows_null_lo, layout.allows_null_hi)
    else {
        return;
    };
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload ReflectionNamedType predicate flags
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the ReflectionNamedType object pointer
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the nullable bit
    emitter.instruction("and rax, 1");                                          // extract the nullable-type flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", allows_null_lo);
    abi::emit_store_zero_to_address(emitter, "r10", allows_null_hi);
    let (Some(is_builtin_lo), Some(is_builtin_hi)) = (layout.is_builtin_lo, layout.is_builtin_hi)
    else {
        return;
    };
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the builtin bit
    emitter.instruction("shr rax, 1");                                          // move the builtin-type bit into position
    emitter.instruction("and rax, 1");                                          // extract the builtin-type flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_builtin_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_builtin_hi);
}

/// Stores incoming ARM64 ReflectionClass metadata name arrays.
fn emit_set_owner_metadata_arrays_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    if let (Some(low), Some(high)) = (layout.interface_names_lo, layout.interface_names_hi) {
        emit_set_owner_metadata_array_slot_aarch64(emitter, 80, low, high, fail_label);
    }
    if let (Some(low), Some(high)) = (layout.trait_names_lo, layout.trait_names_hi) {
        emit_set_owner_metadata_array_slot_aarch64(emitter, 88, low, high, fail_label);
    }
    if let (Some(low), Some(high)) = (layout.method_names_lo, layout.method_names_hi) {
        emit_set_owner_metadata_array_slot_aarch64(emitter, 104, low, high, fail_label);
    }
    if let (Some(low), Some(high)) = (layout.property_names_lo, layout.property_names_hi) {
        emit_set_owner_metadata_array_slot_aarch64(emitter, 112, low, high, fail_label);
    }
    if let (Some(low), Some(high)) = (layout.method_objects_lo, layout.method_objects_hi) {
        emit_set_owner_metadata_array_slot_aarch64(emitter, 120, low, high, fail_label);
    }
    if let (Some(low), Some(high)) = (layout.property_objects_lo, layout.property_objects_hi) {
        emit_set_owner_metadata_array_slot_aarch64(emitter, 128, low, high, fail_label);
    }
}

/// Stores one retained ARM64 boxed metadata-name array into a ReflectionClass slot.
fn emit_set_owner_metadata_array_slot_aarch64(
    emitter: &mut Emitter,
    boxed_slot: usize,
    low_offset: usize,
    high_offset: usize,
    fail_label: &str,
) {
    emitter.instruction(&format!("ldr x0, [sp, #{}]", boxed_slot));             // reload the boxed ReflectionClass metadata-name array
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // reject malformed null metadata-name arrays
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the metadata-name array tag and payload pointer
    emitter.instruction("cmp x0, #4");                                          // runtime tag 4 means indexed array
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject non-array metadata-name metadata
    emitter.instruction("str x1, [sp, #40]");                                   // save the unboxed metadata-name array across incref
    emitter.instruction("mov x0, x1");                                          // move the array payload into the incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain the metadata-name array for ReflectionClass storage
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the retained metadata-name array payload
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", low_offset);
    abi::emit_load_int_immediate(emitter, "x10", 4);
    abi::emit_store_to_address(emitter, "x10", "x9", high_offset);
}

/// Stores incoming x86_64 ReflectionClass metadata name arrays.
fn emit_set_owner_metadata_arrays_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    if let (Some(low), Some(high)) = (layout.interface_names_lo, layout.interface_names_hi) {
        emit_set_owner_metadata_array_slot_x86_64(emitter, -88, low, high, fail_label);
    }
    if let (Some(low), Some(high)) = (layout.trait_names_lo, layout.trait_names_hi) {
        emit_set_owner_metadata_array_slot_x86_64(emitter, -96, low, high, fail_label);
    }
    if let (Some(low), Some(high)) = (layout.method_names_lo, layout.method_names_hi) {
        emit_set_owner_metadata_array_slot_x86_64(emitter, -112, low, high, fail_label);
    }
    if let (Some(low), Some(high)) = (layout.property_names_lo, layout.property_names_hi) {
        emit_set_owner_metadata_array_slot_x86_64(emitter, -120, low, high, fail_label);
    }
    if let (Some(low), Some(high)) = (layout.method_objects_lo, layout.method_objects_hi) {
        emit_set_owner_metadata_array_slot_x86_64(emitter, -128, low, high, fail_label);
    }
    if let (Some(low), Some(high)) = (layout.property_objects_lo, layout.property_objects_hi) {
        emit_set_owner_metadata_array_slot_x86_64(emitter, -136, low, high, fail_label);
    }
}

/// Stores one retained x86_64 boxed metadata-name array into a ReflectionClass slot.
fn emit_set_owner_metadata_array_slot_x86_64(
    emitter: &mut Emitter,
    boxed_slot: isize,
    low_offset: usize,
    high_offset: usize,
    fail_label: &str,
) {
    let boxed_slot = if boxed_slot < 0 {
        format!("- {}", -boxed_slot)
    } else {
        format!("+ {}", boxed_slot)
    };
    emitter.instruction(&format!("mov rax, QWORD PTR [rbp {}]", boxed_slot));   // reload the boxed ReflectionClass metadata-name array
    emitter.instruction("test rax, rax");                                       // check whether the boxed metadata-name array is null
    emitter.instruction(&format!("jz {}", fail_label));                         // reject malformed null metadata-name arrays
    emitter.instruction("call __rt_mixed_unbox");                               // expose the metadata-name array tag and payload pointer
    emitter.instruction("cmp rax, 4");                                          // runtime tag 4 means indexed array
    emitter.instruction(&format!("jne {}", fail_label));                        // reject non-array metadata-name metadata
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // save the unboxed metadata-name array across incref
    emitter.instruction("mov rax, rdi");                                        // move the array payload into the incref argument register
    emitter.instruction("call __rt_incref");                                    // retain the metadata-name array for ReflectionClass storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the retained metadata-name array payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "rdi", "r10", low_offset);
    abi::emit_load_int_immediate(emitter, "r11", 4);
    abi::emit_store_to_address(emitter, "r11", "r10", high_offset);
}

/// Stores a retained ARM64 boxed ReflectionMethod-or-null constructor cell.
fn emit_set_owner_constructor_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (Some(low), Some(high)) = (layout.constructor_lo, layout.constructor_hi) else {
        return;
    };
    emitter.instruction("ldr x0, [sp, #224]");                                  // reload the boxed ReflectionClass constructor value
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // reject malformed null constructor metadata
    emitter.instruction("str x0, [sp, #40]");                                   // save the boxed constructor value across incref
    emitter.instruction("bl __rt_incref");                                      // retain the boxed constructor value for ReflectionClass storage
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the retained boxed constructor value
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", low);
    abi::emit_store_zero_to_address(emitter, "x9", high);
}

/// Stores a retained x86_64 boxed ReflectionMethod-or-null constructor cell.
fn emit_set_owner_constructor_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (Some(low), Some(high)) = (layout.constructor_lo, layout.constructor_hi) else {
        return;
    };
    emitter.instruction("mov rax, QWORD PTR [rbp + 96]");                       // reload the boxed ReflectionClass constructor value
    emitter.instruction("test rax, rax");                                       // check whether the boxed constructor value is null
    emitter.instruction(&format!("jz {}", fail_label));                         // reject malformed null constructor metadata
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the boxed constructor value across incref
    emitter.instruction("call __rt_incref");                                    // retain the boxed constructor value for ReflectionClass storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the retained boxed constructor value
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "rdi", "r10", low);
    abi::emit_store_zero_to_address(emitter, "r10", high);
}

/// Stores a retained ARM64 boxed parent ReflectionClass-or-false cell.
fn emit_set_owner_parent_class_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (Some(low), Some(high)) = (layout.parent_class_lo, layout.parent_class_hi) else {
        return;
    };
    emitter.instruction("ldr x0, [sp, #136]");                                  // reload the boxed ReflectionClass parent value
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // reject malformed null parent metadata
    emitter.instruction("str x0, [sp, #40]");                                   // save the boxed parent value across incref
    emitter.instruction("bl __rt_incref");                                      // retain the boxed parent value for ReflectionClass storage
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the retained boxed parent value
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", low);
    abi::emit_store_zero_to_address(emitter, "x9", high);
}

/// Stores a retained x86_64 boxed parent ReflectionClass-or-false cell.
fn emit_set_owner_parent_class_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (Some(low), Some(high)) = (layout.parent_class_lo, layout.parent_class_hi) else {
        return;
    };
    emitter.instruction("mov rax, QWORD PTR [rbp - 144]");                      // reload the boxed ReflectionClass parent value
    emitter.instruction("test rax, rax");                                       // check whether the boxed parent value is null
    emitter.instruction(&format!("jz {}", fail_label));                         // reject malformed null parent metadata
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the boxed parent value across incref
    emitter.instruction("call __rt_incref");                                    // retain the boxed parent value for ReflectionClass storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the retained boxed parent value
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "rdi", "r10", low);
    abi::emit_store_zero_to_address(emitter, "r10", high);
}

/// Stores a retained ARM64 boxed declaring ReflectionFunction/Method cell.
fn emit_set_owner_declaring_function_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (Some(low), Some(high)) = (layout.declaring_function_lo, layout.declaring_function_hi)
    else {
        return;
    };
    emitter.instruction("ldr x0, [sp, #80]");                                   // reload the boxed ReflectionParameter declaring function
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // reject malformed null declaring-function metadata
    emitter.instruction("str x0, [sp, #40]");                                   // save the boxed declaring function across incref
    emitter.instruction("bl __rt_incref");                                      // retain the declaring function for ReflectionParameter storage
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the retained declaring function value
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", low);
    abi::emit_store_zero_to_address(emitter, "x9", high);
}

/// Stores a retained x86_64 boxed declaring ReflectionFunction/Method cell.
fn emit_set_owner_declaring_function_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (Some(low), Some(high)) = (layout.declaring_function_lo, layout.declaring_function_hi)
    else {
        return;
    };
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // reload the boxed ReflectionParameter declaring function
    emitter.instruction("test rax, rax");                                       // check whether the declaring-function value is null
    emitter.instruction(&format!("jz {}", fail_label));                         // reject malformed null declaring-function metadata
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the boxed declaring function across incref
    emitter.instruction("call __rt_incref");                                    // retain the declaring function for ReflectionParameter storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the retained declaring function value
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "rdi", "r10", low);
    abi::emit_store_zero_to_address(emitter, "r10", high);
}

/// Stores a retained ARM64 boxed ReflectionClassConstant value cell.
fn emit_set_owner_constant_value_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (Some(low), Some(high)) = (layout.value_lo, layout.value_hi) else {
        return;
    };
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload the boxed ReflectionClassConstant value
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // reject malformed null constant-value metadata
    emitter.instruction("str x0, [sp, #40]");                                   // save the boxed constant value across incref
    emitter.instruction("bl __rt_incref");                                      // retain the boxed constant value for ReflectionClassConstant storage
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the retained boxed constant value
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the ReflectionClassConstant object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", low);
    abi::emit_store_zero_to_address(emitter, "x9", high);
}

/// Stores a retained x86_64 boxed ReflectionClassConstant value cell.
fn emit_set_owner_constant_value_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (Some(low), Some(high)) = (layout.value_lo, layout.value_hi) else {
        return;
    };
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the boxed ReflectionClassConstant value
    emitter.instruction("test rax, rax");                                       // check whether the boxed constant value is null
    emitter.instruction(&format!("jz {}", fail_label));                         // reject malformed null constant-value metadata
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the boxed constant value across incref
    emitter.instruction("call __rt_incref");                                    // retain the boxed constant value for ReflectionClassConstant storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the retained boxed constant value
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the ReflectionClassConstant object pointer
    abi::emit_store_to_address(emitter, "rdi", "r10", low);
    abi::emit_store_zero_to_address(emitter, "r10", high);
}

/// Stores a retained ARM64 boxed ReflectionEnumBackedCase backing-value cell.
fn emit_set_owner_backing_value_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (Some(low), Some(high)) = (layout.backing_value_lo, layout.backing_value_hi) else {
        return;
    };
    emitter.instruction("ldr x0, [sp, #64]");                                   // reload the boxed ReflectionEnumBackedCase backing value
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // reject malformed null backing-value metadata
    emitter.instruction("str x0, [sp, #40]");                                   // save the boxed backing value across incref
    emitter.instruction("bl __rt_incref");                                      // retain the boxed backing value for ReflectionEnumBackedCase storage
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the retained boxed backing value
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the ReflectionEnumBackedCase object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", low);
    abi::emit_store_zero_to_address(emitter, "x9", high);
}

/// Stores a retained x86_64 boxed ReflectionEnumBackedCase backing-value cell.
fn emit_set_owner_backing_value_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (Some(low), Some(high)) = (layout.backing_value_lo, layout.backing_value_hi) else {
        return;
    };
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the boxed ReflectionEnumBackedCase backing value
    emitter.instruction("test rax, rax");                                       // check whether the boxed backing value is null
    emitter.instruction(&format!("jz {}", fail_label));                         // reject malformed null backing-value metadata
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the boxed backing value across incref
    emitter.instruction("call __rt_incref");                                    // retain the boxed backing value for ReflectionEnumBackedCase storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the retained boxed backing value
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the ReflectionEnumBackedCase object pointer
    abi::emit_store_to_address(emitter, "rdi", "r10", low);
    abi::emit_store_zero_to_address(emitter, "r10", high);
}

/// Stores a retained ARM64 attribute-array payload into the owner private slot.
fn emit_set_owner_attrs_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed ReflectionAttribute array
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // reject malformed null attribute arrays
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the attribute array tag and payload pointer
    emitter.instruction("cmp x0, #4");                                          // runtime tag 4 means indexed array
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject non-array attribute metadata
    emitter.instruction("str x1, [sp, #40]");                                   // save the unboxed attribute array across incref
    emitter.instruction("mov x0, x1");                                          // move the array payload into the incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain the attribute array for Reflection owner storage
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the retained attribute array payload
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", layout.attrs_lo);
    abi::emit_load_int_immediate(emitter, "x10", 4);
    abi::emit_store_to_address(emitter, "x10", "x9", layout.attrs_hi);
}

/// Stores a retained x86_64 attribute-array payload into the owner private slot.
fn emit_set_owner_attrs_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed ReflectionAttribute array
    emitter.instruction("test rax, rax");                                       // check whether the boxed attribute array is null
    emitter.instruction(&format!("jz {}", fail_label));                         // reject malformed null attribute arrays
    emitter.instruction("call __rt_mixed_unbox");                               // expose the attribute array tag and payload pointer
    emitter.instruction("cmp rax, 4");                                          // runtime tag 4 means indexed array
    emitter.instruction(&format!("jne {}", fail_label));                        // reject non-array attribute metadata
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // save the unboxed attribute array across incref
    emitter.instruction("mov rax, rdi");                                        // move the array payload into the incref argument register
    emitter.instruction("call __rt_incref");                                    // retain the attribute array for Reflection owner storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the retained attribute array payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "rdi", "r10", layout.attrs_lo);
    abi::emit_load_int_immediate(emitter, "r11", 4);
    abi::emit_store_to_address(emitter, "r11", "r10", layout.attrs_hi);
}

/// Emits a C-visible global label with target-specific symbol mangling.
fn label_c_global(module: &Module, emitter: &mut Emitter, name: &str) {
    let symbol = module.target.extern_symbol(name);
    emitter.label_global(&symbol);
}
