use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::names::Name;
use crate::parser::ast::{ClassMethod, TypeExpr, Visibility};
use crate::types::traits::FlattenedClass;
use crate::types::PhpType;

use super::builtin_types::InterfaceDeclInfo;
use super::Checker;

/// Inject the PHP `JsonSerializable` builtin interface so user classes can
/// declare `implements JsonSerializable` and the type checker recognizes the
/// abstract `jsonSerialize(): mixed` method.
///
/// The encoder dispatch (calling `$obj->jsonSerialize()` from
/// `__rt_json_encode_mixed`) lands in a follow-up phase; this declaration
/// alone unblocks user-side typing today.
pub(crate) fn inject_builtin_json_interfaces(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    let builtin_key = php_symbol_key("JsonSerializable");
    if interface_map
        .keys()
        .any(|name| php_symbol_key(name) == builtin_key)
        || class_map
            .keys()
            .any(|name| php_symbol_key(name) == builtin_key)
    {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            "Cannot redeclare built-in interface: JsonSerializable",
        ));
    }

    interface_map.insert(
        "JsonSerializable".to_string(),
        InterfaceDeclInfo {
            name: "JsonSerializable".to_string(),
            extends: Vec::new(),
            methods: vec![json_serialize_method()],
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );

    Ok(())
}

fn json_serialize_method() -> ClassMethod {
    ClassMethod {
        name: "jsonSerialize".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: true,
        is_final: false,
        has_body: false,
        params: Vec::new(),
        variadic: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        body: Vec::new(),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Once the checker has finished merging interface bodies, fix the
/// `jsonSerialize` return type to `PhpType::Mixed` so callers see the right
/// shape regardless of how the user wrote the type expression.
pub(crate) fn patch_builtin_json_signatures(checker: &mut Checker) {
    if let Some(interface_info) = checker.interfaces.get_mut("JsonSerializable") {
        if let Some(sig) = interface_info.methods.get_mut(&php_symbol_key("jsonSerialize")) {
            sig.return_type = PhpType::Mixed;
        }
    }
}
