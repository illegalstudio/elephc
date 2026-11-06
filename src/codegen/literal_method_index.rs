//! Purpose:
//! Caches declared literal method parameter contracts without resolving emitted bodies.
//!
//! Called from:
//! - Instance/interface and late-static argument guards through shared codegen state.
//!
//! Key details:
//! - The module is immutable during emission; empty lookups are cached too.
//! - Entries retain declaring-class diagnostics and source/physical argument counts.

use std::collections::HashMap;
use std::rc::Rc;

use crate::ir::Module;
use crate::types::PhpType;

/// One runtime receiver's declared literal parameter contract.
pub(super) struct LiteralMethodCandidate {
    pub(super) class_id: u64,
    pub(super) class_name: String,
    pub(super) callable_name: String,
    pub(super) params: Vec<(String, PhpType)>,
}

/// Parameter-only index, shared by every function emitted for one module.
#[derive(Default)]
pub(super) struct LiteralMethodIndex {
    entries: HashMap<(String, bool, usize), Rc<Vec<LiteralMethodCandidate>>>,
}

/// Identifies contracts that require checking before ABI representation conversion.
pub(super) fn has_literal_parameters(params: &[(String, PhpType)]) -> bool {
    params.iter().any(|(_, ty)| matches!(ty, PhpType::False | PhpType::Void))
}

impl LiteralMethodIndex {
    /// Resolves each method/arity once, without scanning the emitted function inventory.
    pub(super) fn candidates(
        &mut self,
        module: &Module,
        method_key: &str,
        is_static: bool,
        argument_count: usize,
    ) -> Rc<Vec<LiteralMethodCandidate>> {
        let key = (method_key.to_string(), is_static, argument_count);
        if let Some(candidates) = self.entries.get(&key) {
            return Rc::clone(candidates);
        }
        let mut candidates = Vec::new();
        for (class_name, class) in &module.class_infos {
            let impl_name = if is_static {
                class.static_method_impl_classes.get(method_key)
            } else {
                class.method_impl_classes.get(method_key)
            }.unwrap_or(class_name);
            let Some(declaring_class) = module.class_infos.get(impl_name) else {
                continue;
            };
            let signature = if is_static {
                declaring_class.static_methods.get(method_key)
            } else {
                declaring_class.methods.get(method_key)
            };
            let Some(signature) = signature else { continue; };
            // Reject ordinary signatures before cloning or decoding any ABI adapter.
            if !has_literal_parameters(&signature.params) {
                continue;
            }
            let params = if signature.params.len() == argument_count {
                signature.params.clone()
            } else if !is_static {
                let Ok(source) =
                    crate::codegen_support::source_method_adapters::source_visible_signature(signature)
                else { continue; };
                if source.params.len() != argument_count {
                    continue;
                }
                source.params
            } else {
                continue;
            };
            if !has_literal_parameters(&params) {
                continue;
            }
            candidates.push(LiteralMethodCandidate {
                class_id: class.class_id,
                class_name: class_name.clone(),
                callable_name: format!("{impl_name}::{method_key}"),
                params,
            });
        }
        candidates.sort_by_key(|candidate| candidate.class_id);
        let candidates = Rc::new(candidates);
        self.entries.insert(key, Rc::clone(&candidates));
        candidates
    }
}
