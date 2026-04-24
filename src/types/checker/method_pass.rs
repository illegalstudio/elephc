use crate::errors::CompileError;
use crate::parser::ast::ClassMethod;
use crate::types::{traits::FlattenedClass, PhpType, TypeEnv};

use super::Checker;

impl Checker {
    pub(super) fn type_check_methods_until_stable(
        &mut self,
        flattened_classes: &[FlattenedClass],
        global_env: &TypeEnv,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        let mut method_passes_remaining = (flattened_classes.len().max(1) * 2) + 1;
        loop {
            let classes_before_pass = self.classes.clone();
            let mut pass_errors = Vec::new();

            for class in flattened_classes {
                for method in &class.methods {
                    if method.is_abstract {
                        continue;
                    }
                    let mut method_env: TypeEnv = global_env.clone();
                    if !method.is_static {
                        method_env.insert("this".to_string(), PhpType::Object(class.name.clone()));
                    }
                    let sig_params = if method.is_static {
                        self.classes
                            .get(&class.name)
                            .and_then(|c| c.static_methods.get(&method.name))
                            .map(|s| s.params.clone())
                    } else {
                        self.classes
                            .get(&class.name)
                            .and_then(|c| c.methods.get(&method.name))
                            .map(|s| s.params.clone())
                    };
                    for (i, (pname, type_ann, _, _)) in method.params.iter().enumerate() {
                        let ty = if let Some(type_ann) = type_ann {
                            self.resolve_declared_param_type_hint(
                                type_ann,
                                method.span,
                                &format!("Method parameter ${}", pname),
                            )?
                        } else {
                            sig_params
                                .as_ref()
                                .and_then(|p| p.get(i))
                                .map(|(_, t)| t.clone())
                                .unwrap_or(PhpType::Int)
                        };
                        method_env.insert(pname.clone(), ty);
                    }
                    if let Some(variadic_name) = &method.variadic {
                        let ty = sig_params
                            .as_ref()
                            .and_then(|p| p.get(method.params.len()))
                            .map(|(_, t)| t.clone())
                            .unwrap_or(PhpType::Array(Box::new(PhpType::Int)));
                        method_env.insert(variadic_name.clone(), ty);
                    }
                    if method.name == "__construct" {
                        self.patch_constructor_method_env(class, method, &mut method_env);
                    }

                    self.current_class = Some(class.name.clone());
                    self.current_method = Some(method.name.clone());
                    self.current_method_is_static = method.is_static;
                    let method_ref_params: Vec<String> = method
                        .params
                        .iter()
                        .filter(|(_, _, _, is_ref)| *is_ref)
                        .map(|(name, _, _, _)| name.clone())
                        .collect();
                    let mut method_errors = Vec::new();
                    self.with_local_storage_context(method_ref_params, |checker| {
                        for s in &method.body {
                            if let Err(error) = checker.check_stmt(s, &mut method_env) {
                                method_errors.extend(error.flatten());
                            }
                        }
                        Ok(())
                    })?;
                    let method_has_errors = !method_errors.is_empty();
                    pass_errors.extend(method_errors);

                    if !method_has_errors {
                        self.update_method_return_type(class, method, &method_env, &mut pass_errors);
                    }
                    self.current_class = None;
                    self.current_method = None;
                    self.current_method_is_static = false;
                }
            }

            let stabilized = self.classes == classes_before_pass;
            let out_of_passes = method_passes_remaining == 0;
            if stabilized || out_of_passes {
                errors.extend(pass_errors);
                break;
            }

            method_passes_remaining -= 1;
        }
        Ok(())
    }

    fn patch_constructor_method_env(
        &mut self,
        class: &FlattenedClass,
        method: &ClassMethod,
        method_env: &mut TypeEnv,
    ) {
        if let Some(ci) = self.classes.get(&class.name).cloned() {
            for (i, (pname, type_ann, _, _)) in method.params.iter().enumerate() {
                if type_ann.is_some() {
                    continue;
                }
                if let Some(Some(prop_name)) = ci.constructor_param_to_prop.get(i) {
                    if ci.declared_properties.contains(prop_name) {
                        continue;
                    }
                    if let Some((_, ty)) = ci.properties.iter().find(|(n, _)| n == prop_name) {
                        method_env.insert(pname.clone(), ty.clone());
                        if let Some(ci_mut) = self.classes.get_mut(&class.name) {
                            if let Some(sig) = ci_mut.methods.get_mut("__construct") {
                                if i < sig.params.len() {
                                    sig.params[i].1 = ty.clone();
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn update_method_return_type(
        &mut self,
        class: &FlattenedClass,
        method: &ClassMethod,
        method_env: &TypeEnv,
        pass_errors: &mut Vec<CompileError>,
    ) {
        let inferred_return = self
            .find_return_type_in_body(&method.body, method_env)
            .unwrap_or(PhpType::Void);
        let effective_return = if let Some(type_ann) = method.return_type.as_ref() {
            match self.resolve_declared_return_type_hint(
                type_ann,
                method.span,
                &format!("Method '{}::{}'", class.name, method.name),
            ) {
                Ok(declared) => {
                    if let Err(error) = self.require_compatible_arg_type(
                        &declared,
                        &inferred_return,
                        method.span,
                        &format!("Method '{}::{}' return type", class.name, method.name),
                    ) {
                        pass_errors.extend(error.flatten());
                        self.current_class = None;
                        self.current_method = None;
                        self.current_method_is_static = false;
                        return;
                    }
                    if Self::is_generic_array_hint(&declared)
                        && matches!(inferred_return, PhpType::Array(_) | PhpType::AssocArray { .. })
                    {
                        inferred_return
                    } else {
                        declared
                    }
                }
                Err(error) => {
                    pass_errors.extend(error.flatten());
                    self.current_class = None;
                    self.current_method = None;
                    self.current_method_is_static = false;
                    return;
                }
            }
        } else {
            inferred_return
        };
        if !method.is_static {
            if let Some(ci) = self.classes.get_mut(&class.name) {
                if let Some(sig) = ci.methods.get_mut(&method.name) {
                    sig.return_type = effective_return;
                }
            }
        } else if let Some(ci) = self.classes.get_mut(&class.name) {
            if let Some(sig) = ci.static_methods.get_mut(&method.name) {
                sig.return_type = effective_return;
            }
        }
    }
}
