//! Purpose:
//! Executes EvalIR statements, loops, exception handling, static locals, and eval-declared classes.
//!
//! Called from:
//! - `crate::interpreter::execute_program_outcome_with_context()` and dynamic function execution.
//!
//! Key details:
//! - Statement execution propagates `EvalControl` instead of flattening returns, throws, breaks, or continues.
//! - Scope writes flow through shared scope-cell helpers so global aliases and reference aliases stay coherent.

use super::*;

/// Executes statements in source order and propagates the first eval `return`.
pub(in crate::interpreter) fn execute_statements(
    statements: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    for stmt in statements {
        match execute_stmt(stmt, context, scope, values)? {
            EvalControl::None => {}
            control => return Ok(control),
        }
    }
    Ok(EvalControl::None)
}

/// Executes one statement and returns `Some` only for eval `return`.
pub(in crate::interpreter) fn execute_stmt(
    stmt: &EvalStmt,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    match stmt {
        EvalStmt::ArrayAppendVar { name, value } => {
            let mut ownership = ScopeCellOwnership::Owned;
            let array = if let Some(existing) =
                scope_entry(context, scope, name).filter(|entry| entry.flags().is_visible())
            {
                if values.is_array_like(existing.cell())? {
                    let tag = values.type_tag(existing.cell())?;
                    if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
                        return Err(EvalStatus::UnsupportedConstruct);
                    }
                    ownership = existing.flags().ownership;
                    existing.cell()
                } else {
                    values.array_new(1)?
                }
            } else {
                values.array_new(1)?
            };
            let index = eval_array_append_key(array, values)?;
            let value = eval_expr(value, context, scope, values)?;
            let array = values.array_set(array, index, value)?;
            for replaced in set_scope_cell(context, scope, name.clone(), array, ownership)? {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::ArraySetVar { name, index, value } => {
            let mut ownership = ScopeCellOwnership::Owned;
            let array = if let Some(existing) =
                scope_entry(context, scope, name).filter(|entry| entry.flags().is_visible())
            {
                if values.is_array_like(existing.cell())? {
                    ownership = existing.flags().ownership;
                    existing.cell()
                } else {
                    values.array_new(1)?
                }
            } else {
                values.array_new(1)?
            };
            let index = eval_expr(index, context, scope, values)?;
            let value = eval_expr(value, context, scope, values)?;
            let array = values.array_set(array, index, value)?;
            for replaced in set_scope_cell(context, scope, name.clone(), array, ownership)? {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Break => Ok(EvalControl::Break),
        EvalStmt::Continue => Ok(EvalControl::Continue),
        EvalStmt::DoWhile { body, condition } => {
            execute_do_while_stmt(body, condition, context, scope, values)
        }
        EvalStmt::Echo(expr) => {
            let value = eval_expr(expr, context, scope, values)?;
            values.echo(value)?;
            Ok(EvalControl::None)
        }
        EvalStmt::For {
            init,
            condition,
            update,
            body,
        } => execute_for_stmt(
            init,
            condition.as_ref(),
            update,
            body,
            context,
            scope,
            values,
        ),
        EvalStmt::ClassDecl(class) => {
            execute_class_decl_stmt(class, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::InterfaceDecl(interface) => {
            execute_interface_decl_stmt(interface, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::TraitDecl(trait_decl) => {
            execute_trait_decl_stmt(trait_decl, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::Foreach {
            array,
            key_name,
            value_name,
            body,
        } => execute_foreach_stmt(
            array,
            key_name.as_deref(),
            value_name,
            body,
            context,
            scope,
            values,
        ),
        EvalStmt::FunctionDecl { name, params, body } => {
            let key = name.to_ascii_lowercase();
            context
                .define_function(
                    key,
                    EvalFunction::new(name.clone(), params.clone(), body.clone()),
                )
                .map_err(|_| EvalStatus::RuntimeFatal)?;
            Ok(EvalControl::None)
        }
        EvalStmt::Global { vars } => {
            execute_global_stmt(vars, context, scope)?;
            Ok(EvalControl::None)
        }
        EvalStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = eval_expr(condition, context, scope, values)?;
            if values.truthy(condition)? {
                execute_statements(then_branch, context, scope, values)
            } else {
                execute_statements(else_branch, context, scope, values)
            }
        }
        EvalStmt::Return(Some(expr)) => Ok(EvalControl::Return(eval_expr(
            expr, context, scope, values,
        )?)),
        EvalStmt::Return(None) => Ok(EvalControl::Return(values.null()?)),
        EvalStmt::ReferenceAssign { target, source } => {
            for replaced in set_reference_alias(context, scope, target, source, values)? {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::StaticVar { name, init } => {
            execute_static_var_stmt(name, init, context, scope, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::PropertySet {
            object,
            property,
            value,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let value = eval_expr(value, context, scope, values)?;
            eval_property_set_result(object, property, value, context, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::StaticPropertySet {
            class_name,
            property,
            value,
        } => {
            let value = eval_expr(value, context, scope, values)?;
            eval_static_property_set_result(class_name, property, value, context, values)?;
            Ok(EvalControl::None)
        }
        EvalStmt::StoreVar { name, value } => {
            let value = eval_expr(value, context, scope, values)?;
            for replaced in set_scope_cell(
                context,
                scope,
                name.clone(),
                value,
                ScopeCellOwnership::Owned,
            )? {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Switch { expr, cases } => {
            execute_switch_stmt(expr, cases, context, scope, values)
        }
        EvalStmt::Throw(expr) => {
            let thrown = eval_expr(expr, context, scope, values)?;
            if values.type_tag(thrown)? != EVAL_TAG_OBJECT {
                return Err(EvalStatus::RuntimeFatal);
            }
            Ok(EvalControl::Throw(thrown))
        }
        EvalStmt::Try {
            body,
            catches,
            finally_body,
        } => execute_try_stmt(body, catches, finally_body, context, scope, values),
        EvalStmt::UnsetVar { name } => {
            if let Some(replaced) = unset_scope_cell(scope, name.clone()) {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::While { condition, body } => {
            while {
                let condition = eval_expr(condition, context, scope, values)?;
                values.truthy(condition)?
            } {
                match execute_statements(body, context, scope, values)? {
                    EvalControl::None | EvalControl::Continue => {}
                    EvalControl::Break => break,
                    EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
                    EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
                }
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Expr(expr) => {
            let _ = eval_expr(expr, context, scope, values)?;
            Ok(EvalControl::None)
        }
    }
}

/// Executes an eval `try` body and handles supported `catch` clauses.
pub(in crate::interpreter) fn execute_try_stmt(
    body: &[EvalStmt],
    catches: &[EvalCatch],
    finally_body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let control = match execute_statements(body, context, scope, values) {
        Ok(EvalControl::Throw(thrown)) => {
            execute_matching_catch(thrown, catches, context, scope, values)?
        }
        Err(EvalStatus::UncaughtThrowable) => {
            let Some(thrown) = context.take_pending_throw() else {
                return Err(EvalStatus::UncaughtThrowable);
            };
            execute_matching_catch(thrown, catches, context, scope, values)?
        }
        Ok(control) => control,
        Err(status) => return Err(status),
    };
    if finally_body.is_empty() {
        return Ok(control);
    }
    match execute_statements(finally_body, context, scope, values) {
        Ok(EvalControl::None) => Ok(control),
        Ok(finally_control) => {
            release_overridden_control(control, values)?;
            Ok(finally_control)
        }
        Err(status) => {
            release_overridden_control(control, values)?;
            Err(status)
        }
    }
}

/// Releases a pending control-flow value when `finally` replaces that action.
pub(in crate::interpreter) fn release_overridden_control(
    control: EvalControl,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match control {
        EvalControl::Return(value) | EvalControl::Throw(value) => values.release(value),
        EvalControl::None | EvalControl::Break | EvalControl::Continue => Ok(()),
    }
}

/// Executes the first supported catch clause for a thrown eval object.
pub(in crate::interpreter) fn execute_matching_catch(
    thrown: RuntimeCellHandle,
    catches: &[EvalCatch],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let mut matched = None;
    for catch in catches {
        if catch_types_match_thrown(thrown, &catch.class_names, context, values)? {
            matched = Some(catch);
            break;
        }
    }
    let Some(catch) = matched else {
        return Ok(EvalControl::Throw(thrown));
    };
    if let Some(var_name) = &catch.var_name {
        for replaced in set_scope_cell(
            context,
            scope,
            var_name.clone(),
            thrown,
            ScopeCellOwnership::Owned,
        )? {
            values.release(replaced)?;
        }
    } else {
        values.release(thrown)?;
    }
    execute_statements(&catch.body, context, scope, values)
}

/// Returns true when any type in one catch clause accepts the thrown object.
pub(in crate::interpreter) fn catch_types_match_thrown(
    thrown: RuntimeCellHandle,
    class_names: &[String],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for class_name in class_names {
        let class_name = class_name.trim_start_matches('\\');
        if class_name.eq_ignore_ascii_case("Throwable") {
            return Ok(true);
        }
        if let Some(matched) = dynamic_object_is_a(thrown, class_name, false, context, values)? {
            if matched {
                return Ok(true);
            }
            continue;
        }
        if values.object_is_a(thrown, class_name, false)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Registers an eval-declared class in the dynamic class table.
pub(in crate::interpreter) fn execute_class_decl_stmt(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let name = class.name().trim_start_matches('\\');
    if context.has_class(name)
        || context.has_interface(name)
        || context.has_trait(name)
        || values.class_exists(name)?
        || values.interface_exists(name)?
        || values.trait_exists(name)?
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let class = expand_eval_class_traits(class, context)?;
    let class = &class;
    validate_eval_class_modifiers(class, context)?;
    if let Some(parent) = class.parent() {
        let Some(parent_class) = context.class(parent) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        if parent_class.is_final() || context.class_is_a(parent, name, false) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    for interface in class.interfaces() {
        if !context.has_interface(interface) && !values.interface_exists(interface)? {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    if !class.is_abstract() {
        validate_concrete_class_requirements(class, context)?;
    }
    if context.define_class(class.clone()) {
        initialize_eval_declared_constants(
            class.name(),
            class.constants(),
            context,
            scope,
            values,
        )?;
        initialize_eval_static_properties(class, context, scope, values)
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Initializes class-like constant cells for a newly declared eval class-like.
fn initialize_eval_declared_constants(
    owner_name: &str,
    constants: &[EvalClassConstant],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for constant in constants {
        let value = eval_expr(constant.value(), context, scope, values)?;
        if let Some(replaced) = context.set_class_constant_cell(owner_name, constant.name(), value)
        {
            values.release(replaced)?;
        }
    }
    Ok(())
}

/// Initializes static property cells for a newly declared eval class.
fn initialize_eval_static_properties(
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
            eval_expr(default, context, scope, values)?
        } else {
            values.null()?
        };
        if let Some(replaced) = context.set_static_property(class.name(), property.name(), value) {
            values.release(replaced)?;
        }
    }
    Ok(())
}

/// Registers an eval-declared interface in the dynamic interface table.
pub(in crate::interpreter) fn execute_interface_decl_stmt(
    interface: &EvalInterface,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let name = interface.name().trim_start_matches('\\');
    if context.has_interface(name)
        || context.has_class(name)
        || values.interface_exists(name)?
        || values.class_exists(name)?
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    for parent in interface.parents() {
        if context
            .interface_parent_names(parent)
            .iter()
            .any(|ancestor| ancestor.eq_ignore_ascii_case(name))
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        if !context.has_interface(parent) && !values.interface_exists(parent)? {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    validate_eval_declared_constants(interface.constants())?;
    if context.define_interface(interface.clone()) {
        initialize_eval_declared_constants(
            interface.name(),
            interface.constants(),
            context,
            scope,
            values,
        )
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Registers an eval-declared trait in the dynamic trait table.
pub(in crate::interpreter) fn execute_trait_decl_stmt(
    trait_decl: &EvalTrait,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let name = trait_decl.name().trim_start_matches('\\');
    if context.has_trait(name)
        || context.has_class(name)
        || context.has_interface(name)
        || values.trait_exists(name)?
        || values.class_exists(name)?
        || values.interface_exists(name)?
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    validate_eval_declared_constants(trait_decl.constants())?;
    if context.define_trait(trait_decl.clone()) {
        initialize_eval_declared_constants(
            trait_decl.name(),
            trait_decl.constants(),
            context,
            scope,
            values,
        )
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Expands eval trait uses into the class metadata used by dynamic dispatch.
fn expand_eval_class_traits(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<EvalClass, EvalStatus> {
    if class.traits().is_empty() {
        return Ok(class.clone());
    }
    let class_method_names = class_method_name_set(class);
    let class_property_names = class_property_name_set(class);
    let class_constant_names = class_constant_name_set(class);
    let mut trait_method_names = std::collections::HashSet::new();
    let mut trait_property_names = std::collections::HashSet::new();
    let mut trait_constant_names = std::collections::HashSet::new();
    let mut constants = Vec::new();
    let mut properties = Vec::new();
    let mut methods = Vec::new();
    for trait_name in class.traits() {
        let Some(trait_decl) = context.trait_decl(trait_name) else {
            return Err(EvalStatus::RuntimeFatal);
        };
        append_eval_trait_constants(
            trait_decl,
            &class_constant_names,
            &mut trait_constant_names,
            &mut constants,
        )?;
        append_eval_trait_properties(
            trait_decl,
            &class_property_names,
            &mut trait_property_names,
            &mut properties,
        )?;
        append_eval_trait_methods(
            trait_decl,
            class.trait_adaptations(),
            &class_method_names,
            &mut trait_method_names,
            &mut methods,
        )?;
    }
    constants.extend(class.constants().iter().cloned());
    properties.extend(class.properties().iter().cloned());
    methods.extend(class.methods().iter().cloned());
    Ok(EvalClass::with_modifiers_traits_adaptations_and_constants(
        class.name().to_string(),
        class.is_abstract(),
        class.is_final(),
        class.parent().map(str::to_string),
        class.interfaces().to_vec(),
        class.traits().to_vec(),
        class.trait_adaptations().to_vec(),
        constants,
        properties,
        methods,
    ))
}

/// Returns case-insensitive method names declared directly by a pending class.
fn class_method_name_set(class: &EvalClass) -> std::collections::HashSet<String> {
    class
        .methods()
        .iter()
        .map(|method| method.name().to_ascii_lowercase())
        .collect()
}

/// Returns constant names declared directly by a pending class.
fn class_constant_name_set(class: &EvalClass) -> std::collections::HashSet<String> {
    class
        .constants()
        .iter()
        .map(|constant| constant.name().to_string())
        .collect()
}

/// Returns property names declared directly by a pending class.
fn class_property_name_set(class: &EvalClass) -> std::collections::HashSet<String> {
    class
        .properties()
        .iter()
        .map(|property| property.name().to_string())
        .collect()
}

/// Appends trait constants unless the class provides a same-name constant.
fn append_eval_trait_constants(
    trait_decl: &EvalTrait,
    class_constant_names: &std::collections::HashSet<String>,
    trait_constant_names: &mut std::collections::HashSet<String>,
    constants: &mut Vec<EvalClassConstant>,
) -> Result<(), EvalStatus> {
    for constant in trait_decl.constants() {
        if class_constant_names.contains(constant.name()) {
            continue;
        }
        if !trait_constant_names.insert(constant.name().to_string()) {
            return Err(EvalStatus::RuntimeFatal);
        }
        constants.push(constant.clone());
    }
    Ok(())
}

/// Appends trait properties unless the class provides a same-name property.
fn append_eval_trait_properties(
    trait_decl: &EvalTrait,
    class_property_names: &std::collections::HashSet<String>,
    trait_property_names: &mut std::collections::HashSet<String>,
    properties: &mut Vec<EvalClassProperty>,
) -> Result<(), EvalStatus> {
    for property in trait_decl.properties() {
        if class_property_names.contains(property.name()) {
            continue;
        }
        if !trait_property_names.insert(property.name().to_string()) {
            return Err(EvalStatus::RuntimeFatal);
        }
        properties.push(property.clone());
    }
    Ok(())
}

/// Appends trait methods unless the class provides a same-name method.
fn append_eval_trait_methods(
    trait_decl: &EvalTrait,
    trait_adaptations: &[EvalTraitAdaptation],
    class_method_names: &std::collections::HashSet<String>,
    trait_method_names: &mut std::collections::HashSet<String>,
    methods: &mut Vec<EvalClassMethod>,
) -> Result<(), EvalStatus> {
    for method in trait_decl.methods() {
        if trait_method_suppressed_by_insteadof(trait_decl.name(), method.name(), trait_adaptations)
        {
            continue;
        }
        let key = method.name().to_ascii_lowercase();
        if class_method_names.contains(&key) {
            continue;
        }
        let method =
            apply_trait_visibility_adaptations(trait_decl.name(), method, trait_adaptations);
        if !trait_method_names.insert(key) {
            return Err(EvalStatus::RuntimeFatal);
        }
        methods.push(method);
    }
    append_eval_trait_method_aliases(
        trait_decl,
        trait_adaptations,
        class_method_names,
        trait_method_names,
        methods,
    )
}

/// Appends trait method aliases declared with `as`.
fn append_eval_trait_method_aliases(
    trait_decl: &EvalTrait,
    trait_adaptations: &[EvalTraitAdaptation],
    class_method_names: &std::collections::HashSet<String>,
    trait_method_names: &mut std::collections::HashSet<String>,
    methods: &mut Vec<EvalClassMethod>,
) -> Result<(), EvalStatus> {
    for adaptation in trait_adaptations {
        let EvalTraitAdaptation::Alias {
            trait_name,
            method,
            alias: Some(alias),
            visibility,
        } = adaptation
        else {
            continue;
        };
        if !trait_adaptation_target_matches(
            trait_name.as_deref(),
            method,
            trait_decl.name(),
            method,
        ) {
            continue;
        }
        let Some(source_method) = trait_decl
            .methods()
            .iter()
            .find(|trait_method| trait_method.name().eq_ignore_ascii_case(method))
        else {
            if trait_name.is_some() {
                return Err(EvalStatus::RuntimeFatal);
            }
            continue;
        };
        let mut alias_method = source_method.renamed(alias.clone());
        if let Some(visibility) = visibility {
            alias_method = alias_method.with_visibility_override(*visibility);
        }
        let key = alias_method.name().to_ascii_lowercase();
        if class_method_names.contains(&key) || !trait_method_names.insert(key) {
            return Err(EvalStatus::RuntimeFatal);
        }
        methods.push(alias_method);
    }
    Ok(())
}

/// Returns whether an `insteadof` adaptation suppresses this trait method import.
fn trait_method_suppressed_by_insteadof(
    trait_name: &str,
    method_name: &str,
    trait_adaptations: &[EvalTraitAdaptation],
) -> bool {
    trait_adaptations.iter().any(|adaptation| {
        let EvalTraitAdaptation::InsteadOf {
            trait_name: selected_trait,
            method,
            instead_of,
        } = adaptation
        else {
            return false;
        };
        method.eq_ignore_ascii_case(method_name)
            && instead_of
                .iter()
                .any(|suppressed| same_eval_class_name(suppressed, trait_name))
            && !selected_trait
                .as_deref()
                .is_some_and(|selected| same_eval_class_name(selected, trait_name))
    })
}

/// Applies visibility-only `as` adaptations to an imported trait method.
fn apply_trait_visibility_adaptations(
    trait_name: &str,
    method: &EvalClassMethod,
    trait_adaptations: &[EvalTraitAdaptation],
) -> EvalClassMethod {
    let mut method = method.clone();
    for adaptation in trait_adaptations {
        let EvalTraitAdaptation::Alias {
            trait_name: target_trait,
            method: target_method,
            alias: None,
            visibility: Some(visibility),
        } = adaptation
        else {
            continue;
        };
        if trait_adaptation_target_matches(
            target_trait.as_deref(),
            target_method,
            trait_name,
            method.name(),
        ) {
            method = method.with_visibility_override(*visibility);
        }
    }
    method
}

/// Returns whether an adaptation target selects one trait method.
fn trait_adaptation_target_matches(
    target_trait: Option<&str>,
    target_method: &str,
    trait_name: &str,
    method_name: &str,
) -> bool {
    target_method.eq_ignore_ascii_case(method_name)
        && target_trait.map_or(true, |target_trait| {
            same_eval_class_name(target_trait, trait_name)
        })
}

/// Validates abstract/final modifiers on an eval-declared class and its methods.
fn validate_eval_class_modifiers(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if class.is_abstract() && class.is_final() {
        return Err(EvalStatus::RuntimeFatal);
    }
    validate_eval_declared_constants(class.constants())?;
    for method in class.methods() {
        if method.is_abstract() && method.is_final() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if method.is_abstract() && method.visibility() == EvalVisibility::Private {
            return Err(EvalStatus::RuntimeFatal);
        }
        if method.is_static() && method.name().eq_ignore_ascii_case("__construct") {
            return Err(EvalStatus::RuntimeFatal);
        }
        if method.is_abstract() && !class.is_abstract() {
            return Err(EvalStatus::RuntimeFatal);
        }
        validate_method_parent_override(class, method, context)?;
    }
    Ok(())
}

/// Validates constant declarations that can be checked before registration.
fn validate_eval_declared_constants(constants: &[EvalClassConstant]) -> Result<(), EvalStatus> {
    let mut names = std::collections::HashSet::new();
    for constant in constants {
        if !names.insert(constant.name().to_string()) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates one method declaration against inherited eval method metadata.
fn validate_method_parent_override(
    class: &EvalClass,
    method: &EvalClassMethod,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    let Some(parent) = class.parent() else {
        return Ok(());
    };
    let Some((_, parent_method)) = context.class_method(parent, method.name()) else {
        return Ok(());
    };
    if parent_method.visibility() == EvalVisibility::Private {
        return Ok(());
    }
    if parent_method.is_static() != method.is_static() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if method_visibility_rank(method.visibility())
        < method_visibility_rank(parent_method.visibility())
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    if parent_method.is_final() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if method.is_abstract() && !parent_method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Returns a comparable rank where larger means less restrictive visibility.
fn method_visibility_rank(visibility: EvalVisibility) -> u8 {
    match visibility {
        EvalVisibility::Private => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Public => 3,
    }
}

/// Validates that a concrete class has satisfied inherited abstract and interface requirements.
fn validate_concrete_class_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if !pending_class_abstract_method_requirements(class, context).is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    for interface in pending_class_interface_names(class, context) {
        if context.has_interface(&interface) {
            validate_class_implements_eval_interface(class, &interface, context)?;
        }
    }
    Ok(())
}

/// Returns inherited abstract methods that the pending class has not concretized.
fn pending_class_abstract_method_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Vec<EvalClassMethod> {
    let mut requirements = std::collections::HashMap::new();
    if let Some(parent) = class.parent() {
        collect_class_abstract_method_requirements(parent, context, &mut requirements);
    }
    apply_class_abstract_method_requirements(class, &mut requirements);
    requirements.into_values().collect()
}

/// Collects abstract method requirements from one declared eval class ancestry chain.
fn collect_class_abstract_method_requirements(
    class_name: &str,
    context: &ElephcEvalContext,
    requirements: &mut std::collections::HashMap<String, EvalClassMethod>,
) {
    let Some(class) = context.class(class_name) else {
        return;
    };
    if let Some(parent) = class.parent() {
        collect_class_abstract_method_requirements(parent, context, requirements);
    }
    apply_class_abstract_method_requirements(class, requirements);
}

/// Applies one class's methods to the open abstract-method requirement set.
fn apply_class_abstract_method_requirements(
    class: &EvalClass,
    requirements: &mut std::collections::HashMap<String, EvalClassMethod>,
) {
    for method in class.methods() {
        let key = method.name().to_ascii_lowercase();
        if method.is_abstract() {
            requirements.insert(key, method.clone());
        } else {
            requirements.remove(&key);
        }
    }
}

/// Returns interface names inherited or directly declared by a pending eval class.
fn pending_class_interface_names(class: &EvalClass, context: &ElephcEvalContext) -> Vec<String> {
    let mut interfaces = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(parent) = class.parent() {
        for interface in context.class_interface_names(parent) {
            push_pending_class_interface_name(&interface, &mut interfaces, &mut seen);
        }
    }
    for interface in class.interfaces() {
        push_pending_class_interface_tree(interface, context, &mut interfaces, &mut seen);
    }
    interfaces
}

/// Adds one interface and its eval-declared parent interfaces to a pending class list.
fn push_pending_class_interface_tree(
    interface: &str,
    context: &ElephcEvalContext,
    interfaces: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    push_pending_class_interface_name(interface, interfaces, seen);
    for parent in context.interface_parent_names(interface) {
        push_pending_class_interface_name(&parent, interfaces, seen);
    }
}

/// Adds one interface name once using PHP class-name case-insensitive matching.
fn push_pending_class_interface_name(
    interface: &str,
    interfaces: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    let interface = interface.trim_start_matches('\\');
    if seen.insert(interface.to_ascii_lowercase()) {
        interfaces.push(interface.to_string());
    }
}

/// Validates that one eval class provides methods required by one eval interface.
fn validate_class_implements_eval_interface(
    class: &EvalClass,
    interface_name: &str,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for requirement in context.interface_method_requirements(interface_name) {
        if !class_has_interface_method(class, &requirement, context) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Returns whether a class or its eval parents satisfy one interface method signature.
fn class_has_interface_method(
    class: &EvalClass,
    requirement: &EvalInterfaceMethod,
    context: &ElephcEvalContext,
) -> bool {
    if let Some(method) = class.method(requirement.name()) {
        return method.visibility() == EvalVisibility::Public
            && !method.is_static()
            && !method.is_abstract()
            && method.params().len() == requirement.params().len();
    }
    class
        .parent()
        .and_then(|parent| context.class_method(parent, requirement.name()))
        .is_some_and(|(_, method)| {
            method.visibility() == EvalVisibility::Public
                && !method.is_static()
                && !method.is_abstract()
                && method.params().len() == requirement.params().len()
        })
}

/// Reads one object property while enforcing eval-declared member visibility.
pub(in crate::interpreter) fn eval_property_get_result(
    object: RuntimeCellHandle,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return values.property_get(object, property_name);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        return values.property_get(object, property_name);
    };
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(class.name(), property_name, context)
    {
        validate_eval_member_access(&declaring_class, property.visibility(), context)?;
    }
    values.property_get(object, property_name)
}

/// Writes one object property while enforcing eval-declared member visibility.
pub(in crate::interpreter) fn eval_property_set_result(
    object: RuntimeCellHandle,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return values.property_set(object, property_name, value);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        return values.property_set(object, property_name, value);
    };
    if let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(class.name(), property_name, context)
    {
        validate_eval_member_access(&declaring_class, property.visibility(), context)?;
    }
    values.property_set(object, property_name, value)
}

/// Resolves the property metadata visible from the current class scope, if any.
fn eval_dynamic_property_for_access(
    object_class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassProperty)> {
    if let Some(current_class) = context.current_class_scope() {
        if context.class_is_a(object_class_name, current_class, false) {
            if let Some((declaring_class, property)) =
                context.class_own_property(current_class, property_name)
            {
                if property.visibility() == EvalVisibility::Private {
                    return Some((declaring_class, property));
                }
            }
        }
    }
    context.class_property(object_class_name, property_name)
}

/// Reads one eval-declared static property after resolving the class-like receiver.
pub(in crate::interpreter) fn eval_static_property_get_result(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    _values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name = resolve_eval_static_class_name(class_name, context)?;
    let (declaring_class, property) = context
        .class_property(&class_name, property_name)
        .ok_or(EvalStatus::RuntimeFatal)?;
    if !property.is_static() {
        return Err(EvalStatus::RuntimeFatal);
    }
    validate_eval_member_access(&declaring_class, property.visibility(), context)?;
    context
        .static_property(&declaring_class, property.name())
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Reads one eval-declared class constant after resolving the class-like receiver.
pub(in crate::interpreter) fn eval_class_constant_fetch_result(
    class_name: &str,
    constant_name: &str,
    context: &mut ElephcEvalContext,
    _values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name = resolve_eval_static_class_like_name(class_name, context)?;
    let (declaring_class, constant) = context
        .class_constant(&class_name, constant_name)
        .ok_or(EvalStatus::RuntimeFatal)?;
    validate_eval_member_access(&declaring_class, constant.visibility(), context)?;
    context
        .class_constant_cell(&declaring_class, constant.name())
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Returns the PHP class-name literal for `ClassName::class`-style eval expressions.
pub(in crate::interpreter) fn eval_class_name_fetch_result(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name = resolve_eval_class_name_literal(class_name, context)?;
    values.string(&class_name)
}

/// Writes one eval-declared static property after resolving the class-like receiver.
pub(in crate::interpreter) fn eval_static_property_set_result(
    class_name: &str,
    property_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let class_name = resolve_eval_static_class_name(class_name, context)?;
    let (declaring_class, property) = context
        .class_property(&class_name, property_name)
        .ok_or(EvalStatus::RuntimeFatal)?;
    if !property.is_static() {
        return Err(EvalStatus::RuntimeFatal);
    }
    validate_eval_member_access(&declaring_class, property.visibility(), context)?;
    if let Some(replaced) = context.set_static_property(&declaring_class, property.name(), value) {
        values.release(replaced)?;
    }
    Ok(())
}

/// Dispatches a static method call to an eval-declared static method.
pub(in crate::interpreter) fn eval_static_method_call_result(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name = resolve_eval_static_class_name(class_name, context)?;
    let (declaring_class, method) =
        eval_dynamic_static_method_for_call(&class_name, method_name, context)
            .ok_or(EvalStatus::RuntimeFatal)?;
    if !method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    validate_eval_member_access(&declaring_class, method.visibility(), context)?;
    eval_dynamic_static_method_with_values(
        &declaring_class,
        &class_name,
        &method,
        evaluated_args,
        context,
        values,
    )
}

/// Resolves a static method using private-method scope rules.
fn eval_dynamic_static_method_for_call(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassMethod)> {
    if let Some(current_class) = context.current_class_scope() {
        if eval_classes_are_related(current_class, class_name, context) {
            if let Some((declaring_class, method)) =
                context.class_own_method(current_class, method_name)
            {
                if method.visibility() == EvalVisibility::Private {
                    return Some((declaring_class, method));
                }
            }
        }
    }
    context.class_method(class_name, method_name)
}

/// Resolves `self`, `parent`, and `static` for eval static member access.
fn resolve_eval_static_class_name(
    class_name: &str,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" => context
            .current_class_scope()
            .map(str::to_string)
            .ok_or(EvalStatus::RuntimeFatal),
        "static" => context
            .current_called_class_scope()
            .or_else(|| context.current_class_scope())
            .map(str::to_string)
            .ok_or(EvalStatus::RuntimeFatal),
        "parent" => {
            let current = context
                .current_class_scope()
                .ok_or(EvalStatus::RuntimeFatal)?;
            context
                .class(current)
                .and_then(EvalClass::parent)
                .map(str::to_string)
                .ok_or(EvalStatus::RuntimeFatal)
        }
        _ => context
            .resolve_class_name(class_name)
            .or_else(|| {
                context
                    .has_class(class_name)
                    .then(|| class_name.to_string())
            })
            .ok_or(EvalStatus::RuntimeFatal),
    }
}

/// Resolves `self`, `parent`, `static`, and named class-like receivers for constant access.
fn resolve_eval_static_class_like_name(
    class_name: &str,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" | "parent" | "static" => resolve_eval_static_class_name(class_name, context),
        _ => context
            .resolve_class_like_name(class_name)
            .ok_or(EvalStatus::RuntimeFatal),
    }
}

/// Resolves class-name literal receivers without requiring named classes to exist.
fn resolve_eval_class_name_literal(
    class_name: &str,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" | "parent" | "static" => resolve_eval_static_class_name(class_name, context),
        _ => Ok(context
            .resolve_class_like_name(class_name)
            .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string())),
    }
}

/// Creates a backing object for an eval-declared class and runs its constructor.
pub(in crate::interpreter) fn eval_dynamic_class_new_object(
    class: &EvalClass,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if class.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let object = values.new_object("stdClass")?;
    let identity = values.object_identity(object)?;
    context.register_dynamic_object(identity, class.name());
    let mut class_chain = context.class_chain(class.name());
    if class_chain.is_empty() {
        class_chain.push(class.clone());
    }
    for class in &class_chain {
        for property in class
            .properties()
            .iter()
            .filter(|property| !property.is_static())
        {
            let value = if let Some(default) = property.default() {
                eval_expr(default, context, caller_scope, values)?
            } else {
                values.null()?
            };
            values.property_set(object, property.name(), value)?;
        }
    }
    if let Some((constructor_class, constructor)) =
        context.class_method(class.name(), "__construct")
    {
        validate_eval_member_access(&constructor_class, constructor.visibility(), context)?;
        eval_dynamic_method_with_values(
            &constructor_class,
            class.name(),
            &constructor,
            object,
            evaluated_args,
            context,
            values,
        )?;
    } else if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(object)
}

/// Dispatches a method call to an eval-declared class method or to the runtime hook.
pub(in crate::interpreter) fn eval_method_call_result(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_method_call_result_with_evaluated_args(
        object,
        method_name,
        positional_args(evaluated_args),
        context,
        values,
    )
}

/// Dispatches an object method call while preserving named-argument metadata for eval methods.
pub(in crate::interpreter) fn eval_method_call_result_with_evaluated_args(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        let evaluated_args = positional_evaluated_arg_values(evaluated_args)?;
        return values.method_call(object, method_name, evaluated_args);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let evaluated_args = positional_evaluated_arg_values(evaluated_args)?;
        return values.method_call(object, method_name, evaluated_args);
    };
    let called_class_name = class.name().to_string();
    let (class_name, method) =
        eval_dynamic_method_for_call(&called_class_name, method_name, context)
            .ok_or(EvalStatus::RuntimeFatal)?;
    validate_eval_member_access(&class_name, method.visibility(), context)?;
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_dynamic_method_with_values(
        &class_name,
        &called_class_name,
        &method,
        object,
        evaluated_args,
        context,
        values,
    )
}

/// Resolves the method metadata visible from the current class scope.
fn eval_dynamic_method_for_call(
    object_class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassMethod)> {
    if let Some(current_class) = context.current_class_scope() {
        if context.class_is_a(object_class_name, current_class, false) {
            if let Some((declaring_class, method)) =
                context.class_own_method(current_class, method_name)
            {
                if method.visibility() == EvalVisibility::Private {
                    return Some((declaring_class, method));
                }
            }
        }
    }
    context.class_method(object_class_name, method_name)
}

/// Returns whether the current eval class scope can access one declared member.
fn validate_eval_member_access(
    declaring_class: &str,
    visibility: EvalVisibility,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if visibility == EvalVisibility::Public {
        return Ok(());
    }
    let Some(current_class) = context.current_class_scope() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    match visibility {
        EvalVisibility::Public => Ok(()),
        EvalVisibility::Private => same_eval_class_name(current_class, declaring_class)
            .then_some(())
            .ok_or(EvalStatus::RuntimeFatal),
        EvalVisibility::Protected => {
            eval_classes_are_related(current_class, declaring_class, context)
                .then_some(())
                .ok_or(EvalStatus::RuntimeFatal)
        }
    }
}

/// Returns true when two PHP class names refer to the same eval class.
fn same_eval_class_name(left: &str, right: &str) -> bool {
    left.trim_start_matches('\\')
        .eq_ignore_ascii_case(right.trim_start_matches('\\'))
}

/// Returns true when two eval classes are in the same inheritance family.
fn eval_classes_are_related(left: &str, right: &str, context: &ElephcEvalContext) -> bool {
    same_eval_class_name(left, right)
        || context.class_is_a(left, right, false)
        || context.class_is_a(right, left, false)
}

/// Executes one eval-declared class method with `$this` bound in method scope.
pub(in crate::interpreter) fn eval_dynamic_method_with_values(
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    object: RuntimeCellHandle,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = bind_evaluated_function_args(method.params(), evaluated_args)?;
    let mut method_scope = ElephcEvalScope::new();
    method_scope.set("this", object, ScopeCellOwnership::Borrowed);
    for (name, value) in method.params().iter().zip(evaluated_args) {
        method_scope.set(name.clone(), value, ScopeCellOwnership::Borrowed);
    }
    let qualified_method_name =
        format!("{}::{}", class_name.trim_start_matches('\\'), method.name());
    let static_names = static_var_names(method.body());
    context.push_function(qualified_method_name.clone());
    context.push_class_scope(class_name.to_string());
    context.push_called_class_scope(called_class_name.to_string());
    let result = execute_statements(method.body(), context, &mut method_scope, values);
    let persist_result = persist_static_locals(
        context,
        &qualified_method_name,
        &static_names,
        &method_scope,
        values,
    );
    context.pop_called_class_scope();
    context.pop_class_scope();
    context.pop_function();
    persist_result?;
    match result? {
        EvalControl::None => values.null(),
        EvalControl::Return(result) => Ok(result),
        EvalControl::Throw(result) => {
            context.set_pending_throw(result);
            Err(EvalStatus::UncaughtThrowable)
        }
        EvalControl::Break | EvalControl::Continue => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Executes one eval-declared static class method without binding `$this`.
pub(in crate::interpreter) fn eval_dynamic_static_method_with_values(
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = bind_evaluated_function_args(method.params(), evaluated_args)?;
    let mut method_scope = ElephcEvalScope::new();
    for (name, value) in method.params().iter().zip(evaluated_args) {
        method_scope.set(name.clone(), value, ScopeCellOwnership::Borrowed);
    }
    let qualified_method_name =
        format!("{}::{}", class_name.trim_start_matches('\\'), method.name());
    let static_names = static_var_names(method.body());
    context.push_function(qualified_method_name.clone());
    context.push_class_scope(class_name.to_string());
    context.push_called_class_scope(called_class_name.to_string());
    let result = execute_statements(method.body(), context, &mut method_scope, values);
    let persist_result = persist_static_locals(
        context,
        &qualified_method_name,
        &static_names,
        &method_scope,
        values,
    );
    context.pop_called_class_scope();
    context.pop_class_scope();
    context.pop_function();
    persist_result?;
    match result? {
        EvalControl::None => values.null(),
        EvalControl::Return(result) => Ok(result),
        EvalControl::Throw(result) => {
            context.set_pending_throw(result);
            Err(EvalStatus::UncaughtThrowable)
        }
        EvalControl::Break | EvalControl::Continue => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Wraps positional method arguments into the shared dynamic-call binding shape.
pub(in crate::interpreter) fn positional_args(
    args: Vec<RuntimeCellHandle>,
) -> Vec<EvaluatedCallArg> {
    args.into_iter()
        .map(|value| EvaluatedCallArg { name: None, value })
        .collect()
}

/// Extracts positional runtime values and rejects named args before runtime method dispatch.
pub(in crate::interpreter) fn positional_evaluated_arg_values(
    args: Vec<EvaluatedCallArg>,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if args.iter().any(|arg| arg.name.is_some()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(args.into_iter().map(|arg| arg.value).collect())
}

/// Executes a PHP `static $name = expr;` declaration in the current eval scope.
pub(in crate::interpreter) fn execute_static_var_stmt(
    name: &str,
    init: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(function_name) = context.current_function().map(str::to_string) else {
        let value = eval_expr(init, context, scope, values)?;
        if let Some(replaced) = scope.set(name.to_string(), value, ScopeCellOwnership::Owned) {
            values.release(replaced)?;
        }
        return Ok(());
    };
    if scope.contains_visible(name) {
        return Ok(());
    }
    let value = if let Some(value) = context.static_local(&function_name, name) {
        value
    } else {
        let value = eval_expr(init, context, scope, values)?;
        let _ = context.set_static_local(function_name.clone(), name.to_string(), value);
        value
    };
    if let Some(replaced) = scope.set(name.to_string(), value, ScopeCellOwnership::Borrowed) {
        values.release(replaced)?;
    }
    Ok(())
}

/// Executes a PHP switch with loose case matching, default fallback, and fallthrough.
pub(in crate::interpreter) fn execute_switch_stmt(
    expr: &EvalExpr,
    cases: &[EvalSwitchCase],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let subject = eval_expr(expr, context, scope, values)?;
    let mut default_index = None;
    let mut matched_index = None;
    for (index, case) in cases.iter().enumerate() {
        let Some(condition) = &case.condition else {
            if default_index.is_none() {
                default_index = Some(index);
            }
            continue;
        };
        let condition = eval_expr(condition, context, scope, values)?;
        let matches = values.compare(EvalBinOp::LooseEq, subject, condition)?;
        if values.truthy(matches)? {
            matched_index = Some(index);
            break;
        }
    }
    let Some(start_index) = matched_index.or(default_index) else {
        return Ok(EvalControl::None);
    };
    for case in &cases[start_index..] {
        match execute_statements(&case.body, context, scope, values)? {
            EvalControl::None => {}
            EvalControl::Break | EvalControl::Continue => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `do/while` loop, evaluating the condition after every body run.
pub(in crate::interpreter) fn execute_do_while_stmt(
    body: &[EvalStmt],
    condition: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    loop {
        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
        let condition = eval_expr(condition, context, scope, values)?;
        if !values.truthy(condition)? {
            break;
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `for` loop while preserving update-on-continue semantics.
pub(in crate::interpreter) fn execute_for_stmt(
    init: &[EvalStmt],
    condition: Option<&EvalExpr>,
    update: &[EvalStmt],
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    match execute_statements(init, context, scope, values)? {
        EvalControl::None | EvalControl::Continue => {}
        EvalControl::Break => return Ok(EvalControl::None),
        EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
        EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
    }
    loop {
        if let Some(condition) = condition {
            let condition = eval_expr(condition, context, scope, values)?;
            if !values.truthy(condition)? {
                break;
            }
        }
        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
        match execute_statements(update, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `foreach` loop over eval array values.
pub(in crate::interpreter) fn execute_foreach_stmt(
    array: &EvalExpr,
    key_name: Option<&str>,
    value_name: &str,
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let array = eval_expr(array, context, scope, values)?;
    let len = values.array_len(array)?;
    for index in 0..len {
        let key = values.array_iter_key(array, index)?;
        let value = values.array_get(array, key)?;
        if let Some(key_name) = key_name {
            for replaced in set_scope_cell(
                context,
                scope,
                key_name.to_string(),
                key,
                ScopeCellOwnership::Owned,
            )? {
                values.release(replaced)?;
            }
        } else {
            values.release(key)?;
        }
        for replaced in set_scope_cell(
            context,
            scope,
            value_name.to_string(),
            value,
            ScopeCellOwnership::Owned,
        )? {
            values.release(replaced)?;
        }
        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Throw(result) => return Ok(EvalControl::Throw(result)),
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
    }
    Ok(EvalControl::None)
}

/// Returns PHP's next automatic integer key for `$array[]` append writes.
pub(in crate::interpreter) fn eval_array_append_key(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut next_key = None;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? != EVAL_TAG_INT {
            continue;
        }
        let one = values.int(1)?;
        let candidate = values.add(key, one)?;
        let replace = if let Some(current) = next_key {
            let is_greater = values.compare(EvalBinOp::Gt, candidate, current)?;
            values.truthy(is_greater)?
        } else {
            true
        };
        if replace {
            next_key = Some(candidate);
        }
    }
    next_key.map_or_else(|| values.int(0), Ok)
}
