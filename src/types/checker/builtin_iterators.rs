use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::names::Name;
use crate::parser::ast::{ClassMethod, TypeExpr, Visibility};
use crate::types::traits::FlattenedClass;

use super::builtin_types::InterfaceDeclInfo;

pub(crate) fn inject_builtin_iterators(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    for builtin_name in ["Iterator", "IteratorAggregate"] {
        let builtin_key = php_symbol_key(builtin_name);
        if interface_map
            .keys()
            .any(|name| php_symbol_key(name) == builtin_key)
            || class_map
                .keys()
                .any(|name| php_symbol_key(name) == builtin_key)
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("Cannot redeclare built-in interface: {}", builtin_name),
            ));
        }
    }

    interface_map.insert(
        "Iterator".to_string(),
        InterfaceDeclInfo {
            name: "Iterator".to_string(),
            extends: Vec::new(),
            methods: vec![
                builtin_iterator_method("current", TypeExpr::Named(Name::unqualified("mixed"))),
                builtin_iterator_method("key", TypeExpr::Named(Name::unqualified("mixed"))),
                builtin_iterator_method("next", TypeExpr::Void),
                builtin_iterator_method("valid", TypeExpr::Bool),
                builtin_iterator_method("rewind", TypeExpr::Void),
            ],
            span: crate::span::Span::dummy(),
        },
    );

    interface_map.insert(
        "IteratorAggregate".to_string(),
        InterfaceDeclInfo {
            name: "IteratorAggregate".to_string(),
            extends: Vec::new(),
            methods: vec![builtin_iterator_method(
                "getIterator",
                TypeExpr::Named(Name::unqualified("Iterator")),
            )],
            span: crate::span::Span::dummy(),
        },
    );

    Ok(())
}

fn builtin_iterator_method(name: &str, return_type: TypeExpr) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: true,
        is_final: false,
        has_body: false,
        params: Vec::new(),
        variadic: None,
        return_type: Some(return_type),
        body: Vec::new(),
        span: crate::span::Span::dummy(),
    }
}
