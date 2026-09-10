//! Purpose:
//! Loads stable native operation opcodes generated from the locked PHP extension surface.
//! Provides compiler phases with one case-aware function/method/property dispatch lookup.
//!
//! Called from:
//! - EIR internal-extension lowering, effects, Reflection, and native bridge request emission.
//!
//! Key details:
//! - Numeric IDs and manifest digest are checked in and ABI-versioned.
//! - Class/function/method names are case-insensitive; PHP property names remain case-sensitive.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

/// One stable native operation record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationSpec {
    pub opcode: u32,
    pub key: String,
    pub kind: String,
    pub extension: String,
    pub class: Option<String>,
    pub member: String,
    pub static_operation: bool,
    pub required_parameters: usize,
    pub parameter_count: usize,
}

/// Indexed, immutable operation metadata shared by compiler phases.
#[derive(Debug)]
pub struct OperationRegistry {
    pub abi_version: u32,
    pub manifest_sha256: String,
    operations: Vec<OperationSpec>,
    key_index: HashMap<String, usize>,
}

impl OperationRegistry {
    /// Parses and validates the generated operation manifest.
    fn parse(source: &str) -> Result<Self, String> {
        let root: Value =
            serde_json::from_str(source).map_err(|error| format!("invalid JSON: {error}"))?;
        let root = root
            .as_object()
            .ok_or_else(|| "operation manifest must be an object".to_string())?;
        let abi_version = unsigned(root.get("abi_version"), "abi_version")?;
        if abi_version != 1 {
            return Err(format!("unsupported DOM operation ABI version: {abi_version}"));
        }
        let first_public_opcode =
            u32::try_from(unsigned(root.get("first_public_opcode"), "first_public_opcode")?)
                .map_err(|_| "first_public_opcode exceeds u32".to_string())?;
        let manifest_sha256 = string(root.get("manifest_sha256"), "manifest_sha256")?.to_string();
        let values = root
            .get("operations")
            .and_then(Value::as_array)
            .ok_or_else(|| "operations must be an array".to_string())?;
        let mut operations = Vec::with_capacity(values.len());
        let mut key_index = HashMap::with_capacity(values.len());

        for (index, value) in values.iter().enumerate() {
            let path = format!("operations[{index}]");
            let value = value
                .as_object()
                .ok_or_else(|| format!("{path} must be an object"))?;
            let opcode = u32::try_from(unsigned(value.get("opcode"), &format!("{path}.opcode"))?)
                .map_err(|_| format!("{path}.opcode exceeds u32"))?;
            let expected_opcode = first_public_opcode
                .checked_add(u32::try_from(index).map_err(|_| "too many operations".to_string())?)
                .ok_or_else(|| "operation opcode range overflow".to_string())?;
            if opcode != expected_opcode {
                return Err(format!(
                    "{path}.opcode is {opcode}, expected contiguous {expected_opcode}"
                ));
            }
            let key = string(value.get("key"), &format!("{path}.key"))?.to_string();
            if key_index.insert(key.clone(), index).is_some() {
                return Err(format!("duplicate operation key: {key}"));
            }
            operations.push(OperationSpec {
                opcode,
                key,
                kind: string(value.get("kind"), &format!("{path}.kind"))?.to_string(),
                extension: string(value.get("extension"), &format!("{path}.extension"))?
                    .to_string(),
                class: optional_string(value.get("class"), &format!("{path}.class"))?,
                member: string(value.get("member"), &format!("{path}.member"))?.to_string(),
                static_operation: boolean(value.get("static"), &format!("{path}.static"))?,
                required_parameters: usize::try_from(unsigned(
                    value.get("required_parameters"),
                    &format!("{path}.required_parameters"),
                )?)
                .map_err(|_| format!("{path}.required_parameters exceeds usize"))?,
                parameter_count: usize::try_from(unsigned(
                    value.get("parameter_count"),
                    &format!("{path}.parameter_count"),
                )?)
                .map_err(|_| format!("{path}.parameter_count exceeds usize"))?,
            });
        }

        Ok(Self {
            abi_version: u32::try_from(abi_version)
                .map_err(|_| "abi_version exceeds u32".to_string())?,
            manifest_sha256,
            operations,
            key_index,
        })
    }

    /// Returns one native operation by its exact generated key.
    pub fn operation(&self, key: &str) -> Option<&OperationSpec> {
        self.key_index
            .get(key)
            .map(|index| &self.operations[*index])
    }

    /// Returns one native operation by its stable numeric opcode.
    pub fn opcode(&self, opcode: u32) -> Option<&OperationSpec> {
        self.operations
            .binary_search_by_key(&opcode, |operation| operation.opcode)
            .ok()
            .map(|index| &self.operations[index])
    }

    /// Returns the native operation for one case-insensitive internal function.
    pub fn function(&self, name: &str) -> Option<&OperationSpec> {
        self.operation(&format!(
            "function:{}",
            name.trim_start_matches('\\').to_ascii_lowercase()
        ))
    }

    /// Returns the native operation for one case-insensitive internal method.
    pub fn method(&self, class: &str, method: &str) -> Option<&OperationSpec> {
        let canonical = super::registry()
            .class(class)
            .map(|class| class.canonical_name.as_str())
            .unwrap_or(class);
        self.operation(&format!(
            "method:{}::{}",
            canonical.to_ascii_lowercase(),
            method.to_ascii_lowercase()
        ))
    }

    /// Returns the native get/set operation for one case-sensitive internal property.
    pub fn property(
        &self,
        class: &str,
        property: &str,
        write: bool,
    ) -> Option<&OperationSpec> {
        let canonical = super::registry()
            .class(class)
            .map(|class| class.canonical_name.as_str())
            .unwrap_or(class);
        let kind = if write { "property-set" } else { "property-get" };
        self.operation(&format!(
            "{kind}:{}::${property}",
            canonical.to_ascii_lowercase()
        ))
    }

    /// Returns one extension object-handler operation by case-insensitive names.
    pub fn object_handler(&self, extension: &str, handler: &str) -> Option<&OperationSpec> {
        self.operation(&format!(
            "object-handler:{}::{}",
            extension.to_ascii_lowercase(),
            handler.to_ascii_lowercase()
        ))
    }

    /// Iterates stable operations in numeric opcode order.
    pub fn operations(&self) -> impl Iterator<Item = &OperationSpec> {
        self.operations.iter()
    }
}

/// Returns the process-wide locked native operation registry.
pub fn operation_registry() -> &'static OperationRegistry {
    static REGISTRY: OnceLock<OperationRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        OperationRegistry::parse(include_str!(
            "../../tests/php_dom/surface/opcodes-php-8.5.8.json"
        ))
        .unwrap_or_else(|error| panic!("invalid locked DOM opcode manifest: {error}"))
    })
}

/// Cross-checks generated operation metadata and its representative lookup paths.
pub(crate) fn validate_locked_operation_surface() -> Result<(), String> {
    let registry = operation_registry();
    if registry.abi_version != 1 {
        return Err(format!(
            "expected ABI version 1, got {}",
            registry.abi_version
        ));
    }
    if registry.manifest_sha256
        != "d471b44820908f1ddda4194d89341a4ce2fd53ef408ced1b1fb1e9576592e41d"
    {
        return Err("opcode manifest SHA-256 differs from the generated Rust table".to_string());
    }
    if registry.operations().count() != 603 {
        return Err("locked opcode manifest must contain 603 operations".to_string());
    }
    for operation in registry.operations() {
        if operation.key.is_empty()
            || operation.kind.is_empty()
            || operation.extension.is_empty()
            || operation.member.is_empty()
            || operation.required_parameters > operation.parameter_count
        {
            return Err(format!("malformed operation metadata: {}", operation.key));
        }
        if matches!(
            operation.kind.as_str(),
            "method" | "property-get" | "property-set"
        ) && operation.class.is_none()
        {
            return Err(format!(
                "class member operation lacks a class: {}",
                operation.key
            ));
        }
        let _ = operation.static_operation;
    }
    if registry
        .operation("internal:bridge.wrapper.retain")
        .is_none()
        || registry.function("DOM_IMPORT_SIMPLEXML").is_none()
        || registry.method("Dom\\Element", "QUERYSELECTOR").is_none()
        || registry
            .property("Dom\\Node", "nodeName", false)
            .is_none()
        || registry
            .property("Dom\\Node", "nodeName", true)
            .is_some()
        || registry
            .property("Dom\\Node", "nodeValue", true)
            .is_none()
        || registry
            .property("Dom\\NamespaceInfo", "namespaceURI", false)
            .is_none()
        || registry
            .property("Dom\\NamespaceInfo", "namespaceURI", true)
            .is_some()
        || registry
            .object_handler("SimpleXML", "READ_PROPERTY")
            .is_none()
    {
        return Err("representative operation lookup is inconsistent".to_string());
    }
    Ok(())
}

/// Returns one required unsigned JSON scalar.
fn unsigned(value: Option<&Value>, path: &str) -> Result<u64, String> {
    value
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{path} must be an unsigned integer"))
}

/// Returns one required string JSON scalar.
fn string<'a>(value: Option<&'a Value>, path: &str) -> Result<&'a str, String> {
    value
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{path} must be a string"))
}

/// Returns one required boolean JSON scalar.
fn boolean(value: Option<&Value>, path: &str) -> Result<bool, String> {
    value
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{path} must be a boolean"))
}

/// Returns one nullable string JSON scalar.
fn optional_string(value: Option<&Value>, path: &str) -> Result<Option<String>, String> {
    match value {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(format!("{path} must be a string or null")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the checked-in manifest exposes one contiguous 603-operation ABI range.
    #[test]
    fn locked_operation_registry_has_contiguous_public_range() {
        let registry = operation_registry();
        let operations = registry.operations().collect::<Vec<_>>();
        assert_eq!(registry.abi_version, 1);
        assert_eq!(operations.len(), 603);
        assert_eq!(operations.first().map(|operation| operation.opcode), Some(4096));
        assert_eq!(operations.last().map(|operation| operation.opcode), Some(4698));
        assert_eq!(
            registry.opcode(4103).map(|operation| operation.key.as_str()),
            Some("function:libxml_set_external_entity_loader")
        );
    }

    /// Verifies aliases and mixed-case method/function calls share canonical native opcodes.
    #[test]
    fn locked_operation_registry_normalizes_php_callable_names() {
        assert_eq!(
            operation_registry()
                .function("DOM\\IMPORT_SIMPLEXML")
                .map(|operation| operation.opcode),
            Some(4096)
        );
        assert_eq!(
            operation_registry()
                .method("dom\\element", "QUERYSELECTOR")
                .map(|operation| operation.key.as_str()),
            Some("method:dom\\element::queryselector")
        );
    }

    /// Verifies semantic readonly properties omit setters while writable peers keep them.
    #[test]
    fn locked_operation_registry_omits_readonly_property_writes() {
        assert!(operation_registry()
            .property("Dom\\Node", "nodeName", false)
            .is_some());
        assert!(operation_registry()
            .property("Dom\\Node", "nodeName", true)
            .is_none());
        assert!(operation_registry()
            .property("Dom\\Node", "nodeValue", true)
            .is_some());
        assert!(operation_registry()
            .property("Dom\\NamespaceInfo", "namespaceURI", false)
            .is_some());
        assert!(operation_registry()
            .property("Dom\\NamespaceInfo", "namespaceURI", true)
            .is_none());
    }

    /// Verifies extension and handler spelling normalize to the locked handler key.
    #[test]
    fn locked_operation_registry_normalizes_object_handler_names() {
        assert_eq!(
            operation_registry()
                .object_handler("SimpleXML", "READ_PROPERTY")
                .map(|operation| operation.key.as_str()),
            Some("object-handler:simplexml::read_property")
        );
    }
}
