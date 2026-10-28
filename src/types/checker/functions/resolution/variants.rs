//! Purpose:
//! Publishes generic include-dispatch groups as templates for call-site inference.
//!
//! Called from:
//! - The checker driver's function-declaration collection phase.
//!
//! Key details:
//! - The public template borrows the variants' shared generic signature.
//! - Concrete dispatch groups are checked only after their type arguments are bound.

use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{TypeExpr, TypeParam};
use crate::types::checker::Checker;

impl Checker {
    /// Registers the shared template of each generic include group without resolving its types.
    pub(in crate::types::checker) fn collect_generic_function_variant_templates(
        &mut self,
        errors: &mut Vec<CompileError>,
    ) {
        for (name, variants) in self.function_variant_groups.clone() {
            let Some(template) = variants.iter().filter_map(|variant| self.fn_decls.get(variant))
                .find(|decl| !decl.type_params.is_empty()).cloned()
            else { continue };
            if variants.iter().any(|variant| self.fn_decls.get(variant)
                .is_none_or(|decl| !equivalent_parameters(&decl.type_params, &template.type_params)))
            {
                let error = CompileError::new(template.span, &format!(
                    "Generic function variants for '{}' must declare matching type parameter contracts",
                    name,
                ));
                self.failed_variant_contracts.insert(name.clone(), error.clone());
                errors.push(error);
            }
            self.fn_decls.insert(name, template);
        }
    }
}

/// Compares bounds, defaults and variance after renaming parameters by declaration position.
fn equivalent_parameters(params: &[TypeParam], template: &[TypeParam]) -> bool {
    if params.len() != template.len() { return false; }
    let renames: crate::generics::Bindings = params.iter().zip(template).map(|(param, canonical)| {
        (param.name.clone(), TypeExpr::Named(Name::unqualified(&canonical.name)))
    }).collect();
    params.iter().zip(template).all(|(param, canonical)| {
        param.variance == canonical.variance
            && param.bound.as_ref().map(|ty| ty.substitute_type_params(&renames)) == canonical.bound
            && param.default.as_ref().map(|ty| ty.substitute_type_params(&renames)) == canonical.default
    })
}
