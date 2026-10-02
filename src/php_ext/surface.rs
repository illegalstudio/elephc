//! Purpose:
//! The PHP-visible surface of a hosted extension — functions with their
//! arginfo, classes, constants and INI directives — as the extension itself
//! registers it once started.
//!
//! Called from:
//! - `crate::php_ext::build`, which records it at install time, and
//!   `crate::php_ext::prelude`, which declares it to the compiler.
//!
//! Key details:
//! - Produced by running the built extension's MINIT against the engine and
//!   walking what it registered (`elephc_php_ext_describe` in the engine's host
//!   layer), not by reading `*.stub.php`: a stub cannot see `#ifdef`s, aliases
//!   or C-side registrations, and many extensions (simdjson among them) declare
//!   their functions in C arginfo with no stub at all.
//! - Types are PHP's own rendering (`zend_type_to_string`), e.g. `?string` or
//!   `array|false`. Defaults are arginfo's default strings, or absent when the C
//!   code keeps the real default to itself.

use serde::{Deserialize, Serialize};

/// Everything one hosted extension registers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtensionSurface {
    pub module: String,
    pub version: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub functions: Vec<SurfaceFunction>,
    #[serde(default)]
    pub classes: Vec<SurfaceClass>,
    #[serde(default)]
    pub constants: Vec<SurfaceConstant>,
    #[serde(default)]
    pub ini: Vec<SurfaceIni>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfaceFunction {
    pub name: String,
    /// Arguments that must be passed; the rest are optional.
    pub required: u32,
    /// Declared return type, when the arginfo declares one.
    pub returns: Option<String>,
    #[serde(default)]
    pub returns_by_ref: bool,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub params: Vec<SurfaceParam>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfaceParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Option<String>,
    #[serde(default)]
    pub by_ref: bool,
    #[serde(default)]
    pub variadic: bool,
    /// The arginfo default as PHP source text, e.g. `false`, `512`, `null`,
    /// `PHP_INT_MAX`; absent when the C code keeps it to itself.
    pub default: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfaceClass {
    pub name: String,
    pub parent: Option<String>,
    #[serde(default)]
    pub interface: bool,
    #[serde(default, rename = "abstract")]
    pub is_abstract: bool,
    #[serde(default, rename = "final")]
    pub is_final: bool,
    #[serde(default)]
    pub methods: Vec<SurfaceFunction>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfaceConstant {
    pub name: String,
    pub value: ConstantValue,
}

/// A constant's value, tagged by its PHP type.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ConstantValue {
    Null,
    Bool { value: bool },
    Int { value: i64 },
    /// Rendered with 17 significant digits, which round-trips every double.
    Float { value: String },
    String { value: String },
    /// Arrays, objects and resources: registered by the extension but not a
    /// value Elephc can declare as a constant.
    #[serde(other)]
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfaceIni {
    pub name: String,
    pub default: Option<String>,
}

impl ExtensionSurface {
    /// Parses the introspection document.
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|error| format!("invalid extension surface: {error}"))
    }

    /// Serialises the surface for the artifact cache.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("surface is plain data")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The document the engine's `elephc_php_ext_describe` printed for the
    /// real simdjson 4.0.0 — which declares no return types and no defaults.
    const SIMDJSON: &str = r#"{"module":"simdjson","version":"4.0.0","dependencies":[],
        "functions":[{"name":"simdjson_decode","required":1,"returns":null,"returns_by_ref":false,
        "deprecated":false,"params":[{"name":"json","type":"string","by_ref":false,"variadic":false,"default":null},
        {"name":"associative","type":"bool","by_ref":false,"variadic":false,"default":null}]}],
        "classes":[{"name":"SimdJsonException","parent":"RuntimeException","interface":false,
        "abstract":false,"final":false,"methods":[]}],
        "constants":[{"name":"SIMDJSON_ERR_CAPACITY","value":{"type":"int","value":1}},
        {"name":"X_LIST","value":{"type":"array"}}],"ini":[]}"#;

    #[test]
    fn reads_what_the_engine_prints() {
        let surface = ExtensionSurface::from_json(SIMDJSON).expect("parses");
        assert_eq!(surface.module, "simdjson");
        let decode = &surface.functions[0];
        assert_eq!(decode.required, 1);
        assert_eq!(decode.returns, None);
        assert_eq!(decode.params[1].ty.as_deref(), Some("bool"));
        assert_eq!(decode.params[1].default, None, "simdjson keeps its defaults in C");
        assert_eq!(surface.classes[0].parent.as_deref(), Some("RuntimeException"));
        assert_eq!(surface.constants[0].value, ConstantValue::Int { value: 1 });
        assert_eq!(surface.constants[1].value, ConstantValue::Unsupported);
    }

    #[test]
    fn round_trips_through_the_artifact_cache() {
        let surface = ExtensionSurface::from_json(SIMDJSON).expect("parses");
        let again = ExtensionSurface::from_json(&surface.to_json()).expect("re-parses");
        assert_eq!(surface, again);
    }
}
