use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver, Stmt, StmtKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

struct StaticPropertyAssignmentTarget {
    class_name: String,
    declaring_class: String,
    property_has_declared_type: bool,
    prop_ty: PhpType,
}

impl Checker {
    fn null_coalesce_assignment_default<'a>(name: &str, value: &'a Expr) -> Option<&'a Expr> {
        if let ExprKind::NullCoalesce {
            value: current,
            default,
        } = &value.kind
        {
            if matches!(&current.kind, ExprKind::Variable(current_name) if current_name == name) {
                return Some(default);
            }
        }
        None
    }

    fn null_coalesce_assignment_type(
        &self,
        name: &str,
        existing: &PhpType,
        default_ty: &PhpType,
        default: &Expr,
        span: crate::span::Span,
    ) -> Result<PhpType, CompileError> {
        if *existing == PhpType::Void {
            return Ok(default_ty.clone());
        }
        if *existing == PhpType::Mixed {
            return Ok(PhpType::Mixed);
        }
        if matches!(existing, PhpType::Union(_)) {
            if *default_ty == PhpType::Void || self.type_accepts(existing, default_ty) {
                return Ok(existing.clone());
            }
            return Err(CompileError::new(
                span,
                &format!(
                    "Type error: null coalescing assignment for ${} must keep {}, got {}",
                    name, existing, default_ty
                ),
            ));
        }
        if existing == default_ty || matches!(default.kind, ExprKind::Null) {
            return Ok(existing.clone());
        }
        Err(CompileError::new(
            span,
            &format!(
                "Type error: null coalescing assignment for ${} must keep {}, got {}",
                name, existing, default_ty
            ),
        ))
    }

    fn resolve_static_property_assignment_target(
        &self,
        receiver: &StaticReceiver,
        property: &str,
        span: crate::span::Span,
    ) -> Result<StaticPropertyAssignmentTarget, CompileError> {
        let class_name = match receiver {
            StaticReceiver::Named(class_name) => class_name.as_str().to_string(),
            StaticReceiver::Self_ => self.current_class.as_ref().cloned().ok_or_else(|| {
                CompileError::new(span, "Cannot use self:: outside class method scope")
            })?,
            StaticReceiver::Static => self.current_class.as_ref().cloned().ok_or_else(|| {
                CompileError::new(span, "Cannot use static:: outside class method scope")
            })?,
            StaticReceiver::Parent => {
                let current_class = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(span, "Cannot use parent:: outside class method scope")
                })?;
                let current_info = self.classes.get(current_class).ok_or_else(|| {
                    CompileError::new(span, &format!("Undefined class: {}", current_class))
                })?;
                current_info.parent.as_ref().cloned().ok_or_else(|| {
                    CompileError::new(
                        span,
                        &format!("Class {} has no parent class", current_class),
                    )
                })?
            }
        };

        let class_info = self.classes.get(&class_name).ok_or_else(|| {
            CompileError::new(span, &format!("Undefined class: {}", class_name))
        })?;
        if !class_info
            .static_properties
            .iter()
            .any(|(name, _)| name == property)
        {
            return Err(CompileError::new(
                span,
                &format!("Undefined static property: {}::{}", class_name, property),
            ));
        }
        if let Some(visibility) = class_info.static_property_visibilities.get(property) {
            let declaring_class = class_info
                .static_property_declaring_classes
                .get(property)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            if !self.can_access_member(declaring_class, visibility) {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Cannot access {} static property: {}::{}",
                        Self::visibility_label(visibility),
                        class_name,
                        property
                    ),
                ));
            }
        }
        let declaring_class = class_info
            .static_property_declaring_classes
            .get(property)
            .cloned()
            .unwrap_or_else(|| class_name.clone());
        let property_has_declared_type = class_info.declared_static_properties.contains(property);
        let prop_ty = class_info
            .static_properties
            .iter()
            .find(|(name, _)| name == property)
            .map(|(_, ty)| ty.clone())
            .unwrap_or(PhpType::Int);

        Ok(StaticPropertyAssignmentTarget {
            class_name,
            declaring_class,
            property_has_declared_type,
            prop_ty,
        })
    }

    fn update_static_property_type(
        &mut self,
        property: &str,
        declaring_class: &str,
        updated_ty: PhpType,
    ) {
        for class_info in self.classes.values_mut() {
            if class_info
                .static_property_declaring_classes
                .get(property)
                .map(String::as_str)
                != Some(declaring_class)
            {
                continue;
            }
            if let Some(prop) = class_info
                .static_properties
                .iter_mut()
                .find(|(name, _)| name == property)
            {
                prop.1 = updated_ty.clone();
            }
        }
    }

    fn refine_static_property_assignment_type(
        &mut self,
        property: &str,
        declaring_class: &str,
        val_ty: &PhpType,
    ) {
        for class_info in self.classes.values_mut() {
            if class_info
                .static_property_declaring_classes
                .get(property)
                .map(String::as_str)
                != Some(declaring_class)
            {
                continue;
            }
            if let Some(prop) = class_info
                .static_properties
                .iter_mut()
                .find(|(name, _)| name == property)
            {
                if matches!(prop.1, PhpType::Int | PhpType::Void) && prop.1 != *val_ty {
                    prop.1 = val_ty.clone();
                } else {
                    let refined_ty = Self::specialize_generic_array_hint(&prop.1, val_ty);
                    if refined_ty != prop.1 {
                        prop.1 = refined_ty;
                    }
                }
            }
        }
    }

    pub(crate) fn check_assignment_like_stmt(
        &mut self,
        stmt: &Stmt,
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                let null_coalesce_default =
                    Self::null_coalesce_assignment_default(name, value);
                let ty = if let Some(default) = null_coalesce_default {
                    if let Some(existing) = env.get(name).cloned() {
                        let default_ty = self.infer_type(default, env)?;
                        self.null_coalesce_assignment_type(
                            name,
                            &existing,
                            &default_ty,
                            default,
                            stmt.span,
                        )?
                    } else {
                        self.infer_type(value, env)?
                    }
                } else {
                    self.infer_type(value, env)?
                };
                let callable_source = if let Some(default) = null_coalesce_default {
                    if matches!(env.get(name), Some(existing) if *existing == PhpType::Void) {
                        default
                    } else {
                        value
                    }
                } else {
                    value
                };
                if ty == PhpType::Callable {
                    if let Some(sig) = self.resolve_expr_callable_sig(callable_source, env)? {
                        self.closure_return_types
                            .insert(name.clone(), sig.return_type.clone());
                        self.callable_sigs.insert(name.clone(), sig);
                        if let ExprKind::FirstClassCallable(target) = &callable_source.kind {
                            self.first_class_callable_targets
                                .insert(name.clone(), target.clone());
                        } else if let ExprKind::Variable(src_name) = &callable_source.kind {
                            if let Some(target) =
                                self.first_class_callable_targets.get(src_name).cloned()
                            {
                                self.first_class_callable_targets
                                    .insert(name.clone(), target);
                            } else {
                                self.first_class_callable_targets.remove(name);
                            }
                        } else {
                            self.first_class_callable_targets.remove(name);
                        }
                    } else {
                        self.closure_return_types.remove(name);
                        self.callable_sigs.remove(name);
                        self.first_class_callable_targets.remove(name);
                    }
                } else {
                    self.closure_return_types.remove(name);
                    self.callable_sigs.remove(name);
                    self.first_class_callable_targets.remove(name);
                }
                if let Some(existing) = env.get(name) {
                    let merged_ty = self.merged_assignment_type(existing, &ty);
                    if merged_ty.is_none() {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!(
                                "Type error: cannot reassign ${} from {} to {}",
                                name, existing, ty
                            ),
                        ));
                    }
                    if let Some(merged_ty) = merged_ty {
                        if &merged_ty != existing {
                            env.insert(name.clone(), merged_ty);
                        }
                    }
                } else {
                    env.insert(name.clone(), ty);
                }
                Ok(())
            }
            StmtKind::ArrayAssign {
                array,
                index,
                value,
            } => {
                let arr_ty = env.get(array).cloned().ok_or_else(|| {
                    CompileError::new(stmt.span, &format!("Undefined variable: ${}", array))
                })?;
                self.infer_type(index, env)?;
                let val_ty = self.infer_type(value, env)?;
                if arr_ty == PhpType::Str {
                    return Err(CompileError::new(
                        stmt.span,
                        "String offset assignment is not supported",
                    ));
                }
                if let PhpType::Array(elem_ty) = &arr_ty {
                    if **elem_ty != val_ty {
                        let merged_ty = self
                            .merge_array_element_type(elem_ty, &val_ty)
                            .unwrap_or(val_ty);
                        env.insert(array.clone(), PhpType::Array(Box::new(merged_ty)));
                    }
                } else if let PhpType::AssocArray {
                    key,
                    value: existing_value,
                } = &arr_ty
                {
                    let merged_value = if **existing_value == val_ty {
                        *existing_value.clone()
                    } else {
                        PhpType::Mixed
                    };
                    env.insert(
                        array.clone(),
                        PhpType::AssocArray {
                            key: key.clone(),
                            value: Box::new(merged_value),
                        },
                    );
                } else if let PhpType::Buffer(elem_ty) = &arr_ty {
                    if !matches!(self.infer_type(index, env)?, PhpType::Int) {
                        return Err(CompileError::new(stmt.span, "Buffer index must be integer"));
                    }
                    match elem_ty.as_ref() {
                        PhpType::Packed(_) => {
                            return Err(CompileError::new(
                                stmt.span,
                                "Assign packed buffer elements through field access like $buf[$i]->field",
                            ))
                        }
                        inner if inner != &val_ty => {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!(
                                    "Buffer element type mismatch: expected {:?}, got {:?}",
                                    inner, val_ty
                                ),
                            ));
                        }
                        _ => {}
                    }
                }
                Ok(())
            }
            StmtKind::ArrayPush { array, value } => {
                let arr_ty = env.get(array).cloned().ok_or_else(|| {
                    CompileError::new(stmt.span, &format!("Undefined variable: ${}", array))
                })?;
                let val_ty = self.infer_type(value, env)?;
                if let PhpType::Array(elem_ty) = &arr_ty {
                    if **elem_ty != val_ty {
                        let merged_ty = self
                            .merge_array_element_type(elem_ty, &val_ty)
                            .unwrap_or(val_ty);
                        env.insert(array.clone(), PhpType::Array(Box::new(merged_ty)));
                    }
                } else if matches!(arr_ty, PhpType::Buffer(_)) {
                    return Err(CompileError::new(
                        stmt.span,
                        "buffer<T> does not support push; allocate with buffer_new<T>(len)",
                    ));
                }
                Ok(())
            }
            StmtKind::TypedAssign {
                type_expr,
                name,
                value,
            } => {
                let declared_ty = self.resolve_type_expr(type_expr, stmt.span)?;
                let value_ty = self.infer_type(value, env)?;
                if !self.type_accepts(&declared_ty, &value_ty) {
                    return Err(CompileError::new(
                        stmt.span,
                        &format!(
                            "Type error: cannot initialize ${} as {} with {}",
                            name, declared_ty, value_ty
                        ),
                    ));
                }
                env.insert(name.clone(), declared_ty);
                Ok(())
            }
            StmtKind::ConstDecl { name, value } => {
                let ty = self.infer_type(value, env)?;
                self.constants.entry(name.clone()).or_insert(ty);
                Ok(())
            }
            StmtKind::ListUnpack { vars, value } => {
                let arr_ty = self.infer_type(value, env)?;
                match &arr_ty {
                    PhpType::Array(elem_ty) => {
                        for var in vars {
                            env.insert(var.clone(), *elem_ty.clone());
                        }
                    }
                    _ => {
                        return Err(CompileError::new(
                            stmt.span,
                            "List unpacking requires an array on the right-hand side",
                        ));
                    }
                }
                Ok(())
            }
            StmtKind::Global { vars } => {
                for var in vars {
                    self.active_globals.insert(var.clone());
                    if !env.contains_key(var) {
                        if let Some(global_ty) = self.top_level_env.get(var) {
                            env.insert(var.clone(), global_ty.clone());
                        } else {
                            env.insert(var.clone(), PhpType::Int);
                        }
                    }
                }
                Ok(())
            }
            StmtKind::StaticVar { name, init } => {
                let ty = self.infer_type(init, env)?;
                self.active_statics.insert(name.clone());
                env.insert(name.clone(), ty);
                Ok(())
            }
            StmtKind::StaticPropertyAssign {
                receiver,
                property,
                value,
            } => {
                let val_ty = self.infer_type(value, env)?;
                let target =
                    self.resolve_static_property_assignment_target(receiver, property, stmt.span)?;

                if target.property_has_declared_type {
                    self.require_compatible_arg_type(
                        &target.prop_ty,
                        &val_ty,
                        stmt.span,
                        &format!("Static property {}::${}", target.class_name, property),
                    )?;
                }

                if !target.property_has_declared_type {
                    self.refine_static_property_assignment_type(
                        property,
                        &target.declaring_class,
                        &val_ty,
                    );
                }
                Ok(())
            }
            StmtKind::StaticPropertyArrayPush {
                receiver,
                property,
                value,
            } => {
                let val_ty = self.infer_type(value, env)?;
                let target =
                    self.resolve_static_property_assignment_target(receiver, property, stmt.span)?;
                let updated_prop_ty = match target.prop_ty {
                    PhpType::Array(elem_ty) => {
                        if target.property_has_declared_type {
                            self.require_compatible_arg_type(
                                elem_ty.as_ref(),
                                &val_ty,
                                stmt.span,
                                &format!(
                                    "Static property {}::${}[]",
                                    target.class_name, property
                                ),
                            )?;
                            PhpType::Array(elem_ty)
                        } else if *elem_ty == val_ty {
                            PhpType::Array(elem_ty)
                        } else {
                            let merged_ty = self
                                .merge_array_element_type(&elem_ty, &val_ty)
                                .unwrap_or(val_ty.clone());
                            PhpType::Array(Box::new(merged_ty))
                        }
                    }
                    PhpType::Int | PhpType::Void if !target.property_has_declared_type => {
                        PhpType::Array(Box::new(val_ty.clone()))
                    }
                    PhpType::Buffer(_) => {
                        return Err(CompileError::new(
                            stmt.span,
                            "buffer<T> does not support push; allocate with buffer_new<T>(len)",
                        ))
                    }
                    other => {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!("Array push requires an array static property, got {}", other),
                        ))
                    }
                };

                if !target.property_has_declared_type {
                    self.update_static_property_type(
                        property,
                        &target.declaring_class,
                        updated_prop_ty,
                    );
                }
                Ok(())
            }
            StmtKind::StaticPropertyArrayAssign {
                receiver,
                property,
                index,
                value,
            } => {
                let idx_ty = self.infer_type(index, env)?;
                let val_ty = self.infer_type(value, env)?;
                let target =
                    self.resolve_static_property_assignment_target(receiver, property, stmt.span)?;
                if idx_ty != PhpType::Int {
                    return Err(CompileError::new(stmt.span, "Array index must be integer"));
                }

                let updated_prop_ty = match target.prop_ty {
                    PhpType::Array(elem_ty) => {
                        if target.property_has_declared_type {
                            self.require_compatible_arg_type(
                                elem_ty.as_ref(),
                                &val_ty,
                                stmt.span,
                                &format!(
                                    "Static property {}::${}[]",
                                    target.class_name, property
                                ),
                            )?;
                            PhpType::Array(elem_ty)
                        } else if *elem_ty == val_ty {
                            PhpType::Array(elem_ty)
                        } else {
                            let merged_ty = self
                                .merge_array_element_type(&elem_ty, &val_ty)
                                .unwrap_or(val_ty.clone());
                            PhpType::Array(Box::new(merged_ty))
                        }
                    }
                    PhpType::Int | PhpType::Void if !target.property_has_declared_type => {
                        PhpType::Array(Box::new(val_ty.clone()))
                    }
                    other => {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!(
                                "Array index assignment requires an array static property, got {}",
                                other
                            ),
                        ))
                    }
                };

                if !target.property_has_declared_type {
                    self.update_static_property_type(
                        property,
                        &target.declaring_class,
                        updated_prop_ty,
                    );
                }
                Ok(())
            }
            StmtKind::PropertyAssign {
                object,
                property,
                value,
            } => {
                let obj_ty = self.infer_type(object, env)?;
                let val_ty = self.infer_type(value, env)?;
                if let PhpType::Object(class_name) = &obj_ty {
                    if let Some(class_info) = self.classes.get(class_name) {
                        if !class_info.properties.iter().any(|(n, _)| n == property) {
                            if class_info.methods.contains_key("__set") {
                                return Ok(());
                            }
                            return Err(CompileError::new(
                                stmt.span,
                                &format!("Undefined property: {}::{}", class_name, property),
                            ));
                        }
                        if let Some(visibility) = class_info.property_visibilities.get(property) {
                            let declaring_class = class_info
                                .property_declaring_classes
                                .get(property)
                                .map(String::as_str)
                                .unwrap_or(class_name);
                            if !self.can_access_member(declaring_class, visibility) {
                                return Err(CompileError::new(
                                    stmt.span,
                                    &format!(
                                        "Cannot access {} property: {}::{}",
                                        Self::visibility_label(visibility),
                                        class_name,
                                        property
                                    ),
                                ));
                            }
                        }
                        if class_info.readonly_properties.contains(property)
                            && !(self.current_class.as_deref()
                                == class_info
                                    .property_declaring_classes
                                    .get(property)
                                    .map(String::as_str)
                                && self.current_method.as_deref() == Some("__construct"))
                        {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!(
                                    "Cannot assign to readonly property outside constructor: {}::{}",
                                    class_name, property
                                ),
                            ));
                        }
                        if class_info.declared_properties.contains(property) {
                            let expected_ty = class_info
                                .properties
                                .iter()
                                .find(|(n, _)| n == property)
                                .map(|(_, ty)| ty.clone())
                                .unwrap_or(PhpType::Int);
                            self.require_compatible_arg_type(
                                &expected_ty,
                                &val_ty,
                                stmt.span,
                                &format!("Property {}::${}", class_name, property),
                            )?;
                        }
                    }
                    if let Some(class_info) = self.classes.get_mut(class_name) {
                        let property_has_declared_type =
                            class_info.declared_properties.contains(property);
                        if let Some(prop) = class_info
                            .properties
                            .iter_mut()
                            .find(|(n, _)| n == property)
                        {
                            if !property_has_declared_type {
                                if matches!(prop.1, PhpType::Int | PhpType::Void)
                                    && prop.1 != val_ty
                                {
                                    prop.1 = val_ty.clone();
                                } else {
                                    let refined_ty =
                                        Self::specialize_generic_array_hint(&prop.1, &val_ty);
                                    if refined_ty != prop.1 {
                                        prop.1 = refined_ty;
                                    }
                                }
                            }
                        }
                    }
                }
                if let PhpType::Pointer(Some(class_name)) = &obj_ty {
                    if let Some(field_ty) = self.extern_field_type(class_name, property) {
                        if field_ty == PhpType::Int && val_ty != PhpType::Int {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!(
                                    "Type error: cannot assign {:?} to extern field {}::{} of type {:?}",
                                    val_ty, class_name, property, field_ty
                                ),
                            ));
                        }
                    } else if let Some(field_ty) = self.packed_field_type(class_name, property) {
                        if field_ty != val_ty {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!(
                                    "Type error: cannot assign {:?} to packed field {}::{} of type {:?}",
                                    val_ty, class_name, property, field_ty
                                ),
                            ));
                        }
                    } else if self.extern_classes.contains_key(class_name) {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!("Undefined extern field: {}::{}", class_name, property),
                        ));
                    } else if self.packed_classes.contains_key(class_name) {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!("Undefined packed field: {}::{}", class_name, property),
                        ));
                    }
                }
                Ok(())
            }
            StmtKind::PropertyArrayPush {
                object,
                property,
                value,
            } => {
                let obj_ty = self.infer_type(object, env)?;
                let val_ty = self.infer_type(value, env)?;
                match &obj_ty {
                    PhpType::Object(class_name) => {
                        let (prop_ty, property_has_declared_type) = {
                            let class_info = self.classes.get(class_name).ok_or_else(|| {
                                CompileError::new(
                                    stmt.span,
                                    &format!("Undefined class: {}", class_name),
                                )
                            })?;
                            if !class_info.properties.iter().any(|(n, _)| n == property) {
                                return Err(CompileError::new(
                                    stmt.span,
                                    &format!("Undefined property: {}::{}", class_name, property),
                                ));
                            }
                            if let Some(visibility) = class_info.property_visibilities.get(property) {
                                let declaring_class = class_info
                                    .property_declaring_classes
                                    .get(property)
                                    .map(String::as_str)
                                    .unwrap_or(class_name);
                                if !self.can_access_member(declaring_class, visibility) {
                                    return Err(CompileError::new(
                                        stmt.span,
                                        &format!(
                                            "Cannot access {} property: {}::{}",
                                            Self::visibility_label(visibility),
                                            class_name,
                                            property
                                        ),
                                    ));
                                }
                            }
                            if class_info.readonly_properties.contains(property)
                                && !(self.current_class.as_deref()
                                    == class_info
                                        .property_declaring_classes
                                        .get(property)
                                        .map(String::as_str)
                                    && self.current_method.as_deref() == Some("__construct"))
                            {
                                return Err(CompileError::new(
                                    stmt.span,
                                    &format!(
                                        "Cannot assign to readonly property outside constructor: {}::{}",
                                        class_name, property
                                    ),
                                ));
                            }
                            let property_has_declared_type =
                                class_info.declared_properties.contains(property);
                            let prop_ty = class_info
                                .properties
                                .iter()
                                .find(|(name, _)| name == property)
                                .map(|(_, ty)| ty.clone())
                                .unwrap_or(PhpType::Int);
                            (prop_ty, property_has_declared_type)
                        };

                        let updated_prop_ty = match prop_ty {
                            PhpType::Array(elem_ty) => {
                                if property_has_declared_type {
                                    self.require_compatible_arg_type(
                                        elem_ty.as_ref(),
                                        &val_ty,
                                        stmt.span,
                                        &format!(
                                            "Property {}::${}[]",
                                            class_name, property
                                        ),
                                    )?;
                                    PhpType::Array(elem_ty)
                                } else if *elem_ty == val_ty {
                                    PhpType::Array(elem_ty)
                                } else {
                                    let merged_ty = self
                                        .merge_array_element_type(&elem_ty, &val_ty)
                                        .unwrap_or(val_ty.clone());
                                    PhpType::Array(Box::new(merged_ty))
                                }
                            }
                            PhpType::Int | PhpType::Void if !property_has_declared_type => {
                                PhpType::Array(Box::new(val_ty.clone()))
                            }
                            PhpType::Buffer(_) => {
                                return Err(CompileError::new(
                                    stmt.span,
                                    "buffer<T> does not support push; allocate with buffer_new<T>(len)",
                                ))
                            }
                            other => {
                                return Err(CompileError::new(
                                    stmt.span,
                                    &format!(
                                        "Array push requires an array property, got {}",
                                        other
                                    ),
                                ))
                            }
                        };

                        if let Some(class_info) = self.classes.get_mut(class_name) {
                            if !property_has_declared_type {
                                if let Some(prop) = class_info
                                    .properties
                                    .iter_mut()
                                    .find(|(name, _)| name == property)
                                {
                                    prop.1 = updated_prop_ty;
                                }
                            }
                        }
                        Ok(())
                    }
                    PhpType::Pointer(Some(class_name)) => {
                        let field_ty = if let Some(field_ty) = self.extern_field_type(class_name, property) {
                            field_ty
                        } else if let Some(field_ty) = self.packed_field_type(class_name, property) {
                            field_ty
                        } else if self.extern_classes.contains_key(class_name) {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!("Undefined extern field: {}::{}", class_name, property),
                            ));
                        } else if self.packed_classes.contains_key(class_name) {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!("Undefined packed field: {}::{}", class_name, property),
                            ));
                        } else {
                            return Err(CompileError::new(
                                stmt.span,
                                "Array push requires an object or typed pointer",
                            ));
                        };

                        match field_ty {
                            PhpType::Array(_) => Ok(()),
                            PhpType::Buffer(_) => Err(CompileError::new(
                                stmt.span,
                                "buffer<T> does not support push; allocate with buffer_new<T>(len)",
                            )),
                            other => Err(CompileError::new(
                                stmt.span,
                                &format!("Array push requires an array property, got {}", other),
                            )),
                        }
                    }
                    _ => Err(CompileError::new(
                        stmt.span,
                        "Array push requires an object or typed pointer",
                    )),
                }
            }
            StmtKind::PropertyArrayAssign {
                object,
                property,
                index,
                value,
            } => {
                let obj_ty = self.infer_type(object, env)?;
                let idx_ty = self.infer_type(index, env)?;
                let val_ty = self.infer_type(value, env)?;
                match &obj_ty {
                    PhpType::Object(class_name) => {
                        let (prop_ty, property_has_declared_type) = {
                            let class_info = self.classes.get(class_name).ok_or_else(|| {
                                CompileError::new(
                                    stmt.span,
                                    &format!("Undefined class: {}", class_name),
                                )
                            })?;
                            if !class_info.properties.iter().any(|(n, _)| n == property) {
                                return Err(CompileError::new(
                                    stmt.span,
                                    &format!("Undefined property: {}::{}", class_name, property),
                                ));
                            }
                            if let Some(visibility) = class_info.property_visibilities.get(property) {
                                let declaring_class = class_info
                                    .property_declaring_classes
                                    .get(property)
                                    .map(String::as_str)
                                    .unwrap_or(class_name);
                                if !self.can_access_member(declaring_class, visibility) {
                                    return Err(CompileError::new(
                                        stmt.span,
                                        &format!(
                                            "Cannot access {} property: {}::{}",
                                            Self::visibility_label(visibility),
                                            class_name,
                                            property
                                        ),
                                    ));
                                }
                            }
                            if class_info.readonly_properties.contains(property)
                                && !(self.current_class.as_deref()
                                    == class_info
                                        .property_declaring_classes
                                        .get(property)
                                        .map(String::as_str)
                                    && self.current_method.as_deref() == Some("__construct"))
                            {
                                return Err(CompileError::new(
                                    stmt.span,
                                    &format!(
                                        "Cannot assign to readonly property outside constructor: {}::{}",
                                        class_name, property
                                    ),
                                ));
                            }
                            let property_has_declared_type =
                                class_info.declared_properties.contains(property);
                            let prop_ty = class_info
                                .properties
                                .iter()
                                .find(|(name, _)| name == property)
                                .map(|(_, ty)| ty.clone())
                                .unwrap_or(PhpType::Int);
                            (prop_ty, property_has_declared_type)
                        };

                        if idx_ty != PhpType::Int {
                            return Err(CompileError::new(
                                stmt.span,
                                "Array index must be integer",
                            ));
                        }

                        let updated_prop_ty = match prop_ty {
                            PhpType::Array(elem_ty) => {
                                if property_has_declared_type {
                                    self.require_compatible_arg_type(
                                        elem_ty.as_ref(),
                                        &val_ty,
                                        stmt.span,
                                        &format!(
                                            "Property {}::${}[]",
                                            class_name, property
                                        ),
                                    )?;
                                    PhpType::Array(elem_ty)
                                } else if *elem_ty == val_ty {
                                    PhpType::Array(elem_ty)
                                } else {
                                    let merged_ty = self
                                        .merge_array_element_type(&elem_ty, &val_ty)
                                        .unwrap_or(val_ty.clone());
                                    PhpType::Array(Box::new(merged_ty))
                                }
                            }
                            other => {
                                return Err(CompileError::new(
                                    stmt.span,
                                    &format!(
                                        "Array index assignment requires an array property, got {}",
                                        other
                                    ),
                                ))
                            }
                        };

                        if let Some(class_info) = self.classes.get_mut(class_name) {
                            if !property_has_declared_type {
                                if let Some(prop) = class_info
                                    .properties
                                    .iter_mut()
                                    .find(|(name, _)| name == property)
                                {
                                    prop.1 = updated_prop_ty;
                                }
                            }
                        }
                        Ok(())
                    }
                    PhpType::Pointer(Some(class_name)) => {
                        let field_ty = if let Some(field_ty) = self.extern_field_type(class_name, property) {
                            field_ty
                        } else if let Some(field_ty) = self.packed_field_type(class_name, property) {
                            field_ty
                        } else if self.extern_classes.contains_key(class_name) {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!("Undefined extern field: {}::{}", class_name, property),
                            ));
                        } else if self.packed_classes.contains_key(class_name) {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!("Undefined packed field: {}::{}", class_name, property),
                            ));
                        } else {
                            return Err(CompileError::new(
                                stmt.span,
                                "Array index assignment requires an object or typed pointer",
                            ));
                        };

                        if idx_ty != PhpType::Int {
                            return Err(CompileError::new(
                                stmt.span,
                                "Array index must be integer",
                            ));
                        }

                        match field_ty {
                            PhpType::Array(_) => Ok(()),
                            other => Err(CompileError::new(
                                stmt.span,
                                &format!(
                                    "Array index assignment requires an array property, got {}",
                                    other
                                ),
                            )),
                        }
                    }
                    _ => Err(CompileError::new(
                        stmt.span,
                        "Array index assignment requires an object or typed pointer",
                    )),
                }
            }
            _ => unreachable!("non-assignment statement routed to assignment checker"),
        }
    }
}
