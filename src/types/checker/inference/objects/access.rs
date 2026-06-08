//! Purpose:
//! Infers object access expression types.
//! Validates class, method, constructor, property, and magic-access contracts against schema metadata.
//!
//! Called from:
//! - `crate::types::checker::inference::objects`
//!
//! Key details:
//! - Object inference depends on flattened class metadata, visibility, inheritance, and declared property types.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;

impl Checker {
    /// Infers the type of a property access expression (`$obj->prop`).
    ///
    /// Returns the declared property type on class/object, handles `Mixed`
    /// receivers (returning `Mixed`), and emits an error for non-object types.
    /// For nullable unions resolved to a single class, returns a nullable type.
    pub(crate) fn infer_property_access_type(
        &mut self,
        object: &Expr,
        property: &str,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let obj_ty = self.infer_type(object, env)?;
        if let PhpType::Object(class_name) = &obj_ty {
            return self.infer_property_on_class_type(class_name, property, expr);
        }
        // Non-nullsafe property access on a nullable / union object type
        // (`?Foo`, `Foo|null`) is allowed when the union resolves to a
        // single class. A null receiver emits a PHP-style warning and
        // evaluates to null, so the inferred type remains nullable.
        if let PhpType::Union(_) = &obj_ty {
            if let Some((class_name, nullable)) =
                self.nullsafe_object_receiver(&obj_ty, expr, "property access")?
            {
                let property_ty = self.infer_property_on_class_type(&class_name, property, expr)?;
                return if nullable {
                    Ok(self.normalize_union_type(vec![property_ty, PhpType::Void]))
                } else {
                    Ok(property_ty)
                };
            }
        }
        if let PhpType::Pointer(Some(class_name)) = &obj_ty {
            if let Some(field_ty) = self.extern_field_type(class_name, property) {
                return Ok(field_ty);
            }
            if let Some(field_ty) = self.packed_field_type(class_name, property) {
                return Ok(field_ty);
            }
            if self.extern_classes.contains_key(class_name) {
                return Err(CompileError::new(
                    expr.span,
                    &format!("Undefined extern field: {}::{}", class_name, property),
                ));
            }
            if self.packed_classes.contains_key(class_name) {
                return Err(CompileError::new(
                    expr.span,
                    &format!("Undefined packed field: {}::{}", class_name, property),
                ));
            }
        }
        // Mixed receivers fall through to runtime dispatch. The decoded
        // value may be a stdClass (e.g. from `json_decode($json)`), an
        // associative array, or a scalar — codegen unboxes the Mixed cell,
        // checks the tag, and routes object payloads through
        // `__rt_stdclass_get`. Non-object payloads return Mixed(null) at
        // runtime, mirroring PHP's "attempt to read property on non-object"
        // diagnostic for the most common idiom (`$obj->name` after
        // json_decode).
        let _ = property;
        if matches!(obj_ty, PhpType::Mixed) {
            return Ok(PhpType::Mixed);
        }
        Err(CompileError::new(
            expr.span,
            "Property access requires an object or typed pointer",
        ))
    }

    /// Infers the type of a nullsafe property access expression (`$obj?->prop`).
    ///
    /// For `Mixed` receivers returns `Mixed`. For valid nullable object unions,
    /// returns a union of the property type with `void`. Returns `void` for
    /// invalid receivers.
    pub(crate) fn infer_nullsafe_property_access_type(
        &mut self,
        object: &Expr,
        property: &str,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let obj_ty = self.infer_type(object, env)?;
        if matches!(obj_ty, PhpType::Mixed) {
            return Ok(PhpType::Mixed);
        }
        let Some((class_name, nullable)) =
            self.nullsafe_object_receiver(&obj_ty, expr, "property access")?
        else {
            return Ok(PhpType::Void);
        };
        let property_ty = self.infer_property_on_class_type(&class_name, property, expr)?;
        if nullable {
            Ok(self.normalize_union_type(vec![property_ty, PhpType::Void]))
        } else {
            Ok(property_ty)
        }
    }

    /// Infers the type of a dynamic property access expression (`$obj->$prop`).
    ///
    /// The property name expression must be `string`, `int`, or `Mixed`.
    /// Resolves string literals via `infer_property_access_type` or
    /// `infer_nullsafe_property_access_type`; returns `Mixed` for runtime
    /// dispatch on `Object`, `Union`, or `Mixed` receivers.
    pub(crate) fn infer_dynamic_property_access_type(
        &mut self,
        object: &Expr,
        property: &Expr,
        expr: &Expr,
        env: &TypeEnv,
        nullsafe: bool,
    ) -> Result<PhpType, CompileError> {
        let obj_ty = self.infer_type(object, env)?;
        if nullsafe && matches!(obj_ty, PhpType::Void) {
            return Ok(PhpType::Void);
        }

        let property_ty = self.infer_type(property, env)?;
        if !matches!(property_ty, PhpType::Str | PhpType::Int | PhpType::Mixed) {
            return Err(CompileError::new(
                property.span,
                "Dynamic property name must be string or integer",
            ));
        }

        if let ExprKind::StringLiteral(name) = &property.kind {
            return if nullsafe {
                self.infer_nullsafe_property_access_type(object, name, expr, env)
            } else {
                self.infer_property_access_type(object, name, expr, env)
            };
        }

        match obj_ty {
            PhpType::Object(_) | PhpType::Union(_) | PhpType::Mixed => Ok(PhpType::Mixed),
            _ if nullsafe => {
                self.nullsafe_object_receiver(&obj_ty, expr, "property access")?;
                Ok(PhpType::Mixed)
            }
            _ => Err(CompileError::new(
                expr.span,
                "Property access requires an object or typed pointer",
            )),
        }
    }

    /// Resolves a property name against a known class's schema.
    ///
    /// Returns the property type after checking visibility, or errors on
    /// undefined properties. Returns `Mixed` for `stdClass` and classes
    /// marked `#[AllowDynamicProperties]`. Uses `__get` signature when
    /// no declared property matches.
    pub(crate) fn infer_property_on_class_type(
        &self,
        class_name: &str,
        property: &str,
        expr: &Expr,
    ) -> Result<PhpType, CompileError> {
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
            return Ok(PhpType::Mixed);
        }
        if let Some(class_info) = self.classes.get(class_name) {
            if let Some(visibility) = class_info.property_visibilities.get(property) {
                let declaring_class = class_info
                    .property_declaring_classes
                    .get(property)
                    .map(String::as_str)
                    .unwrap_or(class_name);
                if !self.can_access_member(declaring_class, visibility) {
                    return Err(CompileError::new(
                        expr.span,
                        &format!(
                            "Cannot access {} property: {}::{}",
                            Self::visibility_label(visibility),
                            class_name,
                            property
                        ),
                    ));
                }
            }
            if let Some((_, ty)) = class_info.properties.iter().find(|(n, _)| n == property) {
                return Ok(ty.clone());
            }
            if let Some(sig) = class_info.methods.get("__get") {
                return Ok(sig.return_type.clone());
            }
            if class_info.allow_dynamic_properties {
                // PHP 8.2 #[\AllowDynamicProperties]: undeclared property
                // reads are dispatched to the side-table hashtable; the
                // value is statically `Mixed` because we cannot infer it.
                return Ok(PhpType::Mixed);
            }
            return Err(CompileError::new(
                expr.span,
                &format!("Undefined property: {}::{}", class_name, property),
            ));
        }
        Err(CompileError::new(
            expr.span,
            &format!("Undefined class: {}", class_name),
        ))
    }

    /// Extracts the single class name from a nullable object type for nullsafe ops.
    ///
    /// Returns `None` for `void`. On `Union` types, validates that exactly one
    /// class is present alongside optional `void`s; errors on mixed non-object
    /// union members. The `bool` in the result indicates whether `void` was
    /// present (i.e. whether the original type was nullable).
    pub(crate) fn nullsafe_object_receiver(
        &self,
        obj_ty: &PhpType,
        expr: &Expr,
        context: &str,
    ) -> Result<Option<(String, bool)>, CompileError> {
        match obj_ty {
            PhpType::Void => Ok(None),
            PhpType::Object(class_name) => Ok(Some((class_name.clone(), false))),
            PhpType::Union(members) => {
                let mut class_name = None;
                let mut nullable = false;
                for member in members {
                    match member {
                        PhpType::Void => nullable = true,
                        PhpType::Object(candidate) => {
                            if class_name
                                .as_ref()
                                .is_some_and(|existing: &String| existing != candidate)
                            {
                                return Err(CompileError::new(
                                    expr.span,
                                    &format!(
                                        "Nullsafe {} requires a single nullable object type",
                                        context
                                    ),
                                ));
                            }
                            class_name = Some(candidate.clone());
                        }
                        _ => {
                            return Err(CompileError::new(
                                expr.span,
                                &format!("Nullsafe {} requires an object or null", context),
                            ));
                        }
                    }
                }
                Ok(class_name.map(|name| (name, nullable)))
            }
            _ => Err(CompileError::new(
                expr.span,
                &format!("Nullsafe {} requires an object or null", context),
            )),
        }
    }

    /// Returns the single distinct object class in a union receiver, ignoring any
    /// non-object members (scalars, `void`).
    ///
    /// Returns `None` when the type is not a union, or the union has zero or more
    /// than one distinct object class. Used to allow regular `->` method calls on
    /// unions such as `Foo|false`: codegen dispatches on the runtime class id and
    /// faults like PHP when the value is not an object, so the checker only needs
    /// the single object class to surface the method's return type.
    pub(crate) fn union_single_object_class(&self, obj_ty: &PhpType) -> Option<String> {
        let PhpType::Union(members) = obj_ty else {
            return None;
        };
        let mut found: Option<String> = None;
        for member in members {
            if let PhpType::Object(name) = member {
                match &found {
                    Some(existing) if existing != name => return None,
                    _ => found = Some(name.clone()),
                }
            }
        }
        found
    }

    /// Infers the type of a static property access (`Foo::$prop`).
    ///
    /// Resolves the static receiver (named, `self::`, `static::`, `parent::`)
    /// to a class name, then looks up the declared static property type after
    /// validating visibility rules.
    pub(crate) fn infer_static_property_access_type(
        &mut self,
        receiver: &StaticReceiver,
        property: &str,
        expr: &Expr,
    ) -> Result<PhpType, CompileError> {
        let class_name = self.resolve_static_property_receiver(receiver, expr)?;
        let class_info = self.classes.get(&class_name).ok_or_else(|| {
            CompileError::new(expr.span, &format!("Undefined class: {}", class_name))
        })?;
        if let Some(visibility) = class_info.static_property_visibilities.get(property) {
            let declaring_class = class_info
                .static_property_declaring_classes
                .get(property)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            if !self.can_access_member(declaring_class, visibility) {
                return Err(CompileError::new(
                    expr.span,
                    &format!(
                        "Cannot access {} static property: {}::{}",
                        Self::visibility_label(visibility),
                        class_name,
                        property
                    ),
                ));
            }
        }
        class_info
            .static_properties
            .iter()
            .find(|(name, _)| name == property)
            .map(|(_, ty)| ty.clone())
            .ok_or_else(|| {
                CompileError::new(
                    expr.span,
                    &format!("Undefined static property: {}::{}", class_name, property),
                )
            })
    }

    /// Resolves a static property receiver to its class name.
    ///
    /// `Named` returns the class directly. `Self_`/`Static` require a class
    /// context. `Parent` returns the parent of the current class.
    pub(crate) fn resolve_static_property_receiver(
        &self,
        receiver: &StaticReceiver,
        expr: &Expr,
    ) -> Result<String, CompileError> {
        match receiver {
            StaticReceiver::Named(class_name) => Ok(class_name.as_str().to_string()),
            StaticReceiver::Self_ => self.current_class.as_ref().cloned().ok_or_else(|| {
                CompileError::new(expr.span, "Cannot use self:: outside class method scope")
            }),
            StaticReceiver::Static => self.current_class.as_ref().cloned().ok_or_else(|| {
                CompileError::new(expr.span, "Cannot use static:: outside class method scope")
            }),
            StaticReceiver::Parent => {
                let current_class = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(expr.span, "Cannot use parent:: outside class method scope")
                })?;
                let current_info = self.classes.get(current_class).ok_or_else(|| {
                    CompileError::new(expr.span, &format!("Undefined class: {}", current_class))
                })?;
                current_info.parent.as_ref().cloned().ok_or_else(|| {
                    CompileError::new(
                        expr.span,
                        &format!("Class {} has no parent class", current_class),
                    )
                })
            }
        }
    }

    /// Infers the type of `$this` inside a class method.
    ///
    /// Errors if called from a static method or outside a class context.
    /// Returns `PhpType::Object(current_class)` for valid contexts.
    pub(crate) fn infer_this_type(&mut self, expr: &Expr) -> Result<PhpType, CompileError> {
        if self.current_method_is_static {
            return Err(CompileError::new(
                expr.span,
                "Cannot use $this inside a static method",
            ));
        }
        if let Some(class_name) = &self.current_class {
            Ok(PhpType::Object(class_name.clone()))
        } else {
            Err(CompileError::new(
                expr.span,
                "Cannot use $this outside of a class method",
            ))
        }
    }

    /// Infers the type of a `ptr_cast<T>()` expression.
    ///
    /// Validates the inner expression is a pointer type, normalizes the target
    /// type string, and returns `PhpType::Pointer(Some(normalized))`.
    pub(crate) fn infer_ptr_cast_type(
        &mut self,
        target_type: &str,
        inner: &Expr,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let inner_ty = self.infer_type(inner, env)?;
        self.ensure_pointer_type(&inner_ty, expr.span, "ptr_cast()")?;
        let normalized = self
            .normalize_pointer_target_type(target_type)
            .ok_or_else(|| {
                CompileError::new(
                    expr.span,
                    &format!("Unknown ptr_cast target type: {}", target_type),
                )
            })?;
        Ok(PhpType::Pointer(Some(normalized)))
    }
}
