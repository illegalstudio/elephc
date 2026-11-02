//! Purpose:
//! Reflection owner detection and runtime-class dispatch.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;
use std::collections::HashSet;

use crate::codegen::lower_inst::current_method_class;
use crate::ir::Terminator;
use crate::names::label_fragment;

/// Returns true for reflection owner classes that need metadata-aware construction.
pub(in crate::codegen::lower_inst::objects) fn is_reflection_owner_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "ReflectionClass"
            | "ReflectionObject"
            | "ReflectionFunction"
            | "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionParameter"
            | "ReflectionClassConstant"
            | "ReflectionEnum"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    )
}

/// Returns the builtin Reflection owner ancestor represented by a user subclass.
pub(in crate::codegen::lower_inst::objects) fn reflection_owner_base_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
) -> Option<String> {
    let mut current = class_name.trim_start_matches('\\');
    for _ in 0..=ctx.module.class_infos.len() {
        if is_reflection_owner_class(current) {
            return Some(current.to_string());
        }
        let class_info = ctx.module.class_infos.get(current)?;
        current = class_info.parent.as_deref()?;
    }
    None
}

/// Lowers builtin Reflection owner allocation by populating compile-time metadata slots.
pub(in crate::codegen::lower_inst::objects) fn lower_reflection_owner_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
) -> Result<()> {
    if class_name == "ReflectionObject"
        && lower_reflection_object_by_runtime_class(ctx, inst, class_name)?
    {
        // Runtime-class dispatch leaves the fresh ReflectionObject in the result register.
    } else if class_name == "ReflectionClass"
        && reflection_class_operand_is_compiled_object(ctx, inst)?
        && lower_reflection_object_by_runtime_class(ctx, inst, class_name)?
    {
        // `new ReflectionClass($object)` over a compiled class: the object's runtime class
        // selects compile-time metadata, exactly as for `ReflectionObject`. Handing it to the
        // interpreter (whenever the body owns an eval context) lost facts only the compiler
        // knows, such as `isAnonymous()` for an anonymous class.
    } else if class_name == "ReflectionProperty"
        && lower_reflection_property_by_runtime_name(ctx, inst)?
    {
        return Ok(());
    } else if (crate::codegen::lower_inst::builtins::has_eval_context(ctx)
        && !reflection_member_literals_name_a_compiled_member(ctx, class_name, inst)?)
        || reflection_class_literal_requires_runtime_metadata(ctx, class_name, inst)?
        || reflection_owner_requires_runtime_metadata(ctx, inst)?
    {
        return crate::codegen::lower_inst::builtins::lower_eval_native_object_new(ctx, inst);
    } else {
        let metadata = reflection_owner_metadata(ctx, class_name, inst)?;
        emit_direct_reflection_owner_object(ctx, class_name, inst, &metadata)?;
    }
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("reflection object_new missing result"))?;
    ctx.store_result_value(result)
}

/// Populates Reflection metadata after the generic object path has allocated a user subclass.
pub(in crate::codegen::lower_inst::objects) fn populate_reflection_owner_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    result: ValueId,
) -> Result<()> {
    let owner_class_name = reflection_owner_base_class(ctx, class_name).ok_or_else(|| {
        CodegenIrError::unsupported(format!(
            "{} is not a builtin Reflection owner or subclass",
            class_name
        ))
    })?;
    clear_reflection_owner_prefix(ctx, &owner_class_name, result)?;
    if let Some(object_operand) = reflection_object_operand(ctx, &owner_class_name, inst)? {
        emit_reflection_owner_from_runtime_object_into(
            ctx,
            class_name,
            object_operand,
            Some(result),
        )
    } else {
        let metadata = reflection_owner_metadata(ctx, &owner_class_name, inst)?;
        ctx.load_value_to_result(result)?;
        emit_reflection_owner_metadata(ctx, class_name, &metadata)
    }
}

/// Returns the constructor object operand for ReflectionClass/Object object reflection.
fn reflection_object_operand(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<Option<ValueId>> {
    if !matches!(class_name, "ReflectionClass" | "ReflectionObject") {
        return Ok(None);
    }
    let Some(object_operand) = inst.operands.first().copied() else {
        return Ok(None);
    };
    if matches!(ctx.value_php_type(object_operand)?, PhpType::Object(_)) {
        Ok(Some(object_operand))
    } else {
        Ok(None)
    }
}

/// Selects runtime metadata and either allocates an owner or populates an existing subclass.
fn emit_reflection_owner_from_runtime_object_into(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    object_operand: ValueId,
    existing_object: Option<ValueId>,
) -> Result<()> {
    let candidates = reflection_runtime_class_candidates(ctx, object_operand)?;
    if candidates.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "{} constructor for object with no known runtime class candidates",
            class_name
        )));
    }

    let fallback_label = ctx.next_label("reflection_object_fallback");
    let done_label = ctx.next_label("reflection_object_done");
    let case_labels = candidates
        .iter()
        .map(|_| ctx.next_label("reflection_object_case"))
        .collect::<Vec<_>>();

    emit_runtime_object_class_dispatch(ctx, object_operand, &candidates, &case_labels, &fallback_label)?;

    let fallback_metadata = reflection_class_metadata_for_name(ctx, &candidates[0].class_name)?;
    emit_reflection_owner_candidate(ctx, class_name, &fallback_metadata, existing_object)?;
    emit_reflection_dispatch_jump(ctx, &done_label);                            // skip runtime reflection candidates after fallback allocation

    for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        let metadata = reflection_class_metadata_for_name(ctx, &candidate.class_name)?;
        emit_reflection_owner_candidate(ctx, class_name, &metadata, existing_object)?;
        emit_reflection_dispatch_jump(ctx, &done_label);                        // finish after materializing the matched runtime class
    }

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Materializes one selected metadata candidate into a fresh or preallocated Reflection object.
fn emit_reflection_owner_candidate(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
    existing_object: Option<ValueId>,
) -> Result<()> {
    if let Some(object) = existing_object {
        ctx.load_value_to_result(object)?;
        emit_reflection_owner_metadata(ctx, class_name, metadata)
    } else {
        emit_reflection_owner_object(ctx, class_name, metadata)
    }
}

/// Emits an unconditional jump for the reflection runtime-class dispatch.
pub(super) fn emit_reflection_dispatch_jump(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", label));                   // continue after the selected reflection object is ready
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", label));                 // continue after the selected reflection object is ready
        }
    }
}

/// Emits target-specific class-id comparisons for runtime object reflection.
pub(super) fn emit_runtime_object_class_dispatch(
    ctx: &mut FunctionContext<'_>,
    object_operand: ValueId,
    candidates: &[ReflectionRuntimeClassCandidate],
    case_labels: &[String],
    fallback_label: &str,
) -> Result<()> {
    ctx.load_value_to_result(object_operand)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", fallback_label));    // use fallback metadata for null object pointers
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the object's concrete runtime class id
            for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare the object class id with this reflection candidate
                ctx.emitter.instruction(&format!("b.eq {}", label));            // select metadata for the matched runtime class
            }
            ctx.emitter.instruction(&format!("b {}", fallback_label));          // fall back when no generated candidate matches
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // use fallback metadata for null object pointers
            ctx.emitter.instruction(&format!("je {}", fallback_label));         // skip class-id loading when the object pointer is null
            ctx.emitter.instruction("mov r11, QWORD PTR [rax]");                // load the object's concrete runtime class id
            for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // compare the object class id with this reflection candidate
                ctx.emitter.instruction(&format!("je {}", label));              // select metadata for the matched runtime class
            }
            ctx.emitter.instruction(&format!("jmp {}", fallback_label));        // fall back when no generated candidate matches
        }
    }
    ctx.emitter.label(fallback_label);
    Ok(())
}

/// Returns runtime class candidates compatible with the object's static type metadata.
pub(super) fn reflection_runtime_class_candidates(
    ctx: &FunctionContext<'_>,
    object_operand: ValueId,
) -> Result<Vec<ReflectionRuntimeClassCandidate>> {
    let static_type = reflection_object_static_type_name(ctx, object_operand)?;
    let mut candidates = ctx
        .module
        .class_infos
        .iter()
        .filter(|(class_name, _)| reflection_class_matches_object_type(ctx, class_name, &static_type))
        .map(|(class_name, class_info)| ReflectionRuntimeClassCandidate {
            class_name: class_name.clone(),
            class_id: class_info.class_id,
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| candidate.class_id);
    candidates.dedup_by_key(|candidate| candidate.class_id);
    Ok(candidates)
}

/// Resolves the static object type name used to bound runtime ReflectionObject dispatch.
pub(super) fn reflection_object_static_type_name(
    ctx: &FunctionContext<'_>,
    object_operand: ValueId,
) -> Result<String> {
    match ctx.value_php_type(object_operand)? {
        PhpType::Object(class_name) if class_name.is_empty() => reflection_current_method_class(ctx)
            .map(str::to_string)
            .ok_or_else(|| {
                CodegenIrError::unsupported(
                    "ReflectionObject constructor for object with unknown static class",
                )
            }),
        PhpType::Object(class_name) => Ok(class_name),
        other => Err(CodegenIrError::unsupported(format!(
            "ReflectionObject constructor for PHP type {:?}",
            other
        ))),
    }
}

/// Returns the lexical class name encoded in the current EIR method name, if any.
pub(super) fn reflection_current_method_class<'a>(ctx: &'a FunctionContext<'_>) -> Option<&'a str> {
    ctx.function
        .name
        .rsplit_once("::")
        .map(|(class_name, _)| class_name)
}

/// Runtime class candidate used when object reflection must dispatch by object class id.
pub(super) struct ReflectionRuntimeClassCandidate {
    class_name: String,
    class_id: u64,
}

/// Returns whether a Reflection construction names, in literals, a class-like the program compiled
/// (`ReflectionClass`, `ReflectionEnum`) or a member of one (`ReflectionProperty`,
/// `ReflectionMethod`), so compile-time metadata is the whole answer.
///
/// Lowering reserves an eval context for every Reflection construction, because a literal class
/// name may name a class only a runtime include declares; that reserved slot then sent every
/// construction to the interpreter. A class the closed world compiled is never such a class -- a
/// file handed to the runtime autoloader leaves `class_infos` -- so its reflector is built from
/// that metadata. `Response::setHeaders()`' `new \ReflectionProperty(self::class, 'headers')`
/// created an eval context, synced its metadata and materialized the object on every request.
/// A member the class does not declare stays on the interpreter, which raises the
/// `ReflectionException`. The class-like owners followed the same slot to the interpreter:
/// `new ReflectionEnum(Suit::class)` in a program without eval then died on the bridge's
/// missing interpreter, and every literal `new ReflectionClass(Foo::class)` was interpreted.
fn reflection_member_literals_name_a_compiled_member(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<bool> {
    if matches!(class_name, "ReflectionClass" | "ReflectionEnum") {
        return reflection_class_literal_names_a_compiled_class_like(ctx, class_name, inst);
    }
    // A compiled user function. The interpreter rebuilt it from the bridge's signature alone and
    // answered `getAttributes()` with nothing for `#[Marker] function target() {}`.
    if class_name == "ReflectionFunction" {
        if inst.operands.len() != 1 || reflection_owner_requires_runtime_metadata(ctx, inst)? {
            return Ok(false);
        }
        let function = const_required_string_operand(ctx, inst.operands[0], class_name)?;
        // A compiler builtin (`new ReflectionFunction('strlen')`) is compile-time metadata too:
        // the interpreter's reflector of it has no return type and is not `isInternal()`.
        return Ok(ctx.function_by_name(&function).is_some()
            || reflection_builtin_function_signature(&function).is_some());
    }
    // A parameter of a compiled function or method, named by literals (lowering splits the
    // `[Class::class, 'method']` pair into two operands). The interpreter's reflector answered
    // `getAttributes()` through a body it could not run.
    if class_name == "ReflectionParameter" {
        if !matches!(inst.operands.len(), 2 | 3)
            || reflection_owner_requires_runtime_metadata(ctx, inst)?
        {
            return Ok(false);
        }
        return Ok(reflection_parameter_metadata(ctx, inst)?.reflected_name.is_some());
    }
    // A compiled class constant or enum case. The interpreter knows the cases only of enums it
    // declared, so `new ReflectionEnumUnitCase(E::class, 'A')` over a compiled enum threw
    // "Constant E::A is not a case"; and its `$r->class` came back empty.
    if matches!(
        class_name,
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        if inst.operands.len() != 2 || reflection_owner_requires_runtime_metadata(ctx, inst)? {
            return Ok(false);
        }
        let reflected_class = const_string_or_class_operand(ctx, inst.operands[0], class_name)?;
        let member = const_required_string_operand(ctx, inst.operands[1], class_name)?;
        return Ok(match class_name {
            "ReflectionClassConstant" => {
                reflection_class_constant_lookup(ctx, &reflected_class, &member)?.is_some()
            }
            // A pure enum's case is not a backed case: php's exception stays with the interpreter.
            "ReflectionEnumBackedCase" => resolve_reflection_enum_case(ctx, &reflected_class, &member)
                .is_some_and(|(_, case)| case.value.is_some()),
            _ => resolve_reflection_enum_case(ctx, &reflected_class, &member).is_some(),
        });
    }
    if !matches!(class_name, "ReflectionProperty" | "ReflectionMethod")
        || inst.operands.len() != 2
        || reflection_owner_requires_runtime_metadata(ctx, inst)?
    {
        return Ok(false);
    }
    let reflected_class = const_string_or_class_operand(ctx, inst.operands[0], class_name)?;
    let member = const_required_string_operand(ctx, inst.operands[1], class_name)?;
    if class_name == "ReflectionProperty" {
        let Some((_, info)) = resolve_reflection_class(ctx, &reflected_class) else {
            return Ok(false);
        };
        return Ok(reflection_property_has_compiled_answer(info, &member));
    }
    // A method of ANY compiled class-like: an interface's or a trait's method is compile-time
    // metadata too. Requiring a class sent `new ReflectionMethod(SomeTrait::class, 'm')` to the
    // interpreter, which does not know compiled traits and raised "does not exist".
    Ok(reflection_method_member_for_class_like(ctx, &reflected_class, &php_symbol_key(&member))?
        .is_some())
}

/// Returns whether a `ReflectionClass`/`ReflectionEnum` literal names a class-like the program
/// compiled: a class, an enum, an interface or a trait for `ReflectionClass`, an enum for
/// `ReflectionEnum`. Any other name stays on the interpreter, which raises php's
/// `ReflectionException`.
pub(super) fn reflection_class_literal_names_a_compiled_class_like(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<bool> {
    if inst.operands.len() != 1 || reflection_owner_requires_runtime_metadata(ctx, inst)? {
        return Ok(false);
    }
    let reflected = const_string_or_class_operand(ctx, inst.operands[0], class_name)?;
    if class_name == "ReflectionEnum" {
        return Ok(is_reflection_enum(ctx, &reflected));
    }
    Ok(resolve_reflection_class(ctx, &reflected).is_some()
        || resolve_reflection_interface(ctx, &reflected).is_some()
        || resolve_reflection_trait(ctx, &reflected).is_some())
}

/// Returns whether `new ReflectionProperty($class, $property)` is answered by compile-time metadata:
/// the property is one php's reflection sees on the class. An ancestor's private property is not,
/// and stays with the interpreter, which raises the `ReflectionException` php raises.
fn reflection_property_has_compiled_answer(info: &crate::types::ClassInfo, property: &str) -> bool {
    info.reflection_properties.iter().any(|name| name == property)
        && reflection_property_member_flags(info, property).is_some()
}

/// Most declared properties a runtime-name `ReflectionProperty` dispatch compares against; a wider
/// class stays on the interpreter rather than growing one construction site without bound.
const RUNTIME_NAME_PROPERTY_CANDIDATE_LIMIT: usize = 64;

/// Lowers `new ReflectionProperty(<compiled class>, $name)` by matching the runtime name against
/// the properties the class declares, and reports whether it did.
///
/// Each match builds the reflector from compile-time metadata through the shared materializer the
/// literal spelling uses; a name that matches none goes to the interpreter, which raises php's
/// `ReflectionException`. Every path stores the instruction's result. `Request::setProperty()`
/// reaches this with its seven bag names on every Symfony request, and paid an eval context, a
/// metadata sync and an interpreted construction for each.
fn lower_reflection_property_by_runtime_name(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<bool> {
    let [class_operand, name_operand] = inst.operands[..] else {
        return Ok(false);
    };
    if ctx.value_php_type(name_operand)?.codegen_repr() != PhpType::Str
        || reflection_literal_operand(ctx, name_operand)?.is_some()
    {
        return Ok(false);
    }
    let Some(class_key) = reflection_literal_operand(ctx, class_operand)? else {
        return Ok(false);
    };
    let reflected_class =
        const_string_or_class_operand(ctx, class_operand, "ReflectionProperty")?;
    let Some((_, info)) = resolve_reflection_class(ctx, &reflected_class) else {
        return Ok(false);
    };
    let candidates: Vec<String> = info
        .reflection_properties
        .iter()
        .filter(|name| reflection_property_has_compiled_answer(info, name))
        .cloned()
        .collect();
    if candidates.is_empty() || candidates.len() > RUNTIME_NAME_PROPERTY_CANDIDATE_LIMIT {
        return Ok(false);
    }
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("reflection object_new missing result"))?;
    let matched_labels = candidates
        .iter()
        .map(|name| ctx.next_label(&format!("reflection_property_{}", label_fragment(name))))
        .collect::<Vec<_>>();
    let miss_label = ctx.next_label("reflection_property_runtime_name_miss");
    let done_label = ctx.next_label("reflection_property_runtime_name_done");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(name_operand, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    for (name, label) in candidates.iter().zip(&matched_labels) {
        super::super::dynamic_property_read_resolution::emit_branch_if_dynamic_name_matches(
            ctx, name, label,
        );
    }
    abi::emit_jump(ctx.emitter, &miss_label);
    for (name, label) in candidates.iter().zip(&matched_labels) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        let metadata = reflection_property_metadata_for_property(ctx, &reflected_class, name)?;
        emit_direct_reflection_property_object(ctx, class_key.clone(), name, &metadata)?;
        ctx.store_result_value(result)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }
    ctx.emitter.label(&miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    crate::codegen::lower_inst::builtins::lower_eval_native_object_new(ctx, inst)?;
    ctx.emitter.label(&done_label);
    Ok(true)
}

/// Returns whether a literal `ReflectionClass` target needs runtime declaration metadata.
///
/// A class string can be lexically constant while its declaration is intentionally supplied by a
/// dynamic include. A missing AOT doc comment is ambiguous: it can mean either an undocumented
/// static class or a class loaded at runtime. The bridge resolves that ambiguity after loading,
/// returning the actual comment or PHP's `false` result.
fn reflection_class_literal_requires_runtime_metadata(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<bool> {
    if class_name != "ReflectionClass" {
        return Ok(false);
    }
    let Some(operand) = inst.operands.first().copied() else {
        return Ok(false);
    };
    let literal_source = reflection_literal_source(ctx, operand)?;
    let Some(value) = ctx.function.value(literal_source) else {
        return Err(CodegenIrError::missing_entry("value", literal_source.as_raw()));
    };
    let ValueDef::Instruction {
        inst: source_inst, ..
    } = value.def
    else {
        return Ok(false);
    };
    let Some(source_inst) = ctx.function.instruction(source_inst) else {
        return Err(CodegenIrError::missing_entry("instruction", source_inst.as_raw()));
    };
    if !matches!(source_inst.op, Op::ConstStr | Op::ConstClassName) {
        return Ok(false);
    }
    let reflected_class = const_string_or_class_operand(ctx, operand, "ReflectionClass")?;
    // A class the program compiled is never one a runtime include supplies (a file handed to the
    // runtime autoloader leaves `class_infos`), so its metadata -- a missing doc comment included
    // -- is the answer. Sending every undocumented class to the interpreter also cost the enum
    // facts: `(new ReflectionClass(Suit::class))->isEnum()` answered false there.
    if resolve_reflection_class(ctx, &reflected_class).is_some() {
        return Ok(false);
    }
    Ok(resolve_reflection_interface(ctx, &reflected_class).is_none()
        && resolve_reflection_trait(ctx, &reflected_class).is_none()
        && !is_reflection_enum(ctx, &reflected_class))
}

/// Returns whether `new ReflectionClass(<operand>)` reflects an object statically typed as one of
/// the program's compiled classes (not a class-name string, not an untyped value).
fn reflection_class_operand_is_compiled_object(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<bool> {
    let [operand] = inst.operands[..] else {
        return Ok(false);
    };
    Ok(match ctx.value_php_type(operand)?.codegen_repr() {
        PhpType::Object(class_name) => ctx
            .module
            .class_infos
            .contains_key(class_name.trim_start_matches('\\')),
        _ => false,
    })
}

/// Materializes `ReflectionObject` (or `ReflectionClass`, per `owner_class`) metadata from the
/// concrete runtime class of an object operand.
///
/// The eval bridge exposes one script-level source file and therefore cannot represent per-class
/// files from includes or autoloading. Closed object types dispatch directly to cached AOT
/// materializers, preserving exact file, line, parent, interface, and member metadata. Very broad
/// object universes stay on the existing dynamic bridge to avoid emitting thousands of graphs.
fn lower_reflection_object_by_runtime_class(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    owner_class: &str,
) -> Result<bool> {
    let Some(source) = inst.operands.first().copied() else {
        return Ok(false);
    };
    let (mut static_type, boxed_source) = match ctx.value_php_type(source)?.codegen_repr() {
        PhpType::Object(static_type) => (static_type, false),
        PhpType::Mixed | PhpType::Union(_) => {
            let Some(lexical_class) = current_method_class(ctx).ok() else {
                return Ok(false);
            };
            (lexical_class.to_string(), true)
        }
        _ => return Ok(false),
    };
    if static_type.is_empty() || static_type.eq_ignore_ascii_case("object") {
        let Some(lexical_class) = current_method_class(ctx).ok() else {
            return Ok(false);
        };
        if !ctx.module.class_infos.contains_key(lexical_class) {
            return Ok(false);
        }
        static_type = lexical_class.to_string();
    }
    let mut candidates = ctx
        .module
        .class_infos
        .iter()
        .filter(|(class_name, _)| {
            reflection_class_matches_object_type(ctx, class_name, &static_type)
        })
        .map(|(class_name, class_info)| (class_name.clone(), class_info.class_id))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(_, class_id)| *class_id);
    if candidates.is_empty() || candidates.len() > 64 {
        return Ok(false);
    }

    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    let done_label = ctx.next_label("reflection_object_runtime_done");
    let miss_label = ctx.next_label("reflection_object_runtime_miss");
    let match_labels = candidates
        .iter()
        .map(|(class_name, _)| {
            ctx.next_label(&format!(
                "reflection_object_runtime_{}",
                label_fragment(class_name)
            ))
        })
        .collect::<Vec<_>>();
    ctx.load_value_to_reg(source, &result_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if boxed_source {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                ctx.emitter.instruction("cmp x0, #6");                          // ReflectionObject requires a boxed object payload
                ctx.emitter.instruction(&format!("b.ne {}", miss_label));
                ctx.emitter.instruction("ldr x9, [x1]");                        // load the reflected object's concrete class id
            } else {
                ctx.emitter.instruction("ldr x9, [x0]");                        // load the reflected object's concrete class id
            }
            for ((_, class_id), label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", *class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare this reflected-class candidate id
                ctx.emitter.instruction(&format!("b.eq {}", label));            // materialize metadata for the matching object class
            }
        }
        Arch::X86_64 => {
            if boxed_source {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                ctx.emitter.instruction("cmp rax, 6");                          // ReflectionObject requires a boxed object payload
                ctx.emitter.instruction(&format!("jne {}", miss_label));
                ctx.emitter.instruction("mov r11, QWORD PTR [rdi]");            // load the reflected object's concrete class id
            } else {
                ctx.emitter.instruction("mov r11, QWORD PTR [rax]");            // load the reflected object's concrete class id
            }
            for ((_, class_id), label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", *class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // compare this reflected-class candidate id
                ctx.emitter.instruction(&format!("je {}", label));              // materialize metadata for the matching object class
            }
        }
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    let is_reflection_class = owner_class == "ReflectionClass";
    let source_metadata_only =
        !is_reflection_class && reflection_object_uses_only_source_metadata(ctx, inst);
    for ((class_name, _), label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        if is_reflection_class {
            emit_full_reflection_class_object(ctx, class_name)?;
        } else if source_metadata_only {
            emit_source_file_reflection_object(ctx, class_name)?;
        } else {
            emit_full_reflection_object(ctx, class_name)?;
        }
        abi::emit_jump(ctx.emitter, &done_label);
    }
    ctx.emitter.label(&miss_label);
    // The candidates are the classes THIS binary compiled. An object whose class an eval fragment
    // or a runtime include declared matches none of them, yet it is a perfectly ordinary object:
    // Twig's compiled templates are exactly that, and `new \ReflectionObject($template)` is how
    // Twig and Symfony UX find a template's file. The bridge owns those classes' metadata, so a
    // program that can reach it asks it instead of dying.
    if ctx.module.required_runtime_features.eval_bridge {
        crate::codegen::lower_inst::builtins::lower_eval_native_object_new(ctx, inst)?;
        if let Some(result) = inst.result {
            let result_reg = abi::int_result_reg(ctx.emitter).to_string();
            ctx.load_value_to_reg(result, &result_reg)?;
        }
        abi::emit_jump(ctx.emitter, &done_label);
    } else {
        let message = b"Fatal error: ReflectionObject runtime class is not available\n";
        super::super::emit_fatal_message(ctx, message);
    }
    ctx.emitter.label(&done_label);
    Ok(true)
}

/// Returns whether a reflection object only flows into source-location and name queries.
///
/// The source metadata materializer preserves every observable field consumed by
/// `getFileName()` and `name` while avoiding eager construction of method, property, and constant
/// reflection graphs. Any escape, control-flow transfer, unrecognized alias, or other member
/// access conservatively selects the full materializer instead.
fn reflection_object_uses_only_source_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> bool {
    let Some(result) = inst.result else {
        return false;
    };
    let mut object_aliases = HashSet::from([result]);
    collect_transparent_aliases(ctx, &mut object_aliases);

    let mut slots = HashSet::new();
    let mut saw_direct_source_metadata = false;
    for instruction in &ctx.function.instructions {
        if !instruction
            .operands
            .iter()
            .any(|operand| object_aliases.contains(operand))
        {
            continue;
        }
        match instruction.op {
            Op::StoreLocal => match instruction.immediate {
                Some(Immediate::LocalSlot(slot)) => {
                    slots.insert(slot);
                }
                _ => return false,
            },
            Op::MethodCall
                if instruction
                    .operands
                    .first()
                    .is_some_and(|receiver| object_aliases.contains(receiver))
                    && reflection_member_name(ctx, instruction)
                        .is_some_and(|method| method.eq_ignore_ascii_case("getFileName")) =>
            {
                saw_direct_source_metadata = true;
            }
            Op::PropGet
                if instruction
                    .operands
                    .first()
                    .is_some_and(|receiver| object_aliases.contains(receiver))
                    && reflection_member_name(ctx, instruction)
                        .is_some_and(|property| property.eq_ignore_ascii_case("name")) =>
            {
                saw_direct_source_metadata = true;
            }
            Op::Acquire | Op::Borrow | Op::Move | Op::Release | Op::ReleaseUnlessAliases => {}
            _ => return false,
        }
    }
    if saw_direct_source_metadata {
        return !ctx
            .function
            .blocks
            .iter()
            .filter_map(|block| block.terminator.as_ref())
            .any(|terminator| terminator_uses_alias(terminator, &object_aliases));
    }
    let Some(slot) = (slots.len() == 1).then(|| *slots.iter().next().unwrap()) else {
        return false;
    };

    let mut local_aliases = ctx
        .function
        .instructions
        .iter()
        .filter_map(|instruction| {
            (instruction.op == Op::LoadLocal
                && instruction.immediate == Some(Immediate::LocalSlot(slot)))
            .then_some(instruction.result)
            .flatten()
        })
        .collect::<HashSet<_>>();
    if local_aliases.is_empty() {
        return false;
    }
    collect_transparent_aliases(ctx, &mut local_aliases);

    let mut saw_source_metadata = false;
    for instruction in &ctx.function.instructions {
        if !instruction
            .operands
            .iter()
            .any(|operand| local_aliases.contains(operand))
        {
            continue;
        }
        if instruction.op == Op::MethodCall
            && instruction
                .operands
                .first()
                .is_some_and(|receiver| local_aliases.contains(receiver))
            && reflection_member_name(ctx, instruction)
                .is_some_and(|method| method.eq_ignore_ascii_case("getFileName"))
        {
            saw_source_metadata = true;
            continue;
        }
        if instruction.op == Op::PropGet
            && instruction
                .operands
                .first()
                .is_some_and(|receiver| local_aliases.contains(receiver))
            && reflection_member_name(ctx, instruction)
                .is_some_and(|property| property.eq_ignore_ascii_case("name"))
        {
            saw_source_metadata = true;
            continue;
        }
        if matches!(instruction.op, Op::Acquire | Op::Borrow | Op::Move | Op::Release) {
            continue;
        }
        return false;
    }

    saw_source_metadata
        && !ctx
            .function
            .blocks
            .iter()
            .filter_map(|block| block.terminator.as_ref())
            .any(|terminator| terminator_uses_alias(terminator, &local_aliases))
}

/// Extends `aliases` through SSA ownership-preserving forwarding instructions.
fn collect_transparent_aliases(ctx: &FunctionContext<'_>, aliases: &mut HashSet<ValueId>) {
    loop {
        let mut changed = false;
        for instruction in &ctx.function.instructions {
            if !matches!(instruction.op, Op::Acquire | Op::Borrow | Op::Move)
                || instruction.operands.len() != 1
                || !aliases.contains(&instruction.operands[0])
            {
                continue;
            }
            if let Some(result) = instruction.result {
                changed |= aliases.insert(result);
            }
        }
        if !changed {
            return;
        }
    }
}

/// Returns the source-level property or method name attached to an EIR member instruction.
fn reflection_member_name<'a>(
    ctx: &'a FunctionContext<'_>,
    instruction: &Instruction,
) -> Option<&'a str> {
    let Some(Immediate::Data(data)) = instruction.immediate else {
        return None;
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Returns whether a control-flow terminator consumes any alias from `aliases`.
fn terminator_uses_alias(terminator: &Terminator, aliases: &HashSet<ValueId>) -> bool {
    let contains = |value: &ValueId| aliases.contains(value);
    match terminator {
        Terminator::Br { args, .. } => args.iter().any(contains),
        Terminator::CondBr {
            cond,
            then_args,
            else_args,
            ..
        } => contains(cond) || then_args.iter().any(contains) || else_args.iter().any(contains),
        Terminator::Switch {
            scrutinee,
            cases,
            default_args,
            ..
        } => {
            contains(scrutinee)
                || cases.iter().any(|case| case.args.iter().any(contains))
                || default_args.iter().any(contains)
        }
        Terminator::Return { value } => value.as_ref().is_some_and(contains),
        Terminator::Throw { value } => contains(value),
        Terminator::GeneratorSuspend {
            key,
            value,
            resume_args,
            ..
        } => {
            key.as_ref().is_some_and(contains)
                || value.as_ref().is_some_and(contains)
                || resume_args.iter().any(contains)
        }
        Terminator::Fatal { .. } | Terminator::Unreachable => false,
    }
}

/// Returns whether a constructor argument needs runtime Reflection metadata lookup.
pub(super) fn reflection_owner_requires_runtime_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<bool> {
    for operand in &inst.operands {
        let literal_source = reflection_literal_source(ctx, *operand)?;
        if literal_source != *operand {
            let source_value = ctx.function.value(literal_source).ok_or_else(|| {
                CodegenIrError::missing_entry("value", literal_source.as_raw())
            })?;
            if let ValueDef::Instruction { inst: source_inst, .. } = source_value.def {
                let source_inst = ctx.function.instruction(source_inst).ok_or_else(|| {
                    CodegenIrError::missing_entry("instruction", source_inst.as_raw())
                })?;
                if matches!(
                    source_inst.op,
                    Op::ConstStr | Op::ConstClassName | Op::ConstI64
                ) {
                    continue;
                }
            }
        }
        let ty = ctx.value_php_type(*operand)?.codegen_repr();
        if matches!(ty, PhpType::Callable | PhpType::Object(_)) {
            return Ok(true);
        }
        if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
            return Ok(true);
        }
        if ty == PhpType::Str {
            let Some(value) = ctx.function.value(*operand) else {
                return Err(CodegenIrError::missing_entry("value", operand.as_raw()));
            };
            let ValueDef::Instruction { inst: defining_inst, .. } = value.def else {
                return Ok(true);
            };
            let Some(defining_inst) = ctx.function.instruction(defining_inst) else {
                return Err(CodegenIrError::missing_entry(
                    "instruction",
                    defining_inst.as_raw(),
                ));
            };
            if !matches!(defining_inst.op, Op::ConstStr | Op::ConstClassName) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Returns true when a runtime candidate class can inhabit an object's static type.
pub(in crate::codegen::lower_inst::objects) fn reflection_class_matches_object_type(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    static_type: &str,
) -> bool {
    if reflection_same_php_type_name(class_name, static_type) {
        return true;
    }
    if resolve_reflection_interface(ctx, static_type).is_some() {
        return reflection_class_implements_interface(ctx, class_name, static_type);
    }
    reflection_class_extends_class(ctx, class_name, static_type)
}

/// Returns true when two PHP type names compare case-insensitively after namespace trimming.
fn reflection_same_php_type_name(left: &str, right: &str) -> bool {
    php_symbol_key(left.trim_start_matches('\\')) == php_symbol_key(right.trim_start_matches('\\'))
}

/// Returns true when a runtime candidate class is or extends `target_class`.
fn reflection_class_extends_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    target_class: &str,
) -> bool {
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        if reflection_same_php_type_name(&name, target_class) {
            return true;
        }
        current = resolve_reflection_class(ctx, &name)
            .and_then(|(_, class_info)| class_info.parent.clone());
    }
    false
}

/// Returns true when a runtime candidate class implements the requested interface.
fn reflection_class_implements_interface(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    target_interface: &str,
) -> bool {
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        let Some((_, class_info)) = resolve_reflection_class(ctx, &name) else {
            return false;
        };
        if class_info.interfaces.iter().any(|interface_name| {
            reflection_interface_extends_interface(ctx, interface_name, target_interface)
        }) {
            return true;
        }
        current = class_info.parent.clone();
    }
    false
}

/// Returns true when an interface is or extends the requested interface target.
fn reflection_interface_extends_interface(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    target_interface: &str,
) -> bool {
    if reflection_same_php_type_name(interface_name, target_interface) {
        return true;
    }
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return false;
    };
    let Some(interface) = ctx.module.interface_infos.get(interface_name) else {
        return false;
    };
    interface
        .parents
        .iter()
        .any(|parent| reflection_interface_extends_interface(ctx, parent, target_interface))
}
