//! Purpose:
//! Injects SPL container class metadata into the checker.
//! Provides nominal class/interface/signature contracts for runtime-backed and synthetic SPL containers.
//!
//! Called from:
//! - `crate::types::checker::driver`
//!
//! Key details:
//! - Direct storage and legacy serialization methods use runtime `IntrinsicCall` backing.
//! - Structured serialization/debug helpers keep small synthetic PHP bodies.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{
    BinOp, CastType, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, PropertyHooks, Stmt,
    StmtKind, TypeExpr, Visibility, InstanceOfTarget, StaticReceiver,
};
use crate::types::{traits::FlattenedClass, PhpType};

use super::{builtin_types::InterfaceDeclInfo, Checker};

pub(crate) fn inject_builtin_spl_classes(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    for class_name in SPL_CLASS_NAMES {
        let class_key = php_symbol_key(class_name);
        if interface_map
            .keys()
            .any(|name| php_symbol_key(name) == class_key)
            || class_map
                .keys()
                .any(|name| php_symbol_key(name) == class_key)
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("Cannot redeclare built-in SPL class: {}", class_name),
            ));
        }
    }

    class_map.insert(
        "SplDoublyLinkedList".to_string(),
        FlattenedClass {
            name: "SplDoublyLinkedList".to_string(),
            extends: None,
            implements: vec![
                "Iterator".to_string(),
                "Countable".to_string(),
                "ArrayAccess".to_string(),
            ],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_doubly_linked_list_methods(),
            attributes: Vec::new(),
            constants: spl_doubly_linked_list_constants(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "SplStack".to_string(),
        FlattenedClass {
            name: "SplStack".to_string(),
            extends: Some("SplDoublyLinkedList".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "SplQueue".to_string(),
        FlattenedClass {
            name: "SplQueue".to_string(),
            extends: Some("SplDoublyLinkedList".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: vec![
                method("enqueue", vec![param("value", mixed_type())], Some(TypeExpr::Void)),
                method("dequeue", Vec::new(), Some(mixed_type())),
            ],
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "SplFixedArray".to_string(),
        FlattenedClass {
            name: "SplFixedArray".to_string(),
            extends: None,
            implements: vec![
                "IteratorAggregate".to_string(),
                "ArrayAccess".to_string(),
                "Countable".to_string(),
                "JsonSerializable".to_string(),
            ],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_fixed_array_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "EmptyIterator".to_string(),
        FlattenedClass {
            name: "EmptyIterator".to_string(),
            extends: None,
            implements: vec!["Iterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_empty_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "InternalIterator".to_string(),
        FlattenedClass {
            name: "InternalIterator".to_string(),
            extends: None,
            implements: vec!["Iterator".to_string()],
            is_abstract: false,
            is_final: true,
            is_readonly_class: false,
            properties: internal_iterator_properties(),
            methods: spl_internal_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "ArrayIterator".to_string(),
        FlattenedClass {
            name: "ArrayIterator".to_string(),
            extends: None,
            implements: vec![
                "Iterator".to_string(),
                "ArrayAccess".to_string(),
                "SeekableIterator".to_string(),
                "Countable".to_string(),
            ],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: array_iterator_properties(),
            methods: spl_array_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "RecursiveArrayIterator".to_string(),
        FlattenedClass {
            name: "RecursiveArrayIterator".to_string(),
            extends: Some("ArrayIterator".to_string()),
            implements: vec!["RecursiveIterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_recursive_array_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "ArrayObject".to_string(),
        FlattenedClass {
            name: "ArrayObject".to_string(),
            extends: None,
            implements: vec![
                "IteratorAggregate".to_string(),
                "ArrayAccess".to_string(),
                "Countable".to_string(),
            ],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: array_object_properties(),
            methods: spl_array_object_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "IteratorIterator".to_string(),
        FlattenedClass {
            name: "IteratorIterator".to_string(),
            extends: None,
            implements: vec!["OuterIterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: iterator_iterator_properties(),
            methods: spl_iterator_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "LimitIterator".to_string(),
        FlattenedClass {
            name: "LimitIterator".to_string(),
            extends: Some("IteratorIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: limit_iterator_properties(),
            methods: spl_limit_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "NoRewindIterator".to_string(),
        FlattenedClass {
            name: "NoRewindIterator".to_string(),
            extends: Some("IteratorIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_no_rewind_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "InfiniteIterator".to_string(),
        FlattenedClass {
            name: "InfiniteIterator".to_string(),
            extends: Some("IteratorIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_infinite_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "FilterIterator".to_string(),
        FlattenedClass {
            name: "FilterIterator".to_string(),
            extends: Some("IteratorIterator".to_string()),
            implements: Vec::new(),
            is_abstract: true,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_filter_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "CallbackFilterIterator".to_string(),
        FlattenedClass {
            name: "CallbackFilterIterator".to_string(),
            extends: Some("FilterIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: callback_filter_iterator_properties(),
            methods: spl_callback_filter_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "CachingIterator".to_string(),
        FlattenedClass {
            name: "CachingIterator".to_string(),
            extends: Some("IteratorIterator".to_string()),
            implements: vec![
                "ArrayAccess".to_string(),
                "Countable".to_string(),
                "Stringable".to_string(),
            ],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: caching_iterator_properties(),
            methods: spl_caching_iterator_methods(),
            attributes: Vec::new(),
            constants: caching_iterator_constants(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "RecursiveFilterIterator".to_string(),
        FlattenedClass {
            name: "RecursiveFilterIterator".to_string(),
            extends: Some("FilterIterator".to_string()),
            implements: vec!["RecursiveIterator".to_string()],
            is_abstract: true,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_recursive_filter_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "RecursiveCallbackFilterIterator".to_string(),
        FlattenedClass {
            name: "RecursiveCallbackFilterIterator".to_string(),
            extends: Some("CallbackFilterIterator".to_string()),
            implements: vec!["RecursiveIterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_recursive_callback_filter_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "RecursiveIteratorIterator".to_string(),
        FlattenedClass {
            name: "RecursiveIteratorIterator".to_string(),
            extends: None,
            implements: vec!["OuterIterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: recursive_iterator_iterator_properties(),
            methods: spl_recursive_iterator_iterator_methods(),
            attributes: Vec::new(),
            constants: recursive_iterator_iterator_constants(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "ParentIterator".to_string(),
        FlattenedClass {
            name: "ParentIterator".to_string(),
            extends: Some("RecursiveFilterIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: spl_parent_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "AppendIterator".to_string(),
        FlattenedClass {
            name: "AppendIterator".to_string(),
            extends: Some("IteratorIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: append_iterator_properties(),
            methods: spl_append_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "__ElephcAppendIteratorArrayIterator".to_string(),
        FlattenedClass {
            name: "__ElephcAppendIteratorArrayIterator".to_string(),
            extends: Some("ArrayIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: append_iterator_array_iterator_properties(),
            methods: spl_append_iterator_array_iterator_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "MultipleIterator".to_string(),
        FlattenedClass {
            name: "MultipleIterator".to_string(),
            extends: None,
            implements: vec!["Iterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: multiple_iterator_properties(),
            methods: spl_multiple_iterator_methods(),
            attributes: Vec::new(),
            constants: multiple_iterator_constants(),
            used_traits: Vec::new(),
        },
    );

    Ok(())
}

pub(crate) fn patch_builtin_spl_storage_signatures(checker: &mut Checker) {
    let return_type = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };
    let method_key = php_symbol_key("getArrayCopy");
    for class_name in ["ArrayIterator", "ArrayObject"] {
        if let Some(class_info) = checker.classes.get_mut(class_name) {
            if let Some(sig) = class_info.methods.get_mut(&method_key) {
                sig.return_type = return_type.clone();
            }
        }
    }
    if let Some(class_info) = checker.classes.get_mut("IteratorIterator") {
        if let Some(sig) = class_info.methods.get_mut("__construct") {
            if let Some((_, ty)) = sig.params.first_mut() {
                *ty = PhpType::Object("Traversable".to_string());
            }
            if sig.params.len() == 1 {
                sig.params.push((
                    "class".to_string(),
                    PhpType::Union(vec![PhpType::Str, PhpType::Void]),
                ));
                sig.defaults.push(Some(null_expr()));
                sig.ref_params.push(false);
                sig.declared_params.push(true);
            }
        }
    }
    if let Some(class_info) = checker.classes.get_mut("AppendIterator") {
        for (name, ty) in &mut class_info.properties {
            if name == "iterators" {
                *ty = PhpType::Array(Box::new(PhpType::Object("Iterator".to_string())));
            } else if name == "iteratorKeys" {
                *ty = PhpType::Array(Box::new(PhpType::Mixed));
            } else if name == "iteratorActive" {
                *ty = PhpType::Array(Box::new(PhpType::Bool));
            } else if name == "arrayIterator" {
                *ty = PhpType::Object("__ElephcAppendIteratorArrayIterator".to_string());
            }
        }
    }
    let iterator_array_type = PhpType::Array(Box::new(PhpType::Object("Iterator".to_string())));
    if let Some(class_info) = checker.classes.get_mut("MultipleIterator") {
        for (name, ty) in &mut class_info.properties {
            if name == "iterators" {
                *ty = iterator_array_type.clone();
            } else if name == "infos" {
                *ty = PhpType::Array(Box::new(PhpType::Mixed));
            }
        }
        for method in ["key", "current"] {
            if let Some(sig) = class_info.methods.get_mut(&php_symbol_key(method)) {
                sig.return_type = PhpType::Mixed;
            }
        }
    }
    if let Some(class_info) = checker.classes.get_mut("CallbackFilterIterator") {
        for (name, ty) in &mut class_info.properties {
            if name == "callback" {
                *ty = PhpType::Callable;
            } else if name == "callbackEnv" {
                *ty = PhpType::Pointer(None);
            }
        }
    }
    for class_name in [
        "RecursiveFilterIterator",
        "RecursiveCallbackFilterIterator",
        "ParentIterator",
    ] {
        if let Some(class_info) = checker.classes.get_mut(class_name) {
            for (name, ty) in &mut class_info.properties {
                if name == "inner" {
                    *ty = PhpType::Object("RecursiveIterator".to_string());
                } else if name == "callback" {
                    *ty = PhpType::Callable;
                } else if name == "callbackEnv" {
                    *ty = PhpType::Pointer(None);
                }
            }
        }
    }
    if let Some(class_info) = checker.classes.get_mut("RecursiveIteratorIterator") {
        for (name, ty) in &mut class_info.properties {
            match name.as_str() {
                "root" => *ty = PhpType::Object("RecursiveIterator".to_string()),
                "states" | "depths" => {
                    *ty = PhpType::Array(Box::new(PhpType::Int));
                }
                "iterators" => {
                    *ty = PhpType::Array(Box::new(PhpType::Object(
                        "RecursiveIterator".to_string(),
                    )));
                }
                _ => {}
            }
        }
    }
    if let Some(class_info) = checker.classes.get_mut("CachingIterator") {
        for (name, ty) in &mut class_info.properties {
            if name == "cache" {
                *ty = PhpType::AssocArray {
                    key: Box::new(PhpType::Mixed),
                    value: Box::new(PhpType::Mixed),
                };
            } else if name == "currentKey" || name == "currentValue" {
                *ty = PhpType::Mixed;
            }
        }
        if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getCache")) {
            sig.return_type = PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            };
        }
    }
}

const SPL_CLASS_NAMES: &[&str] = &[
    "SplDoublyLinkedList",
    "SplStack",
    "SplQueue",
    "SplFixedArray",
    "EmptyIterator",
    "InternalIterator",
    "ArrayIterator",
    "RecursiveArrayIterator",
    "ArrayObject",
    "IteratorIterator",
    "LimitIterator",
    "NoRewindIterator",
    "InfiniteIterator",
    "FilterIterator",
    "CallbackFilterIterator",
    "CachingIterator",
    "RecursiveFilterIterator",
    "RecursiveCallbackFilterIterator",
    "RecursiveIteratorIterator",
    "ParentIterator",
    "AppendIterator",
    "MultipleIterator",
];

fn spl_empty_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body("current", Vec::new(), Some(mixed_type()), null_return_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), null_return_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), Vec::new()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), Vec::new()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), return_body(bool_expr(false))),
    ]
}

fn spl_internal_iterator_methods() -> Vec<ClassMethod> {
    let mut construct = method_with_body(
        "__construct",
        vec![param("owner", named_type("SplFixedArray"))],
        Some(TypeExpr::Void),
        internal_iterator_construct_body(),
    );
    construct.visibility = Visibility::Private;

    vec![
        construct,
        method_with_body("current", Vec::new(), Some(mixed_type()), internal_iterator_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), return_body(internal_iterator_position_expr())),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), internal_iterator_next_body()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), internal_iterator_rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), internal_iterator_valid_body()),
    ]
}

fn internal_iterator_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("owner", named_type("SplFixedArray")),
        storage_property("position", TypeExpr::Int),
    ]
}

fn array_iterator_properties() -> Vec<ClassProperty> {
    vec![
        protected_storage_property("keys", array_type()),
        protected_storage_property("values", array_type()),
        protected_storage_property("position", TypeExpr::Int),
        protected_storage_property("flags", TypeExpr::Int),
    ]
}

fn array_object_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("keys", array_type()),
        storage_property("values", array_type()),
        storage_property("flags", TypeExpr::Int),
    ]
}

fn iterator_iterator_properties() -> Vec<ClassProperty> {
    vec![storage_property("inner", named_type("Iterator"))]
}

fn limit_iterator_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("position", TypeExpr::Int),
        storage_property("offset", TypeExpr::Int),
        storage_property("limit", TypeExpr::Int),
    ]
}

fn append_iterator_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("iterators", array_type()),
        storage_property("iteratorKeys", array_type()),
        storage_property("iteratorActive", array_type()),
        storage_property("index", TypeExpr::Int),
        storage_property("arrayIterator", named_type("__ElephcAppendIteratorArrayIterator")),
    ]
}

fn append_iterator_array_iterator_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("owner", named_type("AppendIterator")),
        storage_property_default("appendPosition", TypeExpr::Int, int_expr(0)),
    ]
}

fn multiple_iterator_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("iterators", array_type()),
        storage_property("infos", array_type()),
        storage_property("flags", TypeExpr::Int),
    ]
}

fn callback_filter_iterator_properties() -> Vec<ClassProperty> {
    vec![
        protected_storage_property_untyped("callback"),
        protected_storage_property("callbackEnv", TypeExpr::Ptr(None)),
    ]
}

fn caching_iterator_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("flags", TypeExpr::Int),
        storage_property("cache", array_type()),
        storage_property("currentKey", mixed_type()),
        storage_property("currentValue", mixed_type()),
        storage_property("currentValid", TypeExpr::Bool),
        storage_property("cachedHasNext", TypeExpr::Bool),
    ]
}

fn recursive_iterator_iterator_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("root", named_type("RecursiveIterator")),
        storage_property("mode", TypeExpr::Int),
        storage_property("flags", TypeExpr::Int),
        storage_property("iterators", array_type()),
        storage_property("states", array_type()),
        storage_property("depths", array_type()),
        storage_property("depth", TypeExpr::Int),
        storage_property("slot", TypeExpr::Int),
        storage_property("currentValid", TypeExpr::Bool),
    ]
}

fn spl_array_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param_default("array", array_type(), empty_array_expr()),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(TypeExpr::Void),
            array_iterator_construct_body(),
        ),
        method_with_body("current", Vec::new(), Some(mixed_type()), array_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), array_key_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), array_next_body()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), array_rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), array_valid_body()),
        method_with_body(
            "seek",
            vec![param("offset", TypeExpr::Int)],
            Some(TypeExpr::Void),
            vec![property_assign_stmt(this_expr(), "position", var_expr("offset"))],
        ),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), array_count_body()),
        method_with_body(
            "offsetExists",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Bool),
            array_offset_exists_body(),
        ),
        method_with_body(
            "offsetGet",
            vec![param("offset", mixed_type())],
            Some(mixed_type()),
            array_offset_get_body(),
        ),
        method_with_body(
            "offsetSet",
            vec![param("offset", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
            array_offset_set_body(),
        ),
        method_with_body(
            "offsetUnset",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Void),
            array_offset_unset_body(),
        ),
        method_with_body(
            "append",
            vec![param("value", mixed_type())],
            Some(TypeExpr::Void),
            array_append_body(),
        ),
        method_with_body("getArrayCopy", Vec::new(), Some(array_type()), array_copy_body()),
    ]
}

fn spl_recursive_array_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param_default("array", mixed_type(), empty_array_expr()),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(TypeExpr::Void),
            recursive_array_iterator_construct_body(),
        ),
        method_with_body("hasChildren", Vec::new(), Some(TypeExpr::Bool), recursive_array_has_children_body()),
        method_with_body(
            "getChildren",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("RecursiveIterator")))),
            recursive_array_get_children_body(),
        ),
        method_with_body(
            "__elephcAssumeRecursiveIterator",
            vec![param("iterator", mixed_type())],
            Some(named_type("RecursiveIterator")),
            Vec::new(),
        ),
    ]
}

fn spl_array_object_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param_default("array", array_type(), empty_array_expr()),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(TypeExpr::Void),
            array_object_construct_body(),
        ),
        method_with_body("getIterator", Vec::new(), Some(named_type("ArrayIterator")), array_object_get_iterator_body()),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), array_count_body()),
        method_with_body(
            "offsetExists",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Bool),
            array_offset_exists_body(),
        ),
        method_with_body(
            "offsetGet",
            vec![param("offset", mixed_type())],
            Some(mixed_type()),
            array_offset_get_body(),
        ),
        method_with_body(
            "offsetSet",
            vec![param("offset", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
            array_offset_set_body(),
        ),
        method_with_body(
            "offsetUnset",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Void),
            array_offset_unset_body(),
        ),
        method_with_body(
            "append",
            vec![param("value", mixed_type())],
            Some(TypeExpr::Void),
            array_append_body(),
        ),
        method_with_body("getArrayCopy", Vec::new(), Some(array_type()), array_copy_body()),
    ]
}

fn spl_iterator_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("iterator", named_type("Traversable")),
                param_default(
                    "class",
                    TypeExpr::Nullable(Box::new(TypeExpr::Str)),
                    null_expr(),
                ),
            ],
            Some(TypeExpr::Void),
            iterator_iterator_construct_body(),
        ),
        method_with_body("current", Vec::new(), Some(mixed_type()), inner_return_body("current")),
        method_with_body("key", Vec::new(), Some(mixed_type()), inner_return_body("key")),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), inner_void_body("next")),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), inner_void_body("rewind")),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), inner_return_body("valid")),
        method_with_body(
            "getInnerIterator",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("Iterator")))),
            return_body(inner_expr()),
        ),
    ]
}

fn spl_limit_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("iterator", named_type("Iterator")),
                param_default("offset", TypeExpr::Int, int_expr(0)),
                param_default("limit", TypeExpr::Int, int_expr(-1)),
            ],
            Some(TypeExpr::Void),
            limit_iterator_construct_body(),
        ),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), limit_rewind_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), limit_next_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), limit_valid_body()),
        method_with_body(
            "seek",
            vec![param("offset", TypeExpr::Int)],
            Some(TypeExpr::Void),
            limit_seek_body(),
        ),
        method_with_body("getPosition", Vec::new(), Some(TypeExpr::Int), return_body(limit_position_expr())),
    ]
}

fn spl_no_rewind_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            iterator_iterator_construct_body(),
        ),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), Vec::new()),
    ]
}

fn spl_infinite_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            iterator_iterator_construct_body(),
        ),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), infinite_next_body()),
    ]
}

fn spl_filter_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            iterator_iterator_construct_body(),
        ),
        abstract_method("accept", Vec::new(), Some(TypeExpr::Bool)),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), filter_rewind_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), filter_next_body()),
    ]
}

fn spl_callback_filter_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("iterator", named_type("Iterator")),
                param("callback", named_type("callable")),
            ],
            Some(TypeExpr::Void),
            callback_filter_construct_body(),
        ),
        method_with_body(
            "__elephcAcceptCallback",
            vec![
                param("current", mixed_type()),
                param("key", mixed_type()),
                param("iterator", named_type("Iterator")),
            ],
            Some(TypeExpr::Bool),
            Vec::new(),
        ),
        method_with_body("accept", Vec::new(), Some(TypeExpr::Bool), callback_filter_accept_body()),
        method_with_body(
            "__elephcSetCallbackEnv",
            vec![param("env", TypeExpr::Ptr(None))],
            Some(TypeExpr::Void),
            vec![property_assign_stmt(this_expr(), "callbackEnv", var_expr("env"))],
        ),
    ]
}

fn spl_caching_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("iterator", named_type("Iterator")),
                param_default("flags", TypeExpr::Int, int_expr(1)),
            ],
            Some(TypeExpr::Void),
            caching_construct_body(),
        ),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), caching_rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), return_body(caching_current_valid_expr())),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), caching_next_body()),
        method_with_body("current", Vec::new(), Some(mixed_type()), caching_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), caching_key_body()),
        method_with_body("hasNext", Vec::new(), Some(TypeExpr::Bool), return_body(caching_has_next_expr())),
        method_with_body("__toString", Vec::new(), Some(TypeExpr::Str), caching_to_string_body()),
        method_with_body("getFlags", Vec::new(), Some(TypeExpr::Int), return_body(caching_flags_expr())),
        method_with_body(
            "setFlags",
            vec![param("flags", TypeExpr::Int)],
            Some(TypeExpr::Void),
            caching_set_flags_body(),
        ),
        method_with_body(
            "offsetGet",
            vec![param("key", mixed_type())],
            Some(mixed_type()),
            caching_offset_get_body(),
        ),
        method_with_body(
            "offsetSet",
            vec![param("key", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
            caching_offset_set_body(),
        ),
        method_with_body(
            "offsetUnset",
            vec![param("key", mixed_type())],
            Some(TypeExpr::Void),
            caching_offset_unset_body(),
        ),
        method_with_body(
            "offsetExists",
            vec![param("key", mixed_type())],
            Some(TypeExpr::Bool),
            caching_offset_exists_body(),
        ),
        method_with_body("getCache", Vec::new(), Some(array_type()), caching_get_cache_body()),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), caching_count_body()),
        method_with_body(
            "__elephcCaptureCurrent",
            Vec::new(),
            Some(TypeExpr::Void),
            caching_capture_current_body(),
        ),
    ]
}

fn spl_recursive_filter_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("iterator", named_type("RecursiveIterator"))],
            Some(TypeExpr::Void),
            iterator_iterator_construct_body(),
        ),
        method_with_body("hasChildren", Vec::new(), Some(TypeExpr::Bool), recursive_inner_return_body("hasChildren")),
        method_with_body(
            "getChildren",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("RecursiveIterator")))),
            recursive_filter_get_children_body(),
        ),
        method_with_body(
            "__elephcAssumeRecursiveIterator",
            vec![param("iterator", mixed_type())],
            Some(named_type("RecursiveIterator")),
            Vec::new(),
        ),
    ]
}

fn spl_recursive_callback_filter_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("iterator", named_type("RecursiveIterator")),
                param("callback", named_type("callable")),
            ],
            Some(TypeExpr::Void),
            callback_filter_construct_body(),
        ),
        method_with_body("hasChildren", Vec::new(), Some(TypeExpr::Bool), recursive_inner_return_body("hasChildren")),
        method_with_body(
            "getChildren",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("RecursiveIterator")))),
            recursive_callback_filter_get_children_body(),
        ),
        method_with_body(
            "__elephcAssumeRecursiveIterator",
            vec![param("iterator", mixed_type())],
            Some(named_type("RecursiveIterator")),
            Vec::new(),
        ),
    ]
}

fn spl_recursive_iterator_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("iterator", named_type("RecursiveIterator")),
                param_default("mode", TypeExpr::Int, int_expr(0)),
                param_default("flags", TypeExpr::Int, int_expr(0)),
            ],
            Some(TypeExpr::Void),
            recursive_iterator_iterator_construct_body(),
        ),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), recursive_iterator_iterator_rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), recursive_iterator_iterator_valid_body()),
        method_with_body("current", Vec::new(), Some(mixed_type()), recursive_iterator_iterator_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), recursive_iterator_iterator_key_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), recursive_iterator_iterator_next_body()),
        method_with_body(
            "getDepth",
            Vec::new(),
            Some(TypeExpr::Int),
            recursive_iterator_iterator_get_depth_body(),
        ),
        method_with_body(
            "getInnerIterator",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("Iterator")))),
            recursive_iterator_iterator_get_inner_iterator_body(),
        ),
        method_with_body(
            "getSubIterator",
            vec![param_default("level", TypeExpr::Int, int_expr(-1))],
            Some(TypeExpr::Nullable(Box::new(named_type("RecursiveIterator")))),
            recursive_iterator_iterator_get_sub_iterator_body(),
        ),
        method_with_body(
            "__elephcAdvance",
            Vec::new(),
            Some(TypeExpr::Void),
            recursive_iterator_iterator_advance_body(),
        ),
        method_with_body(
            "__elephcSlotForDepth",
            vec![param("level", TypeExpr::Int)],
            Some(TypeExpr::Int),
            recursive_iterator_iterator_slot_for_depth_body(),
        ),
        method_with_body(
            "__elephcAssumeRecursiveIterator",
            vec![param("iterator", mixed_type())],
            Some(named_type("RecursiveIterator")),
            Vec::new(),
        ),
    ]
}

fn spl_parent_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("iterator", named_type("RecursiveIterator"))],
            Some(TypeExpr::Void),
            iterator_iterator_construct_body(),
        ),
        method_with_body("accept", Vec::new(), Some(TypeExpr::Bool), return_body(method_call(this_expr(), "hasChildren", Vec::new()))),
        method_with_body(
            "getChildren",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("RecursiveIterator")))),
            parent_iterator_get_children_body(),
        ),
        method_with_body(
            "__elephcAssumeRecursiveIterator",
            vec![param("iterator", mixed_type())],
            Some(named_type("RecursiveIterator")),
            Vec::new(),
        ),
    ]
}

fn spl_append_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body("__construct", Vec::new(), Some(TypeExpr::Void), append_construct_body()),
        method_with_body(
            "append",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            append_append_body(),
        ),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), append_rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), append_valid_body()),
        method_with_body("current", Vec::new(), Some(mixed_type()), append_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), append_key_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), append_next_body()),
        method_with_body(
            "getInnerIterator",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("Iterator")))),
            append_get_inner_iterator_body(),
        ),
        method_with_body(
            "getIteratorIndex",
            Vec::new(),
            Some(mixed_type()),
            append_get_iterator_index_body(),
        ),
        method_with_body(
            "getArrayIterator",
            Vec::new(),
            Some(named_type("__ElephcAppendIteratorArrayIterator")),
            append_get_array_iterator_body(),
        ),
        method_with_body(
            "__elephcStorageCount",
            Vec::new(),
            Some(TypeExpr::Int),
            append_storage_count_body(),
        ),
        method_with_body(
            "__elephcStoragePhysicalCount",
            Vec::new(),
            Some(TypeExpr::Int),
            return_body(count_expr(append_iterators_expr())),
        ),
        method_with_body(
            "__elephcStorageIsActive",
            vec![param("position", TypeExpr::Int)],
            Some(TypeExpr::Bool),
            return_body(append_active_at_position_expr(var_expr("position"))),
        ),
        method_with_body(
            "__elephcStorageAppend",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            append_storage_append_body(),
        ),
        method_with_body(
            "__elephcStorageOffsetSet",
            vec![param("offset", mixed_type()), param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            append_storage_offset_set_body(),
        ),
        method_with_body(
            "__elephcStorageOffsetExists",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Bool),
            append_storage_offset_exists_body(),
        ),
        method_with_body(
            "__elephcStorageOffsetGet",
            vec![param("offset", mixed_type())],
            Some(mixed_type()),
            append_storage_offset_get_body(),
        ),
        method_with_body(
            "__elephcStorageOffsetUnset",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Void),
            append_storage_offset_unset_body(),
        ),
        method_with_body(
            "__elephcStorageGetArrayCopy",
            Vec::new(),
            Some(array_type()),
            append_storage_get_array_copy_body(),
        ),
        method_with_body(
            "__elephcStorageKey",
            vec![param("position", TypeExpr::Int)],
            Some(mixed_type()),
            return_body(append_key_at_position_expr(var_expr("position"))),
        ),
        method_with_body(
            "__elephcStorageCurrent",
            vec![param("position", TypeExpr::Int)],
            Some(mixed_type()),
            append_storage_current_body(),
        ),
    ]
}

fn spl_append_iterator_array_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param("owner", named_type("AppendIterator"))],
            Some(TypeExpr::Void),
            append_array_iterator_construct_body(),
        ),
        method_with_body("count", Vec::new(), Some(TypeExpr::Int), append_array_iterator_count_body()),
        method_with_body(
            "append",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            append_array_iterator_append_body(),
        ),
        method_with_body(
            "offsetSet",
            vec![param("offset", mixed_type()), param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            append_array_iterator_offset_set_body(),
        ),
        method_with_body(
            "offsetExists",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Bool),
            append_array_iterator_offset_exists_body(),
        ),
        method_with_body(
            "offsetGet",
            vec![param("offset", mixed_type())],
            Some(mixed_type()),
            append_array_iterator_offset_get_body(),
        ),
        method_with_body(
            "offsetUnset",
            vec![param("offset", mixed_type())],
            Some(TypeExpr::Void),
            append_array_iterator_offset_unset_body(),
        ),
        method_with_body("getArrayCopy", Vec::new(), Some(array_type()), append_array_iterator_copy_body()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), append_array_iterator_rewind_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), append_array_iterator_next_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), append_array_iterator_valid_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), append_array_iterator_key_body()),
        method_with_body("current", Vec::new(), Some(mixed_type()), append_array_iterator_current_body()),
    ]
}

fn spl_multiple_iterator_methods() -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![param_default("flags", TypeExpr::Int, int_expr(1))],
            Some(TypeExpr::Void),
            multiple_construct_body(),
        ),
        method_with_body("getFlags", Vec::new(), Some(TypeExpr::Int), return_body(multiple_flags_expr())),
        method_with_body(
            "setFlags",
            vec![param("flags", TypeExpr::Int)],
            Some(TypeExpr::Void),
            vec![property_assign_stmt(this_expr(), "flags", var_expr("flags"))],
        ),
        method_with_body(
            "attachIterator",
            vec![
                param("iterator", named_type("Iterator")),
                param_default(
                    "info",
                    TypeExpr::Nullable(Box::new(TypeExpr::Union(vec![TypeExpr::Str, TypeExpr::Int]))),
                    null_expr(),
                ),
            ],
            Some(TypeExpr::Void),
            multiple_attach_iterator_body(),
        ),
        method_with_body(
            "detachIterator",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Void),
            multiple_detach_iterator_body(),
        ),
        method_with_body(
            "containsIterator",
            vec![param("iterator", named_type("Iterator"))],
            Some(TypeExpr::Bool),
            multiple_contains_iterator_body(),
        ),
        method_with_body(
            "countIterators",
            Vec::new(),
            Some(TypeExpr::Int),
            return_body(count_expr(multiple_iterators_expr())),
        ),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), multiple_rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), multiple_valid_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), multiple_output_body("key")),
        method_with_body("current", Vec::new(), Some(mixed_type()), multiple_output_body("current")),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), multiple_next_body()),
        method_with_body("__debugInfo", Vec::new(), Some(array_type()), multiple_debug_info_body()),
    ]
}

fn spl_doubly_linked_list_methods() -> Vec<ClassMethod> {
    vec![
        method(
            "add",
            vec![param("index", TypeExpr::Int), param("value", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method("pop", Vec::new(), Some(mixed_type())),
        method("shift", Vec::new(), Some(mixed_type())),
        method("push", vec![param("value", mixed_type())], Some(TypeExpr::Void)),
        method(
            "unshift",
            vec![param("value", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method("top", Vec::new(), Some(mixed_type())),
        method("bottom", Vec::new(), Some(mixed_type())),
        method("count", Vec::new(), Some(TypeExpr::Int)),
        method("isEmpty", Vec::new(), Some(TypeExpr::Bool)),
        method(
            "setIteratorMode",
            vec![param("mode", TypeExpr::Int)],
            Some(TypeExpr::Void),
        ),
        method("getIteratorMode", Vec::new(), Some(TypeExpr::Int)),
        method(
            "offsetExists",
            vec![param("index", mixed_type())],
            Some(TypeExpr::Bool),
        ),
        method(
            "offsetGet",
            vec![param("index", mixed_type())],
            Some(mixed_type()),
        ),
        method(
            "offsetSet",
            vec![param("index", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method(
            "offsetUnset",
            vec![param("index", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method("rewind", Vec::new(), Some(TypeExpr::Void)),
        method("current", Vec::new(), Some(mixed_type())),
        method("key", Vec::new(), Some(mixed_type())),
        method("prev", Vec::new(), Some(TypeExpr::Void)),
        method("next", Vec::new(), Some(TypeExpr::Void)),
        method("valid", Vec::new(), Some(TypeExpr::Bool)),
        method("serialize", Vec::new(), Some(TypeExpr::Str)),
        method(
            "unserialize",
            vec![param("data", TypeExpr::Str)],
            Some(TypeExpr::Void),
        ),
        method_with_body(
            "__serialize",
            Vec::new(),
            Some(array_type()),
            dll_serialize_array_body(),
        ),
        method_with_body(
            "__unserialize",
            vec![param("data", array_type())],
            Some(TypeExpr::Void),
            dll_unserialize_body(),
        ),
        method_with_body(
            "__debugInfo",
            Vec::new(),
            Some(array_type()),
            dll_debug_info_body(),
        ),
    ]
}

fn spl_fixed_array_methods() -> Vec<ClassMethod> {
    vec![
        method(
            "__construct",
            vec![param_default("size", TypeExpr::Int, int_expr(0))],
            Some(TypeExpr::Void),
        ),
        method_with_body("__wakeup", Vec::new(), Some(TypeExpr::Void), Vec::new()),
        class_method(
            "fromArray",
            true,
            vec![
                param("array", array_type()),
                param_default("preserveKeys", TypeExpr::Bool, bool_expr(true)),
            ],
            Some(named_type("SplFixedArray")),
        ),
        method_with_body(
            "__serialize",
            Vec::new(),
            Some(array_type()),
            vec![return_stmt(method_call(this_expr(), "toArray", Vec::new()))],
        ),
        method("__unserialize", vec![param("data", array_type())], Some(TypeExpr::Void)),
        method("count", Vec::new(), Some(TypeExpr::Int)),
        method_with_body(
            "getIterator",
            Vec::new(),
            Some(named_type("Iterator")),
            fixed_array_get_iterator_body(),
        ),
        method("toArray", Vec::new(), Some(array_type())),
        method("getSize", Vec::new(), Some(TypeExpr::Int)),
        method(
            "setSize",
            vec![param("size", TypeExpr::Int)],
            Some(TypeExpr::Void),
        ),
        method(
            "offsetExists",
            vec![param("index", mixed_type())],
            Some(TypeExpr::Bool),
        ),
        method(
            "offsetGet",
            vec![param("index", mixed_type())],
            Some(mixed_type()),
        ),
        method(
            "offsetSet",
            vec![param("index", mixed_type()), param("value", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method(
            "offsetUnset",
            vec![param("index", mixed_type())],
            Some(TypeExpr::Void),
        ),
        method("jsonSerialize", Vec::new(), Some(array_type())),
    ]
}

fn spl_doubly_linked_list_constants() -> Vec<ClassConst> {
    vec![
        class_const("IT_MODE_LIFO", 2),
        class_const("IT_MODE_FIFO", 0),
        class_const("IT_MODE_DELETE", 1),
        class_const("IT_MODE_KEEP", 0),
    ]
}

fn multiple_iterator_constants() -> Vec<ClassConst> {
    vec![
        class_const("MIT_NEED_ANY", 0),
        class_const("MIT_NEED_ALL", 1),
        class_const("MIT_KEYS_NUMERIC", 0),
        class_const("MIT_KEYS_ASSOC", 2),
    ]
}

fn caching_iterator_constants() -> Vec<ClassConst> {
    vec![
        class_const("CALL_TOSTRING", 1),
        class_const("CATCH_GET_CHILD", 16),
        class_const("TOSTRING_USE_KEY", 2),
        class_const("TOSTRING_USE_CURRENT", 4),
        class_const("TOSTRING_USE_INNER", 8),
        class_const("FULL_CACHE", 256),
    ]
}

fn recursive_iterator_iterator_constants() -> Vec<ClassConst> {
    vec![
        class_const("LEAVES_ONLY", 0),
        class_const("SELF_FIRST", 1),
        class_const("CHILD_FIRST", 2),
        class_const("CATCH_GET_CHILD", 16),
    ]
}

fn method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
) -> ClassMethod {
    class_method(name, false, params, return_type)
}

fn method_with_body(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
) -> ClassMethod {
    class_method_with_body(name, false, params, return_type, body)
}

fn abstract_method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
) -> ClassMethod {
    let mut method = class_method_with_body(name, false, params, return_type, Vec::new());
    method.is_abstract = true;
    method.has_body = false;
    method
}

fn class_method(
    name: &str,
    is_static: bool,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
) -> ClassMethod {
    class_method_with_body(
        name,
        is_static,
        params,
        return_type.clone(),
        dummy_body_for(return_type.as_ref()),
    )
}

fn class_method_with_body(
    name: &str,
    is_static: bool,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params,
        variadic: None,
        return_type,
        body,
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

fn storage_property(name: &str, type_expr: TypeExpr) -> ClassProperty {
    storage_property_with_default(name, type_expr, None)
}

fn protected_storage_property(name: &str, type_expr: TypeExpr) -> ClassProperty {
    storage_property_with_visibility(name, Some(type_expr), None, Visibility::Protected)
}

fn storage_property_default(name: &str, type_expr: TypeExpr, default: Expr) -> ClassProperty {
    storage_property_with_default(name, type_expr, Some(default))
}

fn protected_storage_property_untyped(name: &str) -> ClassProperty {
    storage_property_with_visibility(name, None, None, Visibility::Protected)
}

fn storage_property_with_default(
    name: &str,
    type_expr: TypeExpr,
    default: Option<Expr>,
) -> ClassProperty {
    storage_property_with_visibility(name, Some(type_expr), default, Visibility::Private)
}

fn storage_property_with_visibility(
    name: &str,
    type_expr: Option<TypeExpr>,
    default: Option<Expr>,
    visibility: Visibility,
) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility,
        type_expr,
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        default,
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

fn dummy_body_for(return_type: Option<&TypeExpr>) -> Vec<Stmt> {
    match return_type {
        Some(TypeExpr::Void) | None => Vec::new(),
        Some(TypeExpr::Int) => return_body(int_expr(0)),
        Some(TypeExpr::Bool) => return_body(bool_expr(false)),
        Some(TypeExpr::Str) => return_body(Expr::new(
            ExprKind::StringLiteral(String::new()),
            crate::span::Span::dummy(),
        )),
        Some(TypeExpr::Named(name)) if name.as_canonical() == "array" => {
            return_body(Expr::new(ExprKind::ArrayLiteral(Vec::new()), crate::span::Span::dummy()))
        }
        _ => return_body(Expr::new(ExprKind::Null, crate::span::Span::dummy())),
    }
}

fn return_body(value: Expr) -> Vec<Stmt> {
    vec![return_stmt(value)]
}

fn null_return_body() -> Vec<Stmt> {
    return_body(expr(ExprKind::Null))
}

fn return_stmt(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Return(Some(value)), crate::span::Span::dummy())
}

fn return_void_stmt() -> Stmt {
    Stmt::new(StmtKind::Return(None), crate::span::Span::dummy())
}

fn throw_stmt(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Throw(value), crate::span::Span::dummy())
}

fn param(name: &str, ty: TypeExpr) -> (String, Option<TypeExpr>, Option<Expr>, bool) {
    (name.to_string(), Some(ty), None, false)
}

fn param_default(
    name: &str,
    ty: TypeExpr,
    default: Expr,
) -> (String, Option<TypeExpr>, Option<Expr>, bool) {
    (name.to_string(), Some(ty), Some(default), false)
}

fn class_const(name: &str, value: i64) -> ClassConst {
    ClassConst {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_final: false,
        value: int_expr(value),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

fn int_expr(value: i64) -> Expr {
    Expr::new(ExprKind::IntLiteral(value), crate::span::Span::dummy())
}

fn bool_expr(value: bool) -> Expr {
    Expr::new(ExprKind::BoolLiteral(value), crate::span::Span::dummy())
}

fn empty_array_expr() -> Expr {
    expr(ExprKind::ArrayLiteral(Vec::new()))
}

fn empty_assoc_array_expr() -> Expr {
    expr(ExprKind::ArrayLiteralAssoc(Vec::new()))
}

fn mixed_type() -> TypeExpr {
    named_type("mixed")
}

fn array_type() -> TypeExpr {
    named_type("array")
}

fn named_type(name: &str) -> TypeExpr {
    TypeExpr::Named(Name::unqualified(name))
}

fn expr(kind: ExprKind) -> Expr {
    Expr::new(kind, crate::span::Span::dummy())
}

fn string_expr(value: &str) -> Expr {
    expr(ExprKind::StringLiteral(value.to_string()))
}

fn var_expr(name: &str) -> Expr {
    expr(ExprKind::Variable(name.to_string()))
}

fn this_expr() -> Expr {
    expr(ExprKind::This)
}

fn null_expr() -> Expr {
    expr(ExprKind::Null)
}

fn binary_expr(left: Expr, op: BinOp, right: Expr) -> Expr {
    expr(ExprKind::BinaryOp {
        left: Box::new(left),
        op,
        right: Box::new(right),
    })
}

fn not_expr(value: Expr) -> Expr {
    expr(ExprKind::Not(Box::new(value)))
}

fn cast_expr(target: CastType, value: Expr) -> Expr {
    expr(ExprKind::Cast {
        target,
        expr: Box::new(value),
    })
}

fn method_call(object: Expr, method: &str, args: Vec<Expr>) -> Expr {
    expr(ExprKind::MethodCall {
        object: Box::new(object),
        method: method.to_string(),
        args,
    })
}

fn function_call(name: &str, args: Vec<Expr>) -> Expr {
    expr(ExprKind::FunctionCall {
        name: Name::unqualified(name),
        args,
    })
}

fn new_object_expr(class_name: &str, args: Vec<Expr>) -> Expr {
    expr(ExprKind::NewObject {
        class_name: Name::unqualified(class_name),
        args,
    })
}

fn new_static_expr(args: Vec<Expr>) -> Expr {
    expr(ExprKind::NewScopedObject {
        receiver: StaticReceiver::Static,
        args,
    })
}

fn instanceof_expr(value: Expr, class_name: &str) -> Expr {
    expr(ExprKind::InstanceOf {
        value: Box::new(value),
        target: InstanceOfTarget::Name(Name::unqualified(class_name)),
    })
}

fn property_access(object: Expr, property: &str) -> Expr {
    expr(ExprKind::PropertyAccess {
        object: Box::new(object),
        property: property.to_string(),
    })
}

fn array_access(array: Expr, index: Expr) -> Expr {
    expr(ExprKind::ArrayAccess {
        array: Box::new(array),
        index: Box::new(index),
    })
}

fn assign_stmt(name: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::Assign {
            name: name.to_string(),
            value,
        },
        crate::span::Span::dummy(),
    )
}

fn typed_assign_stmt(name: &str, type_expr: TypeExpr, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::TypedAssign {
            type_expr,
            name: name.to_string(),
            value,
        },
        crate::span::Span::dummy(),
    )
}

fn expr_stmt(value: Expr) -> Stmt {
    Stmt::new(StmtKind::ExprStmt(value), crate::span::Span::dummy())
}

fn property_assign_stmt(object: Expr, property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyAssign {
            object: Box::new(object),
            property: property.to_string(),
            value,
        },
        crate::span::Span::dummy(),
    )
}

fn property_array_push_stmt(object: Expr, property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyArrayPush {
            object: Box::new(object),
            property: property.to_string(),
            value,
        },
        crate::span::Span::dummy(),
    )
}

fn property_array_assign_stmt(object: Expr, property: &str, index: Expr, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyArrayAssign {
            object: Box::new(object),
            property: property.to_string(),
            index,
            value,
        },
        crate::span::Span::dummy(),
    )
}

fn array_assign_stmt(array: &str, index: Expr, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::ArrayAssign {
            array: array.to_string(),
            index,
            value,
        },
        crate::span::Span::dummy(),
    )
}

fn array_push_stmt(array: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::ArrayPush {
            array: array.to_string(),
            value,
        },
        crate::span::Span::dummy(),
    )
}

fn while_stmt(condition: Expr, body: Vec<Stmt>) -> Stmt {
    Stmt::new(
        StmtKind::While { condition, body },
        crate::span::Span::dummy(),
    )
}

fn if_stmt(condition: Expr, then_body: Vec<Stmt>, else_body: Option<Vec<Stmt>>) -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses: Vec::new(),
            else_body,
        },
        crate::span::Span::dummy(),
    )
}

fn foreach_stmt(array: Expr, key_var: Option<&str>, value_var: &str, body: Vec<Stmt>) -> Stmt {
    Stmt::new(
        StmtKind::Foreach {
            array,
            key_var: key_var.map(str::to_string),
            value_var: value_var.to_string(),
            value_by_ref: false,
            body,
        },
        crate::span::Span::dummy(),
    )
}

fn increment_stmt(name: &str) -> Stmt {
    assign_stmt(name, binary_expr(var_expr(name), BinOp::Add, int_expr(1)))
}

fn keys_expr() -> Expr {
    property_access(this_expr(), "keys")
}

fn values_expr() -> Expr {
    property_access(this_expr(), "values")
}

fn position_expr() -> Expr {
    property_access(this_expr(), "position")
}

fn count_expr(value: Expr) -> Expr {
    function_call("count", vec![value])
}

fn internal_iterator_owner_expr() -> Expr {
    property_access(this_expr(), "owner")
}

fn internal_iterator_position_expr() -> Expr {
    property_access(this_expr(), "position")
}

fn internal_iterator_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "owner", var_expr("owner")),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
    ]
}

fn internal_iterator_current_body() -> Vec<Stmt> {
    return_body(method_call(
        internal_iterator_owner_expr(),
        "offsetGet",
        vec![internal_iterator_position_expr()],
    ))
}

fn internal_iterator_next_body() -> Vec<Stmt> {
    vec![property_assign_stmt(
        this_expr(),
        "position",
        binary_expr(internal_iterator_position_expr(), BinOp::Add, int_expr(1)),
    )]
}

fn internal_iterator_rewind_body() -> Vec<Stmt> {
    vec![property_assign_stmt(this_expr(), "position", int_expr(0))]
}

fn internal_iterator_valid_body() -> Vec<Stmt> {
    return_body(binary_expr(
        internal_iterator_position_expr(),
        BinOp::Lt,
        method_call(internal_iterator_owner_expr(), "count", Vec::new()),
    ))
}

fn fixed_array_get_iterator_body() -> Vec<Stmt> {
    return_body(new_object_expr("InternalIterator", vec![this_expr()]))
}

fn key_at(index: Expr) -> Expr {
    array_access(keys_expr(), index)
}

fn value_at(index: Expr) -> Expr {
    array_access(values_expr(), index)
}

fn array_iterator_construct_body() -> Vec<Stmt> {
    let mut body = array_object_construct_body();
    body.insert(2, property_assign_stmt(this_expr(), "position", int_expr(0)));
    body
}

fn array_object_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "keys", function_call("array_keys", vec![var_expr("array")])),
        property_assign_stmt(
            this_expr(),
            "values",
            function_call("array_values", vec![var_expr("array")]),
        ),
        property_assign_stmt(this_expr(), "flags", var_expr("flags")),
    ]
}

fn array_current_body() -> Vec<Stmt> {
    return_body(value_at(position_expr()))
}

fn array_key_body() -> Vec<Stmt> {
    return_body(key_at(position_expr()))
}

fn array_next_body() -> Vec<Stmt> {
    vec![property_assign_stmt(
        this_expr(),
        "position",
        binary_expr(position_expr(), BinOp::Add, int_expr(1)),
    )]
}

fn array_rewind_body() -> Vec<Stmt> {
    vec![property_assign_stmt(this_expr(), "position", int_expr(0))]
}

fn array_valid_body() -> Vec<Stmt> {
    return_body(binary_expr(position_expr(), BinOp::Lt, count_expr(values_expr())))
}

fn array_count_body() -> Vec<Stmt> {
    return_body(count_expr(values_expr()))
}

fn array_append_body() -> Vec<Stmt> {
    vec![
        property_array_push_stmt(this_expr(), "keys", count_expr(keys_expr())),
        property_array_push_stmt(this_expr(), "values", var_expr("value")),
    ]
}

fn array_offset_exists_body() -> Vec<Stmt> {
    let mut body = array_search_prelude();
    body.push(while_stmt(
        binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
        vec![
            if_stmt(
                binary_expr(key_at(var_expr("i")), BinOp::StrictEq, var_expr("offset")),
                return_body(bool_expr(true)),
                None,
            ),
            increment_stmt("i"),
        ],
    ));
    body.push(return_stmt(bool_expr(false)));
    body
}

fn array_offset_get_body() -> Vec<Stmt> {
    let mut body = array_search_prelude();
    body.push(while_stmt(
        binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
        vec![
            if_stmt(
                binary_expr(key_at(var_expr("i")), BinOp::StrictEq, var_expr("offset")),
                return_body(value_at(var_expr("i"))),
                None,
            ),
            increment_stmt("i"),
        ],
    ));
    body.push(return_stmt(null_expr()));
    body
}

fn array_offset_set_body() -> Vec<Stmt> {
    let mut body = vec![if_stmt(
        binary_expr(var_expr("offset"), BinOp::StrictEq, null_expr()),
        vec![
            expr_stmt(method_call(this_expr(), "append", vec![var_expr("value")])),
            return_void_stmt(),
        ],
        None,
    )];
    body.extend(array_search_prelude());
    body.push(while_stmt(
        binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
        vec![
            if_stmt(
                binary_expr(key_at(var_expr("i")), BinOp::StrictEq, var_expr("offset")),
                vec![
                    property_array_assign_stmt(this_expr(), "values", var_expr("i"), var_expr("value")),
                    return_void_stmt(),
                ],
                None,
            ),
            increment_stmt("i"),
        ],
    ));
    body.push(property_array_push_stmt(this_expr(), "keys", var_expr("offset")));
    body.push(property_array_push_stmt(this_expr(), "values", var_expr("value")));
    body
}

fn array_offset_unset_body() -> Vec<Stmt> {
    vec![
        assign_stmt("newKeys", empty_array_expr()),
        assign_stmt("newValues", empty_array_expr()),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(keys_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    not_expr(binary_expr(key_at(var_expr("i")), BinOp::StrictEq, var_expr("offset"))),
                    vec![
                        array_push_stmt("newKeys", key_at(var_expr("i"))),
                        array_push_stmt("newValues", value_at(var_expr("i"))),
                    ],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        property_assign_stmt(this_expr(), "keys", var_expr("newKeys")),
        property_assign_stmt(this_expr(), "values", var_expr("newValues")),
    ]
}

fn array_copy_body() -> Vec<Stmt> {
    vec![
        assign_stmt("out", empty_assoc_array_expr()),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(keys_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                array_assign_stmt("out", key_at(var_expr("i")), value_at(var_expr("i"))),
                increment_stmt("i"),
            ],
        ),
        return_stmt(var_expr("out")),
    ]
}

fn array_object_get_iterator_body() -> Vec<Stmt> {
    vec![
        assign_stmt("it", new_object_expr("ArrayIterator", vec![empty_array_expr()])),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(keys_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                expr_stmt(method_call(
                    var_expr("it"),
                    "offsetSet",
                    vec![key_at(var_expr("i")), value_at(var_expr("i"))],
                )),
                increment_stmt("i"),
            ],
        ),
        return_stmt(var_expr("it")),
    ]
}

fn recursive_array_iterator_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "keys", empty_array_expr()),
        property_assign_stmt(this_expr(), "values", empty_array_expr()),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
        property_assign_stmt(this_expr(), "flags", var_expr("flags")),
        foreach_stmt(
            var_expr("array"),
            Some("key"),
            "value",
            vec![
                property_array_push_stmt(this_expr(), "keys", var_expr("key")),
                property_array_push_stmt(this_expr(), "values", var_expr("value")),
            ],
        ),
    ]
}

fn gettype_is_array_expr(value: Expr) -> Expr {
    binary_expr(
        function_call("gettype", vec![value]),
        BinOp::StrictEq,
        string_expr("array"),
    )
}

fn recursive_current_expr() -> Expr {
    method_call(this_expr(), "current", Vec::new())
}

fn assume_recursive_iterator_expr(value: Expr) -> Expr {
    method_call(this_expr(), "__elephcAssumeRecursiveIterator", vec![value])
}

fn recursive_array_has_children_body() -> Vec<Stmt> {
    vec![
        assign_stmt("value", recursive_current_expr()),
        return_stmt(binary_expr(
            instanceof_expr(var_expr("value"), "RecursiveIterator"),
            BinOp::Or,
            gettype_is_array_expr(var_expr("value")),
        )),
    ]
}

fn recursive_array_get_children_body() -> Vec<Stmt> {
    vec![
        assign_stmt("value", recursive_current_expr()),
        if_stmt(
            instanceof_expr(var_expr("value"), "RecursiveIterator"),
            return_body(assume_recursive_iterator_expr(var_expr("value"))),
            None,
        ),
        if_stmt(
            gettype_is_array_expr(var_expr("value")),
            return_body(new_object_expr("RecursiveArrayIterator", vec![var_expr("value")])),
            None,
        ),
        return_stmt(null_expr()),
    ]
}

fn iterator_iterator_construct_body() -> Vec<Stmt> {
    vec![property_assign_stmt(this_expr(), "inner", var_expr("iterator"))]
}

fn inner_expr() -> Expr {
    property_access(this_expr(), "inner")
}

fn inner_call(method: &str) -> Expr {
    method_call(inner_expr(), method, Vec::new())
}

fn inner_return_body(method: &str) -> Vec<Stmt> {
    return_body(inner_call(method))
}

fn inner_void_body(method: &str) -> Vec<Stmt> {
    vec![expr_stmt(inner_call(method))]
}

fn recursive_inner_return_body(method: &str) -> Vec<Stmt> {
    return_body(method_call(inner_expr(), method, Vec::new()))
}

fn limit_position_expr() -> Expr {
    property_access(this_expr(), "position")
}

fn limit_offset_expr() -> Expr {
    property_access(this_expr(), "offset")
}

fn limit_bound_expr() -> Expr {
    property_access(this_expr(), "limit")
}

fn limit_iterator_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
        property_assign_stmt(this_expr(), "offset", var_expr("offset")),
        property_assign_stmt(this_expr(), "limit", var_expr("limit")),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
    ]
}

fn limit_rewind_body() -> Vec<Stmt> {
    vec![
        expr_stmt(inner_call("rewind")),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
        while_stmt(
            binary_expr(limit_position_expr(), BinOp::Lt, limit_offset_expr()),
            vec![
                if_stmt(not_expr(inner_call("valid")), vec![return_void_stmt()], None),
                expr_stmt(inner_call("next")),
                property_assign_stmt(
                    this_expr(),
                    "position",
                    binary_expr(limit_position_expr(), BinOp::Add, int_expr(1)),
                ),
            ],
        ),
    ]
}

fn limit_next_body() -> Vec<Stmt> {
    vec![
        expr_stmt(inner_call("next")),
        property_assign_stmt(
            this_expr(),
            "position",
            binary_expr(limit_position_expr(), BinOp::Add, int_expr(1)),
        ),
    ]
}

fn limit_valid_body() -> Vec<Stmt> {
    vec![
        if_stmt(not_expr(inner_call("valid")), return_body(bool_expr(false)), None),
        if_stmt(
            binary_expr(limit_bound_expr(), BinOp::Lt, int_expr(0)),
            return_body(bool_expr(true)),
            None,
        ),
        return_stmt(binary_expr(
            binary_expr(limit_position_expr(), BinOp::Sub, limit_offset_expr()),
            BinOp::Lt,
            limit_bound_expr(),
        )),
    ]
}

fn limit_seek_body() -> Vec<Stmt> {
    vec![
        expr_stmt(method_call(this_expr(), "rewind", Vec::new())),
        while_stmt(
            binary_expr(limit_position_expr(), BinOp::Lt, var_expr("offset")),
            vec![
                if_stmt(
                    not_expr(method_call(this_expr(), "valid", Vec::new())),
                    vec![return_void_stmt()],
                    None,
                ),
                expr_stmt(method_call(this_expr(), "next", Vec::new())),
            ],
        ),
    ]
}

fn infinite_next_body() -> Vec<Stmt> {
    vec![
        expr_stmt(inner_call("next")),
        if_stmt(not_expr(inner_call("valid")), inner_void_body("rewind"), None),
    ]
}

fn filter_rewind_body() -> Vec<Stmt> {
    let mut body = inner_void_body("rewind");
    body.extend(filter_skip_rejected_body());
    body
}

fn filter_next_body() -> Vec<Stmt> {
    let mut body = inner_void_body("next");
    body.extend(filter_skip_rejected_body());
    body
}

fn filter_skip_rejected_body() -> Vec<Stmt> {
    vec![while_stmt(
        binary_expr(
            inner_call("valid"),
            BinOp::And,
            not_expr(method_call(this_expr(), "accept", Vec::new())),
        ),
        inner_void_body("next"),
    )]
}

fn callback_filter_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
        property_assign_stmt(this_expr(), "callback", var_expr("callback")),
    ]
}

fn callback_filter_accept_body() -> Vec<Stmt> {
    return_body(cast_expr(
        CastType::Bool,
        method_call(
            this_expr(),
            "__elephcAcceptCallback",
            vec![
                inner_call("current"),
                inner_call("key"),
                inner_expr(),
            ],
        ),
    ))
}

fn recursive_filter_get_children_body() -> Vec<Stmt> {
    vec![
        assign_stmt("child", method_call(inner_expr(), "getChildren", Vec::new())),
        if_stmt(
            function_call("is_null", vec![var_expr("child")]),
            return_body(null_expr()),
            None,
        ),
        return_stmt(new_static_expr(vec![assume_recursive_iterator_expr(var_expr("child"))])),
    ]
}

fn recursive_callback_filter_get_children_body() -> Vec<Stmt> {
    vec![
        assign_stmt("child", method_call(inner_expr(), "getChildren", Vec::new())),
        if_stmt(
            function_call("is_null", vec![var_expr("child")]),
            return_body(null_expr()),
            None,
        ),
        assign_stmt(
            "next",
            new_object_expr(
                "RecursiveCallbackFilterIterator",
                vec![
                    assume_recursive_iterator_expr(var_expr("child")),
                    property_access(this_expr(), "callback"),
                ],
            ),
        ),
        expr_stmt(method_call(
            var_expr("next"),
            "__elephcSetCallbackEnv",
            vec![property_access(this_expr(), "callbackEnv")],
        )),
        return_stmt(var_expr("next")),
    ]
}

fn parent_iterator_get_children_body() -> Vec<Stmt> {
    vec![
        assign_stmt("child", method_call(inner_expr(), "getChildren", Vec::new())),
        if_stmt(
            function_call("is_null", vec![var_expr("child")]),
            return_body(null_expr()),
            None,
        ),
        return_stmt(new_object_expr(
            "ParentIterator",
            vec![assume_recursive_iterator_expr(var_expr("child"))],
        )),
    ]
}

fn recursive_iterator_iterator_root_expr() -> Expr {
    property_access(this_expr(), "root")
}

fn recursive_iterator_iterator_mode_expr() -> Expr {
    property_access(this_expr(), "mode")
}

fn recursive_iterator_iterator_iterators_expr() -> Expr {
    property_access(this_expr(), "iterators")
}

fn recursive_iterator_iterator_states_expr() -> Expr {
    property_access(this_expr(), "states")
}

fn recursive_iterator_iterator_depths_expr() -> Expr {
    property_access(this_expr(), "depths")
}

fn recursive_iterator_iterator_depth_expr() -> Expr {
    property_access(this_expr(), "depth")
}

fn recursive_iterator_iterator_slot_expr() -> Expr {
    property_access(this_expr(), "slot")
}

fn recursive_iterator_iterator_current_valid_expr() -> Expr {
    property_access(this_expr(), "currentValid")
}

fn recursive_iterator_iterator_valid_expr() -> Expr {
    recursive_iterator_iterator_current_valid_expr()
}

fn recursive_iterator_iterator_iterator_at_depth(depth: Expr) -> Expr {
    array_access(
        recursive_iterator_iterator_iterators_expr(),
        depth,
    )
}

fn recursive_iterator_iterator_state_at_current_slot() -> Expr {
    array_access(
        recursive_iterator_iterator_states_expr(),
        recursive_iterator_iterator_slot_expr(),
    )
}

fn recursive_iterator_iterator_depth_at_current_slot() -> Expr {
    array_access(
        recursive_iterator_iterator_depths_expr(),
        recursive_iterator_iterator_slot_expr(),
    )
}

fn recursive_iterator_iterator_current_iterator_expr() -> Expr {
    recursive_iterator_iterator_iterator_at_depth(recursive_iterator_iterator_slot_expr())
}

fn recursive_iterator_iterator_slot_for_depth_expr(depth: Expr) -> Expr {
    method_call(this_expr(), "__elephcSlotForDepth", vec![depth])
}

fn recursive_iterator_iterator_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "root", var_expr("iterator")),
        property_assign_stmt(this_expr(), "mode", var_expr("mode")),
        property_assign_stmt(this_expr(), "flags", var_expr("flags")),
        property_assign_stmt(this_expr(), "iterators", empty_array_expr()),
        property_assign_stmt(this_expr(), "states", empty_array_expr()),
        property_assign_stmt(this_expr(), "depths", empty_array_expr()),
        property_assign_stmt(this_expr(), "depth", int_expr(0)),
        property_assign_stmt(this_expr(), "slot", int_expr(0)),
        property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
    ]
}

fn recursive_iterator_iterator_rewind_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "iterators", empty_array_expr()),
        property_assign_stmt(this_expr(), "states", empty_array_expr()),
        property_assign_stmt(this_expr(), "depths", empty_array_expr()),
        property_assign_stmt(this_expr(), "depth", int_expr(0)),
        property_assign_stmt(this_expr(), "slot", int_expr(0)),
        property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
        expr_stmt(method_call(
            recursive_iterator_iterator_root_expr(),
            "rewind",
            Vec::new(),
        )),
        if_stmt(
            method_call(recursive_iterator_iterator_root_expr(), "valid", Vec::new()),
            vec![
                property_array_push_stmt(this_expr(), "iterators", recursive_iterator_iterator_root_expr()),
                property_array_push_stmt(this_expr(), "states", int_expr(0)),
                property_array_push_stmt(this_expr(), "depths", int_expr(0)),
                expr_stmt(method_call(this_expr(), "__elephcAdvance", Vec::new())),
            ],
            None,
        ),
    ]
}

fn recursive_iterator_iterator_valid_body() -> Vec<Stmt> {
    return_body(recursive_iterator_iterator_valid_expr())
}

fn recursive_iterator_iterator_current_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(recursive_iterator_iterator_valid_expr()),
            null_return_body(),
            None,
        ),
        return_stmt(method_call(
            recursive_iterator_iterator_current_iterator_expr(),
            "current",
            Vec::new(),
        )),
    ]
}

fn recursive_iterator_iterator_key_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(recursive_iterator_iterator_valid_expr()),
            null_return_body(),
            None,
        ),
        return_stmt(method_call(
            recursive_iterator_iterator_current_iterator_expr(),
            "key",
            Vec::new(),
        )),
    ]
}

fn recursive_iterator_iterator_next_body() -> Vec<Stmt> {
    vec![if_stmt(
        recursive_iterator_iterator_valid_expr(),
        vec![expr_stmt(method_call(this_expr(), "__elephcAdvance", Vec::new()))],
        None,
    )]
}

fn recursive_iterator_iterator_get_depth_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(recursive_iterator_iterator_valid_expr()),
            return_body(int_expr(0)),
            None,
        ),
        return_stmt(recursive_iterator_iterator_depth_expr()),
    ]
}

fn recursive_iterator_iterator_get_inner_iterator_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            recursive_iterator_iterator_valid_expr(),
            return_body(recursive_iterator_iterator_current_iterator_expr()),
            None,
        ),
        return_stmt(recursive_iterator_iterator_root_expr()),
    ]
}

fn recursive_iterator_iterator_get_sub_iterator_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(recursive_iterator_iterator_valid_expr()),
            vec![
                if_stmt(
                    binary_expr(var_expr("level"), BinOp::LtEq, int_expr(0)),
                    return_body(recursive_iterator_iterator_root_expr()),
                    None,
                ),
                return_stmt(null_expr()),
            ],
            None,
        ),
        if_stmt(
            binary_expr(var_expr("level"), BinOp::Lt, int_expr(0)),
            return_body(recursive_iterator_iterator_current_iterator_expr()),
            None,
        ),
        if_stmt(
            binary_expr(
                var_expr("level"),
                BinOp::LtEq,
                recursive_iterator_iterator_depth_expr(),
            ),
            return_body(recursive_iterator_iterator_iterator_at_depth(
                recursive_iterator_iterator_slot_for_depth_expr(var_expr("level")),
            )),
            None,
        ),
        return_stmt(null_expr()),
    ]
}

fn recursive_iterator_iterator_slot_for_depth_body() -> Vec<Stmt> {
    vec![
        typed_assign_stmt("i", TypeExpr::Int, int_expr(0)),
        typed_assign_stmt("slot", TypeExpr::Int, int_expr(0)),
        typed_assign_stmt("limit", TypeExpr::Int, count_expr(recursive_iterator_iterator_depths_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    binary_expr(
                        array_access(recursive_iterator_iterator_depths_expr(), var_expr("i")),
                        BinOp::StrictEq,
                        var_expr("level"),
                    ),
                    vec![assign_stmt("slot", var_expr("i"))],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(var_expr("slot")),
    ]
}

fn recursive_iterator_iterator_advance_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
        property_assign_stmt(
            this_expr(),
            "depth",
            recursive_iterator_iterator_depth_at_current_slot(),
        ),
        assign_stmt("iterator", recursive_iterator_iterator_current_iterator_expr()),
        if_stmt(
            not_expr(method_call(var_expr("iterator"), "valid", Vec::new())),
            recursive_iterator_iterator_pop_invalid_frame_body(),
            None,
        ),
        assign_stmt(
            "state",
            recursive_iterator_iterator_state_at_current_slot(),
        ),
        if_stmt(
            binary_expr(var_expr("state"), BinOp::StrictEq, int_expr(2)),
            vec![
                expr_stmt(method_call(var_expr("iterator"), "next", Vec::new())),
                property_array_assign_stmt(this_expr(), "states", recursive_iterator_iterator_slot_expr(), int_expr(0)),
                expr_stmt(method_call(this_expr(), "__elephcAdvance", Vec::new())),
                return_void_stmt(),
            ],
            None,
        ),
        if_stmt(
            binary_expr(
                recursive_iterator_iterator_mode_expr(),
                BinOp::StrictEq,
                int_expr(1),
            ),
            recursive_iterator_iterator_advance_self_first_body(),
            Some(recursive_iterator_iterator_advance_children_first_or_leaves_body()),
        ),
    ]
}

fn recursive_iterator_iterator_pop_invalid_frame_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(recursive_iterator_iterator_depth_expr(), BinOp::StrictEq, int_expr(0)),
            vec![
                property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
                return_void_stmt(),
            ],
            Some(vec![
                typed_assign_stmt(
                    "previousDepth",
                    TypeExpr::Int,
                    binary_expr(recursive_iterator_iterator_depth_expr(), BinOp::Sub, int_expr(1)),
                ),
                property_assign_stmt(
                    this_expr(),
                    "depth",
                    var_expr("previousDepth"),
                ),
                property_assign_stmt(
                    this_expr(),
                    "slot",
                    recursive_iterator_iterator_slot_for_depth_expr(var_expr("previousDepth")),
                ),
                expr_stmt(method_call(this_expr(), "__elephcAdvance", Vec::new())),
                return_void_stmt(),
            ]),
        ),
    ]
}

fn recursive_iterator_iterator_advance_self_first_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(var_expr("state"), BinOp::StrictEq, int_expr(0)),
            vec![
                property_array_assign_stmt(this_expr(), "states", recursive_iterator_iterator_slot_expr(), int_expr(1)),
                property_assign_stmt(this_expr(), "currentValid", bool_expr(true)),
                return_void_stmt(),
            ],
            None,
        ),
        assign_stmt(
            "hasChildren",
            method_call(var_expr("iterator"), "hasChildren", Vec::new()),
        ),
        if_stmt(
            var_expr("hasChildren"),
            vec![
                assign_stmt("child", method_call(var_expr("iterator"), "getChildren", Vec::new())),
                if_stmt(
                    not_expr(function_call("is_null", vec![var_expr("child")])),
                    recursive_iterator_iterator_descend_current_child_body(int_expr(2)),
                    None,
                ),
            ],
            None,
        ),
        property_array_assign_stmt(this_expr(), "states", recursive_iterator_iterator_slot_expr(), int_expr(2)),
        expr_stmt(method_call(this_expr(), "__elephcAdvance", Vec::new())),
        return_void_stmt(),
    ]
}

fn recursive_iterator_iterator_advance_children_first_or_leaves_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(var_expr("state"), BinOp::StrictEq, int_expr(1)),
            vec![
                property_array_assign_stmt(this_expr(), "states", recursive_iterator_iterator_slot_expr(), int_expr(2)),
                property_assign_stmt(this_expr(), "currentValid", bool_expr(true)),
                return_void_stmt(),
            ],
            None,
        ),
        assign_stmt(
            "hasChildren",
            method_call(var_expr("iterator"), "hasChildren", Vec::new()),
        ),
        if_stmt(
            var_expr("hasChildren"),
            vec![
                assign_stmt("child", method_call(var_expr("iterator"), "getChildren", Vec::new())),
                if_stmt(
                    not_expr(function_call("is_null", vec![var_expr("child")])),
                    recursive_iterator_iterator_non_self_child_body(),
                    None,
                ),
            ],
            None,
        ),
        property_array_assign_stmt(this_expr(), "states", recursive_iterator_iterator_slot_expr(), int_expr(2)),
        property_assign_stmt(this_expr(), "currentValid", bool_expr(true)),
        return_void_stmt(),
    ]
}

fn recursive_iterator_iterator_non_self_child_body() -> Vec<Stmt> {
    let mut body = recursive_iterator_iterator_descend_current_child_body(int_expr(2));
    body.push(if_stmt(
        binary_expr(
            recursive_iterator_iterator_mode_expr(),
            BinOp::StrictEq,
            int_expr(0),
        ),
        vec![
            property_array_assign_stmt(this_expr(), "states", recursive_iterator_iterator_slot_expr(), int_expr(2)),
            expr_stmt(method_call(this_expr(), "__elephcAdvance", Vec::new())),
            return_void_stmt(),
        ],
        None,
    ));
    body
}

fn recursive_iterator_iterator_descend_current_child_body(parent_state: Expr) -> Vec<Stmt> {
    vec![
        assign_stmt(
            "recursiveChild",
            assume_recursive_iterator_expr(var_expr("child")),
        ),
        expr_stmt(method_call(var_expr("recursiveChild"), "rewind", Vec::new())),
        if_stmt(
            method_call(var_expr("recursiveChild"), "valid", Vec::new()),
            vec![
                if_stmt(
                    binary_expr(
                        recursive_iterator_iterator_mode_expr(),
                        BinOp::StrictEq,
                        int_expr(2),
                    ),
                    vec![property_array_assign_stmt(this_expr(), "states", recursive_iterator_iterator_slot_expr(), int_expr(1))],
                    Some(vec![property_array_assign_stmt(
                        this_expr(),
                        "states",
                        recursive_iterator_iterator_slot_expr(),
                        parent_state,
                    )]),
                ),
                typed_assign_stmt(
                    "nextDepth",
                    TypeExpr::Int,
                    binary_expr(recursive_iterator_iterator_depth_expr(), BinOp::Add, int_expr(1)),
                ),
                typed_assign_stmt("nextSlot", TypeExpr::Int, count_expr(recursive_iterator_iterator_iterators_expr())),
                property_array_push_stmt(this_expr(), "iterators", var_expr("recursiveChild")),
                property_array_push_stmt(this_expr(), "states", int_expr(0)),
                property_array_push_stmt(this_expr(), "depths", var_expr("nextDepth")),
                property_assign_stmt(this_expr(), "depth", var_expr("nextDepth")),
                property_assign_stmt(this_expr(), "slot", var_expr("nextSlot")),
                expr_stmt(method_call(this_expr(), "__elephcAdvance", Vec::new())),
                return_void_stmt(),
            ],
            None,
        ),
    ]
}

fn caching_flags_expr() -> Expr {
    property_access(this_expr(), "flags")
}

fn caching_cache_expr() -> Expr {
    property_access(this_expr(), "cache")
}

fn caching_current_key_expr() -> Expr {
    property_access(this_expr(), "currentKey")
}

fn caching_current_value_expr() -> Expr {
    property_access(this_expr(), "currentValue")
}

fn caching_current_valid_expr() -> Expr {
    property_access(this_expr(), "currentValid")
}

fn caching_has_next_expr() -> Expr {
    property_access(this_expr(), "cachedHasNext")
}

fn caching_flag_enabled_expr(flags: Expr, bit: i64) -> Expr {
    binary_expr(
        binary_expr(flags, BinOp::BitAnd, int_expr(bit)),
        BinOp::NotEq,
        int_expr(0),
    )
}

fn caching_full_cache_expr() -> Expr {
    caching_flag_enabled_expr(caching_flags_expr(), 256)
}

fn caching_construct_body() -> Vec<Stmt> {
    let mut body = caching_validate_flags_body("CachingIterator::__construct", "flags");
    body.extend(vec![
        property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
        property_assign_stmt(this_expr(), "flags", var_expr("flags")),
        property_assign_stmt(this_expr(), "cache", empty_assoc_array_expr()),
        property_assign_stmt(this_expr(), "currentKey", null_expr()),
        property_assign_stmt(this_expr(), "currentValue", null_expr()),
        property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
        property_assign_stmt(this_expr(), "cachedHasNext", bool_expr(false)),
    ]);
    body
}

fn caching_validate_flags_body(context: &str, var_name: &str) -> Vec<Stmt> {
    let mut body = vec![assign_stmt("stringFlagCount", int_expr(0))];
    for bit in [1, 2, 4, 8] {
        body.push(if_stmt(
            caching_flag_enabled_expr(var_expr(var_name), bit),
            vec![assign_stmt(
                "stringFlagCount",
                binary_expr(var_expr("stringFlagCount"), BinOp::Add, int_expr(1)),
            )],
            None,
        ));
    }
    body.push(if_stmt(
        binary_expr(var_expr("stringFlagCount"), BinOp::Gt, int_expr(1)),
        vec![throw_stmt(new_object_expr(
            "ValueError",
            vec![string_expr(&format!(
                "{context}(): Argument #{} ($flags) must contain only one of CachingIterator::CALL_TOSTRING, CachingIterator::TOSTRING_USE_KEY, CachingIterator::TOSTRING_USE_CURRENT, or CachingIterator::TOSTRING_USE_INNER",
                if context.ends_with("__construct") { 2 } else { 1 }
            ))],
        ))],
        None,
    ));
    body
}

fn caching_rewind_body() -> Vec<Stmt> {
    vec![
        expr_stmt(inner_call("rewind")),
        expr_stmt(method_call(this_expr(), "__elephcCaptureCurrent", Vec::new())),
    ]
}

fn caching_next_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(caching_has_next_expr()),
            vec![
                property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
                property_assign_stmt(this_expr(), "cachedHasNext", bool_expr(false)),
                property_assign_stmt(this_expr(), "currentKey", null_expr()),
                property_assign_stmt(this_expr(), "currentValue", null_expr()),
                return_void_stmt(),
            ],
            None,
        ),
        expr_stmt(method_call(this_expr(), "__elephcCaptureCurrent", Vec::new())),
    ]
}

fn caching_capture_current_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(inner_call("valid")),
            vec![
                property_assign_stmt(this_expr(), "currentValid", bool_expr(false)),
                property_assign_stmt(this_expr(), "cachedHasNext", bool_expr(false)),
                property_assign_stmt(this_expr(), "currentKey", null_expr()),
                property_assign_stmt(this_expr(), "currentValue", null_expr()),
                return_void_stmt(),
            ],
            None,
        ),
        property_assign_stmt(this_expr(), "currentKey", inner_call("key")),
        property_assign_stmt(this_expr(), "currentValue", inner_call("current")),
        property_assign_stmt(this_expr(), "currentValid", bool_expr(true)),
        if_stmt(
            caching_full_cache_expr(),
            vec![property_array_assign_stmt(
                this_expr(),
                "cache",
                caching_current_key_expr(),
                caching_current_value_expr(),
            )],
            None,
        ),
        expr_stmt(inner_call("next")),
        property_assign_stmt(this_expr(), "cachedHasNext", inner_call("valid")),
    ]
}

fn caching_current_body() -> Vec<Stmt> {
    vec![
        if_stmt(not_expr(caching_current_valid_expr()), null_return_body(), None),
        return_stmt(caching_current_value_expr()),
    ]
}

fn caching_key_body() -> Vec<Stmt> {
    vec![
        if_stmt(not_expr(caching_current_valid_expr()), null_return_body(), None),
        return_stmt(caching_current_key_expr()),
    ]
}

fn caching_to_string_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(
                binary_expr(caching_flags_expr(), BinOp::BitAnd, int_expr(15)),
                BinOp::StrictEq,
                int_expr(0),
            ),
            vec![throw_stmt(new_object_expr(
                "BadMethodCallException",
                vec![string_expr(
                    "CachingIterator does not fetch string value (see CachingIterator::__construct)",
                )],
            ))],
            None,
        ),
        if_stmt(not_expr(caching_current_valid_expr()), return_body(string_expr("")), None),
        if_stmt(
            caching_flag_enabled_expr(caching_flags_expr(), 2),
            return_body(cast_expr(CastType::String, caching_current_key_expr())),
            None,
        ),
        if_stmt(
            caching_flag_enabled_expr(caching_flags_expr(), 8),
            return_body(cast_expr(CastType::String, inner_expr())),
            None,
        ),
        return_stmt(cast_expr(CastType::String, caching_current_value_expr())),
    ]
}

fn caching_set_flags_body() -> Vec<Stmt> {
    let mut body = caching_validate_flags_body("CachingIterator::setFlags", "flags");
    body.push(if_stmt(
        binary_expr(
            caching_flag_enabled_expr(caching_flags_expr(), 1),
            BinOp::And,
            not_expr(caching_flag_enabled_expr(var_expr("flags"), 1)),
        ),
        vec![throw_stmt(new_object_expr(
            "InvalidArgumentException",
            vec![string_expr("Unsetting flag CALL_TO_STRING is not possible")],
        ))],
        None,
    ));
    body.push(property_assign_stmt(this_expr(), "flags", var_expr("flags")));
    body
}

fn caching_require_full_cache_body(mut body: Vec<Stmt>) -> Vec<Stmt> {
    let mut out = vec![if_stmt(
        not_expr(caching_full_cache_expr()),
        vec![throw_stmt(new_object_expr(
            "BadMethodCallException",
            vec![string_expr(
                "CachingIterator does not use a full cache (see CachingIterator::__construct)",
            )],
        ))],
        None,
    )];
    out.append(&mut body);
    out
}

fn caching_offset_get_body() -> Vec<Stmt> {
    caching_require_full_cache_body(return_body(array_access(caching_cache_expr(), var_expr("key"))))
}

fn caching_offset_set_body() -> Vec<Stmt> {
    caching_require_full_cache_body(vec![property_array_assign_stmt(
        this_expr(),
        "cache",
        var_expr("key"),
        var_expr("value"),
    )])
}

fn caching_offset_unset_body() -> Vec<Stmt> {
    caching_require_full_cache_body(vec![
        assign_stmt("newCache", empty_assoc_array_expr()),
        foreach_stmt(
            caching_cache_expr(),
            Some("cacheKey"),
            "cacheValue",
            vec![if_stmt(
                not_expr(binary_expr(var_expr("cacheKey"), BinOp::StrictEq, var_expr("key"))),
                vec![array_assign_stmt(
                    "newCache",
                    var_expr("cacheKey"),
                    var_expr("cacheValue"),
                )],
                None,
            )],
        ),
        property_assign_stmt(this_expr(), "cache", var_expr("newCache")),
    ])
}

fn caching_offset_exists_body() -> Vec<Stmt> {
    caching_require_full_cache_body(return_body(function_call(
        "array_key_exists",
        vec![var_expr("key"), caching_cache_expr()],
    )))
}

fn caching_get_cache_body() -> Vec<Stmt> {
    caching_require_full_cache_body(return_body(caching_cache_expr()))
}

fn caching_count_body() -> Vec<Stmt> {
    caching_require_full_cache_body(return_body(count_expr(caching_cache_expr())))
}

fn append_iterators_expr() -> Expr {
    property_access(this_expr(), "iterators")
}

fn append_iterator_keys_expr() -> Expr {
    property_access(this_expr(), "iteratorKeys")
}

fn append_iterator_active_expr() -> Expr {
    property_access(this_expr(), "iteratorActive")
}

fn append_array_iterator_expr() -> Expr {
    property_access(this_expr(), "arrayIterator")
}

fn append_index_expr() -> Expr {
    property_access(this_expr(), "index")
}

fn append_key_at_position_expr(position: Expr) -> Expr {
    array_access(append_iterator_keys_expr(), position)
}

fn append_active_at_position_expr(position: Expr) -> Expr {
    array_access(append_iterator_active_expr(), position)
}

fn append_current_key_expr() -> Expr {
    append_key_at_position_expr(append_index_expr())
}

fn append_current_iterator_expr() -> Expr {
    array_access(append_iterators_expr(), append_index_expr())
}

fn append_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "inner", new_object_expr("EmptyIterator", Vec::new())),
        property_assign_stmt(this_expr(), "iterators", empty_array_expr()),
        property_assign_stmt(this_expr(), "iteratorKeys", empty_array_expr()),
        property_assign_stmt(this_expr(), "iteratorActive", empty_array_expr()),
        property_assign_stmt(this_expr(), "index", int_expr(0)),
        property_assign_stmt(
            this_expr(),
            "arrayIterator",
            new_object_expr("__ElephcAppendIteratorArrayIterator", vec![this_expr()]),
        ),
    ]
}

fn append_append_body() -> Vec<Stmt> {
    append_storage_append_body()
}

fn append_rewind_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "index", int_expr(0)),
        property_assign_stmt(this_expr(), "inner", new_object_expr("EmptyIterator", Vec::new())),
        if_stmt(
            binary_expr(count_expr(append_iterators_expr()), BinOp::StrictEq, int_expr(0)),
            vec![return_void_stmt()],
            None,
        ),
        typed_assign_stmt("iterator", named_type("Iterator"), append_current_iterator_expr()),
        property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
        expr_stmt(method_call(var_expr("iterator"), "rewind", Vec::new())),
        expr_stmt(method_call(this_expr(), "valid", Vec::new())),
    ]
}

fn append_valid_body() -> Vec<Stmt> {
    let mut active_body = vec![
        typed_assign_stmt("iterator", named_type("Iterator"), append_current_iterator_expr()),
        if_stmt(
            method_call(var_expr("iterator"), "valid", Vec::new()),
            vec![
                property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
                return_stmt(bool_expr(true)),
            ],
            None,
        ),
    ];
    active_body.extend(append_advance_index_body());

    vec![
        while_stmt(
            binary_expr(append_index_expr(), BinOp::Lt, count_expr(append_iterators_expr())),
            vec![if_stmt(
                not_expr(append_active_at_position_expr(append_index_expr())),
                append_advance_index_body(),
                Some(active_body),
            )],
        ),
        property_assign_stmt(this_expr(), "inner", new_object_expr("EmptyIterator", Vec::new())),
        return_stmt(bool_expr(false)),
    ]
}

fn append_advance_index_body() -> Vec<Stmt> {
    vec![
        append_advance_index_stmt(),
        if_stmt(
            binary_expr(append_index_expr(), BinOp::Lt, count_expr(append_iterators_expr())),
            vec![
                typed_assign_stmt("iterator", named_type("Iterator"), append_current_iterator_expr()),
                property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
                expr_stmt(method_call(var_expr("iterator"), "rewind", Vec::new())),
            ],
            None,
        ),
    ]
}

fn append_advance_index_stmt() -> Stmt {
    property_assign_stmt(
        this_expr(),
        "index",
        binary_expr(append_index_expr(), BinOp::Add, int_expr(1)),
    )
}

fn append_current_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(method_call(this_expr(), "valid", Vec::new())),
            null_return_body(),
            None,
        ),
        return_stmt(inner_call("current")),
    ]
}

fn append_key_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(method_call(this_expr(), "valid", Vec::new())),
            null_return_body(),
            None,
        ),
        return_stmt(inner_call("key")),
    ]
}

fn append_next_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(method_call(this_expr(), "valid", Vec::new())),
            vec![return_void_stmt()],
            None,
        ),
        typed_assign_stmt("iterator", named_type("Iterator"), inner_expr()),
        expr_stmt(method_call(var_expr("iterator"), "next", Vec::new())),
        if_stmt(
            not_expr(method_call(var_expr("iterator"), "valid", Vec::new())),
            vec![
                property_assign_stmt(
                    this_expr(),
                    "index",
                    binary_expr(append_index_expr(), BinOp::Add, int_expr(1)),
                ),
                if_stmt(
                    binary_expr(append_index_expr(), BinOp::Lt, count_expr(append_iterators_expr())),
                    vec![
                        typed_assign_stmt("iterator", named_type("Iterator"), append_current_iterator_expr()),
                        property_assign_stmt(this_expr(), "inner", var_expr("iterator")),
                        expr_stmt(method_call(var_expr("iterator"), "rewind", Vec::new())),
                    ],
                    None,
                ),
            ],
            None,
        ),
        expr_stmt(method_call(this_expr(), "valid", Vec::new())),
    ]
}

fn append_get_inner_iterator_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(method_call(this_expr(), "valid", Vec::new())),
            null_return_body(),
            None,
        ),
        return_stmt(inner_expr()),
    ]
}

fn append_get_iterator_index_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            not_expr(method_call(this_expr(), "valid", Vec::new())),
            null_return_body(),
            None,
        ),
        return_stmt(append_current_key_expr()),
    ]
}

fn append_get_array_iterator_body() -> Vec<Stmt> {
    return_body(append_array_iterator_expr())
}

fn append_storage_append_body() -> Vec<Stmt> {
    vec![
        property_array_push_stmt(this_expr(), "iteratorKeys", count_expr(append_iterator_keys_expr())),
        property_array_push_stmt(this_expr(), "iterators", var_expr("iterator")),
        property_array_push_stmt(this_expr(), "iteratorActive", bool_expr(true)),
    ]
}

fn append_storage_offset_set_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(var_expr("offset"), BinOp::StrictEq, null_expr()),
            append_storage_append_body(),
            Some(vec![
                assign_stmt("i", int_expr(0)),
                assign_stmt("limit", count_expr(append_iterator_keys_expr())),
                while_stmt(
                    binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
                    vec![
                        if_stmt(
                            binary_expr(append_key_at_position_expr(var_expr("i")), BinOp::StrictEq, var_expr("offset")),
                            vec![
                                property_array_assign_stmt(
                                    this_expr(),
                                    "iterators",
                                    var_expr("i"),
                                    var_expr("iterator"),
                                ),
                                property_array_assign_stmt(
                                    this_expr(),
                                    "iteratorActive",
                                    var_expr("i"),
                                    bool_expr(true),
                                ),
                                return_void_stmt(),
                            ],
                            None,
                        ),
                        increment_stmt("i"),
                    ],
                ),
                property_array_push_stmt(this_expr(), "iteratorKeys", var_expr("offset")),
                property_array_push_stmt(this_expr(), "iterators", var_expr("iterator")),
                property_array_push_stmt(this_expr(), "iteratorActive", bool_expr(true)),
            ]),
        ),
    ]
}

fn append_storage_count_body() -> Vec<Stmt> {
    vec![
        assign_stmt("count", int_expr(0)),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(append_iterators_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    append_active_at_position_expr(var_expr("i")),
                    vec![assign_stmt(
                        "count",
                        binary_expr(var_expr("count"), BinOp::Add, int_expr(1)),
                    )],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(var_expr("count")),
    ]
}

fn append_storage_offset_exists_body() -> Vec<Stmt> {
    let mut body = vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(append_iterator_keys_expr())),
    ];
    body.push(while_stmt(
        binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
        vec![
            if_stmt(
                binary_expr(
                    binary_expr(
                        append_key_at_position_expr(var_expr("i")),
                        BinOp::StrictEq,
                        var_expr("offset"),
                    ),
                    BinOp::And,
                    append_active_at_position_expr(var_expr("i")),
                ),
                return_body(bool_expr(true)),
                None,
            ),
            increment_stmt("i"),
        ],
    ));
    body.push(return_stmt(bool_expr(false)));
    body
}

fn append_storage_offset_get_body() -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(append_iterator_keys_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    binary_expr(
                        binary_expr(
                            append_key_at_position_expr(var_expr("i")),
                            BinOp::StrictEq,
                            var_expr("offset"),
                        ),
                        BinOp::And,
                        append_active_at_position_expr(var_expr("i")),
                    ),
                    return_body(array_access(append_iterators_expr(), var_expr("i"))),
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(null_expr()),
    ]
}

fn append_storage_offset_unset_body() -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(append_iterator_keys_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    binary_expr(append_key_at_position_expr(var_expr("i")), BinOp::StrictEq, var_expr("offset")),
                    vec![
                        property_array_assign_stmt(
                            this_expr(),
                            "iteratorActive",
                            var_expr("i"),
                            bool_expr(false),
                        ),
                        return_void_stmt(),
                    ],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
    ]
}

fn append_storage_get_array_copy_body() -> Vec<Stmt> {
    vec![
        assign_stmt("out", empty_array_expr()),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(append_iterator_keys_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    append_active_at_position_expr(var_expr("i")),
                    vec![array_assign_stmt(
                        "out",
                        append_key_at_position_expr(var_expr("i")),
                        array_access(append_iterators_expr(), var_expr("i")),
                    )],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(var_expr("out")),
    ]
}

fn append_storage_current_body() -> Vec<Stmt> {
    vec![
        assign_stmt("i", var_expr("position")),
        return_stmt(array_access(append_iterators_expr(), var_expr("i"))),
    ]
}

fn append_array_iterator_owner_expr() -> Expr {
    property_access(this_expr(), "owner")
}

fn append_array_iterator_position_expr() -> Expr {
    property_access(this_expr(), "appendPosition")
}

fn append_array_iterator_owner_call(method: &str, args: Vec<Expr>) -> Expr {
    method_call(append_array_iterator_owner_expr(), method, args)
}

fn append_array_iterator_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "owner", var_expr("owner")),
        property_assign_stmt(this_expr(), "appendPosition", int_expr(0)),
    ]
}

fn append_array_iterator_count_body() -> Vec<Stmt> {
    return_body(append_array_iterator_owner_call("__elephcStorageCount", Vec::new()))
}

fn append_array_iterator_append_body() -> Vec<Stmt> {
    vec![expr_stmt(append_array_iterator_owner_call(
        "__elephcStorageAppend",
        vec![var_expr("iterator")],
    ))]
}

fn append_array_iterator_offset_set_body() -> Vec<Stmt> {
    vec![expr_stmt(append_array_iterator_owner_call(
        "__elephcStorageOffsetSet",
        vec![var_expr("offset"), var_expr("iterator")],
    ))]
}

fn append_array_iterator_offset_exists_body() -> Vec<Stmt> {
    return_body(append_array_iterator_owner_call(
        "__elephcStorageOffsetExists",
        vec![var_expr("offset")],
    ))
}

fn append_array_iterator_offset_get_body() -> Vec<Stmt> {
    return_body(append_array_iterator_owner_call(
        "__elephcStorageOffsetGet",
        vec![var_expr("offset")],
    ))
}

fn append_array_iterator_offset_unset_body() -> Vec<Stmt> {
    vec![expr_stmt(append_array_iterator_owner_call(
        "__elephcStorageOffsetUnset",
        vec![var_expr("offset")],
    ))]
}

fn append_array_iterator_copy_body() -> Vec<Stmt> {
    return_body(append_array_iterator_owner_call(
        "__elephcStorageGetArrayCopy",
        Vec::new(),
    ))
}

fn append_array_iterator_rewind_body() -> Vec<Stmt> {
    vec![property_assign_stmt(this_expr(), "appendPosition", int_expr(0))]
}

fn append_array_iterator_next_body() -> Vec<Stmt> {
    vec![property_assign_stmt(
        this_expr(),
        "appendPosition",
        binary_expr(append_array_iterator_position_expr(), BinOp::Add, int_expr(1)),
    )]
}

fn append_array_iterator_valid_body() -> Vec<Stmt> {
    vec![
        while_stmt(
            binary_expr(
                append_array_iterator_position_expr(),
                BinOp::Lt,
                append_array_iterator_owner_call("__elephcStoragePhysicalCount", Vec::new()),
            ),
            vec![
                if_stmt(
                    append_array_iterator_owner_call(
                        "__elephcStorageIsActive",
                        vec![append_array_iterator_position_expr()],
                    ),
                    return_body(bool_expr(true)),
                    None,
                ),
                property_assign_stmt(
                    this_expr(),
                    "appendPosition",
                    binary_expr(append_array_iterator_position_expr(), BinOp::Add, int_expr(1)),
                ),
            ],
        ),
        return_stmt(bool_expr(false)),
    ]
}

fn append_array_iterator_key_body() -> Vec<Stmt> {
    return_body(append_array_iterator_owner_call(
        "__elephcStorageKey",
        vec![append_array_iterator_position_expr()],
    ))
}

fn append_array_iterator_current_body() -> Vec<Stmt> {
    return_body(append_array_iterator_owner_call(
        "__elephcStorageCurrent",
        vec![append_array_iterator_position_expr()],
    ))
}

fn multiple_iterators_expr() -> Expr {
    property_access(this_expr(), "iterators")
}

fn multiple_infos_expr() -> Expr {
    property_access(this_expr(), "infos")
}

fn multiple_flags_expr() -> Expr {
    property_access(this_expr(), "flags")
}

fn multiple_iterator_at(index: Expr) -> Expr {
    array_access(multiple_iterators_expr(), index)
}

fn multiple_info_at(index: Expr) -> Expr {
    array_access(multiple_infos_expr(), index)
}

fn multiple_need_all_expr() -> Expr {
    binary_expr(
        binary_expr(multiple_flags_expr(), BinOp::BitAnd, int_expr(1)),
        BinOp::NotEq,
        int_expr(0),
    )
}

fn multiple_assoc_keys_expr() -> Expr {
    binary_expr(
        binary_expr(multiple_flags_expr(), BinOp::BitAnd, int_expr(2)),
        BinOp::NotEq,
        int_expr(0),
    )
}

fn multiple_construct_body() -> Vec<Stmt> {
    vec![
        property_assign_stmt(this_expr(), "iterators", empty_array_expr()),
        property_assign_stmt(this_expr(), "infos", empty_array_expr()),
        property_assign_stmt(this_expr(), "flags", var_expr("flags")),
    ]
}

fn multiple_attach_iterator_body() -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(multiple_iterators_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    binary_expr(multiple_iterator_at(var_expr("i")), BinOp::StrictEq, var_expr("iterator")),
                    vec![
                        property_array_assign_stmt(this_expr(), "infos", var_expr("i"), var_expr("info")),
                        return_void_stmt(),
                    ],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        property_array_push_stmt(this_expr(), "iterators", var_expr("iterator")),
        property_array_push_stmt(this_expr(), "infos", var_expr("info")),
    ]
}

fn multiple_detach_iterator_body() -> Vec<Stmt> {
    vec![
        assign_stmt("newIterators", empty_array_expr()),
        assign_stmt("newInfos", empty_array_expr()),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(multiple_iterators_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                assign_stmt("candidate", multiple_iterator_at(var_expr("i"))),
                if_stmt(
                    not_expr(binary_expr(var_expr("candidate"), BinOp::StrictEq, var_expr("iterator"))),
                    vec![
                        array_push_stmt("newIterators", var_expr("candidate")),
                        array_push_stmt("newInfos", multiple_info_at(var_expr("i"))),
                    ],
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        property_assign_stmt(this_expr(), "iterators", var_expr("newIterators")),
        property_assign_stmt(this_expr(), "infos", var_expr("newInfos")),
    ]
}

fn multiple_contains_iterator_body() -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(multiple_iterators_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                if_stmt(
                    binary_expr(multiple_iterator_at(var_expr("i")), BinOp::StrictEq, var_expr("iterator")),
                    return_body(bool_expr(true)),
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(bool_expr(false)),
    ]
}

fn multiple_each_iterator_body(method: &str) -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(multiple_iterators_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                typed_assign_stmt("iterator", named_type("Iterator"), multiple_iterator_at(var_expr("i"))),
                expr_stmt(method_call(var_expr("iterator"), method, Vec::new())),
                increment_stmt("i"),
            ],
        ),
    ]
}

fn multiple_rewind_body() -> Vec<Stmt> {
    multiple_each_iterator_body("rewind")
}

fn multiple_next_body() -> Vec<Stmt> {
    multiple_each_iterator_body("next")
}

fn multiple_valid_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(count_expr(multiple_iterators_expr()), BinOp::StrictEq, int_expr(0)),
            return_body(bool_expr(false)),
            None,
        ),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(multiple_iterators_expr())),
        if_stmt(
            multiple_need_all_expr(),
            vec![
                while_stmt(
                    binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
                    vec![
                        typed_assign_stmt("iterator", named_type("Iterator"), multiple_iterator_at(var_expr("i"))),
                        if_stmt(
                            not_expr(method_call(var_expr("iterator"), "valid", Vec::new())),
                            return_body(bool_expr(false)),
                            None,
                        ),
                        increment_stmt("i"),
                    ],
                ),
                return_stmt(bool_expr(true)),
            ],
            None,
        ),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                typed_assign_stmt("iterator", named_type("Iterator"), multiple_iterator_at(var_expr("i"))),
                if_stmt(
                    method_call(var_expr("iterator"), "valid", Vec::new()),
                    return_body(bool_expr(true)),
                    None,
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(bool_expr(false)),
    ]
}

fn multiple_output_body(method: &str) -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(count_expr(multiple_iterators_expr()), BinOp::StrictEq, int_expr(0)),
            vec![throw_stmt(new_object_expr(
                "RuntimeException",
                vec![string_expr(&format!("Called {method}() on an invalid iterator"))],
            ))],
            None,
        ),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(multiple_iterators_expr())),
        if_stmt(
            multiple_need_all_expr(),
            vec![
                while_stmt(
                    binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
                    vec![
                        typed_assign_stmt("iterator", named_type("Iterator"), multiple_iterator_at(var_expr("i"))),
                        if_stmt(
                            not_expr(method_call(var_expr("iterator"), "valid", Vec::new())),
                            vec![throw_stmt(new_object_expr(
                                "RuntimeException",
                                vec![string_expr(&format!(
                                    "Called {method}() with non valid sub iterator"
                                ))],
                            ))],
                            None,
                        ),
                        increment_stmt("i"),
                    ],
                ),
            ],
            None,
        ),
        assign_stmt("out", empty_array_expr()),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(multiple_iterators_expr())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                typed_assign_stmt("iterator", named_type("Iterator"), multiple_iterator_at(var_expr("i"))),
                assign_stmt("info", multiple_info_at(var_expr("i"))),
                if_stmt(
                    multiple_assoc_keys_expr(),
                    vec![if_stmt(
                        function_call("is_null", vec![var_expr("info")]),
                        vec![throw_stmt(new_object_expr(
                            "InvalidArgumentException",
                            vec![string_expr("Sub-Iterator is associated with NULL")],
                        ))],
                        None,
                    )],
                    None,
                ),
                typed_assign_stmt("item", mixed_type(), null_expr()),
                if_stmt(
                    method_call(var_expr("iterator"), "valid", Vec::new()),
                    vec![assign_stmt("item", method_call(var_expr("iterator"), method, Vec::new()))],
                    None,
                ),
                if_stmt(
                    multiple_assoc_keys_expr(),
                    vec![array_assign_stmt("out", var_expr("info"), var_expr("item"))],
                    Some(vec![array_assign_stmt("out", var_expr("i"), var_expr("item"))]),
                ),
                increment_stmt("i"),
            ],
        ),
        return_stmt(var_expr("out")),
    ]
}

fn multiple_debug_info_body() -> Vec<Stmt> {
    return_body(expr(ExprKind::ArrayLiteralAssoc(vec![
        (string_expr("\0MultipleIterator\0iterators"), multiple_iterators_expr()),
        (string_expr("\0MultipleIterator\0infos"), multiple_infos_expr()),
        (string_expr("\0MultipleIterator\0flags"), multiple_flags_expr()),
    ])))
}

fn array_search_prelude() -> Vec<Stmt> {
    vec![
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", count_expr(keys_expr())),
    ]
}

fn dll_items_snapshot_prelude() -> Vec<Stmt> {
    vec![
        assign_stmt("items", expr(ExprKind::ArrayLiteral(Vec::new()))),
        assign_stmt("i", int_expr(0)),
        assign_stmt("limit", method_call(this_expr(), "count", Vec::new())),
        while_stmt(
            binary_expr(var_expr("i"), BinOp::Lt, var_expr("limit")),
            vec![
                array_push_stmt("items", method_call(this_expr(), "offsetGet", vec![var_expr("i")])),
                increment_stmt("i"),
            ],
        ),
    ]
}

fn dll_serialize_array_body() -> Vec<Stmt> {
    let mut body = dll_items_snapshot_prelude();
    body.push(return_stmt(expr(ExprKind::ArrayLiteral(vec![
        method_call(this_expr(), "getIteratorMode", Vec::new()),
        var_expr("items"),
        expr(ExprKind::ArrayLiteral(Vec::new())),
    ]))));
    body
}

fn dll_debug_info_body() -> Vec<Stmt> {
    let mut body = vec![
        assign_stmt("mode", method_call(this_expr(), "getIteratorMode", Vec::new())),
        expr_stmt(method_call(this_expr(), "setIteratorMode", vec![int_expr(0)])),
    ];
    body.extend(dll_items_snapshot_prelude());
    body.push(expr_stmt(method_call(
        this_expr(),
        "setIteratorMode",
        vec![var_expr("mode")],
    )));
    body.push(return_stmt(expr(ExprKind::ArrayLiteralAssoc(vec![
        (
            string_expr("\0SplDoublyLinkedList\0flags"),
            var_expr("mode"),
        ),
        (
            string_expr("\0SplDoublyLinkedList\0dllist"),
            var_expr("items"),
        ),
    ]))));
    body
}

fn dll_unserialize_body() -> Vec<Stmt> {
    vec![
        expr_stmt(method_call(
            this_expr(),
            "setIteratorMode",
            vec![array_access(var_expr("data"), int_expr(0))],
        )),
        while_stmt(
            not_expr(method_call(this_expr(), "isEmpty", Vec::new())),
            vec![expr_stmt(method_call(this_expr(), "pop", Vec::new()))],
        ),
        foreach_stmt(
            array_access(var_expr("data"), int_expr(1)),
            None,
            "value",
            vec![expr_stmt(method_call(this_expr(), "push", vec![var_expr("value")]))],
        ),
    ]
}
