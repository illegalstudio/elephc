//! Purpose:
//! Implements eval-backed `ReflectionClass` query and static-property APIs.
//! It keeps the public method dispatch layer separate from owner construction.
//!
//! Called from:
//! - `crate::interpreter::statements` while dispatching Reflection methods.
//!
//! Key details:
//! - Queries combine eval declarations with focused AOT metadata hooks.
//! - PHP-visible errors and missing-member results are preserved at this boundary.

use super::*;

/// Handles lazy parent and constructor queries for eval-owned ReflectionClass objects.
pub(in crate::interpreter) fn eval_reflection_class_lazy_relation_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    #[cfg(not(test))]
    context.sync_global_eval_classes();
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    match method_name.to_ascii_lowercase().as_str() {
        "getconstructor" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            eval_reflection_constructor_object_result(
                EVAL_REFLECTION_OWNER_CLASS,
                &reflected_name,
                true,
                context,
                values,
            )
            .map(Some)
        }
        "getparentclass" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            let parent_class_name =
                match eval_reflection_class_like_attributes(&reflected_name, context) {
                    Some(metadata) => metadata.parent_class_name,
                    None => eval_reflection_aot_parent_class_name(&reflected_name, values)?,
                };
            eval_reflection_related_class_result(
                EVAL_REFLECTION_OWNER_CLASS,
                parent_class_name.as_deref(),
                false,
                context,
                values,
            )
            .map(Some)
        }
        _ => Ok(None),
    }
}

/// Handles `ReflectionClass::getName()` from the class identity retained by the eval bridge.
pub(in crate::interpreter) fn eval_reflection_class_name_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getName") {
        return Ok(None);
    }
    let Some(reflected_name) = context.eval_reflection_class_name(identity) else {
        return Ok(None);
    };
    eval_reflection_bind_no_args(evaluated_args)?;
    values.string(reflected_name).map(Some)
}

/// Handles eval-backed `ReflectionClass::implementsInterface()` calls.
pub(in crate::interpreter) fn eval_reflection_class_implements_interface_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("implementsInterface") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("interface")], evaluated_args)?;
    let interface_name = eval_reflection_string_arg(args[0], values)?;
    let _ = crate::interpreter::eval_spl_autoload_class(&interface_name, context, values)?;
    if !eval_reflection_interface_exists(&interface_name, context, values)? {
        if eval_reflection_non_interface_exists(&interface_name, context, values)? {
            return eval_throw_reflection_exception(
                &format!("{} is not an interface", interface_name),
                context,
                values,
            );
        }
        return eval_throw_reflection_exception(
            &format!("Interface \"{}\" does not exist", interface_name),
            context,
            values,
        );
    }
    let result = if eval_reflection_class_like_exists(&reflected_name, context) {
        eval_reflection_class_implements_interface_name(
            &reflected_name,
            &interface_name,
            context,
            values,
        )?
    } else if eval_runtime_interface_exists(&reflected_name, values)? {
        eval_reflection_same_class_like_name(&reflected_name, &interface_name)
    } else {
        let reflected_class = values.string(&reflected_name)?;
        let result = values.object_is_a(reflected_class, &interface_name, false);
        values.release(reflected_class)?;
        result?
    };
    values.bool_value(result).map(Some)
}

/// Handles eval-backed `ReflectionClass::isSubclassOf()` calls.
pub(in crate::interpreter) fn eval_reflection_class_is_subclass_of_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("isSubclassOf") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("class")], evaluated_args)?;
    let target_name = eval_reflection_string_arg(args[0], values)?;
    let _ = crate::interpreter::eval_spl_autoload_class(&target_name, context, values)?;
    if !eval_reflection_class_like_exists(&target_name, context)
        && !values.class_exists(&target_name)?
        && !eval_runtime_interface_exists(&target_name, values)?
        && !values.trait_exists(&target_name)?
        && !values.enum_exists(&target_name)?
    {
        return eval_throw_reflection_exception(
            &format!("Class \"{}\" does not exist", target_name),
            context,
            values,
        );
    }
    let result = if eval_reflection_class_like_exists(&reflected_name, context) {
        eval_reflection_class_is_subclass_of_name(
            &reflected_name,
            &target_name,
            context,
            values,
        )?
    } else {
        let reflected_class = values.string(&reflected_name)?;
        let result = values.object_is_a(reflected_class, &target_name, true)?;
        values.release(reflected_class)?;
        result
    };
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=reflection_is_subclass_of reflected={reflected_name:?} target={target_name:?} result={result}"
        );
    }
    values.bool_value(result).map(Some)
}

/// Handles eval-backed `ReflectionClass::isInstance()` calls.
pub(in crate::interpreter) fn eval_reflection_class_is_instance_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("isInstance") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("object")], evaluated_args)?;
    let object = args[0];
    if values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    let result = dynamic_object_is_a(object, &reflected_name, false, context, values)?
        .map_or_else(|| values.object_is_a(object, &reflected_name, false), Ok)?;
    values.bool_value(result).map(Some)
}

/// Handles eval-backed `ReflectionClass` source-location metadata calls.
pub(in crate::interpreter) fn eval_reflection_class_source_location_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let method_key = method_name.to_ascii_lowercase();
    if !matches!(
        method_key.as_str(),
        "getfilename" | "getstartline" | "getendline"
    ) {
        return Ok(None);
    }
    let Some(reflected_name) = context.eval_reflection_class_name(identity) else {
        return Ok(None);
    };
    let (aot_source_file, aot_source_location) =
        eval_reflection_aot_class_source_metadata(reflected_name, values)?;
    // Interpreted declarations record their own file when they execute. AOT declarations use the
    // class-name keyed source table; the module entry path is not a substitute for a declaration
    // path (for example, a bundle's getPath() reflects on its concrete class).
    let eval_metadata = eval_reflection_class_like_attributes(reflected_name, context);
    let eval_source_file = eval_metadata
        .as_ref()
        .and_then(|_| context.class_source_file(reflected_name))
        .map(str::to_string);
    let source_file = eval_source_file.or(aot_source_file);
    let source_location = eval_metadata
        .as_ref()
        .and_then(|metadata| metadata.source_location)
        .or(aot_source_location);
    eval_reflection_source_location_result(
        method_key.as_str(),
        source_file.as_deref(),
        source_location,
        evaluated_args,
        context,
        values,
    )
}

/// Returns AOT source-file and line metadata for a generated ReflectionClass.
pub(super) fn eval_reflection_aot_class_source_metadata(
    class_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<(Option<String>, Option<EvalSourceLocation>), EvalStatus> {
    let class_name = class_name.trim_start_matches('\\');
    let class_flags = values.reflection_class_flags(class_name)?;
    let source_file = match values.reflection_class_source_file(class_name)? {
        Some(source_file) => Some(source_file),
        None
            if class_flags.is_some_and(|flags| {
                flags & EVAL_REFLECTION_CLASS_FLAG_INTERNAL == 0
            }) =>
        {
            values.reflection_source_file()?
        }
        None => None,
    };
    let source_location = class_flags.and_then(eval_reflection_aot_class_source_location_from_flags);
    Ok((source_file, source_location))
}

/// Decodes AOT ReflectionClass source lines packed into high flag bits.
fn eval_reflection_aot_class_source_location_from_flags(flags: u64) -> Option<EvalSourceLocation> {
    let start_line = ((flags >> EVAL_REFLECTION_CLASS_SOURCE_START_SHIFT)
        & EVAL_REFLECTION_CLASS_SOURCE_LINE_MASK) as i64;
    let end_line = ((flags >> EVAL_REFLECTION_CLASS_SOURCE_END_SHIFT)
        & EVAL_REFLECTION_CLASS_SOURCE_LINE_MASK) as i64;
    (start_line > 0 && end_line >= start_line)
        .then(|| EvalSourceLocation::new(start_line, end_line))
}

/// Handles eval-backed `ReflectionClass` scalar metadata methods.
pub(in crate::interpreter) fn eval_reflection_class_basic_metadata_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let method_key = method_name.to_ascii_lowercase();
    let metadata = eval_reflection_class_like_attributes(&reflected_name, context);
    if metadata.is_none() {
        // No eval context DECLARED this class, which is the normal case for a class the compiler
        // produced and for a builtin. The facts still exist -- the generated program publishes
        // them through the reflection hooks -- but only `getDocComment()` ever asked, so every
        // other scalar answered "not handled" and the bridge turned that into an anonymous fatal.
        // Symfony's `ContainerBuilder` reflects on compiled classes while it tracks resources, so
        // this was the cold path's next stop after the backtrace work.
        return eval_reflection_runtime_class_scalar_result(
            &reflected_name,
            method_key.as_str(),
            evaluated_args,
            values,
        );
    }
    let metadata = metadata.expect("class reflection metadata was checked above");
    match method_key.as_str() {
        "getname" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values.string(&metadata.resolved_name).map(Some)
        }
        "getdoccomment" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            if crate::eval_trace::enabled() {
                let preview = metadata.doc_comment.as_deref().map(|doc_comment| {
                    doc_comment.chars().take(160).collect::<String>()
                });
                eprintln!(
                    "[elephc-eval-trace] phase=reflection_doc_comment class={:?} present={} len={} preview={preview:?}",
                    metadata.resolved_name,
                    metadata.doc_comment.is_some(),
                    metadata.doc_comment.as_ref().map_or(0, String::len),
                );
            }
            match metadata.doc_comment {
                Some(doc_comment) => values.string(&doc_comment).map(Some),
                None => values.bool_value(false).map(Some),
            }
        }
        "getshortname" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .string(&eval_reflection_short_name(&metadata.resolved_name))
                .map(Some)
        }
        "getnamespacename" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .string(&eval_reflection_namespace_name(&metadata.resolved_name))
                .map(Some)
        }
        "innamespace" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values
                .bool_value(!eval_reflection_namespace_name(&metadata.resolved_name).is_empty())
                .map(Some)
        }
        "getinterfacenames" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            let interface_names =
                eval_reflection_eval_metadata_interface_names(&metadata, context, values)?;
            eval_reflection_string_array_result(&interface_names, values).map(Some)
        }
        "gettraitnames" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            eval_reflection_string_array_result(&metadata.trait_names, values).map(Some)
        }
        "getparentclass" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            eval_reflection_related_class_result(
                EVAL_REFLECTION_OWNER_CLASS,
                metadata.parent_class_name.as_deref(),
                true,
                context,
                values,
            )
            .map(Some)
        }
        "getconstructor" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            eval_reflection_constructor_object_result(
                EVAL_REFLECTION_OWNER_CLASS,
                &metadata.resolved_name,
                true,
                context,
                values,
            )
            .map(Some)
        }
        "getmodifiers" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            values.int(metadata.modifiers as i64).map(Some)
        }
        "isfinal" => eval_reflection_class_flag_result(
            metadata.flags,
            EVAL_REFLECTION_CLASS_FLAG_FINAL,
            evaluated_args,
            values,
        ),
        "isabstract" => eval_reflection_class_flag_result(
            metadata.flags,
            EVAL_REFLECTION_CLASS_FLAG_ABSTRACT,
            evaluated_args,
            values,
        ),
        "isinterface" => eval_reflection_class_flag_result(
            metadata.flags,
            EVAL_REFLECTION_CLASS_FLAG_INTERFACE,
            evaluated_args,
            values,
        ),
        "istrait" => eval_reflection_class_flag_result(
            metadata.flags,
            EVAL_REFLECTION_CLASS_FLAG_TRAIT,
            evaluated_args,
            values,
        ),
        "isenum" => eval_reflection_class_flag_result(
            metadata.flags,
            EVAL_REFLECTION_CLASS_FLAG_ENUM,
            evaluated_args,
            values,
        ),
        "isreadonly" => eval_reflection_class_flag_result(
            metadata.flags,
            EVAL_REFLECTION_CLASS_FLAG_READONLY,
            evaluated_args,
            values,
        ),
        "isanonymous" => eval_reflection_class_flag_result(
            metadata.flags,
            EVAL_REFLECTION_CLASS_FLAG_ANONYMOUS,
            evaluated_args,
            values,
        ),
        "isinstantiable" => eval_reflection_class_flag_result(
            metadata.flags,
            EVAL_REFLECTION_CLASS_FLAG_INSTANTIABLE,
            evaluated_args,
            values,
        ),
        "iscloneable" => eval_reflection_class_flag_result(
            metadata.flags,
            EVAL_REFLECTION_CLASS_FLAG_CLONEABLE,
            evaluated_args,
            values,
        ),
        "isiterable" | "isiterateable" => {
            let flags = eval_reflection_eval_metadata_flags(&metadata, context, values)?;
            eval_reflection_class_flag_result(
                flags,
                EVAL_REFLECTION_CLASS_FLAG_ITERABLE,
                evaluated_args,
                values,
            )
        }
        "isinternal" => eval_reflection_class_flag_result(
            metadata.flags,
            EVAL_REFLECTION_CLASS_FLAG_INTERNAL,
            evaluated_args,
            values,
        ),
        "isuserdefined" => eval_reflection_class_flag_result(
            metadata.flags,
            EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED,
            evaluated_args,
            values,
        ),
        _ => Ok(None),
    }
}

/// Handles `ReflectionClass::__toString()` calls for eval-visible class metadata.
pub(in crate::interpreter) fn eval_reflection_class_to_string_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("__toString") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    eval_reflection_bind_no_args(evaluated_args)?;
    let rendered = eval_reflection_class_to_string(&reflected_name, context, values)?;
    values.string(&rendered).map(Some)
}

/// Returns one boolean ReflectionClass flag after validating a no-arg call.
fn eval_reflection_class_flag_result(
    flags: u64,
    flag: u64,
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    eval_reflection_bind_no_args(evaluated_args)?;
    values.bool_value(flags & flag != 0).map(Some)
}

/// Answers one `ReflectionClass` scalar from the generated program's own reflection metadata.
///
/// Used when no eval context declared the class. The flag word is the same one the eval path
/// builds by hand, so the member-to-bit mapping is shared; the name-derived members need no
/// metadata at all, and the interface list is already an array the runtime hands back.
fn eval_reflection_runtime_class_scalar_result(
    reflected_name: &str,
    method_key: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if method_key == "getdoccomment" {
        eval_reflection_bind_no_args(evaluated_args)?;
        return match values.reflection_class_doc_comment(reflected_name)? {
            Some(doc_comment) => values.string(&doc_comment).map(Some),
            None => values.bool_value(false).map(Some),
        };
    }
    let canonical = values
        .reflection_canonical_class_name(reflected_name)?
        .unwrap_or_else(|| reflected_name.to_string());
    let canonical = canonical.trim_start_matches('\\').to_string();
    match method_key {
        "getname" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            return values.string(&canonical).map(Some);
        }
        "getshortname" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            let short = canonical.rsplit('\\').next().unwrap_or(&canonical).to_string();
            return values.string(&short).map(Some);
        }
        "getnamespacename" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            let namespace = canonical
                .rsplit_once('\\')
                .map_or(String::new(), |(namespace, _)| namespace.to_string());
            return values.string(&namespace).map(Some);
        }
        "innamespace" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            return values.bool_value(canonical.contains('\\')).map(Some);
        }
        "getinterfacenames" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            return values.reflection_class_interface_names(&canonical).map(Some);
        }
        _ => {}
    }
    let flag = match method_key {
        "isfinal" => EVAL_REFLECTION_CLASS_FLAG_FINAL,
        "isabstract" => EVAL_REFLECTION_CLASS_FLAG_ABSTRACT,
        "isinterface" => EVAL_REFLECTION_CLASS_FLAG_INTERFACE,
        "istrait" => EVAL_REFLECTION_CLASS_FLAG_TRAIT,
        "isenum" => EVAL_REFLECTION_CLASS_FLAG_ENUM,
        "isreadonly" => EVAL_REFLECTION_CLASS_FLAG_READONLY,
        "isanonymous" => EVAL_REFLECTION_CLASS_FLAG_ANONYMOUS,
        "isinstantiable" => EVAL_REFLECTION_CLASS_FLAG_INSTANTIABLE,
        "iscloneable" => EVAL_REFLECTION_CLASS_FLAG_CLONEABLE,
        "isiterable" | "isiterateable" => EVAL_REFLECTION_CLASS_FLAG_ITERABLE,
        "isinternal" => EVAL_REFLECTION_CLASS_FLAG_INTERNAL,
        "isuserdefined" => EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED,
        _ => return Ok(None),
    };
    // The AOT answer the reflector was built from, not the raw published word: that word has no
    // enum bit and marks an enum instantiable and cloneable like any final class.
    let flags = match eval_reflection_aot_class_flags(&canonical, values)? {
        Some((flags, _)) => flags,
        None => match values.reflection_class_flags(&canonical)? {
            Some(flags) => flags,
            None => return Ok(None),
        },
    };
    if flag == EVAL_REFLECTION_CLASS_FLAG_USER_DEFINED {
        // The generated program publishes flags only for the classes it compiled FROM SOURCE, so
        // reaching here at all means the class is user-defined unless it is marked internal. The
        // flag word itself does not carry the bit, and answering from the bit gave `false` for
        // every compiled class.
        eval_reflection_bind_no_args(evaluated_args)?;
        let internal = flags & EVAL_REFLECTION_CLASS_FLAG_INTERNAL != 0;
        return values.bool_value(!internal).map(Some);
    }
    eval_reflection_class_flag_result(flags, flag, evaluated_args, values)
}

/// Handles eval-backed `ReflectionClass::hasMethod()` calls.
pub(in crate::interpreter) fn eval_reflection_class_has_method_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("hasMethod") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("name")], evaluated_args)?;
    let requested_name = eval_reflection_string_arg(args[0], values)?;
    let exists = if eval_reflection_is_builtin_closure(&reflected_name, context) {
        eval_reflection_builtin_closure_method_metadata(&requested_name).is_some()
    } else if let Some(metadata) =
        eval_reflection_class_like_attributes(&reflected_name, context)
    {
        let declared = metadata
            .method_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&requested_name));
        if declared {
            true
        } else if let Some(parent) = context.class_native_parent_name(&reflected_name) {
            eval_reflection_aot_method_exists_in_ancestry(&parent, &requested_name, values)?
        } else {
            false
        }
    } else {
        eval_reflection_aot_method_exists_in_ancestry(&reflected_name, &requested_name, values)?
    };
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=reflection_has_method reflected={reflected_name:?} method={requested_name:?} result={exists}"
        );
    }
    values.bool_value(exists).map(Some)
}

/// Returns whether a compiled class or one of its compiled ancestors declares a method.
///
/// A class's AOT method table omits the PRIVATE methods its ancestors declare, which php still
/// reports: `hasMethod()` is true for them, as `new ReflectionMethod()` finds them.
fn eval_reflection_aot_method_exists_in_ancestry(
    class_name: &str,
    method_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let mut owner = Some(class_name.to_string());
    while let Some(current) = owner {
        if eval_reflection_aot_method_metadata_if_exists(&current, method_name, values)?.is_some() {
            return Ok(true);
        }
        owner = eval_reflection_aot_parent_class_name(&current, values)?;
    }
    Ok(false)
}

/// Handles eval-backed `ReflectionClass::hasProperty()` and inherited `ReflectionObject` calls.
pub(in crate::interpreter) fn eval_reflection_class_has_property_result(
    object: RuntimeCellHandle,
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("hasProperty") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("name")], evaluated_args)?;
    let property_name = eval_reflection_string_arg(args[0], values)?;
    let mut exists =
        if let Some(metadata) = eval_reflection_class_like_attributes(&reflected_name, context) {
            metadata
                .property_names
                .iter()
                .any(|name| name == &property_name)
        } else {
            eval_reflection_aot_property_metadata_if_exists(
                &reflected_name,
                &property_name,
                context,
                values,
            )?
            .is_some()
                || eval_reflection_native_interface_property_requirement(
                    &reflected_name,
                    &property_name,
                    context,
                )
                .is_some()
        };
    if !exists {
        if let Some(dynamic_object) =
            eval_reflection_object_reflected_object(object, context, values)?
        {
            let dynamic_exists = eval_reflection_object_dynamic_property_exists(
                dynamic_object,
                &property_name,
                context,
                values,
            );
            values.release(dynamic_object)?;
            exists = dynamic_exists?;
        }
    }
    values.bool_value(exists).map(Some)
}

/// Handles eval-backed `ReflectionClass::hasConstant()` calls.
pub(in crate::interpreter) fn eval_reflection_class_has_constant_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("hasConstant") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("name")], evaluated_args)?;
    let constant_name = eval_reflection_string_arg(args[0], values)?;
    let constant_names = eval_reflection_constant_names(&reflected_name, context, values)?;
    values
        .bool_value(constant_names.iter().any(|name| name == &constant_name))
        .map(Some)
}

/// Handles eval-backed `ReflectionEnum` methods that are not inherited from `ReflectionClass`.
pub(in crate::interpreter) fn eval_reflection_enum_methods_result(
    object: RuntimeCellHandle,
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !eval_reflection_object_has_class(object, "ReflectionEnum", values)? {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let Some(enum_name) = context.resolve_enum_name(&reflected_name) else {
        return Ok(None);
    };
    match method_name.to_ascii_lowercase().as_str() {
        "hascase" => {
            let args = bind_evaluated_function_args(&[String::from("name")], evaluated_args)?;
            let requested_name = eval_reflection_string_arg(args[0], values)?;
            let exists = context
                .enum_decl(&enum_name)
                .and_then(|enum_decl| enum_decl.case(&requested_name))
                .is_some();
            values.bool_value(exists).map(Some)
        }
        "getcase" => {
            let args = bind_evaluated_function_args(&[String::from("name")], evaluated_args)?;
            let requested_name = eval_reflection_string_arg(args[0], values)?;
            let owner_kind = eval_reflection_enum_case_owner_kind(&enum_name, context)?;
            let result = eval_reflection_enum_case_object_result(
                owner_kind,
                &enum_name,
                &requested_name,
                context,
                values,
            )?;
            Ok(Some(result))
        }
        "getcases" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            let (case_names, owner_kind) = {
                let enum_decl = context.enum_decl(&enum_name).ok_or(EvalStatus::RuntimeFatal)?;
                let case_names = enum_decl
                    .cases()
                    .iter()
                    .map(|case| case.name().to_string())
                    .collect::<Vec<_>>();
                (case_names, eval_reflection_enum_case_owner_kind(&enum_name, context)?)
            };
            let mut result = values.array_new(case_names.len())?;
            for (index, case_name) in case_names.iter().enumerate() {
                let case_object = eval_reflection_enum_case_object_result(
                    owner_kind, &enum_name, case_name, context, values,
                )?;
                let key = values.int(index as i64)?;
                result = values.array_set(result, key, case_object)?;
            }
            Ok(Some(result))
        }
        "isbacked" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            let is_backed = context
                .enum_decl(&enum_name)
                .and_then(EvalEnum::backing_type)
                .is_some();
            values.bool_value(is_backed).map(Some)
        }
        "getbackingtype" => {
            eval_reflection_bind_no_args(evaluated_args)?;
            let backing_type = context
                .enum_decl(&enum_name)
                .and_then(EvalEnum::backing_type);
            let Some(backing_type) = backing_type else {
                return values.null().map(Some);
            };
            let metadata = eval_reflection_enum_backing_type_metadata(backing_type);
            eval_reflection_type_object_result(&metadata, values).map(Some)
        }
        _ => Ok(None),
    }
}

/// Handles eval-backed `ReflectionClass::getInterfaces()` and `getTraits()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_relation_objects_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let relation_kind = if method_name.eq_ignore_ascii_case("getInterfaces") {
        "interfaces"
    } else if method_name.eq_ignore_ascii_case("getTraits") {
        "traits"
    } else {
        return Ok(None);
    };
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=reflection_class_relations_start method={method_name:?} identity={identity} args={}",
            evaluated_args.len()
        );
    }
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let names =
        if let Some(metadata) = eval_reflection_class_like_attributes(&reflected_name, context) {
            if relation_kind == "interfaces" {
                eval_reflection_eval_metadata_interface_names(&metadata, context, values)?
            } else {
                metadata.trait_names
            }
        } else if relation_kind == "interfaces" {
            eval_reflection_aot_class_interface_names(&reflected_name, values)?
        } else {
            eval_reflection_aot_class_trait_names(&reflected_name, values)?
        };
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=reflection_class_relations reflected={reflected_name:?} kind={relation_kind} names={names:?}"
        );
    }
    eval_reflection_class_object_map_result(&names, Some(relation_kind), context, values).map(Some)
}

/// Handles eval-backed `ReflectionClass::getTraitAliases()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_trait_aliases_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getTraitAliases") {
        return Ok(None);
    }
    eval_reflection_bind_no_args(evaluated_args)?;
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let aliases = if context.trait_decl(&reflected_name).is_some() {
        context.trait_trait_aliases(&reflected_name)
    } else if eval_reflection_class_like_exists(&reflected_name, context) {
        context.class_trait_aliases(&reflected_name)
    } else {
        eval_reflection_aot_class_trait_aliases(&reflected_name, values)?
    };
    eval_reflection_string_assoc_result(aliases, values).map(Some)
}

/// Handles eval-backed `ReflectionClass::getConstant()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_constant_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getConstant") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(&[String::from("name")], evaluated_args)?;
    let constant_name = eval_reflection_string_arg(args[0], values)?;
    if let Some(value) =
        eval_reflection_constant_value(&reflected_name, &constant_name, context, values)?
    {
        return Ok(Some(value));
    }
    values.bool_value(false).map(Some)
}

/// Handles eval-backed `ReflectionClass::getConstants()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_constants_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getConstants") {
        return Ok(None);
    }
    let filter = eval_reflection_member_filter(evaluated_args, values)?;
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let names = eval_reflection_constant_names(&reflected_name, context, values)?;
    let mut result = values.assoc_new(names.len())?;
    for name in names {
        if !eval_reflection_constant_matches_filter(&reflected_name, &name, filter, context, values)?
        {
            continue;
        }
        let Some(value) = eval_reflection_constant_value(&reflected_name, &name, context, values)?
        else {
            continue;
        };
        let key = values.string(&name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(Some(result))
}

/// Handles eval-backed `ReflectionClass::getDefaultProperties()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_default_properties_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getDefaultProperties") {
        return Ok(None);
    }
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let property_names = eval_reflection_default_property_names(&reflected_name, context, values)?;
    // php lists the static defaults first, then the instance ones, each in reflection order.
    let mut statics = Vec::new();
    let mut instance = Vec::new();
    for name in property_names {
        let Some(member) =
            eval_reflection_default_property_metadata(&reflected_name, &name, context, values)?
        else {
            continue;
        };
        let declaring_class = member.declaring_class_name.as_deref().unwrap_or(&reflected_name);
        if let Some(message) = context.native_property_default_error(declaring_class, &name)
            .or_else(|| context.native_property_default_error(&reflected_name, &name))
            .map(str::to_string)
        {
            // Check retained errors before skipping missing values or allocating the result.
            return super::super::throwables::eval_throw_error(&message, context, values);
        }
        if member.default_value.is_none() {
            continue;
        }
        if member.is_static {
            statics.push((name, member));
        } else {
            instance.push((name, member));
        }
    }
    let mut result = values.assoc_new(statics.len() + instance.len())?;
    let outcome = (|| {
        for (name, member) in statics.into_iter().chain(instance) {
            let default = member.default_value.as_ref().ok_or(EvalStatus::RuntimeFatal)?;
            let value = eval_reflection_member_default_value(&member, default, context, values)?;
            let key = match values.string(&name) {
                Ok(key) => key,
                Err(status) => { let _ = eval_release_value(context, values, value); return Err(status); }
            };
            let stored = values.array_set(result, key, value);
            let released_value = eval_release_value(context, values, value);
            let released_key = values.release(key);
            result = stored?;
            released_value.and(released_key)?;
        }
        Ok(Some(result))
    })();
    if outcome.is_err() { let _ = eval_release_value(context, values, result); }
    outcome
}

/// Handles eval-backed `ReflectionClass::getStaticProperties()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_static_properties_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getStaticProperties") {
        return Ok(None);
    }
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let property_names = eval_reflection_static_property_names(&reflected_name, context, values)?;
    let mut result = values.assoc_new(property_names.len())?;
    for name in property_names {
        let Some(value) =
            eval_reflection_static_property_value(&reflected_name, &name, context, values)?
        else {
            continue;
        };
        let key = values.string(&name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(Some(result))
}

/// Handles eval-backed `ReflectionClass::getStaticPropertyValue()` calls.
pub(in crate::interpreter) fn eval_reflection_class_get_static_property_value_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("getStaticPropertyValue") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let (property_name, default_value) =
        eval_reflection_static_property_value_args(evaluated_args)?;
    let property_name = eval_reflection_string_arg(property_name, values)?;
    if let Some(value) =
        eval_reflection_static_property_value(&reflected_name, &property_name, context, values)?
    {
        return Ok(Some(value));
    }
    if let Some(default_value) = default_value {
        return Ok(Some(default_value));
    }
    eval_throw_reflection_exception(
        &format!(
            "Property {}::${} does not exist",
            reflected_name, property_name
        ),
        context,
        values,
    )
}

/// Handles eval-backed `ReflectionClass::setStaticPropertyValue()` calls.
pub(in crate::interpreter) fn eval_reflection_class_set_static_property_value_result(
    identity: u64,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !method_name.eq_ignore_ascii_case("setStaticPropertyValue") {
        return Ok(None);
    }
    let Some(reflected_name) = context
        .eval_reflection_class_name(identity)
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let args = bind_evaluated_function_args(
        &[String::from("name"), String::from("value")],
        evaluated_args,
    )?;
    let property_name = eval_reflection_string_arg(args[0], values)?;
    let Some(member) =
        eval_reflection_static_property_metadata(&reflected_name, &property_name, context, values)?
    else {
        return eval_reflection_static_property_missing_for_set(
            &reflected_name,
            &property_name,
            context,
            values,
        );
    };
    if !member.is_static {
        return eval_reflection_static_property_missing_for_set(
            &reflected_name,
            &property_name,
            context,
            values,
        );
    }
    if eval_reflection_class_like_exists(&reflected_name, context) {
        let declaring_class = member
            .declaring_class_name
            .as_deref()
            .ok_or(EvalStatus::RuntimeFatal)?;
        store_borrowed_static_property(declaring_class, &property_name, args[1], context, values)?;
    } else {
        let declaring_class = member
            .declaring_class_name
            .as_deref()
            .unwrap_or(reflected_name.as_str());
        let updated = eval_reflection_with_declaring_class_scope(declaring_class, context, |_| {
            values.static_property_set(&reflected_name, &property_name, args[1])
        })?;
        if updated {
            return values.null().map(Some);
        }
        return eval_reflection_static_property_missing_for_set(
            &reflected_name,
            &property_name,
            context,
            values,
        );
    }
    values.null().map(Some)
}
