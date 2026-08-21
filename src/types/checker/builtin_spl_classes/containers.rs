//! Purpose:
//! Injects SPL phase-4 container class metadata and the internal SplFixedArray iterator helper.
//! Keeps runtime-backed container declarations separate from phase-5 iterator decorators.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Runtime-backed methods stay bodyless so codegen intrinsics own their behavior.
//! - Small serialization/debug helpers are synthetic PHP-like method bodies.

use std::collections::HashMap;

use crate::parser::ast::{
    BinOp, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, Stmt, TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;

use super::common::*;

/// Inserts classes into the supplied builtin metadata registry.
pub(super) fn insert_classes(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "SplDoublyLinkedList".to_string(),
        FlattenedClass {
            name: "SplDoublyLinkedList".to_string(),
            span: crate::span::Span::dummy(),
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
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "SplStack".to_string(),
        FlattenedClass {
            name: "SplStack".to_string(),
            span: crate::span::Span::dummy(),
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
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "SplQueue".to_string(),
        FlattenedClass {
            name: "SplQueue".to_string(),
            span: crate::span::Span::dummy(),
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
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "SplFixedArray".to_string(),
        FlattenedClass {
            name: "SplFixedArray".to_string(),
            span: crate::span::Span::dummy(),
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
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "InternalIterator".to_string(),
        FlattenedClass {
            name: "InternalIterator".to_string(),
            span: crate::span::Span::dummy(),
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
            trait_aliases: Vec::new(),
        },
    );
}

/// Returns the union type used for the `InternalIterator` owner property.
///
/// The owner must accept every collection class that the wrapper can iterate,
/// so the property is typed as a union rather than `mixed`. This keeps property
/// storage as an object reference instead of a generic `Mixed` box, which lets
/// `instanceof` narrowing and direct method calls work on the stored owner.
fn internal_iterator_owner_type() -> TypeExpr {
    TypeExpr::Union(vec![
        named_type("SplFixedArray"),
        named_type("DOMNodeList"),
        named_type("DOMNamedNodeMap"),
        named_type("Dom\\NodeList"),
        named_type("Dom\\NamedNodeMap"),
        named_type("Dom\\DtdNamedNodeMap"),
        named_type("Dom\\HTMLCollection"),
        named_type("Dom\\TokenList"),
    ])
}

/// Returns the nullable DTD member type retained between repeated `current()` calls.
fn internal_iterator_dtd_current_type() -> TypeExpr {
    TypeExpr::Nullable(Box::new(TypeExpr::Union(vec![
        named_type("Dom\\Entity"),
        named_type("Dom\\Notation"),
    ])))
}

/// Builds the method list for the shared `InternalIterator` wrapper.
///
/// The wrapper backs `IteratorAggregate::getIterator()` for `SplFixedArray` and for the
/// DOM live collections that are in scope for this worktree (`DOMNodeList`,
/// `DOMNamedNodeMap`, `Dom\\NodeList`, `Dom\\NamedNodeMap`, `Dom\\DtdNamedNodeMap`,
/// `Dom\\HTMLCollection`, `Dom\\TokenList`). Keys follow PHP's internal iterator semantics: numeric for
/// node-like collections and `SplFixedArray`, and `nodeName` for attribute maps.
/// The constructor takes an optional `named_keys` flag so that `NamedNodeMap`
/// iterators can return attribute names without hard-coding DOM class names in
/// the wrapper bodies.
fn spl_internal_iterator_methods() -> Vec<ClassMethod> {
    let mut construct = method_with_body(
        "__construct",
        vec![
            param("owner", internal_iterator_owner_type()),
            param_default("named_keys", TypeExpr::Bool, bool_expr(false)),
        ],
        Some(TypeExpr::Void),
        internal_iterator_construct_body(),
    );
    construct.visibility = Visibility::Private;

    let mut methods = vec![
        construct,
        method_with_body("current", Vec::new(), Some(mixed_type()), internal_iterator_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), internal_iterator_key_body()),
        method_with_body("next", Vec::new(), Some(TypeExpr::Void), internal_iterator_next_body()),
        method_with_body("rewind", Vec::new(), Some(TypeExpr::Void), internal_iterator_rewind_body()),
        method_with_body("valid", Vec::new(), Some(TypeExpr::Bool), internal_iterator_valid_body()),
    ];
    methods.extend(internal_iterator_typed_helper_methods());
    methods
}

/// Builds the property list for the shared internal iterator wrapper.
fn internal_iterator_properties() -> Vec<ClassProperty> {
    let mut properties = vec![
        storage_property("kind", TypeExpr::Int),
        storage_property("position", TypeExpr::Int),
        storage_property_default("started", TypeExpr::Bool, bool_expr(false)),
        storage_property_default("exhausted", TypeExpr::Bool, bool_expr(false)),
        storage_property_default("named_keys", TypeExpr::Bool, bool_expr(false)),
        storage_property_default(
            "dtd_current",
            internal_iterator_dtd_current_type(),
            null_expr(),
        ),
    ];
    for class_name in [
        "SplFixedArray",
        "DOMNodeList",
        "DOMNamedNodeMap",
        "Dom\\NodeList",
        "Dom\\NamedNodeMap",
        "Dom\\DtdNamedNodeMap",
        "Dom\\HTMLCollection",
        "Dom\\TokenList",
    ] {
        properties.push(storage_property(
            internal_iterator_owner_property_name(class_name),
            named_type(class_name),
        ));
    }
    properties
}

/// Builds the method list for SPL doubly linked list.
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

/// Builds the method list for SPL fixed array.
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

/// Provides the SPL doubly linked list constants helper used by the containers module.
fn spl_doubly_linked_list_constants() -> Vec<ClassConst> {
    vec![
        class_const("IT_MODE_LIFO", 2),
        class_const("IT_MODE_FIFO", 0),
        class_const("IT_MODE_DELETE", 1),
        class_const("IT_MODE_KEEP", 0),
    ]
}

/// Builds the AST expression for internal iterator position.
fn internal_iterator_position_expr() -> Expr {
    property_access(this_expr(), "position")
}

/// Builds the AST expression for the concrete owner-kind discriminator.
fn internal_iterator_kind_expr() -> Expr {
    property_access(this_expr(), "kind")
}

/// Builds the AST expression for the internal iterator named-keys flag.
fn internal_iterator_named_keys_expr() -> Expr {
    property_access(this_expr(), "named_keys")
}

/// Builds the AST expression for the sticky DOM end-of-iteration state.
fn internal_iterator_exhausted_expr() -> Expr {
    property_access(this_expr(), "exhausted")
}

/// Calls `count()` through a concrete collection parameter.
fn internal_iterator_typed_count_expr(class_name: &str) -> Expr {
    method_call(
        this_expr(),
        internal_iterator_count_helper_name(class_name),
        Vec::new(),
    )
}

/// Calls `item()` through a concrete DOM collection parameter.
fn internal_iterator_typed_item_expr(class_name: &str) -> Expr {
    method_call(
        this_expr(),
        internal_iterator_item_helper_name(class_name),
        vec![internal_iterator_position_expr()],
    )
}

/// Reads one DTD member directly from the concretely typed retained map.
fn internal_iterator_dtd_item_expr() -> Expr {
    method_call(
        property_access(
            this_expr(),
            internal_iterator_owner_property_name("Dom\\DtdNamedNodeMap"),
        ),
        "item",
        vec![internal_iterator_position_expr()],
    )
}

/// Reads the current `SplFixedArray` element through a concrete parameter.
fn internal_iterator_typed_fixed_array_item_expr() -> Expr {
    method_call(
        this_expr(),
        internal_iterator_item_helper_name("SplFixedArray"),
        vec![internal_iterator_position_expr()],
    )
}

/// Returns the concrete private owner-property name for one supported collection.
fn internal_iterator_owner_property_name(class_name: &str) -> &'static str {
    match class_name {
        "SplFixedArray" => "__elephc_owner_spl_fixed_array",
        "DOMNodeList" => "__elephc_owner_legacy_node_list",
        "DOMNamedNodeMap" => "__elephc_owner_legacy_named_node_map",
        "Dom\\NodeList" => "__elephc_owner_modern_node_list",
        "Dom\\NamedNodeMap" => "__elephc_owner_modern_named_node_map",
        "Dom\\DtdNamedNodeMap" => "__elephc_owner_modern_dtd_named_node_map",
        "Dom\\HTMLCollection" => "__elephc_owner_modern_html_collection",
        "Dom\\TokenList" => "__elephc_owner_modern_token_list",
        _ => unreachable!("unsupported InternalIterator collection class"),
    }
}

/// Returns the stable private discriminator for one supported collection class.
fn internal_iterator_kind(class_name: &str) -> i64 {
    match class_name {
        "SplFixedArray" => 0,
        "DOMNodeList" => 1,
        "DOMNamedNodeMap" => 2,
        "Dom\\NodeList" => 3,
        "Dom\\NamedNodeMap" => 4,
        "Dom\\HTMLCollection" => 5,
        "Dom\\TokenList" => 6,
        "Dom\\DtdNamedNodeMap" => 7,
        _ => unreachable!("unsupported InternalIterator collection class"),
    }
}

/// Returns the private count-helper name for one supported collection class.
fn internal_iterator_count_helper_name(class_name: &str) -> &'static str {
    match class_name {
        "SplFixedArray" => "__elephcInternalIteratorCountSplFixedArray",
        "DOMNodeList" => "__elephcInternalIteratorCountLegacyNodeList",
        "DOMNamedNodeMap" => "__elephcInternalIteratorCountLegacyNamedNodeMap",
        "Dom\\NodeList" => "__elephcInternalIteratorCountModernNodeList",
        "Dom\\NamedNodeMap" => "__elephcInternalIteratorCountModernNamedNodeMap",
        "Dom\\DtdNamedNodeMap" => "__elephcInternalIteratorCountModernDTDNamedNodeMap",
        "Dom\\HTMLCollection" => "__elephcInternalIteratorCountModernHTMLCollection",
        "Dom\\TokenList" => "__elephcInternalIteratorCountModernTokenList",
        _ => unreachable!("unsupported InternalIterator collection class"),
    }
}

/// Returns the private item-helper name for one supported collection class.
fn internal_iterator_item_helper_name(class_name: &str) -> &'static str {
    match class_name {
        "SplFixedArray" => "__elephcInternalIteratorItemSplFixedArray",
        "DOMNodeList" => "__elephcInternalIteratorItemLegacyNodeList",
        "DOMNamedNodeMap" => "__elephcInternalIteratorItemLegacyNamedNodeMap",
        "Dom\\NodeList" => "__elephcInternalIteratorItemModernNodeList",
        "Dom\\NamedNodeMap" => "__elephcInternalIteratorItemModernNamedNodeMap",
        "Dom\\DtdNamedNodeMap" => "__elephcInternalIteratorItemModernDTDNamedNodeMap",
        "Dom\\HTMLCollection" => "__elephcInternalIteratorItemModernHTMLCollection",
        "Dom\\TokenList" => "__elephcInternalIteratorItemModernTokenList",
        _ => unreachable!("unsupported InternalIterator collection class"),
    }
}

/// Returns the private named-key-helper name for one supported map class.
fn internal_iterator_named_key_helper_name(class_name: &str) -> &'static str {
    match class_name {
        "DOMNamedNodeMap" => "__elephcInternalIteratorKeyLegacyNamedNodeMap",
        "Dom\\NamedNodeMap" => "__elephcInternalIteratorKeyModernNamedNodeMap",
        "Dom\\DtdNamedNodeMap" => "__elephcInternalIteratorKeyModernDTDNamedNodeMap",
        _ => unreachable!("unsupported InternalIterator named-key class"),
    }
}

/// Builds one private synthetic method outside the public InternalIterator surface.
fn internal_iterator_private_helper(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: TypeExpr,
    body: Vec<Stmt>,
) -> ClassMethod {
    let mut helper = method_with_body(name, params, Some(return_type), body);
    helper.visibility = Visibility::Private;
    helper
}

/// Builds concrete helper methods used to bypass generic mixed-object vtable dispatch.
fn internal_iterator_typed_helper_methods() -> Vec<ClassMethod> {
    let classes: &[&str] = &[
        "SplFixedArray",
        "DOMNodeList",
        "DOMNamedNodeMap",
        "Dom\\NodeList",
        "Dom\\NamedNodeMap",
        "Dom\\DtdNamedNodeMap",
        "Dom\\HTMLCollection",
        "Dom\\TokenList",
    ];
    let mut methods = Vec::new();
    for class_name in classes {
        let typed_owner = property_access(
            this_expr(),
            internal_iterator_owner_property_name(class_name),
        );
        methods.push(internal_iterator_private_helper(
            internal_iterator_count_helper_name(class_name),
            Vec::new(),
            TypeExpr::Int,
            return_body(method_call(typed_owner.clone(), "count", Vec::new())),
        ));
        let item = if *class_name == "SplFixedArray" {
            array_access(typed_owner, var_expr("position"))
        } else {
            method_call(typed_owner, "item", vec![var_expr("position")])
        };
        let item_return_type = if *class_name == "Dom\\DtdNamedNodeMap" {
            internal_iterator_dtd_current_type()
        } else {
            mixed_type()
        };
        methods.push(internal_iterator_private_helper(
            internal_iterator_item_helper_name(class_name),
            vec![param("position", TypeExpr::Int)],
            item_return_type,
            return_body(item),
        ));
    }
    for class_name in [
        "DOMNamedNodeMap",
        "Dom\\NamedNodeMap",
        "Dom\\DtdNamedNodeMap",
    ] {
        methods.push(internal_iterator_private_helper(
            internal_iterator_named_key_helper_name(class_name),
            vec![param("position", TypeExpr::Int)],
            TypeExpr::Str,
            vec![
                assign_stmt(
                    "node",
                    method_call(
                        property_access(
                            this_expr(),
                            internal_iterator_owner_property_name(class_name),
                        ),
                        "item",
                        vec![var_expr("position")],
                    ),
                ),
                if_stmt(
                    binary_expr(var_expr("node"), BinOp::StrictNotEq, null_expr()),
                    return_body(property_access(var_expr("node"), "nodeName")),
                    Some(return_body(string_expr(""))),
                ),
            ],
        ));
    }
    methods
}

/// Builds the synthetic method body for internal iterator construct.
fn internal_iterator_construct_body() -> Vec<Stmt> {
    let dom_classes: &[&str] = &[
        "DOMNodeList",
        "DOMNamedNodeMap",
        "Dom\\NodeList",
        "Dom\\NamedNodeMap",
        "Dom\\DtdNamedNodeMap",
        "Dom\\HTMLCollection",
        "Dom\\TokenList",
    ];
    let mut dom_chain: Option<Vec<Stmt>> = None;
    for class_name in dom_classes.iter().rev() {
        dom_chain = Some(vec![if_stmt(
            instanceof_expr(var_expr("owner"), class_name),
            vec![
                property_assign_stmt(
                    this_expr(),
                    "kind",
                    int_expr(internal_iterator_kind(class_name)),
                ),
                property_assign_stmt(
                    this_expr(),
                    internal_iterator_owner_property_name(class_name),
                    var_expr("owner"),
                ),
                property_assign_stmt(
                    this_expr(),
                    "exhausted",
                    binary_expr(
                        internal_iterator_typed_count_expr(class_name),
                        BinOp::StrictEq,
                        int_expr(0),
                    ),
                ),
            ],
            dom_chain,
        )]);
    }
    let mut body = vec![
        property_assign_stmt(this_expr(), "kind", int_expr(0)),
        property_assign_stmt(this_expr(), "position", int_expr(0)),
        property_assign_stmt(this_expr(), "started", bool_expr(false)),
        property_assign_stmt(this_expr(), "exhausted", bool_expr(false)),
        property_assign_stmt(this_expr(), "named_keys", var_expr("named_keys")),
        if_stmt(
            instanceof_expr(var_expr("owner"), "SplFixedArray"),
            vec![
                property_assign_stmt(this_expr(), "kind", int_expr(0)),
                property_assign_stmt(
                    this_expr(),
                    internal_iterator_owner_property_name("SplFixedArray"),
                    var_expr("owner"),
                ),
            ],
            None,
        ),
    ];
    if let Some(dom_chain) = dom_chain {
        body.extend(dom_chain);
    }
    body
}

/// Builds the synthetic method body for internal iterator current.
fn internal_iterator_current_body() -> Vec<Stmt> {
    let cases = vec![
        ("SplFixedArray", internal_iterator_typed_fixed_array_item_expr()),
        (
            "DOMNodeList",
            internal_iterator_typed_item_expr("DOMNodeList"),
        ),
        (
            "DOMNamedNodeMap",
            internal_iterator_typed_item_expr("DOMNamedNodeMap"),
        ),
        (
            "Dom\\NodeList",
            internal_iterator_typed_item_expr("Dom\\NodeList"),
        ),
        (
            "Dom\\NamedNodeMap",
            internal_iterator_typed_item_expr("Dom\\NamedNodeMap"),
        ),
        (
            "Dom\\DtdNamedNodeMap",
            internal_iterator_typed_item_expr("Dom\\DtdNamedNodeMap"),
        ),
        (
            "Dom\\HTMLCollection",
            internal_iterator_typed_item_expr("Dom\\HTMLCollection"),
        ),
        (
            "Dom\\TokenList",
            internal_iterator_typed_item_expr("Dom\\TokenList"),
        ),
    ];
    let mut body = vec![if_stmt(
        internal_iterator_exhausted_expr(),
        return_body(null_expr()),
        None,
    )];
    body.push(if_stmt(
        binary_expr(
            internal_iterator_kind_expr(),
            BinOp::StrictEq,
            int_expr(internal_iterator_kind("Dom\\DtdNamedNodeMap")),
        ),
        vec![
            if_stmt(
                binary_expr(
                    property_access(this_expr(), "dtd_current"),
                    BinOp::StrictEq,
                    null_expr(),
                ),
                vec![property_assign_stmt(
                    this_expr(),
                    "dtd_current",
                    internal_iterator_dtd_item_expr(),
                )],
                None,
            ),
            return_stmt(property_access(this_expr(), "dtd_current")),
        ],
        None,
    ));
    body.extend(internal_iterator_dispatch_return_body(&cases, None));
    body
}

/// Builds the synthetic method body for internal iterator key.
fn internal_iterator_key_body() -> Vec<Stmt> {
    let named_key_cases: &[(&str, &str)] = &[
        ("DOMNamedNodeMap", "nodeName"),
        ("Dom\\NamedNodeMap", "nodeName"),
        ("Dom\\DtdNamedNodeMap", "nodeName"),
    ];
    let named_branch = internal_iterator_dispatch_property_body(
        internal_iterator_position_expr(),
        named_key_cases,
        "",
    );
    vec![if_stmt(
        internal_iterator_named_keys_expr(),
        named_branch,
        Some(return_body(internal_iterator_position_expr())),
    )]
}

/// Builds the synthetic method body for internal iterator next.
///
/// DOM live collections expose mutations until they reach their native sticky
/// end state. Once exhausted, repeated `next()` calls and later appends leave
/// the key and validity unchanged. `SplFixedArray` keeps PHP's unconditional
/// advancement.
fn internal_iterator_next_body() -> Vec<Stmt> {
    let dom_classes: &[&str] = &[
        "DOMNodeList",
        "DOMNamedNodeMap",
        "Dom\\NodeList",
        "Dom\\NamedNodeMap",
        "Dom\\DtdNamedNodeMap",
        "Dom\\HTMLCollection",
        "Dom\\TokenList",
    ];
    let position_expr = internal_iterator_position_expr();
    let advance = property_assign_stmt(
        this_expr(),
        "position",
        binary_expr(position_expr.clone(), BinOp::Add, int_expr(1)),
    );
    let mut dom_chain: Option<Vec<Stmt>> = None;
    for class_name in dom_classes.iter().rev() {
        let dom_advance_guard = binary_expr(
            binary_expr(position_expr.clone(), BinOp::Add, int_expr(1)),
            BinOp::Lt,
            var_expr("count"),
        );
        dom_chain = Some(vec![if_stmt(
            binary_expr(
                internal_iterator_kind_expr(),
                BinOp::StrictEq,
                int_expr(internal_iterator_kind(class_name)),
            ),
            vec![
                assign_stmt("count", internal_iterator_typed_count_expr(class_name)),
                if_stmt(
                    dom_advance_guard,
                    vec![advance.clone()],
                    Some(vec![
                        property_assign_stmt(
                            this_expr(),
                            "position",
                            var_expr("count"),
                        ),
                        property_assign_stmt(
                            this_expr(),
                            "exhausted",
                            bool_expr(true),
                        ),
                    ]),
                ),
            ],
            dom_chain,
        )]);
    }
    let mut body = Vec::new();
    body.push(property_assign_stmt(this_expr(), "dtd_current", null_expr()));
    body.push(property_assign_stmt(this_expr(), "started", bool_expr(true)));
    body.push(if_stmt(
        binary_expr(
            internal_iterator_kind_expr(),
            BinOp::StrictEq,
            int_expr(internal_iterator_kind("SplFixedArray")),
        ),
        vec![advance.clone()],
        Some(vec![if_stmt(
            not_expr(internal_iterator_exhausted_expr()),
            dom_chain.unwrap_or_default(),
            None,
        )]),
    ));
    body
}

/// Builds the synthetic method body for internal iterator rewind.
///
/// PHP's internal iterator allows repeated rewinds for `SplFixedArray`, but DOM
/// live collections throw `Error: Iterator does not support rewinding` once
/// `next()` has advanced the cursor. The guard therefore checks both the
/// `started` flag and the owner class.
fn internal_iterator_rewind_body() -> Vec<Stmt> {
    let dom_check = binary_expr(
        internal_iterator_kind_expr(),
        BinOp::StrictNotEq,
        int_expr(internal_iterator_kind("SplFixedArray")),
    );
    let throw_guard = binary_expr(
        property_access(this_expr(), "started"),
        BinOp::And,
        dom_check,
    );
    vec![if_stmt(
        throw_guard,
        vec![throw_stmt(new_object_expr(
            "Error",
            vec![string_expr("Iterator does not support rewinding")],
        ))],
        Some(vec![
            property_assign_stmt(this_expr(), "position", int_expr(0)),
            property_assign_stmt(this_expr(), "dtd_current", null_expr()),
        ]),
    )]
}

/// Builds the synthetic method body for internal iterator valid.
fn internal_iterator_valid_body() -> Vec<Stmt> {
    let cases: &[&str] = &[
        "SplFixedArray",
        "DOMNodeList",
        "DOMNamedNodeMap",
        "Dom\\NodeList",
        "Dom\\NamedNodeMap",
        "Dom\\DtdNamedNodeMap",
        "Dom\\HTMLCollection",
        "Dom\\TokenList",
    ];
    let mut body = vec![if_stmt(
        internal_iterator_exhausted_expr(),
        return_body(bool_expr(false)),
        None,
    )];
    body.extend(internal_iterator_dispatch_count_body(cases));
    body
}

/// Builds an if/elseif chain that returns one expression narrowed to each
/// supported collection class. Cases are tested in order; the final `else`
/// branch returns the fallback expression when provided.
fn internal_iterator_dispatch_return_body(
    cases: &[(&str, Expr)],
    fallback: Option<Expr>,
) -> Vec<Stmt> {
    let mut chain: Option<Vec<Stmt>> = fallback.map(return_body);
    for (class_name, then_expr) in cases.iter().rev() {
        chain = Some(vec![if_stmt(
            binary_expr(
                internal_iterator_kind_expr(),
                BinOp::StrictEq,
                int_expr(internal_iterator_kind(class_name)),
            ),
            return_body(then_expr.clone()),
            chain,
        )]);
    }
    chain.unwrap_or_default()
}

/// Builds an if/elseif chain that returns `$position < $owner->count()`
/// narrowed to each supported collection class.
fn internal_iterator_dispatch_count_body(cases: &[&str]) -> Vec<Stmt> {
    let expr_cases: Vec<(&str, Expr)> = cases
        .iter()
        .map(|&class_name| {
            (
                class_name,
                binary_expr(
                    internal_iterator_position_expr(),
                    BinOp::Lt,
                    internal_iterator_typed_count_expr(class_name),
                ),
            )
        })
        .collect();
    internal_iterator_dispatch_return_body(&expr_cases, Some(bool_expr(false)))
}

/// Builds an if/elseif chain that calls `$owner->item($position)->$property`,
/// returning the fallback value when the item is null or no class matches.
fn internal_iterator_dispatch_property_body(
    position_expr: Expr,
    cases: &[(&str, &str)],
    fallback: &str,
) -> Vec<Stmt> {
    let mut chain: Option<Vec<Stmt>> = Some(return_body(string_expr(fallback)));
    for (class_name, _property_name) in cases.iter().rev() {
        let typed_property_expr = method_call(
            this_expr(),
            internal_iterator_named_key_helper_name(class_name),
            vec![position_expr.clone()],
        );
        chain = Some(vec![if_stmt(
            binary_expr(
                internal_iterator_kind_expr(),
                BinOp::StrictEq,
                int_expr(internal_iterator_kind(class_name)),
            ),
            return_body(typed_property_expr),
            chain,
        )]);
    }
    chain.unwrap_or_default()
}

/// Builds the synthetic method body for fixed array get iterator.
fn fixed_array_get_iterator_body() -> Vec<Stmt> {
    return_body(new_object_expr("InternalIterator", vec![this_expr()]))
}

/// Provides the Dll items snapshot prelude helper used by the containers module.
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

/// Builds the synthetic method body for dll serialize array.
fn dll_serialize_array_body() -> Vec<Stmt> {
    let mut body = dll_items_snapshot_prelude();
    body.push(return_stmt(expr(ExprKind::ArrayLiteral(vec![
        method_call(this_expr(), "getIteratorMode", Vec::new()),
        var_expr("items"),
        expr(ExprKind::ArrayLiteral(Vec::new())),
    ]))));
    body
}

/// Builds the synthetic method body for dll debug info.
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

/// Builds the synthetic method body for dll unserialize.
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
