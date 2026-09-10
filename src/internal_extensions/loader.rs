//! Purpose:
//! Parses the locked Reflection JSON into typed internal-extension metadata.
//! Rejects missing fields and incompatible snapshot versions before compiler phases consume the registry.
//!
//! Called from:
//! - `crate::internal_extensions::registry()`.
//!
//! Key details:
//! - Parsing happens once through `OnceLock`; normal compilation performs indexed lookups afterward.
//! - Errors name the missing/invalid field so a corrupt generated manifest never becomes partial metadata.

use serde_json::{Map, Value};

use super::schema::{
    AttributeSpec, ClassConstantSpec, ClassSpec, ExtensionSpec, FunctionSpec, MethodSpec,
    ParameterSpec, PropertySpec, SignatureSpec,
};

/// Parses the complete locked surface and returns its extension records.
pub(super) fn parse_surface(source: &str) -> Result<Vec<ExtensionSpec>, String> {
    let root: Value =
        serde_json::from_str(source).map_err(|error| format!("invalid JSON: {error}"))?;
    let root = object(&root, "surface")?;
    if unsigned(field(root, "schema")?, "surface.schema")? != 2 {
        return Err("unsupported internal-extension surface schema".to_string());
    }
    if string(field(root, "php_version")?, "surface.php_version")? != super::dom::PHP_VERSION {
        return Err("internal-extension PHP version mismatch".to_string());
    }
    if unsigned(field(root, "libxml_version")?, "surface.libxml_version")?
        != u64::from(super::dom::LIBXML_VERSION)
    {
        return Err("internal-extension libxml version mismatch".to_string());
    }

    array(field(root, "extensions")?, "surface.extensions")?
        .iter()
        .enumerate()
        .map(|(index, value)| parse_extension(value, &format!("extensions[{index}]")))
        .collect()
}

/// Parses one complete extension record.
fn parse_extension(value: &Value, path: &str) -> Result<ExtensionSpec, String> {
    let value = object(value, path)?;
    let classes = parse_array(value, "classes", path, parse_class)?;
    let functions = parse_array(value, "functions", path, parse_function)?;
    let constants = array(field(value, "constants")?, &format!("{path}.constants"))?
        .iter()
        .enumerate()
        .map(|(index, constant)| {
            let constant_path = format!("{path}.constants[{index}]");
            let constant = object(constant, &constant_path)?;
            Ok((
                owned_string(field(constant, "name")?, &format!("{constant_path}.name"))?,
                field(constant, "value")?.clone(),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(ExtensionSpec {
        name: owned_string(field(value, "name")?, &format!("{path}.name"))?,
        version: optional_string(field(value, "version")?, &format!("{path}.version"))?,
        classes,
        functions,
        constants,
    })
}

/// Parses one exported class record.
fn parse_class(value: &Value, path: &str) -> Result<ClassSpec, String> {
    let value = object(value, path)?;
    Ok(ClassSpec {
        exported_name: required_string(value, "exported_name", path)?,
        canonical_name: required_string(value, "canonical_name", path)?,
        extension: required_string(value, "extension", path)?,
        internal: required_bool(value, "internal", path)?,
        interface: required_bool(value, "interface", path)?,
        trait_type: required_bool(value, "trait", path)?,
        enum_type: required_bool(value, "enum", path)?,
        abstract_class: required_bool(value, "abstract", path)?,
        final_class: required_bool(value, "final", path)?,
        readonly_class: required_bool(value, "readonly", path)?,
        instantiable: required_bool(value, "instantiable", path)?,
        cloneable: required_bool(value, "cloneable", path)?,
        parent: optional_string(field(value, "parent")?, &format!("{path}.parent"))?,
        interfaces: parse_strings(value, "interfaces", path)?,
        methods: parse_array(value, "methods", path, parse_method)?,
        properties: parse_array(value, "properties", path, parse_property)?,
        constants: parse_array(value, "constants", path, parse_class_constant)?,
        attributes: parse_array(value, "attributes", path, parse_attribute)?,
    })
}

/// Parses one exported function record.
fn parse_function(value: &Value, path: &str) -> Result<FunctionSpec, String> {
    let value = object(value, path)?;
    Ok(FunctionSpec {
        exported_name: required_string(value, "exported_name", path)?,
        signature: parse_signature(value, path)?,
    })
}

/// Parses one method record.
fn parse_method(value: &Value, path: &str) -> Result<MethodSpec, String> {
    let value = object(value, path)?;
    Ok(MethodSpec {
        signature: parse_signature(value, path)?,
        declaring_class: required_string(value, "declaring_class", path)?,
        public: required_bool(value, "public", path)?,
        protected: required_bool(value, "protected", path)?,
        private: required_bool(value, "private", path)?,
        static_method: required_bool(value, "static", path)?,
        abstract_method: required_bool(value, "abstract", path)?,
        final_method: required_bool(value, "final", path)?,
        constructor: required_bool(value, "constructor", path)?,
        destructor: required_bool(value, "destructor", path)?,
    })
}

/// Parses the callable fields shared by methods and functions.
fn parse_signature(value: &Map<String, Value>, path: &str) -> Result<SignatureSpec, String> {
    Ok(SignatureSpec {
        name: required_string(value, "name", path)?,
        internal: required_bool(value, "internal", path)?,
        deprecated: required_bool(value, "deprecated", path)?,
        returns_reference: required_bool(value, "returns_reference", path)?,
        required_parameters: usize::try_from(unsigned(
            field(value, "required_parameters")?,
            &format!("{path}.required_parameters"),
        )?)
        .map_err(|_| format!("{path}.required_parameters exceeds usize"))?,
        parameters: parse_array(value, "parameters", path, parse_parameter)?,
        return_type: optional_string(
            field(value, "return_type")?,
            &format!("{path}.return_type"),
        )?,
        tentative_return_type: optional_string(
            field(value, "tentative_return_type")?,
            &format!("{path}.tentative_return_type"),
        )?,
        attributes: parse_array(value, "attributes", path, parse_attribute)?,
    })
}

/// Parses one callable parameter record.
fn parse_parameter(value: &Value, path: &str) -> Result<ParameterSpec, String> {
    let value = object(value, path)?;
    Ok(ParameterSpec {
        name: required_string(value, "name", path)?,
        position: usize::try_from(unsigned(
            field(value, "position")?,
            &format!("{path}.position"),
        )?)
        .map_err(|_| format!("{path}.position exceeds usize"))?,
        php_type: optional_string(field(value, "type")?, &format!("{path}.type"))?,
        optional: required_bool(value, "optional", path)?,
        variadic: required_bool(value, "variadic", path)?,
        by_reference: required_bool(value, "by_reference", path)?,
        can_be_passed_by_value: required_bool(value, "can_be_passed_by_value", path)?,
        allows_null: required_bool(value, "allows_null", path)?,
        default: optional_value(field(value, "default")?),
        attributes: parse_array(value, "attributes", path, parse_attribute)?,
    })
}

/// Parses one property record.
fn parse_property(value: &Value, path: &str) -> Result<PropertySpec, String> {
    let value = object(value, path)?;
    let hooks = parse_hooks(field(value, "hooks")?, &format!("{path}.hooks"))?;

    Ok(PropertySpec {
        name: required_string(value, "name", path)?,
        declaring_class: required_string(value, "declaring_class", path)?,
        php_type: optional_string(field(value, "type")?, &format!("{path}.type"))?,
        public: required_bool(value, "public", path)?,
        protected: required_bool(value, "protected", path)?,
        private: required_bool(value, "private", path)?,
        static_property: required_bool(value, "static", path)?,
        writable: required_bool(value, "writable", path)?,
        readonly: required_bool(value, "readonly", path)?,
        virtual_property: required_bool(value, "virtual", path)?,
        deprecated: required_bool(value, "deprecated", path)?,
        has_default: required_bool(value, "has_default", path)?,
        default: optional_value(field(value, "default")?),
        hooks,
        attributes: parse_array(value, "attributes", path, parse_attribute)?,
    })
}

/// Parses the property-hook map, accepting PHP's empty-array JSON representation.
fn parse_hooks(value: &Value, path: &str) -> Result<Vec<(String, MethodSpec)>, String> {
    if value.as_array().is_some_and(Vec::is_empty) {
        return Ok(Vec::new());
    }
    object(value, path)?
        .iter()
        .map(|(name, hook)| {
            Ok((
                name.clone(),
                parse_method(hook, &format!("{path}.{name}"))?,
            ))
        })
        .collect()
}

/// Parses one class constant or enum case record.
fn parse_class_constant(value: &Value, path: &str) -> Result<ClassConstantSpec, String> {
    let value = object(value, path)?;
    Ok(ClassConstantSpec {
        name: required_string(value, "name", path)?,
        declaring_class: required_string(value, "declaring_class", path)?,
        value: field(value, "value")?.clone(),
        public: required_bool(value, "public", path)?,
        protected: required_bool(value, "protected", path)?,
        private: required_bool(value, "private", path)?,
        final_constant: required_bool(value, "final", path)?,
        deprecated: required_bool(value, "deprecated", path)?,
        php_type: optional_string(field(value, "type")?, &format!("{path}.type"))?,
        attributes: parse_array(value, "attributes", path, parse_attribute)?,
    })
}

/// Parses one PHP attribute record.
fn parse_attribute(value: &Value, path: &str) -> Result<AttributeSpec, String> {
    let value = object(value, path)?;
    Ok(AttributeSpec {
        name: required_string(value, "name", path)?,
        arguments: field(value, "arguments")?.clone(),
    })
}

/// Parses one named array field with a record-specific parser.
fn parse_array<T>(
    value: &Map<String, Value>,
    name: &str,
    path: &str,
    parser: fn(&Value, &str) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    array(field(value, name)?, &format!("{path}.{name}"))?
        .iter()
        .enumerate()
        .map(|(index, item)| parser(item, &format!("{path}.{name}[{index}]")))
        .collect()
}

/// Parses one named array of strings.
fn parse_strings(
    value: &Map<String, Value>,
    name: &str,
    path: &str,
) -> Result<Vec<String>, String> {
    array(field(value, name)?, &format!("{path}.{name}"))?
        .iter()
        .enumerate()
        .map(|(index, item)| owned_string(item, &format!("{path}.{name}[{index}]")))
        .collect()
}

/// Returns one required object field.
fn field<'a>(value: &'a Map<String, Value>, name: &str) -> Result<&'a Value, String> {
    value
        .get(name)
        .ok_or_else(|| format!("missing required field {name}"))
}

/// Parses one required string field.
fn required_string(value: &Map<String, Value>, name: &str, path: &str) -> Result<String, String> {
    owned_string(field(value, name)?, &format!("{path}.{name}"))
}

/// Parses one required boolean field.
fn required_bool(value: &Map<String, Value>, name: &str, path: &str) -> Result<bool, String> {
    boolean(field(value, name)?, &format!("{path}.{name}"))
}

/// Returns a JSON object or a path-specific type error.
fn object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{path} must be an object"))
}

/// Returns a JSON array or a path-specific type error.
fn array<'a>(value: &'a Value, path: &str) -> Result<&'a Vec<Value>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{path} must be an array"))
}

/// Returns a borrowed JSON string or a path-specific type error.
fn string<'a>(value: &'a Value, path: &str) -> Result<&'a str, String> {
    value
        .as_str()
        .ok_or_else(|| format!("{path} must be a string"))
}

/// Returns an owned JSON string.
fn owned_string(value: &Value, path: &str) -> Result<String, String> {
    string(value, path).map(str::to_string)
}

/// Returns an optional JSON string.
fn optional_string(value: &Value, path: &str) -> Result<Option<String>, String> {
    if value.is_null() {
        Ok(None)
    } else {
        owned_string(value, path).map(Some)
    }
}

/// Returns a required JSON boolean.
fn boolean(value: &Value, path: &str) -> Result<bool, String> {
    value
        .as_bool()
        .ok_or_else(|| format!("{path} must be a boolean"))
}

/// Returns a required non-negative JSON integer.
fn unsigned(value: &Value, path: &str) -> Result<u64, String> {
    value
        .as_u64()
        .ok_or_else(|| format!("{path} must be an unsigned integer"))
}

/// Clones a non-null JSON value into optional metadata.
fn optional_value(value: &Value) -> Option<Value> {
    (!value.is_null()).then(|| value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies incompatible snapshot schema versions are rejected.
    #[test]
    fn parse_surface_rejects_unknown_schema() {
        let source = r#"{
            "schema": 3,
            "php_version": "8.5.8",
            "libxml_version": 21503,
            "extensions": []
        }"#;
        assert_eq!(
            parse_surface(source).expect_err("schema must fail"),
            "unsupported internal-extension surface schema"
        );
    }

    /// Verifies malformed JSON produces a stable contextual parse error.
    #[test]
    fn parse_surface_rejects_malformed_json() {
        assert!(parse_surface("{").expect_err("JSON must fail").starts_with("invalid JSON:"));
    }
}
