use crate::errors::CompileError;
use crate::parser::ast::{ExprKind, Stmt, StmtKind};
use crate::types::{PhpType, TypeEnv};

use super::Checker;

impl Checker {
    pub fn check_stmt(&mut self, stmt: &Stmt, env: &mut TypeEnv) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::IfDef { .. } => {
                Err(CompileError::new(stmt.span, "Unresolved ifdef statement"))
            }
            StmtKind::NamespaceDecl { .. }
            | StmtKind::NamespaceBlock { .. }
            | StmtKind::UseDecl { .. } => Err(CompileError::new(
                stmt.span,
                "Unresolved namespace/use statement",
            )),
            StmtKind::Echo(expr) => {
                self.infer_type(expr, env)?;
                Ok(())
            }
            StmtKind::Assign { name, value } => {
                let ty = self.infer_type(value, env)?;
                if ty == PhpType::Callable {
                    if let Some(sig) = self.resolve_expr_callable_sig(value, env)? {
                        self.closure_return_types
                            .insert(name.clone(), sig.return_type.clone());
                        self.callable_sigs.insert(name.clone(), sig);
                        if let ExprKind::FirstClassCallable(target) = &value.kind {
                            self.first_class_callable_targets
                                .insert(name.clone(), target.clone());
                        } else if let ExprKind::Variable(src_name) = &value.kind {
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
                        // Upgrade array element type when assigning a
                        // different type (e.g. empty [] defaults to
                        // Array(Int), first string assign upgrades it)
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
                        // Upgrade array type when pushing a different type
                        // (e.g. empty [] defaults to Array(Int), first push
                        // of a string should upgrade to Array(Str))
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
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                body,
            } => {
                let arr_ty = self.infer_type(array, env)?;
                if let PhpType::Array(elem_ty) = &arr_ty {
                    if let Some(k) = key_var {
                        env.insert(k.clone(), PhpType::Int);
                    }
                    env.insert(value_var.clone(), *elem_ty.clone());
                } else if let PhpType::AssocArray { key, value } = &arr_ty {
                    if let Some(k) = key_var {
                        env.insert(k.clone(), *key.clone());
                    }
                    env.insert(value_var.clone(), *value.clone());
                } else {
                    return Err(CompileError::new(stmt.span, "foreach requires an array"));
                }
                let mut errors = Vec::new();
                for s in body {
                    if let Err(error) = self.check_stmt(s, env) {
                        errors.extend(error.flatten());
                    }
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => {
                self.infer_type(subject, env)?;
                let mut errors = Vec::new();
                for (values, body) in cases {
                    for v in values {
                        self.infer_type(v, env)?;
                    }
                    for s in body {
                        if let Err(error) = self.check_stmt(s, env) {
                            errors.extend(error.flatten());
                        }
                    }
                }
                if let Some(body) = default {
                    for s in body {
                        if let Err(error) = self.check_stmt(s, env) {
                            errors.extend(error.flatten());
                        }
                    }
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                self.infer_type(condition, env)?;
                let mut errors = Vec::new();
                for s in then_body {
                    if let Err(error) = self.check_stmt(s, env) {
                        errors.extend(error.flatten());
                    }
                }
                for (cond, body) in elseif_clauses {
                    self.infer_type(cond, env)?;
                    for s in body {
                        if let Err(error) = self.check_stmt(s, env) {
                            errors.extend(error.flatten());
                        }
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        if let Err(error) = self.check_stmt(s, env) {
                            errors.extend(error.flatten());
                        }
                    }
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::DoWhile { body, condition } => {
                let mut errors = Vec::new();
                for s in body {
                    if let Err(error) = self.check_stmt(s, env) {
                        errors.extend(error.flatten());
                    }
                }
                self.infer_type(condition, env)?;
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::While { condition, body } => {
                self.infer_type(condition, env)?;
                let mut errors = Vec::new();
                for s in body {
                    if let Err(error) = self.check_stmt(s, env) {
                        errors.extend(error.flatten());
                    }
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(s) = init {
                    self.check_stmt(s, env)?;
                }
                if let Some(c) = condition {
                    self.infer_type(c, env)?;
                }
                if let Some(s) = update {
                    self.check_stmt(s, env)?;
                }
                let mut errors = Vec::new();
                for s in body {
                    if let Err(error) = self.check_stmt(s, env) {
                        errors.extend(error.flatten());
                    }
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::Throw(expr) => {
                let thrown_ty = self.infer_type(expr, env)?;
                match thrown_ty {
                    PhpType::Object(type_name)
                        if self.object_type_implements_throwable(&type_name) =>
                    {
                        Ok(())
                    }
                    PhpType::Object(_) => Err(CompileError::new(
                        stmt.span,
                        "Type error: throw requires an object implementing Throwable",
                    )),
                    _ => Err(CompileError::new(
                        stmt.span,
                        "Type error: throw requires an object value",
                    )),
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                let mut errors = Vec::new();
                for s in try_body {
                    if let Err(error) = self.check_stmt(s, env) {
                        errors.extend(error.flatten());
                    }
                }
                for catch_clause in catches {
                    let mut resolved_types = Vec::new();
                    for raw_exception_type in &catch_clause.exception_types {
                        let exception_type =
                            self.resolve_catch_type_name(raw_exception_type, stmt.span)?;
                        if !self.classes.contains_key(&exception_type)
                            && !self.interfaces.contains_key(&exception_type)
                        {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!("Undefined class: {}", exception_type),
                            ));
                        }
                        if !self.object_type_implements_throwable(&exception_type) {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!(
                                    "Catch type must extend or implement Throwable: {}",
                                    exception_type
                                ),
                            ));
                        }
                        resolved_types.push(exception_type);
                    }
                    if let Some(variable) = &catch_clause.variable {
                        env.insert(
                            variable.clone(),
                            PhpType::Object(self.common_catch_type_name(&resolved_types)),
                        );
                    }
                    for s in &catch_clause.body {
                        if let Err(error) = self.check_stmt(s, env) {
                            errors.extend(error.flatten());
                        }
                    }
                }
                if let Some(body) = finally_body {
                    for s in body {
                        if let Err(error) = self.check_stmt(s, env) {
                            errors.extend(error.flatten());
                        }
                    }
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::Include { .. } => {
                // Should have been resolved before type checking
                Err(CompileError::new(stmt.span, "Unresolved include statement"))
            }
            StmtKind::PackedClassDecl { .. } => Ok(()),
            StmtKind::Break | StmtKind::Continue => Ok(()),
            StmtKind::ExprStmt(expr) => {
                self.infer_type(expr, env)?;
                Ok(())
            }
            StmtKind::ConstDecl { name, value } => {
                let ty = self.infer_type(value, env)?;
                self.constants.insert(name.clone(), ty);
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
                // global vars are accessible; they reference variables from the outer scope
                // Mark them in the environment if not already present
                for var in vars {
                    self.active_globals.insert(var.clone());
                    if !env.contains_key(var) {
                        if let Some(global_ty) = self.top_level_env.get(var) {
                            env.insert(var.clone(), global_ty.clone());
                        } else {
                            // Default to Int — will be refined by actual usage
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
            StmtKind::FunctionDecl { .. } => Ok(()),
            StmtKind::Return(expr) => {
                if let Some(e) = expr {
                    self.infer_type(e, env)?;
                }
                Ok(())
            }
            StmtKind::ClassDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. } => {
                // Method bodies are type-checked in a post-pass (after all new ClassName()
                // calls have updated property types from constructor arg types)
                Ok(())
            }
            StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. } => {
                // Extern declarations are processed in the pre-scan pass
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
                    }
                    // Update property type from assigned value (e.g., Object type from $a->next = $b)
                    if let Some(class_info) = self.classes.get_mut(class_name) {
                        if let Some(prop) = class_info
                            .properties
                            .iter_mut()
                            .find(|(n, _)| n == property)
                        {
                            if prop.1 == PhpType::Int && val_ty != PhpType::Int {
                                prop.1 = val_ty.clone();
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
        }
    }
}
