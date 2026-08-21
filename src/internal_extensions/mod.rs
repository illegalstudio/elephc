//! Purpose:
//! Owns compiler metadata for PHP internal extensions whose classes require native object handlers.
//! Provides case-insensitive class/function lookup without representing internal APIs as userland source.
//!
//! Called from:
//! - Type-checker builtin schema initialization.
//! - Reflection/catalog, EIR lowering, effects, and bridge-requirement discovery.
//!
//! Key details:
//! - DOM/libxml/SimpleXML metadata is generated from the locked PHP 8.5.8 Reflection snapshot.
//! - The registry describes PHP-visible contracts only; bridge handles and helper operations remain hidden.

mod declarations;
mod dom;
mod loader;
mod operations;
mod schema;
mod types;

use std::collections::HashMap;
use std::sync::OnceLock;

pub use schema::{
    ClassConstantSpec, ClassSpec, ExtensionSpec, FunctionSpec, MethodSpec, ParameterSpec,
    PropertySpec, SignatureSpec,
};
pub(crate) use operations::{operation_registry, validate_locked_operation_surface};
pub(crate) use types::{
    function_signature_for, method_result_type_override, type_expr_from_name,
    value_expression, value_php_type,
};
pub(crate) use declarations::inject_checker_declarations;

/// Indexed, immutable metadata for all compiler-owned internal extensions.
#[derive(Debug)]
pub struct Registry {
    extensions: Vec<ExtensionSpec>,
    extension_index: HashMap<String, usize>,
    class_index: HashMap<String, (usize, usize)>,
    function_index: HashMap<String, (usize, usize)>,
    constant_index: HashMap<String, (usize, usize)>,
}

impl Registry {
    /// Builds lookup indexes for parsed extension metadata.
    fn new(extensions: Vec<ExtensionSpec>) -> Result<Self, String> {
        let mut extension_index = HashMap::new();
        let mut class_index = HashMap::new();
        let mut function_index = HashMap::new();
        let mut constant_index = HashMap::new();

        for (extension_offset, extension) in extensions.iter().enumerate() {
            let extension_key = php_symbol_key(&extension.name);
            if extension_index
                .insert(extension_key.clone(), extension_offset)
                .is_some()
            {
                return Err(format!(
                    "duplicate internal extension metadata: {}",
                    extension.name
                ));
            }

            for (class_offset, class) in extension.classes.iter().enumerate() {
                let key = php_symbol_key(&class.exported_name);
                if class_index
                    .insert(key.clone(), (extension_offset, class_offset))
                    .is_some()
                {
                    return Err(format!(
                        "duplicate internal class export: {}",
                        class.exported_name
                    ));
                }
            }

            for (function_offset, function) in extension.functions.iter().enumerate() {
                let key = php_symbol_key(&function.exported_name);
                if function_index
                    .insert(key.clone(), (extension_offset, function_offset))
                    .is_some()
                {
                    return Err(format!(
                        "duplicate internal function export: {}",
                        function.exported_name
                    ));
                }
            }

            for (constant_offset, (name, _)) in extension.constants.iter().enumerate() {
                if constant_index
                    .insert(name.clone(), (extension_offset, constant_offset))
                    .is_some()
                {
                    return Err(format!("duplicate internal extension constant: {name}"));
                }
            }
        }

        Ok(Self {
            extensions,
            extension_index,
            class_index,
            function_index,
            constant_index,
        })
    }

    /// Returns one internal extension by case-insensitive PHP name.
    pub fn extension(&self, name: &str) -> Option<&ExtensionSpec> {
        self.extension_index
            .get(&php_symbol_key(name))
            .map(|offset| &self.extensions[*offset])
    }

    /// Returns one exported internal class or alias by case-insensitive PHP name.
    pub fn class(&self, name: &str) -> Option<&ClassSpec> {
        let (extension_offset, class_offset) =
            *self.class_index.get(&php_symbol_key(name))?;
        Some(&self.extensions[extension_offset].classes[class_offset])
    }

    /// Returns whether php-src exposes a native write handler for one direct property.
    pub(crate) fn property_is_writable(
        &self,
        declaring_class: &str,
        property: &str,
    ) -> Option<bool> {
        self.class(declaring_class)?
            .properties
            .iter()
            .find(|candidate| candidate.name == property)
            .map(|candidate| candidate.writable)
    }

    /// Returns one exported internal function by case-insensitive PHP name.
    pub fn function(&self, name: &str) -> Option<&FunctionSpec> {
        let (extension_offset, function_offset) =
            *self.function_index.get(&php_symbol_key(name))?;
        Some(&self.extensions[extension_offset].functions[function_offset])
    }

    /// Returns one exact, case-sensitive internal extension constant value.
    pub fn constant(&self, name: &str) -> Option<&serde_json::Value> {
        let (extension_offset, constant_offset) = *self.constant_index.get(name)?;
        Some(&self.extensions[extension_offset].constants[constant_offset].1)
    }

    /// Iterates internal extensions in the locked Reflection declaration order.
    pub fn extensions(&self) -> impl Iterator<Item = &ExtensionSpec> {
        self.extensions.iter()
    }

    /// Iterates exported internal classes and aliases in locked declaration order.
    pub fn classes(&self) -> impl Iterator<Item = &ClassSpec> {
        self.extensions
            .iter()
            .flat_map(|extension| extension.classes.iter())
    }

    /// Iterates exported internal function names in locked declaration order.
    pub fn function_names(&'static self) -> impl Iterator<Item = &'static str> {
        self.extensions
            .iter()
            .flat_map(|extension| extension.functions.iter())
            .map(|function| function.exported_name.as_str())
    }

    /// Iterates exact internal extension constant names in locked declaration order.
    pub fn constant_names(&'static self) -> impl Iterator<Item = &'static str> {
        self.extensions
            .iter()
            .flat_map(|extension| extension.constants.iter())
            .map(|(name, _)| name.as_str())
    }
}

/// Returns the process-wide immutable internal-extension registry.
pub fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let extensions = loader::parse_surface(dom::PHP_8_5_8_SURFACE)
            .unwrap_or_else(|error| panic!("invalid locked internal-extension surface: {error}"));
        Registry::new(extensions)
            .unwrap_or_else(|error| panic!("invalid internal-extension registry: {error}"))
    })
}

/// Returns true when a canonical or exported internal class is backed by a native bridge handle.
pub(crate) fn is_native_wrapper_class(name: &str) -> bool {
    registry().class(name).is_some_and(|class| {
        !class.interface
            && !class.enum_type
            && class.canonical_name != "DOMException"
            && !matches!(
                class.canonical_name.as_str(),
                "LibXMLError" | "Dom\\NamespaceInfo"
            )
            && matches!(class.extension.as_str(), "dom" | "libxml" | "SimpleXML")
    })
}

/// Returns true when a native operation materializes an ordinary PHP value object.
pub(crate) fn is_native_value_object_class(name: &str) -> bool {
    registry().class(name).is_some_and(|class| {
        matches!(
            class.canonical_name.as_str(),
            "LibXMLError" | "Dom\\NamespaceInfo"
        )
    })
}

/// Number of 16-byte compiler-hidden slots appended to every native wrapper object.
pub(crate) const NATIVE_WRAPPER_HIDDEN_SLOTS: usize = 6;

/// Byte offset of one php-src-compatible strong auxiliary wrapper owner.
///
/// XPath node lists use it for their eager member array, while legacy namespace
/// nodes use it for the parent element wrapper owned by the fake declaration.
pub(crate) const NATIVE_WRAPPER_AUX_OWNER_OFFSET: usize = 24;

/// Byte offset, relative to the hidden metadata base, of a SimpleXML iterator's
/// strong current wrapper.
///
/// The slot is zero for every other native wrapper. Keeping it outside declared
/// properties prevents Reflection and serialization from exposing php-src's
/// private `sxe->iter.data` owner.
pub(crate) const NATIVE_WRAPPER_ITERATOR_CURRENT_OFFSET: usize = 80;

/// Byte offset of the mutation epoch paired with the hidden SimpleXML current owner.
///
/// Re-entrant `next()`, `rewind()`, or constructor calls increment this value.
/// An outer iterator move snapshots it before releasing the old wrapper and
/// abandons its stale move when userland changed the iterator during destruction.
pub(crate) const NATIVE_WRAPPER_ITERATOR_EPOCH_OFFSET: usize = 88;

/// High-bit marker used by the DOM bridge to distinguish userland wrapper class identifiers.
pub(crate) const DOM_USER_WRAPPER_MARKER: u64 = 1 << 63;

/// Legacy DOM wrapper classes keyed by `100 + libxml2 node type`.
pub(crate) const LEGACY_DOM_WRAPPER_KINDS: &[(u64, &str)] = &[
    (101, "DOMElement"),
    (102, "DOMAttr"),
    (103, "DOMText"),
    (104, "DOMCdataSection"),
    (105, "DOMEntityReference"),
    (107, "DOMProcessingInstruction"),
    (108, "DOMComment"),
    (109, "DOMDocument"),
    (110, "DOMDocumentType"),
    (111, "DOMDocumentFragment"),
    (112, "DOMNotation"),
    (114, "DOMDocumentType"),
    (115, "DOMEntity"),
    (117, "DOMEntity"),
    (118, "DOMNameSpaceNode"),
];

/// Modern XML/HTML wrapper classes keyed by family base plus libxml2 node type.
pub(crate) const MODERN_DOM_WRAPPER_KINDS: &[(u64, &str)] = &[
    (201, "Dom\\Element"),
    (202, "Dom\\Attr"),
    (203, "Dom\\Text"),
    (204, "Dom\\CDATASection"),
    (205, "Dom\\EntityReference"),
    (207, "Dom\\ProcessingInstruction"),
    (208, "Dom\\Comment"),
    (209, "Dom\\XMLDocument"),
    (210, "Dom\\DocumentType"),
    (211, "Dom\\DocumentFragment"),
    (212, "Dom\\Notation"),
    (214, "Dom\\DocumentType"),
    (215, "Dom\\Entity"),
    (217, "Dom\\Entity"),
    (301, "Dom\\HTMLElement"),
    (302, "Dom\\Attr"),
    (303, "Dom\\Text"),
    (304, "Dom\\CDATASection"),
    (305, "Dom\\EntityReference"),
    (307, "Dom\\ProcessingInstruction"),
    (308, "Dom\\Comment"),
    (310, "Dom\\DocumentType"),
    (311, "Dom\\DocumentFragment"),
    (312, "Dom\\Notation"),
    (313, "Dom\\HTMLDocument"),
    (314, "Dom\\DocumentType"),
    (315, "Dom\\Entity"),
    (317, "Dom\\Entity"),
];

/// Returns whether a class is a native wrapper or inherits from one through compiler class metadata.
///
/// The metadata-count bound guarantees malformed or cyclic parent graphs terminate.
pub(crate) fn is_native_wrapper_descendant(
    class_infos: &HashMap<String, crate::types::ClassInfo>,
    name: &str,
) -> bool {
    follows_native_wrapper_parent(name, class_infos.len().saturating_add(1), |current| {
        class_infos
            .get(current)
            .and_then(|class_info| class_info.parent.clone())
    })
}

/// Walks a bounded compiler parent chain until it reaches a native wrapper root.
fn follows_native_wrapper_parent(
    name: &str,
    max_hops: usize,
    mut parent: impl FnMut(&str) -> Option<String>,
) -> bool {
    let mut current = name.to_string();
    for _ in 0..max_hops {
        if is_native_wrapper_class(&current) {
            return true;
        }
        let Some(next) = parent(&current) else {
            return false;
        };
        current = next;
    }
    false
}

/// Returns the compiler-hidden slot count for a direct native wrapper or one of its descendants.
pub(crate) fn hidden_slot_count_for(
    class_infos: &HashMap<String, crate::types::ClassInfo>,
    name: &str,
) -> usize {
    if is_native_wrapper_descendant(class_infos, name) {
        NATIVE_WRAPPER_HIDDEN_SLOTS
    } else {
        0
    }
}

/// Validates that the locked Reflection and opcode snapshots remain mutually coherent.
pub(crate) fn validate_locked_snapshots() {
    let registry = registry();
    for expected in ["dom", "libxml", "simplexml"] {
        assert!(
            registry.extension(expected).is_some(),
            "locked internal extension is absent: {expected}"
        );
    }
    assert_eq!(
        registry.extensions().count(),
        3,
        "unexpected locked internal extension count"
    );
    validate_locked_operation_surface()
        .unwrap_or_else(|error| panic!("invalid locked DOM operation surface: {error}"));
}

/// Normalizes PHP symbols for case-insensitive registry lookup.
fn php_symbol_key(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies direct native wrappers are recognized without compiler class metadata.
    #[test]
    fn native_wrapper_descendant_accepts_direct_wrapper() {
        let class_infos = HashMap::new();
        assert!(is_native_wrapper_descendant(
            &class_infos,
            "Dom\\Element"
        ));
        assert!(is_native_wrapper_descendant(
            &class_infos,
            "SimpleXMLElement"
        ));
        assert_eq!(
            hidden_slot_count_for(&class_infos, "Dom\\Element"),
            NATIVE_WRAPPER_HIDDEN_SLOTS
        );
    }

    /// Verifies one userland inheritance hop preserves native wrapper hidden slots.
    #[test]
    fn native_wrapper_descendant_accepts_child() {
        let parents =
            HashMap::from([("CustomElement".to_string(), "Dom\\Element".to_string())]);
        assert!(follows_native_wrapper_parent("CustomElement", 2, |name| {
            parents.get(name).cloned()
        }));
    }

    /// Verifies native wrapper ancestry is retained across multiple userland generations.
    #[test]
    fn native_wrapper_descendant_accepts_grandchild() {
        let parents = HashMap::from([
            (
                "CustomElement".to_string(),
                "Dom\\Element".to_string(),
            ),
            (
                "SpecialElement".to_string(),
                "CustomElement".to_string(),
            ),
        ]);
        assert!(follows_native_wrapper_parent("SpecialElement", 3, |name| {
            parents.get(name).cloned()
        }));
    }

    /// Verifies cyclic and unresolved parent graphs terminate without claiming native storage.
    #[test]
    fn native_wrapper_descendant_rejects_cycle_and_unknown_parent() {
        let parents = HashMap::from([
            ("CycleA".to_string(), "CycleB".to_string()),
            ("CycleB".to_string(), "CycleA".to_string()),
            ("UnknownChild".to_string(), "MissingParent".to_string()),
        ]);
        assert!(!follows_native_wrapper_parent("CycleA", 3, |name| {
            parents.get(name).cloned()
        }));
        assert!(!follows_native_wrapper_parent(
            "UnknownChild",
            4,
            |name| parents.get(name).cloned(),
        ));
    }

    /// Verifies the locked registry exposes all three companion extensions.
    #[test]
    fn locked_registry_contains_dom_dependency_closure() {
        let registry = registry();
        assert!(registry.extension("dom").is_some());
        assert!(registry.extension("LIBXML").is_some());
        assert!(registry.extension("SimpleXML").is_some());
    }

    /// Verifies class and function names follow PHP's case-insensitive lookup.
    #[test]
    fn locked_registry_looks_up_exported_symbols_case_insensitively() {
        let registry = registry();
        assert_eq!(
            registry
                .class("\\dOm\\hTmLdOcUmEnT")
                .expect("modern HTML document")
                .canonical_name,
            "Dom\\HTMLDocument"
        );
        assert_eq!(
            registry
                .function("DOM\\IMPORT_SIMPLEXML")
                .expect("modern import function")
                .signature
                .parameters
                .len(),
            1
        );
    }

    /// Verifies stub semantics distinguish virtual readonly properties from writable handlers.
    #[test]
    fn locked_registry_preserves_virtual_property_write_semantics() {
        let registry = registry();
        assert_eq!(
            registry.property_is_writable("Dom\\Node", "nodeName"),
            Some(false)
        );
        assert_eq!(
            registry.property_is_writable("Dom\\Node", "nodeValue"),
            Some(true)
        );
        assert_eq!(
            registry.property_is_writable("LibXMLError", "message"),
            Some(true)
        );
    }

    /// Verifies the exception alias resolves to PHP's canonical global class.
    #[test]
    fn locked_registry_preserves_dom_exception_alias() {
        let alias = registry()
            .class("Dom\\DOMException")
            .expect("DOM exception alias");
        assert_eq!(alias.canonical_name, "DOMException");
    }

    /// Verifies native-materialized value objects use ordinary PHP property slots.
    #[test]
    fn native_materialized_value_objects_are_not_bridge_wrappers() {
        let class_infos = HashMap::new();
        assert!(!is_native_wrapper_class("LibXMLError"));
        assert!(is_native_value_object_class("\\libxmlerror"));
        assert_eq!(hidden_slot_count_for(&class_infos, "LibXMLError"), 0);
        assert!(!is_native_wrapper_class("Dom\\NamespaceInfo"));
        assert!(is_native_value_object_class("\\dom\\namespaceinfo"));
        assert_eq!(
            hidden_slot_count_for(&class_infos, "Dom\\NamespaceInfo"),
            0
        );
    }

    /// Verifies high-level DOM surface counts against the locked oracle.
    #[test]
    fn locked_registry_matches_dom_surface_counts() {
        let dom = registry().extension("dom").expect("DOM extension");
        let canonical: std::collections::HashSet<&str> = dom
            .classes
            .iter()
            .map(|class| class.canonical_name.as_str())
            .collect();
        assert_eq!(dom.classes.len(), 51);
        assert_eq!(canonical.len(), 50);
        assert_eq!(
            dom.classes
                .iter()
                .map(|class| class.methods.len())
                .sum::<usize>(),
            313
        );
        assert_eq!(dom.functions.len(), 2);
        assert_eq!(dom.constants.len(), 61);
    }

    /// Verifies global constants retain PHP's case-sensitive lookup semantics.
    #[test]
    fn locked_registry_looks_up_constants_case_sensitively() {
        let registry = registry();
        assert_eq!(
            registry
                .constant("LIBXML_VERSION")
                .and_then(serde_json::Value::as_i64),
            Some(21_503)
        );
        assert!(registry.constant("libxml_version").is_none());
    }
}
