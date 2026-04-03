use std::collections::HashSet;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, TypeExpr, Visibility};
use crate::types::{packed_type_size, ClassInfo, EnumInfo, FunctionSig, PhpType, TypeEnv};

use super::inference::syntactic::infer_expr_type_syntactic;
use super::{Checker, FnDecl};

impl Checker {
    pub(crate) fn callable_wrapper_sig(sig: &FunctionSig) -> FunctionSig {
        let Some(variadic_name) = sig.variadic.as_ref() else {
            return sig.clone();
        };
        if sig
            .params
            .last()
            .is_some_and(|(name, ty)| name == variadic_name && matches!(ty, PhpType::Array(_)))
        {
            return sig.clone();
        }

        let mut wrapper_sig = sig.clone();
        wrapper_sig.params.push((
            variadic_name.clone(),
            PhpType::Array(Box::new(PhpType::Mixed)),
        ));
        wrapper_sig.defaults.push(None);
        wrapper_sig.ref_params.push(false);
        wrapper_sig.declared_params.push(false);
        wrapper_sig
    }

    pub(crate) fn resolve_declared_param_type_hint(
        &self,
        type_expr: &TypeExpr,
        span: crate::span::Span,
        context: &str,
    ) -> Result<PhpType, CompileError> {
        let ty = self.resolve_type_expr(type_expr, span)?;
        match ty {
            PhpType::Void => Err(CompileError::new(
                span,
                &format!("{} cannot use type void", context),
            )),
            _ => Ok(ty),
        }
    }

    pub(crate) fn resolve_declared_return_type_hint(
        &self,
        type_expr: &TypeExpr,
        span: crate::span::Span,
        _context: &str,
    ) -> Result<PhpType, CompileError> {
        let ty = self.resolve_type_expr(type_expr, span)?;
        Ok(ty)
    }

    pub(crate) fn require_boxed_by_ref_storage(
        &self,
        expected_ty: &PhpType,
        actual_ty: &PhpType,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if matches!(expected_ty.codegen_repr(), PhpType::Mixed)
            && !matches!(actual_ty.codegen_repr(), PhpType::Mixed)
        {
            return Err(CompileError::new(
                span,
                &format!(
                    "{} requires a variable with mixed/union/nullable storage when passed by reference",
                    context
                ),
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_declared_default_type(
        &self,
        expected_ty: &PhpType,
        default_expr: Option<&Expr>,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if let Some(default_expr) = default_expr {
            let default_ty = infer_expr_type_syntactic(default_expr);
            self.require_compatible_arg_type(expected_ty, &default_ty, span, context)?;
        }
        Ok(())
    }

    pub(crate) fn initial_function_param_types(
        &self,
        name: &str,
        decl: &FnDecl,
    ) -> Result<Vec<(String, PhpType)>, CompileError> {
        let mut param_types = Vec::new();
        for (idx, param_name) in decl.params.iter().enumerate() {
            if let Some(type_ann) = decl.param_types.get(idx).and_then(|t| t.as_ref()) {
                let declared_ty = self.resolve_declared_param_type_hint(
                    type_ann,
                    decl.span,
                    &format!("Function '{}' parameter ${}", name, param_name),
                )?;
                self.validate_declared_default_type(
                    &declared_ty,
                    decl.defaults.get(idx).and_then(|d| d.as_ref()),
                    decl.span,
                    &format!("Function '{}' parameter ${}", name, param_name),
                )?;
                param_types.push((param_name.clone(), declared_ty));
            } else if let Some(default_expr) = decl.defaults.get(idx).and_then(|d| d.as_ref()) {
                param_types.push((param_name.clone(), infer_expr_type_syntactic(default_expr)));
            } else {
                param_types.push((param_name.clone(), PhpType::Int));
            }
        }
        if let Some(variadic_name) = decl.variadic.as_ref() {
            param_types.push((
                variadic_name.clone(),
                PhpType::Array(Box::new(PhpType::Int)),
            ));
        }
        Ok(param_types)
    }

    pub(crate) fn declared_method_param_flags(
        class_info: &ClassInfo,
        method_name: &str,
        is_static: bool,
    ) -> Vec<bool> {
        class_info
            .method_decls
            .iter()
            .find(|method| method.name == method_name && method.is_static == is_static)
            .map(|method| {
                method
                    .params
                    .iter()
                    .map(|(_, type_ann, _, _)| type_ann.is_some())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn callable_sig_for_declared_params(sig: &FunctionSig, declared_flags: &[bool]) -> FunctionSig {
        let mut effective_sig = sig.clone();
        for (idx, (_, ty)) in effective_sig.params.iter_mut().enumerate() {
            if !declared_flags.get(idx).copied().unwrap_or(false) {
                *ty = PhpType::Mixed;
            }
        }
        effective_sig.declared_params = declared_flags.to_vec();
        effective_sig
    }

    pub(crate) fn with_local_storage_context<T, F>(
        &mut self,
        ref_param_names: Vec<String>,
        f: F,
    ) -> Result<T, CompileError>
    where
        F: FnOnce(&mut Self) -> Result<T, CompileError>,
    {
        let saved_ref_params = self.active_ref_params.clone();
        let saved_globals = self.active_globals.clone();
        let saved_statics = self.active_statics.clone();

        self.active_ref_params = ref_param_names.into_iter().collect();
        self.active_globals.clear();
        self.active_statics.clear();

        let result = f(self);

        self.active_ref_params = saved_ref_params;
        self.active_globals = saved_globals;
        self.active_statics = saved_statics;

        result
    }

    pub(crate) fn can_access_member(&self, declaring_class: &str, visibility: &Visibility) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Protected => self.current_class.as_deref().is_some_and(|current| {
                current == declaring_class || self.is_subclass_of(current, declaring_class)
            }),
            Visibility::Private => self.current_class.as_deref() == Some(declaring_class),
        }
    }

    pub(crate) fn visibility_label(visibility: &Visibility) -> &'static str {
        match visibility {
            Visibility::Public => "public",
            Visibility::Protected => "protected",
            Visibility::Private => "private",
        }
    }

    pub(crate) fn is_subclass_of(&self, class_name: &str, ancestor_name: &str) -> bool {
        let mut current = self
            .classes
            .get(class_name)
            .and_then(|class| class.parent.clone());
        while let Some(parent_name) = current {
            if parent_name == ancestor_name {
                return true;
            }
            current = self
                .classes
                .get(&parent_name)
                .and_then(|class| class.parent.clone());
        }
        false
    }

    pub(crate) fn class_implements_interface(&self, class_name: &str, interface_name: &str) -> bool {
        self.classes.get(class_name).is_some_and(|class_info| {
            class_info
                .interfaces
                .iter()
                .any(|name| name == interface_name)
        })
    }

    pub(crate) fn interface_extends_interface(&self, interface_name: &str, ancestor_name: &str) -> bool {
        if interface_name == ancestor_name {
            return true;
        }
        let mut stack = vec![interface_name.to_string()];
        let mut seen = HashSet::new();
        while let Some(current_name) = stack.pop() {
            if !seen.insert(current_name.clone()) {
                continue;
            }
            let Some(interface_info) = self.interfaces.get(&current_name) else {
                continue;
            };
            for parent_name in &interface_info.parents {
                if parent_name == ancestor_name {
                    return true;
                }
                stack.push(parent_name.clone());
            }
        }
        false
    }

    pub(crate) fn object_type_implements_throwable(&self, type_name: &str) -> bool {
        if self.classes.contains_key(type_name) {
            return self.class_implements_interface(type_name, "Throwable");
        }
        if self.interfaces.contains_key(type_name) {
            return self.interface_extends_interface(type_name, "Throwable");
        }
        false
    }

    pub(crate) fn common_catch_type_name(&self, type_names: &[String]) -> String {
        let mut iter = type_names.iter();
        let Some(first_name) = iter.next() else {
            return "Throwable".to_string();
        };
        let mut common = first_name.clone();
        for type_name in iter {
            match self.common_object_type(&common, type_name) {
                Some(PhpType::Object(next_common)) => common = next_common,
                _ => return "Throwable".to_string(),
            }
        }
        common
    }

    pub(crate) fn resolve_catch_type_name(
        &self,
        raw_name: &crate::names::Name,
        span: crate::span::Span,
    ) -> Result<String, CompileError> {
        match raw_name.as_str() {
            "self" => self.current_class.clone().ok_or_else(|| {
                CompileError::new(span, "Cannot use self in catch outside of a class context")
            }),
            "parent" => {
                let current_class = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(
                        span,
                        "Cannot use parent in catch outside of a class context",
                    )
                })?;
                self.classes
                    .get(current_class)
                    .and_then(|class_info| class_info.parent.clone())
                    .ok_or_else(|| CompileError::new(span, "Class has no parent class"))
            }
            _ => Ok(raw_name.to_string()),
        }
    }

    pub(crate) fn is_pointer_type(ty: &PhpType) -> bool {
        matches!(ty, PhpType::Pointer(_))
    }

    pub(crate) fn pointer_types_compatible(left: &PhpType, right: &PhpType) -> bool {
        matches!((left, right), (PhpType::Pointer(_), PhpType::Pointer(_)))
    }

    pub(crate) fn normalize_union_type(&self, members: Vec<PhpType>) -> PhpType {
        let mut flat = Vec::new();
        for member in members {
            match member {
                PhpType::Union(inner) => flat.extend(inner),
                PhpType::Mixed => return PhpType::Mixed,
                other => flat.push(other),
            }
        }

        let mut deduped = Vec::new();
        for member in flat {
            if !deduped.iter().any(|existing| existing == &member) {
                deduped.push(member);
            }
        }

        if deduped.len() == 1 {
            deduped.pop().expect("union member exists")
        } else {
            PhpType::Union(deduped)
        }
    }

    pub(crate) fn type_accepts(&self, expected: &PhpType, actual: &PhpType) -> bool {
        if expected == actual {
            return true;
        }

        match expected {
            PhpType::Mixed => true,
            PhpType::Union(members) => members
                .iter()
                .any(|member| self.type_accepts(member, actual)),
            PhpType::Object(expected_name) => match actual {
                PhpType::Object(actual_name) => {
                    expected_name == actual_name
                        || self.is_subclass_of(actual_name, expected_name)
                        || self.class_implements_interface(actual_name, expected_name)
                        || self.interface_extends_interface(actual_name, expected_name)
                }
                _ => false,
            },
            PhpType::Pointer(_) => Self::pointer_types_compatible(expected, actual),
            _ => false,
        }
    }

    pub(crate) fn union_contains_void(ty: &PhpType) -> bool {
        matches!(ty, PhpType::Union(members) if members.iter().any(|member| *member == PhpType::Void))
    }

    pub(crate) fn strip_void_from_union(&self, ty: &PhpType) -> PhpType {
        match ty {
            PhpType::Union(members) => {
                let filtered: Vec<PhpType> = members
                    .iter()
                    .filter(|member| **member != PhpType::Void)
                    .cloned()
                    .collect();
                self.normalize_union_type(filtered)
            }
            other => other.clone(),
        }
    }

    pub(crate) fn type_supports_mixed_int_dispatch(&self, ty: &PhpType) -> bool {
        let _ = self;
        match ty {
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Str => true,
            PhpType::Union(members) => members
                .iter()
                .all(|member| self.type_supports_mixed_int_dispatch(member)),
            _ => false,
        }
    }

    pub(crate) fn is_union_with_mixed_int_dispatch(&self, ty: &PhpType) -> bool {
        matches!(ty, PhpType::Union(_)) && self.type_supports_mixed_int_dispatch(ty)
    }

    pub(crate) fn check_enum_static_call(
        &mut self,
        enum_info: &EnumInfo,
        class_name: &str,
        method: &str,
        args: &[Expr],
        env: &TypeEnv,
        span: crate::span::Span,
    ) -> Result<PhpType, CompileError> {
        match method {
            "cases" => {
                if !args.is_empty() {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Static method '{}::cases' expects 0 arguments, got {}",
                            class_name,
                            args.len()
                        ),
                    ));
                }
                Ok(PhpType::Array(Box::new(PhpType::Object(
                    class_name.to_string(),
                ))))
            }
            "from" | "tryFrom" => {
                let Some(backing_ty) = enum_info.backing_type.as_ref() else {
                    return Err(CompileError::new(
                        span,
                        &format!("Undefined method: {}::{}", class_name, method),
                    ));
                };
                if args.len() != 1 {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Static method '{}::{}' expects 1 argument, got {}",
                            class_name,
                            method,
                            args.len()
                        ),
                    ));
                }
                let arg_ty = self.infer_type(&args[0], env)?;
                if !self.type_accepts(backing_ty, &arg_ty) {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Type error: {}::{} expects {}, got {}",
                            class_name, method, backing_ty, arg_ty
                        ),
                    ));
                }
                if method == "from" {
                    Ok(PhpType::Object(class_name.to_string()))
                } else {
                    Ok(self.normalize_union_type(vec![
                        PhpType::Object(class_name.to_string()),
                        PhpType::Void,
                    ]))
                }
            }
            _ => Err(CompileError::new(
                span,
                &format!("Undefined method: {}::{}", class_name, method),
            )),
        }
    }

    pub(crate) fn merged_assignment_type(&self, existing: &PhpType, new_ty: &PhpType) -> Option<PhpType> {
        if self.type_accepts(existing, new_ty) {
            return Some(existing.clone());
        }
        if matches!(existing, PhpType::Union(_)) {
            return None;
        }
        if existing == new_ty {
            return Some(existing.clone());
        }
        if matches!(existing, PhpType::Mixed) || matches!(new_ty, PhpType::Mixed) {
            return Some(PhpType::Mixed);
        }
        if *new_ty == PhpType::Void {
            return Some(existing.clone());
        }
        if *existing == PhpType::Void {
            return Some(new_ty.clone());
        }
        if matches!(existing, PhpType::Int | PhpType::Bool | PhpType::Float)
            && matches!(new_ty, PhpType::Int | PhpType::Bool | PhpType::Float)
        {
            return Some(existing.clone());
        }
        if Self::pointer_types_compatible(existing, new_ty) {
            return Some(match (existing, new_ty) {
                (PhpType::Pointer(Some(left)), PhpType::Pointer(Some(right))) if left == right => {
                    PhpType::Pointer(Some(left.clone()))
                }
                (PhpType::Pointer(None), PhpType::Pointer(Some(tag)))
                | (PhpType::Pointer(Some(tag)), PhpType::Pointer(None)) => {
                    PhpType::Pointer(Some(tag.clone()))
                }
                _ => PhpType::Pointer(None),
            });
        }
        None
    }

    pub(crate) fn common_object_type(&self, left: &str, right: &str) -> Option<PhpType> {
        if left == right {
            return Some(PhpType::Object(left.to_string()));
        }
        if self.interfaces.contains_key(left) && self.interface_extends_interface(right, left) {
            return Some(PhpType::Object(left.to_string()));
        }
        if self.interfaces.contains_key(right) && self.interface_extends_interface(left, right) {
            return Some(PhpType::Object(right.to_string()));
        }
        if self.interfaces.contains_key(left) && self.class_implements_interface(right, left) {
            return Some(PhpType::Object(left.to_string()));
        }
        if self.interfaces.contains_key(right) && self.class_implements_interface(left, right) {
            return Some(PhpType::Object(right.to_string()));
        }
        if self.is_subclass_of(left, right) {
            return Some(PhpType::Object(right.to_string()));
        }
        if self.is_subclass_of(right, left) {
            return Some(PhpType::Object(left.to_string()));
        }

        let mut left_ancestors = HashSet::new();
        let mut current = Some(left.to_string());
        while let Some(class_name) = current {
            left_ancestors.insert(class_name.clone());
            current = self
                .classes
                .get(&class_name)
                .and_then(|class_info| class_info.parent.clone());
        }

        let mut current = Some(right.to_string());
        while let Some(class_name) = current {
            if left_ancestors.contains(&class_name) {
                return Some(PhpType::Object(class_name));
            }
            current = self
                .classes
                .get(&class_name)
                .and_then(|class_info| class_info.parent.clone());
        }

        None
    }

    pub(crate) fn merge_array_element_type(&self, existing: &PhpType, new_ty: &PhpType) -> Option<PhpType> {
        if existing == new_ty {
            return Some(existing.clone());
        }
        if matches!(existing, PhpType::Mixed) || matches!(new_ty, PhpType::Mixed) {
            return Some(PhpType::Mixed);
        }

        match (existing, new_ty) {
            (PhpType::Object(left), PhpType::Object(right)) => self.common_object_type(left, right),
            _ => None,
        }
    }

    pub(crate) fn propagate_constructor_arg_type(
        &mut self,
        instantiated_class: &str,
        param_index: usize,
        arg_ty: &PhpType,
    ) {
        let Some((prop_name, declaring_class)) =
            self.classes.get(instantiated_class).and_then(|class_info| {
                class_info
                    .constructor_param_to_prop
                    .get(param_index)
                    .and_then(|mapped| mapped.as_ref())
                    .map(|prop_name| {
                        let declaring_class = class_info
                            .property_declaring_classes
                            .get(prop_name)
                            .cloned()
                            .unwrap_or_else(|| instantiated_class.to_string());
                        (prop_name.clone(), declaring_class)
                    })
            })
        else {
            return;
        };

        for class_info in self.classes.values_mut() {
            let shares_inherited_property = class_info
                .property_declaring_classes
                .get(&prop_name)
                .is_some_and(|owner| owner == &declaring_class);

            if !shares_inherited_property {
                continue;
            }

            if let Some(prop) = class_info
                .properties
                .iter_mut()
                .find(|(name, _)| name == &prop_name)
            {
                prop.1 = arg_ty.clone();
            }

            if let Some(sig) = class_info.methods.get_mut("__construct") {
                if let Some((_, param_ty)) = sig.params.get_mut(param_index) {
                    *param_ty = arg_ty.clone();
                }
            }
        }
    }

    pub(crate) fn normalize_pointer_target_type(&self, target_type: &str) -> Option<String> {
        match target_type {
            "int" | "integer" => Some("int".to_string()),
            "float" | "double" | "real" => Some("float".to_string()),
            "bool" | "boolean" => Some("bool".to_string()),
            "string" => Some("string".to_string()),
            "ptr" | "pointer" => Some("ptr".to_string()),
            class_name if self.classes.contains_key(class_name) => Some(class_name.to_string()),
            class_name if self.packed_classes.contains_key(class_name) => {
                Some(class_name.to_string())
            }
            class_name if self.extern_classes.contains_key(class_name) => {
                Some(class_name.to_string())
            }
            _ => None,
        }
    }

    pub(crate) fn resolve_type_expr(
        &self,
        type_expr: &crate::parser::ast::TypeExpr,
        span: crate::span::Span,
    ) -> Result<PhpType, CompileError> {
        match type_expr {
            crate::parser::ast::TypeExpr::Int => Ok(PhpType::Int),
            crate::parser::ast::TypeExpr::Float => Ok(PhpType::Float),
            crate::parser::ast::TypeExpr::Bool => Ok(PhpType::Bool),
            crate::parser::ast::TypeExpr::Str => Ok(PhpType::Str),
            crate::parser::ast::TypeExpr::Void => Ok(PhpType::Void),
            crate::parser::ast::TypeExpr::Nullable(inner) => {
                let inner_ty = self.resolve_type_expr(inner, span)?;
                Ok(self.normalize_union_type(vec![inner_ty, PhpType::Void]))
            }
            crate::parser::ast::TypeExpr::Union(members) => {
                let resolved = members
                    .iter()
                    .map(|member| self.resolve_type_expr(member, span))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(self.normalize_union_type(resolved))
            }
            crate::parser::ast::TypeExpr::Ptr(target) => {
                let normalized = match target {
                    Some(name) => self
                        .normalize_pointer_target_type(name.as_str())
                        .ok_or_else(|| {
                            CompileError::new(
                                span,
                                &format!("Unknown pointer target type: {}", name.as_str()),
                            )
                        })?,
                    None => return Ok(PhpType::Pointer(None)),
                };
                Ok(PhpType::Pointer(Some(normalized)))
            }
            crate::parser::ast::TypeExpr::Buffer(inner) => {
                let inner_ty = self.resolve_type_expr(inner, span)?;
                if packed_type_size(&inner_ty, &self.packed_classes).is_none() {
                    return Err(CompileError::new(
                        span,
                        "buffer<T> requires a POD scalar, pointer, or packed class element type",
                    ));
                }
                Ok(PhpType::Buffer(Box::new(inner_ty)))
            }
            crate::parser::ast::TypeExpr::Named(name) => match name.as_str() {
                "string" => Ok(PhpType::Str),
                "mixed" => Ok(PhpType::Mixed),
                "callable" => Ok(PhpType::Callable),
                "void" => Ok(PhpType::Void),
                "array" => Ok(PhpType::Array(Box::new(PhpType::Int))),
                _ if self.classes.contains_key(name.as_str())
                    || self.declared_classes.contains(name.as_str())
                    || self.interfaces.contains_key(name.as_str())
                    || self.declared_interfaces.contains(name.as_str())
                    || self.extern_classes.contains_key(name.as_str()) =>
                {
                    Ok(PhpType::Object(name.as_str().to_string()))
                }
                _ if self.packed_classes.contains_key(name.as_str()) => {
                    Ok(PhpType::Packed(name.as_str().to_string()))
                }
                _ => Err(CompileError::new(
                    span,
                    &format!("Unknown type: {}", name.as_str()),
                )),
            },
        }
    }

    pub(crate) fn extern_field_type(&self, class_name: &str, field_name: &str) -> Option<PhpType> {
        self.extern_classes.get(class_name).and_then(|class_info| {
            class_info
                .fields
                .iter()
                .find(|field| field.name == field_name)
                .map(|field| field.php_type.clone())
        })
    }

    pub(crate) fn packed_field_type(&self, class_name: &str, field_name: &str) -> Option<PhpType> {
        self.packed_classes.get(class_name).and_then(|class_info| {
            class_info
                .fields
                .iter()
                .find(|field| field.name == field_name)
                .map(|field| field.php_type.clone())
        })
    }

    pub(crate) fn ensure_pointer_type(
        &self,
        ty: &PhpType,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if Self::is_pointer_type(ty) {
            Ok(())
        } else {
            Err(CompileError::new(
                span,
                &format!("{} requires a pointer argument", context),
            ))
        }
    }

    pub(crate) fn ensure_word_pointer_value(
        &self,
        ty: &PhpType,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        if matches!(
            ty,
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Pointer(_)
        ) {
            Ok(())
        } else {
            Err(CompileError::new(
                span,
                "ptr_set() value must be int, bool, null, or pointer",
            ))
        }
    }
}
