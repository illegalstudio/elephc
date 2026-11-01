//! Purpose:
//! Validates, registers, and initializes eval-declared enums.
//!
//! Called from:
//! - Statement dispatch for enum declarations.
//!
//! Key details:
//! - Trait expansion, synthetic methods, backing values, cases, and static defaults stay ordered.

use super::*;

/// Registers an eval-declared enum and materializes its singleton cases.
pub(in crate::interpreter) fn execute_enum_decl_stmt(
    enum_decl: &EvalEnum,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let interfaces = enum_decl.interfaces().iter().map(|name| ensure_generic_class(name, None, "", context, scope, values)).collect::<Result<_, _>>()?;
    let traits: Vec<String> = enum_decl.traits().iter().map(|name| ensure_generic_class(name, None, "", context, scope, values)).collect::<Result<_, _>>()?;
    let adaptations = concrete_trait_adaptations(enum_decl.trait_adaptations(), &traits);
    let concrete = enum_decl.clone().with_relations(interfaces, traits, adaptations);
    let enum_decl = &concrete;
    let name = enum_decl.name().trim_start_matches('\\');
    if context.generic_template(name).is_some()
        || context.has_enum(name)
        || context.has_class(name)
        || context.has_interface(name)
        || context.has_trait(name)
        || values.enum_exists(name)?
        || values.class_exists(name)?
        || eval_runtime_interface_exists(name, values)?
        || values.trait_exists(name)?
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    validate_eval_enum_direct_method_declarations(enum_decl)?;
    let enum_decl = expand_eval_enum_traits(enum_decl, context)?;
    let enum_decl = &enum_decl;
    validate_eval_enum_decl(enum_decl, context, values)?;
    let declaration_file = eval_declaration_source_file(context);
    if context.define_enum(enum_decl.clone()) {
        context.set_class_source_file(enum_decl.name(), declaration_file);
        initialize_eval_declared_constants(
            enum_decl.name(),
            enum_decl.constants(),
            context,
            scope,
            values,
        )?;
        initialize_eval_enum_cases(enum_decl, context, scope, values)
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Expands eval trait uses into enum metadata while rejecting imported properties.
pub(super) fn expand_eval_enum_traits(
    enum_decl: &EvalEnum,
    context: &ElephcEvalContext,
) -> Result<EvalEnum, EvalStatus> {
    if enum_decl.traits().is_empty() {
        return Ok(enum_decl.clone());
    }
    let enum_class = enum_decl.as_class_metadata();
    validate_eval_trait_adaptations(&enum_class, context)?;
    let mut enum_method_names = class_method_name_set(&enum_class);
    insert_eval_enum_synthetic_method_names(enum_decl, &mut enum_method_names);
    let mut trait_method_names = std::collections::HashSet::new();
    let mut trait_properties = std::collections::HashMap::new();
    let mut trait_constants = std::collections::HashMap::new();
    let mut constants = Vec::new();
    let mut properties = Vec::new();
    let mut methods = Vec::new();
    for trait_name in enum_decl.traits() {
        let Some(trait_decl) = context.trait_decl(trait_name) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        append_eval_trait_constants(
            trait_decl,
            enum_decl.constants(),
            &mut trait_constants,
            &mut constants,
        )?;
        append_eval_trait_properties(
            trait_decl,
            &[],
            &mut trait_properties,
            &mut properties,
        )?;
        append_eval_trait_methods(
            trait_decl,
            enum_decl.trait_adaptations(),
            &enum_method_names,
            &mut trait_method_names,
            &mut methods,
        )?;
    }
    if !properties.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    constants.extend(enum_decl.constants().iter().cloned());
    methods.extend(enum_decl.methods().iter().cloned());
    let mut expanded = EvalEnum::with_members_traits_adaptations(
        enum_decl.name().to_string(),
        enum_decl.backing_type(),
        enum_decl.interfaces().to_vec(),
        enum_decl.cases().to_vec(),
        constants,
        methods,
        enum_decl.traits().to_vec(),
        enum_decl.trait_adaptations().to_vec(),
    )
    .with_attributes(enum_decl.attributes().to_vec());
    if let Some(source_location) = enum_decl.source_location() {
        expanded = expanded.with_source_location(source_location);
    }
    Ok(expanded)
}

/// Adds PHP's enum-provided method names to the set that hides trait imports.
pub(super) fn insert_eval_enum_synthetic_method_names(
    enum_decl: &EvalEnum,
    method_names: &mut std::collections::HashSet<String>,
) {
    method_names.insert(String::from("cases"));
    if enum_decl.backing_type().is_some() {
        method_names.insert(String::from("from"));
        method_names.insert(String::from("tryfrom"));
    }
}

/// Validates enum metadata before it is inserted into the dynamic context.
pub(super) fn validate_eval_enum_decl(
    enum_decl: &EvalEnum,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    validate_eval_enum_attribute_targets(enum_decl)?;
    validate_eval_declared_constants(enum_decl.constants())?;
    validate_eval_enum_case_declarations(enum_decl)?;
    validate_eval_enum_forbidden_magic_methods(enum_decl)?;
    let enum_class = enum_decl.as_class_metadata();
    validate_eval_class_modifiers(&enum_class, context, values)?;
    validate_eval_enum_interfaces(enum_decl, &enum_class, context, values)?;
    validate_declared_class_builtin_interface_members(&enum_class, context, values)?;
    validate_declared_class_aot_interface_members(&enum_class, context, values)?;
    validate_concrete_class_builtin_interface_requirements(&enum_class, context, values)?;
    validate_concrete_class_aot_interface_requirements(&enum_class, context, values, true)?;
    validate_concrete_class_requirements(&enum_class, context, values)
}

/// Validates PHP's special enum interface rules for one eval enum declaration.
pub(super) fn validate_eval_enum_interfaces(
    enum_decl: &EvalEnum,
    enum_class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for interface in enum_decl.interfaces() {
        if eval_builtin_enum_interface_name(interface) {
            return Err(EvalStatus::RuntimeFatal);
        }
        if !context.has_interface(interface) && !eval_runtime_interface_exists(interface, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    validate_eval_class_does_not_implement_throwable_interfaces(enum_class, context, values)?;
    if enum_decl.backing_type().is_none()
        && pending_class_interface_names(enum_class, context)
            .iter()
            .any(|interface| eval_builtin_backed_enum_interface_name(interface))
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Validates enum case names and pure/backed declaration shape.
pub(super) fn validate_eval_enum_case_declarations(enum_decl: &EvalEnum) -> Result<(), EvalStatus> {
    let mut case_names = std::collections::HashSet::new();
    let constant_names = enum_decl
        .constants()
        .iter()
        .map(|constant| constant.name().to_string())
        .collect::<std::collections::HashSet<_>>();
    for case in enum_decl.cases() {
        validate_eval_core_attribute_targets(case.attributes(), 16)?;
        if !case_names.insert(case.name().to_string()) {
            return Err(EvalStatus::RuntimeFatal);
        }
        if constant_names.contains(case.name()) {
            return Err(EvalStatus::RuntimeFatal);
        }
        match (enum_decl.backing_type(), case.value()) {
            (None, None) | (Some(_), Some(_)) => {}
            (None, Some(_)) | (Some(_), None) => return Err(EvalStatus::RuntimeFatal),
        }
    }
    Ok(())
}

/// Validates direct enum methods that PHP reserves on enum declarations.
pub(super) fn validate_eval_enum_direct_method_declarations(enum_decl: &EvalEnum) -> Result<(), EvalStatus> {
    for method in enum_decl.methods() {
        if method.name().eq_ignore_ascii_case("cases") {
            return Err(EvalStatus::RuntimeFatal);
        }
        if enum_decl.backing_type().is_some()
            && (method.name().eq_ignore_ascii_case("from")
                || method.name().eq_ignore_ascii_case("tryFrom"))
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        if is_forbidden_eval_enum_magic_method(method.name()) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates enum methods, including trait imports, that PHP forbids by magic name.
pub(super) fn validate_eval_enum_forbidden_magic_methods(enum_decl: &EvalEnum) -> Result<(), EvalStatus> {
    for method in enum_decl.methods() {
        if is_forbidden_eval_enum_magic_method(method.name()) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Returns whether PHP forbids this magic method name inside enum declarations.
pub(super) fn is_forbidden_eval_enum_magic_method(name: &str) -> bool {
    [
        "__construct",
        "__destruct",
        "__clone",
        "__get",
        "__set",
        "__isset",
        "__unset",
        "__sleep",
        "__wakeup",
        "__serialize",
        "__unserialize",
        "__toString",
        "__debugInfo",
        "__set_state",
    ]
    .iter()
    .any(|method| name.eq_ignore_ascii_case(method))
}

/// Initializes enum singleton case objects for a newly declared eval enum.
pub(super) fn initialize_eval_enum_cases(
    enum_decl: &EvalEnum,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut backing_values = Vec::new();
    for case in enum_decl.cases() {
        let backing_value = if let Some(value_expr) = case.value() {
            // Routed through `eval_class_like_member_default` (like constant initializers and
            // property defaults) so `self::CONST`/`parent::CONST` resolve against the enum's own
            // class scope instead of finding no live method frame -- a case value is php's same
            // compile-time-constant position, so `static::` is refused there too.
            let value = eval_class_like_member_default(
                enum_decl.name(),
                None,
                value_expr,
                context,
                scope,
                values,
            )?;
            validate_eval_enum_backing_value(enum_decl.backing_type(), value, values)?;
            for existing in &backing_values {
                let equal = values.compare(EvalBinOp::StrictEq, value, *existing)?;
                if values.truthy(equal)? {
                    return Err(EvalStatus::RuntimeFatal);
                }
            }
            backing_values.push(value);
            Some(value)
        } else {
            None
        };
        initialize_eval_enum_case(enum_decl, case, backing_value, context, values)?;
    }
    Ok(())
}

/// Validates that one evaluated enum backing value matches the declared backing type.
pub(super) fn validate_eval_enum_backing_value(
    backing_type: Option<EvalEnumBackingType>,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(backing_type) = backing_type else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let tag = values.type_tag(value)?;
    match backing_type {
        EvalEnumBackingType::Int if tag == EVAL_TAG_INT => Ok(()),
        EvalEnumBackingType::String if tag == EVAL_TAG_STRING => Ok(()),
        EvalEnumBackingType::Int | EvalEnumBackingType::String => Err(EvalStatus::RuntimeFatal),
    }
}

/// Creates and stores one enum case singleton object.
pub(super) fn initialize_eval_enum_case(
    enum_decl: &EvalEnum,
    case: &EvalEnumCase,
    backing_value: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let object = values.new_object("stdClass")?;
    let identity = values.object_identity(object)?;
    context.register_dynamic_object(identity, enum_decl.name());
    let name = values.string(case.name())?;
    values.property_set(object, "name", name)?;
    if let Some(value) = backing_value {
        values.property_set(object, "value", value)?;
        if let Some(replaced) = context.set_enum_case_value(enum_decl.name(), case.name(), value) {
            values.release(replaced)?;
        }
    }
    if let Some(replaced) = context.set_enum_case(enum_decl.name(), case.name(), object) {
        values.release(replaced)?;
    }
    Ok(())
}

/// Initializes class-like constant cells for a newly declared eval class-like.
pub(super) fn initialize_eval_declared_constants(
    owner_name: &str,
    constants: &[EvalClassConstant],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for constant in constants {
        let value = eval_class_like_member_default(
            owner_name,
            constant.trait_origin(),
            constant.value(),
            context,
            scope,
            values,
        )?;
        if let Some(replaced) = context.set_class_constant_cell(owner_name, constant.name(), value)
        {
            values.release(replaced)?;
        }
    }
    Ok(())
}

/// Returns one class-like constant's runtime cell, building it on first read when needed.
///
/// Constant cells belong to the context that executed the declaration. A class-like declared
/// inside a nested eval invocation — the `include` an autoloader performs, say — reaches every
/// other context through `sync_global_eval_classes`, which copies the declaration and not the
/// cells, so a later read there found the declaration with nothing behind it: `self::CONST` in a
/// reflected parameter default killed the eval bridge and `ReflectionClass::getConstant()`
/// answered `false`. A constant initializer is a constant expression, so evaluating it here under
/// the declaring class's magic scope reproduces exactly what the declaring context stored; the
/// result is cached under the declaring class, so every later read is a lookup again.
pub(in crate::interpreter) fn eval_class_like_constant_cell(
    declaring_class: &str,
    constant: &EvalClassConstant,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(cell) = context.class_constant_cell(declaring_class, constant.name()) {
        return Ok(cell);
    }
    // A constant's initializer is only evaluated on first read (or the eager pass at
    // declaration time, which reaches the same cache-on-miss path for a forward reference), so
    // `self::A` referencing `self::B` referencing `self::A` recurses through this exact function
    // without ever finding a cached cell. php detects the cycle and raises a catchable `Error`
    // (`Cannot declare self-referencing constant self::X`); marking the pair in progress here
    // reproduces that instead of overflowing the stack.
    if !context.begin_class_constant_evaluation(declaring_class, constant.name()) {
        return eval_throw_error(
            &format!(
                "Cannot declare self-referencing constant self::{}",
                constant.name()
            ),
            context,
            values,
        );
    }
    let mut scope = ElephcEvalScope::new();
    let value = eval_class_like_member_default(
        declaring_class,
        constant.trait_origin(),
        constant.value(),
        context,
        &mut scope,
        values,
    );
    context.end_class_constant_evaluation(declaring_class, constant.name());
    // The cache owns the cell and every read borrows it, the first one included: handing the
    // freshly evaluated cell to this caller as OWNED let its release free the cached value, and
    // the next read returned whatever reused the memory -- Laravel's `DateFactory` read
    // `static::DEFAULT_CLASS_NAME` back as a Carbon translation string.
    let value = value?;
    let owned = if value.is_borrowed() { values.retain(value)? } else { value };
    if let Some(replaced) =
        context.set_class_constant_cell(declaring_class, constant.name(), owned)
    {
        values.release(replaced)?;
    }
    Ok(RuntimeCellHandle::borrowed(owned))
}

/// Evaluates a class-like constant or property initializer with PHP magic scope.
///
/// Constant initializers, property defaults, and enum case values are php's "compile-time
/// constant" positions: `self::`/`parent::` resolve to the DECLARING class regardless of which
/// subclass triggered evaluation, and `static::` has no meaning at all (php refuses it as an
/// uncatchable compile error). Pushing the class scope here — not just the magic-constant scope
/// — is what lets `resolve_eval_static_class_name`'s `self`/`parent` arms see the right class:
/// before this, `self::CONST` used inside one of these positions found `current_class_scope()`
/// empty (that stack is populated only by a live METHOD call frame) and failed with `unsupported
/// ClassConstantFetch expression` for every caller, `symfony/http-foundation/Request.php`'s
/// `self::HEADER_X_FORWARDED_FOR`-keyed constant array included. The compile-time-constant depth
/// counter is what lets `static::` be refused instead of silently falling back to the newly
/// working `self::` answer.
pub(super) fn eval_class_like_member_default(
    owner_name: &str,
    trait_origin: Option<&str>,
    default: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let trait_name = trait_origin.or_else(|| context.has_trait(owner_name).then_some(owner_name));
    context.push_class_like_member_magic_scope(owner_name, trait_name);
    context.push_class_scope(owner_name.to_string());
    context.push_compile_time_constant_context();
    let result = eval_expr(default, context, scope, values);
    context.pop_compile_time_constant_context();
    context.pop_class_scope();
    context.pop_magic_scope();
    result
}

/// Initializes static property cells for a newly declared eval class.
pub(super) fn initialize_eval_static_properties(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for property in class
        .properties()
        .iter()
        .filter(|property| property.is_static())
    {
        let value = if let Some(default) = property.default() {
            Some(eval_class_like_member_default(
                class.name(),
                property.trait_origin(),
                default,
                context,
                scope,
                values,
            )?)
        } else if property.property_type().is_none() {
            Some(values.null()?)
        } else {
            None
        };
        if crate::eval_trace::enabled() {
            eprintln!(
                "[elephc-eval-trace] phase=static_property_seed class={:?} property={:?} has_default={} seeded={}",
                class.name(),
                property.name(),
                property.default().is_some(),
                value.is_some(),
            );
        }
        if let Some(value) = value {
            store_static_property_value(class.name(), property.name(), value, context, values)?;
        }
    }
    Ok(())
}
