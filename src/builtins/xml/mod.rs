//! Purpose:
//! AOT homes of the `ext/xml` registry builtins: `xml_parse_into_struct` (write-only
//! by-reference outputs need a checker hook) and the nine `xml_set_*_handler` setters
//! (unannotated handler closures need parameter hints). Every other `xml_*` /
//! `xmlwriter_*` name is an elephc-PHP declaration of `crate::xml_prelude`.
//!
//! Called from:
//! - `crate::builtins::registry` through `inventory`.
//! - `crate::optimize::reachability` for the prelude helpers the builtins compose.
//!
//! Key details:
//! - These builtins have no runtime routine of their own: their EIR lowering calls the
//!   prelude's `__elephc_xml_*` twins, which is why reachability must keep those alive
//!   whenever the builtin is called (`prelude_helpers_for`).
//! - `handler_closure_params` is re-exported for the handler-setter argument lowering, so
//!   the checker hook and codegen derive a handler closure's parameter list identically.

mod handler_setters;
pub(crate) use handler_setters::handler_closure_params;
pub mod xml_parse_into_struct;
pub mod xml_set_element_handler;
pub mod xml_set_character_data_handler;
pub mod xml_set_processing_instruction_handler;
pub mod xml_set_default_handler;
pub mod xml_set_unparsed_entity_decl_handler;
pub mod xml_set_notation_decl_handler;
pub mod xml_set_external_entity_ref_handler;
pub mod xml_set_start_namespace_decl_handler;
pub mod xml_set_end_namespace_decl_handler;

/// The prelude helpers `xml_parse_into_struct()`'s lowering calls, in call order.
pub(super) const PARSE_INTO_STRUCT_HELPERS: &[&str] = &[
    "__elephc_xml_parse_into_struct",
    "__elephc_xml_struct_values",
    "__elephc_xml_struct_index",
];

/// The nine handler setters with their prelude twin and handler parameter hints.
const SETTERS: &[(&str, &handler_setters::SetterSpec)] = &[
    ("xml_set_element_handler", &xml_set_element_handler::SPEC),
    ("xml_set_character_data_handler", &xml_set_character_data_handler::SPEC),
    ("xml_set_processing_instruction_handler", &xml_set_processing_instruction_handler::SPEC),
    ("xml_set_default_handler", &xml_set_default_handler::SPEC),
    ("xml_set_unparsed_entity_decl_handler", &xml_set_unparsed_entity_decl_handler::SPEC),
    ("xml_set_notation_decl_handler", &xml_set_notation_decl_handler::SPEC),
    ("xml_set_external_entity_ref_handler", &xml_set_external_entity_ref_handler::SPEC),
    ("xml_set_start_namespace_decl_handler", &xml_set_start_namespace_decl_handler::SPEC),
    ("xml_set_end_namespace_decl_handler", &xml_set_end_namespace_decl_handler::SPEC),
];

/// Returns the prelude functions the lowering of `builtin` (a canonical lowercase name)
/// calls directly, so declaration reachability can root them from the builtin call.
pub(crate) fn prelude_helpers_for(builtin: &str) -> &'static [&'static str] {
    if builtin == "xml_parse_into_struct" {
        return PARSE_INTO_STRUCT_HELPERS;
    }
    SETTERS
        .iter()
        .find(|(name, _)| *name == builtin)
        .map(|(_, spec)| std::slice::from_ref(&spec.helper))
        .unwrap_or(&[])
}

/// Returns the closure parameter hints of a handler setter's arguments after `$parser`
/// (empty for any other name), so argument lowering types an unannotated closure the way
/// the checker hook did.
pub(crate) fn handler_param_hints(builtin: &str) -> Vec<Vec<crate::types::PhpType>> {
    SETTERS
        .iter()
        .find(|(name, _)| *name == builtin)
        .map(|(_, spec)| spec.param_hints())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every xml registry contract composes prelude helpers, and unrelated names map to none.
    #[test]
    fn helper_table_covers_every_xml_registry_builtin() {
        let registry: Vec<&str> = elephc_builtin_contract::contracts()
            .iter()
            .filter(|contract| {
                contract.area == elephc_builtin_contract::Area::Xml
                    && contract.kind == elephc_builtin_contract::BuiltinKind::Function
            })
            .map(|contract| contract.name)
            .collect();
        assert_eq!(registry.len(), SETTERS.len() + 1);
        for name in registry {
            assert!(!prelude_helpers_for(name).is_empty(), "{name} must name its helpers");
            let hints = handler_param_hints(name);
            assert_eq!(hints.is_empty(), name == "xml_parse_into_struct", "{name}");
            for handler in hints {
                assert_eq!(
                    handler.first(),
                    Some(&crate::types::PhpType::Object("XMLParser".to_string()))
                );
            }
        }
        assert!(prelude_helpers_for("xml_parse").is_empty());
        assert!(handler_param_hints("xml_parse").is_empty());
    }

    /// The checker's eager argument pass must skip exactly the handler positions the
    /// setters type themselves, or an unannotated closure is checked unhinted first.
    #[test]
    fn contextual_callback_positions_match_the_handler_slots() {
        for (name, spec) in SETTERS {
            let positions =
                crate::types::checker::builtins::contextual_callback_arg_positions(name);
            let expected: Vec<usize> = (1..=spec.handlers.len()).collect();
            assert_eq!(positions, expected.as_slice(), "{name}");
        }
    }
}
