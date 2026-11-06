//! Purpose:
//! Keeps generic templates and in-progress class specializations in the persistent eval context.
//!
//! Called from:
//! - Interpreter declaration registration and lazy generic dispatch.
//!
//! Key details:
//! - Templates occupy names but are absent from PHP runtime class/function inventories.

use super::*;
use crate::eval_ir::EvalGenericTemplate;

impl ElephcEvalContext {
    /// Returns a class-like template under PHP's case-insensitive symbol key.
    pub(crate) fn generic_template(&self, name: &str) -> Option<&EvalGenericTemplate> {
        self.generic_templates.get(&format!("class:{}", name.trim_start_matches('\\').to_ascii_lowercase()))
    }

    /// Looks up a function template in PHP's separate function symbol namespace.
    pub(crate) fn generic_function_template(&self, name: &str) -> Option<&EvalGenericTemplate> {
        self.generic_templates.get(&format!("function:{}", name.trim_start_matches('\\').to_ascii_lowercase()))
    }

    /// Registers a template without publishing an unspecialized runtime declaration.
    pub(crate) fn define_generic_template(&mut self, template: EvalGenericTemplate) -> bool {
        let name = template.name.trim_start_matches('\\').to_ascii_lowercase();
        let is_function = matches!(template.prototype.as_ref(), crate::eval_ir::EvalStmt::FunctionDecl { .. });
        let key = format!("{}:{name}", if is_function { "function" } else { "class" });
        if self.generic_templates.contains_key(&key) || if is_function { self.function(&name).is_some() || self.native_function(&name).is_some() }
            else { self.resolve_class_like_name(&name).is_some() } {
            return false;
        }
        self.generic_templates.insert(key, template);
        true
    }

    /// Returns a cached method specialization within its declaring class.
    pub(crate) fn generic_method(&self, key: &str) -> Option<&crate::eval_ir::EvalClassMethod> {
        self.generic_methods.get(key)
    }

    /// Retains one ordinary concrete method for repeated calls.
    pub(crate) fn cache_generic_method(&mut self, key: String, method: crate::eval_ir::EvalClassMethod) {
        self.generic_methods.insert(key, method);
    }

    /// Removes a template whose declaration validation failed.
    pub(crate) fn remove_generic_template(&mut self, name: &str, is_function: bool) {
        self.generic_templates.remove(&format!("{}:{}", if is_function { "function" } else { "class" }, name.trim_start_matches('\\').to_ascii_lowercase()));
    }

    /// Marks a specialization in progress so recursive inheritance cannot recurse forever.
    pub(crate) fn begin_generic_specialization(&mut self, name: &str) -> bool {
        self.generic_specializations.insert(name.to_ascii_lowercase())
    }

    /// Clears the recursion guard after either successful or failed registration.
    pub(crate) fn end_generic_specialization(&mut self, name: &str) {
        self.generic_specializations.remove(&name.to_ascii_lowercase());
    }
}
