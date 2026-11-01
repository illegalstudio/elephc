//! Purpose:
//! The table-reading constructors of `ReflectionClass`, `ReflectionObject` and `ReflectionEnum`,
//! and the private `__fill_from_row()` method they share.
//!
//! Called from:
//! - `class_model`, which installs them in place of the empty constructor bodies.
//!
//! Key details:
//! - The constructor finds the class-like's row in the module reflection table by lowercase name
//!   (`__elephc_refl_row`), stores every plain-data slot from it (`__elephc_refl_word`,
//!   `__elephc_refl_pool_str`) and then runs the row's extras filler for what needs code. It is
//!   ONE body for every class: nothing in it grows with the number of reflected classes.
//! - The table layout is `crate::reflection_table`; the emitter and this file agree on it and on
//!   nothing else.
//! - A default build still builds a reflector for a runtime name or an eval-declared class
//!   through the interpreter, without running this body; see `table_builds_reflector`.

use crate::parser::ast::{BinOp, CastType, ClassMethod, Expr, Stmt, TypeExpr, Visibility};
use crate::reflection_table::{flag, word};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_cast, e_concat, e_int, e_method_call, e_new, e_str,
    e_this, e_this_prop, e_var, s_array_push, s_assign, s_expr, s_for, s_foreach, s_if,
    s_prop_assign,
};

use super::class_relations::throw_new_reflection_exception;

/// Which constructor to build.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TableConstructor {
    Class,
    Object,
    Enum,
}

fn row_word(at: usize) -> Expr {
    e_call("__elephc_refl_word", vec![e_var("row"), e_int(at as i64)])
}

fn pool_str(pool: Expr, index: Expr) -> Expr {
    e_call("__elephc_refl_pool_str", vec![pool, index])
}

/// `($flags & BIT) !== 0`.
fn has_flag(bit: i64) -> Expr {
    e_binop(
        e_binop(e_var("flags"), BinOp::BitAnd, e_int(bit)),
        BinOp::StrictNotEq,
        e_int(0),
    )
}

/// `$this-><property> = [<the strings of the row's list at `at`>];`
fn list_statements(property: &str, at: usize, suffix: &str) -> Vec<Stmt> {
    let pool = format!("pool_{suffix}");
    let count = format!("count_{suffix}");
    let list = format!("list_{suffix}");
    let index = format!("i_{suffix}");
    vec![
        s_assign(&pool, row_word(at)),
        s_assign(&count, row_word(at + 1)),
        s_assign(&list, e_array(Vec::new())),
        s_for(
            Some(s_assign(&index, e_int(0))),
            Some(e_binop(e_var(&index), BinOp::Lt, e_var(&count))),
            Some(s_assign(&index, e_binop(e_var(&index), BinOp::Add, e_int(1)))),
            vec![s_array_push(&list, pool_str(e_var(&pool), e_var(&index)))],
        ),
        s_prop_assign(e_this(), property, e_var(&list)),
    ]
}

/// The properties holding `row + 1` until the slots they stand for are filled.
pub(super) const LAZY_MARKERS: [&str; 4] =
    ["__lazy_row", "__relations_row", "__member_names_row", "__constant_names_row"];

/// A private `void` method of the reflectors.
pub(super) fn private_method(name: &str, params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>, body: Vec<Stmt>) -> ClassMethod {
    ClassMethod {
        type_params: Vec::new(),
        name: name.to_string(),
        visibility: Visibility::Private,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params,
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Void),
        by_ref_return: false,
        body,
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// `private function __fill_from_row(int $row): void`
///
/// The cheap part of a reflector, stored on every construction: the name, the flag words and
/// the marks of the member slots. Everything else is read from the row when a getter first asks
/// (`__fill_lazy`, `__fill_values`): most reflectors are built for their name and a flag or two,
/// and every string copied into a slot is a heap allocation the construction would pay for.
pub(super) fn fill_from_row_method() -> ClassMethod {
    let mut body = vec![s_assign("flags", row_word(word::FLAGS))];
    body.push(s_prop_assign(
        e_this(),
        "__name",
        pool_str(row_word(word::NAME), e_int(0)),
    ));
    body.push(s_prop_assign(e_this(), "__complete", e_bool(true)));
    body.push(s_prop_assign(e_this(), "__table_built", e_bool(true)));
    // The lazy parts wait for their row (0 means "nothing to read").
    for marker in LAZY_MARKERS {
        body.push(s_prop_assign(e_this(), marker, e_var("row")));
    }
    body.push(s_if(
        e_binop(
            e_binop(row_word(word::CONSTANT_VALUES), BinOp::StrictNotEq, e_int(0)),
            BinOp::Or,
            e_binop(row_word(word::DEFAULT_VALUES), BinOp::StrictNotEq, e_int(0)),
        ),
        vec![s_prop_assign(e_this(), "__values_row", e_var("row"))],
        vec![],
        None,
    ));
    // A closed-world reflector builds each member slot the first time a getter reads it: a
    // non-zero `<slot>_filler` is the mark (there is no compiled filler to point at).
    let generic = crate::reflection_table::generic_slots();
    let mark = |slot: &str| s_prop_assign(e_this(), &format!("{slot}_filler"), e_int(-1));
    let non_empty = |at: usize| e_binop(row_word(at + 1), BinOp::Gt, e_int(0));
    for (slots, at) in [
        (&["__methods"][..], word::METHOD_MEMBERS),
        (&["__constructor"][..], word::METHODS),
        (&["__properties"][..], word::PROPERTY_MEMBERS),
        (&["__reflection_constants"][..], word::CONSTANTS),
        (&["__parent_class"][..], word::PARENTS),
    ] {
        let marks = slots
            .iter()
            .filter(|slot| generic.contains(slot))
            .map(|slot| mark(slot))
            .collect::<Vec<_>>();
        if !marks.is_empty() {
            body.push(s_if(non_empty(at), marks, vec![], None));
        }
    }
    private_method(
        "__fill_from_row",
        vec![("row".to_string(), Some(TypeExpr::Int), None, false)],
        body,
    )
}

/// `private function __fill_lazy(int $row): void` -- the slots a getter reads on demand: the
/// flags, the namespace parts, the documentation and the file and lines.
pub(super) fn fill_lazy_method() -> ClassMethod {
    let mut body = vec![s_assign("flags", row_word(word::FLAGS))];
    for (property, bit) in [
        ("__is_final", flag::FINAL),
        ("__is_abstract", flag::ABSTRACT),
        ("__is_interface", flag::INTERFACE),
        ("__is_trait", flag::TRAIT),
        ("__is_enum", flag::ENUM),
        ("__is_readonly", flag::READONLY),
        ("__is_anonymous", flag::ANONYMOUS),
        ("__is_instantiable", flag::INSTANTIABLE),
        ("__is_cloneable", flag::CLONEABLE),
        ("__is_iterable", flag::ITERABLE),
        ("__is_internal", flag::INTERNAL),
        ("__is_user_defined", flag::USER_DEFINED),
        ("__generic_members", flag::GENERIC_MEMBERS),
    ] {
        body.push(s_prop_assign(e_this(), property, has_flag(bit)));
    }
    body.push(s_prop_assign(e_this(), "__modifiers", row_word(word::MODIFIERS)));
    // The namespace parts follow from the name: everything before the last separator.
    body.push(s_assign(
        "split",
        e_call("strrpos", vec![e_this_prop("__name"), e_str("\\")]),
    ));
    body.push(s_if(
        e_binop(e_var("split"), BinOp::StrictEq, e_bool(false)),
        vec![
            s_prop_assign(e_this(), "__short_name", e_this_prop("__name")),
            s_prop_assign(e_this(), "__namespace_name", e_str("")),
            s_prop_assign(e_this(), "__in_namespace", e_bool(false)),
        ],
        vec![],
        Some(vec![
            s_prop_assign(
                e_this(),
                "__short_name",
                e_call(
                    "substr",
                    vec![
                        e_this_prop("__name"),
                        e_binop(
                            e_cast(CastType::Int, e_var("split")),
                            BinOp::Add,
                            e_int(1),
                        ),
                    ],
                ),
            ),
            s_prop_assign(
                e_this(),
                "__namespace_name",
                e_call(
                    "substr",
                    vec![
                        e_this_prop("__name"),
                        e_int(0),
                        e_cast(CastType::Int, e_var("split")),
                    ],
                ),
            ),
            s_prop_assign(e_this(), "__in_namespace", e_bool(true)),
        ]),
    ));
    body.push(s_if(
        has_flag(flag::HAS_DOC),
        vec![
            s_prop_assign(
                e_this(),
                "__doc_comment",
                pool_str(row_word(word::DOC), e_int(0)),
            ),
            s_prop_assign(e_this(), "__has_doc_comment", e_bool(true)),
        ],
        vec![],
        None,
    ));
    body.push(s_if(
        has_flag(flag::HAS_FILE),
        vec![
            s_prop_assign(
                e_this(),
                "__file_name",
                pool_str(row_word(word::FILE), e_int(0)),
            ),
            s_prop_assign(e_this(), "__start_line", row_word(word::START_LINE)),
            s_prop_assign(e_this(), "__end_line", row_word(word::END_LINE)),
        ],
        vec![],
        None,
    ));
    private_method(
        "__fill_lazy",
        vec![("row".to_string(), Some(TypeExpr::Int), None, false)],
        body,
    )
}

/// `private function __fill_relations(int $row): void` -- the interface, trait and ancestor names.
pub(super) fn fill_relations_method() -> ClassMethod {
    let mut body = Vec::new();
    for (property, at, suffix) in [
        ("__interface_names", word::INTERFACES, "interfaces"),
        ("__trait_names", word::TRAITS, "traits"),
        ("__parent_names", word::PARENTS, "parents"),
    ] {
        body.extend(list_statements(property, at, suffix));
    }
    private_method(
        "__fill_relations",
        vec![("row".to_string(), Some(TypeExpr::Int), None, false)],
        body,
    )
}

/// `private function __fill_member_names(int $row): void` -- the method and property names.
pub(super) fn fill_member_names_method() -> ClassMethod {
    let mut body = Vec::new();
    for (property, at, suffix) in [
        ("__method_names", word::METHODS, "methods"),
        ("__property_names", word::PROPERTIES, "properties"),
        ("__method_member_names", word::METHOD_MEMBERS, "method_members"),
        ("__property_member_names", word::PROPERTY_MEMBERS, "property_members"),
    ] {
        body.extend(list_statements(property, at, suffix));
    }
    private_method(
        "__fill_member_names",
        vec![("row".to_string(), Some(TypeExpr::Int), None, false)],
        body,
    )
}

/// `private function __fill_constant_names(int $row): void` -- the constant names with their
/// modifiers, and an enum's case names.
pub(super) fn fill_constant_names_method(is_enum: bool) -> ClassMethod {
    let mut body = list_statements("__constant_names", word::CONSTANTS, "constants");
    if is_enum {
        body.extend(list_statements("__case_names", word::CASES, "cases"));
    }
    // Constant modifiers, one int per constant name.
    body.extend([
        s_assign("modifiers_at", row_word(word::CONSTANT_MODIFIERS)),
        s_assign("modifiers", e_array(Vec::new())),
        s_for(
            Some(s_assign("mi", e_int(0))),
            Some(e_binop(
                e_var("mi"),
                BinOp::Lt,
                e_call("count", vec![e_this_prop("__constant_names")]),
            )),
            Some(s_assign("mi", e_binop(e_var("mi"), BinOp::Add, e_int(1)))),
            vec![s_array_push(
                "modifiers",
                e_call("__elephc_refl_at", vec![e_var("modifiers_at"), e_var("mi")]),
            )],
        ),
        s_prop_assign(e_this(), "__constant_modifiers", e_var("modifiers")),
    ]);
    private_method(
        "__fill_constant_names",
        vec![("row".to_string(), Some(TypeExpr::Int), None, false)],
        body,
    )
}

/// `private function __fill_values(int $row): void` -- `getConstants()` and
/// `getDefaultProperties()` decoded from the row's value nodes.
pub(super) fn fill_values_method() -> ClassMethod {
    let mut body = Vec::new();
    for (property, at, local) in [
        ("__constants", word::CONSTANT_VALUES, "constants_node"),
        ("__default_properties", word::DEFAULT_VALUES, "defaults_node"),
    ] {
        body.push(s_assign(local, row_word(at)));
        body.push(s_if(
            e_binop(e_var(local), BinOp::StrictNotEq, e_int(0)),
            vec![s_prop_assign(
                e_this(),
                property,
                e_method_call(e_this(), "__decode", vec![e_var(local)]),
            )],
            vec![],
            None,
        ));
    }
    private_method(
        "__fill_values",
        vec![("row".to_string(), Some(TypeExpr::Int), None, false)],
        body,
    )
}

/// `throw new ReflectionException('Class "' . $name . '" does not exist');`
fn missing_class_exception() -> Stmt {
    throw_new_reflection_exception(
        e_concat(
            e_concat(e_str("Class \""), e_var("name")),
            e_str("\" does not exist"),
        ),
        crate::span::Span::dummy(),
    )
}

/// The constructor of `ReflectionClass` (a name or an object), `ReflectionObject` (an object) or
/// `ReflectionEnum` (a name or an object naming an enum).
pub(super) fn table_constructor(kind: TableConstructor) -> ClassMethod {
    let mut body: Vec<Stmt> = Vec::new();
    if kind == TableConstructor::Object {
        body.push(s_assign("name", e_call("get_class", vec![e_var("class_name")])));
        body.push(s_prop_assign(e_this(), "__object", e_var("class_name")));
    } else {
        body.push(s_if(
            e_call("is_object", vec![e_var("class_name")]),
            vec![s_assign("name", e_call("get_class", vec![e_var("class_name")]))],
            vec![],
            Some(vec![s_assign("name", e_cast(CastType::String, e_var("class_name")))]),
        ));
    }
    body.push(s_assign(
        "row",
        e_call(
            "__elephc_refl_row",
            vec![e_call(
                "strtolower",
                vec![e_call("ltrim", vec![e_var("name"), e_str("\\")])],
            )],
        ),
    ));
    body.push(s_if(
        e_binop(e_var("row"), BinOp::StrictEq, e_int(0)),
        vec![missing_class_exception()],
        vec![],
        None,
    ));
    if kind == TableConstructor::Enum {
        body.push(s_if(
            e_binop(
                e_binop(row_word(word::FLAGS), BinOp::BitAnd, e_int(flag::ENUM)),
                BinOp::StrictEq,
                e_int(0),
            ),
            vec![throw_new_reflection_exception(
                e_concat(
                    e_concat(
                        e_str("Class \""),
                        pool_str(row_word(word::NAME), e_int(0)),
                    ),
                    e_str("\" is not an enum"),
                ),
                crate::span::Span::dummy(),
            )],
            vec![],
            None,
        ));
    }
    body.push(s_expr(e_method_call(
        e_this(),
        "__fill_from_row",
        vec![e_var("row")],
    )));
    let extras_word = if kind == TableConstructor::Enum {
        body.push(s_prop_assign(
            e_this(),
            "__is_backed",
            e_binop(
                e_binop(row_word(word::FLAGS), BinOp::BitAnd, e_int(flag::BACKED)),
                BinOp::StrictNotEq,
                e_int(0),
            ),
        ));
        if crate::reflection_table::generic_slots().contains(&"__cases") {
            body.push(s_if(
                e_binop(row_word(word::CASES + 1), BinOp::Gt, e_int(0)),
                vec![s_prop_assign(e_this(), "__cases_filler", e_int(-1))],
                vec![],
                None,
            ));
        }
        word::ENUM_EXTRAS
    } else {
        if kind == TableConstructor::Object { word::OBJECT_EXTRAS } else { word::EXTRAS }
    };
    body.push(s_assign("extras", row_word(extras_word)));
    body.push(s_if(
        e_binop(e_var("extras"), BinOp::StrictNotEq, e_int(0)),
        vec![s_expr(e_call(
            "__elephc_reflection_fill_members",
            vec![e_var("extras"), e_this()],
        ))],
        vec![],
        None,
    ));
    let (param, ty) = match kind {
        TableConstructor::Object => ("class_name", Some(super::ast_literals::object_type())),
        _ => ("class_name", Some(super::ast_literals::mixed_type())),
    };
    ClassMethod {
        type_params: Vec::new(),
        name: "__construct".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(param.to_string(), ty, None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: None,
        by_ref_return: false,
        body,
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// The member slots a closed-world reflector fills on first use, with the private method that
/// fills each (named by [`generic_fill_method_name`]).
pub(super) fn generic_fill_methods(is_enum: bool) -> Vec<ClassMethod> {
    let generic = crate::reflection_table::generic_slots();
    let build = |name: &str, body: Vec<Stmt>| ClassMethod {
        type_params: Vec::new(),
        name: name.to_string(),
        visibility: Visibility::Private,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Void),
        by_ref_return: false,
        body,
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    };
    // `$this-><slot> = [new <Member>($this->__name, $name) for each name of <names>]`
    let members = |names: &str, member_class: &str, slot: &str| {
        vec![
            s_assign("members", e_array(Vec::new())),
            s_foreach(
                e_this_prop(names),
                None,
                "member_name",
                vec![s_array_push(
                    "members",
                    e_new(member_class, vec![e_this_prop("__name"), e_var("member_name")]),
                )],
            ),
            s_prop_assign(e_this(), slot, e_var("members")),
        ]
    };
    let mut methods = vec![
        build(
            &generic_fill_method_name("__methods"),
            members("__method_member_names", "ReflectionMethod", "__methods"),
        ),
        build(
            &generic_fill_method_name("__properties"),
            members("__property_member_names", "ReflectionProperty", "__properties"),
        ),
        build(
            &generic_fill_method_name("__reflection_constants"),
            members(
                "__constant_names",
                "ReflectionClassConstant",
                "__reflection_constants",
            ),
        ),
    ];
    methods.push(build(
        &generic_fill_method_name("__constructor"),
        vec![s_foreach(
            e_this_prop("__method_names"),
            None,
            "method_name",
            vec![s_if(
                e_binop(
                    e_call("strtolower", vec![e_var("method_name")]),
                    BinOp::StrictEq,
                    e_str("__construct"),
                ),
                vec![s_prop_assign(
                    e_this(),
                    "__constructor",
                    e_new("ReflectionMethod", vec![e_this_prop("__name"), e_var("method_name")]),
                )],
                vec![],
                None,
            )],
        )],
    ));
    methods.push(build(
        &generic_fill_method_name("__parent_class"),
        vec![s_prop_assign(
            e_this(),
            "__parent_class",
            e_new(
                "ReflectionClass",
                vec![crate::synthetic_class::e_index(e_this_prop("__parent_names"), e_int(0))],
            ),
        )],
    ));
    if is_enum {
        // One typed array per case class, each in a slot of its own: a mixed local would not store
        // into a typed slot, and an array of backed cases is not an array of unit cases.
        let cases = |case_class: &str, local: &str, slot: &str| {
            vec![
                s_assign(local, e_array(Vec::new())),
                s_foreach(
                    e_this_prop("__case_names"),
                    None,
                    "member_name",
                    vec![s_array_push(
                        local,
                        e_new(case_class, vec![e_this_prop("__name"), e_var("member_name")]),
                    )],
                ),
                s_prop_assign(e_this(), slot, e_var(local)),
            ]
        };
        methods.push(build(
            &generic_fill_method_name("__cases"),
            vec![s_if(
                e_this_prop("__is_backed"),
                cases("ReflectionEnumBackedCase", "backed", "__backed_cases"),
                vec![],
                Some(cases("ReflectionEnumUnitCase", "units", "__cases")),
            )],
        ));
    }
    methods.retain(|method| {
        generic.iter().any(|slot| method.name == generic_fill_method_name(slot))
    });
    methods
}

/// The private method that fills `slot` of a closed-world reflector.
pub(super) fn generic_fill_method_name(slot: &str) -> String {
    format!("__fill_generic{slot}")
}

fn node_word(at: i64) -> Expr {
    e_call("__elephc_refl_at", vec![e_var("node"), e_int(at)])
}

/// `private function __decode(int $node): mixed` -- one value node of the table as a PHP
/// value. Arrays recurse through their entry tables, enum cases resolve through `constant()`.
pub(super) fn decode_method() -> ClassMethod {
    use crate::reflection_table::value;
    let tag = || e_var("tag");
    let is = |wanted: i64| e_binop(tag(), BinOp::StrictEq, e_int(wanted));
    let ret = |expr: Expr| vec![crate::synthetic_class::s_return(expr)];
    let string_a = || pool_str(node_word(1), e_int(0));
    let mut body = vec![s_assign("tag", node_word(0))];
    body.push(s_if(is(value::NULL), ret(crate::synthetic_class::e_null()), vec![], None));
    body.push(s_if(
        is(value::BOOL),
        ret(e_binop(node_word(1), BinOp::StrictNotEq, e_int(0))),
        vec![],
        None,
    ));
    body.push(s_if(is(value::INT), ret(node_word(1)), vec![], None));
    body.push(s_if(
        is(value::FLOAT),
        ret(e_cast(CastType::Float, string_a())),
        vec![],
        None,
    ));
    body.push(s_if(is(value::STRING), ret(string_a()), vec![], None));
    body.push(s_if(
        is(value::INFINITY),
        ret(crate::synthetic_class::e_const("INF")),
        vec![],
        None,
    ));
    body.push(s_if(
        is(value::NEGATIVE_INFINITY),
        ret(crate::synthetic_class::e_neg(crate::synthetic_class::e_const("INF"))),
        vec![],
        None,
    ));
    body.push(s_if(is(value::NAN), ret(crate::synthetic_class::e_const("NAN")), vec![], None));
    body.push(s_if(
        is(value::ENUM_CASE),
        ret(e_call(
            "constant",
            vec![e_concat(
                e_concat(
                    pool_str(node_word(1), e_int(0)),
                    e_str("::"),
                ),
                pool_str(node_word(1), e_int(1)),
            )],
        )),
        vec![],
        None,
    ));
    body.push(s_if(
        is(value::OBJECT),
        ret(crate::synthetic_class::e_new_dynamic(
            string_a(),
            vec![crate::synthetic_class::e_spread(e_method_call(
                e_this(),
                "__decode",
                vec![node_word(2)],
            ))],
        )),
        vec![],
        None,
    ));
    // value::ARRAY
    body.push(s_assign("items", e_array(Vec::new())));
    body.push(s_assign("table", node_word(1)));
    body.push(s_assign("count", node_word(2)));
    body.push(s_for(
        Some(s_assign("i", e_int(0))),
        Some(e_binop(e_var("i"), BinOp::Lt, e_var("count"))),
        Some(s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1)))),
        vec![
            s_assign(
                "entry",
                e_binop(
                    e_var("table"),
                    BinOp::Add,
                    e_binop(e_var("i"), BinOp::Mul, e_int(value::ENTRY_BYTES)),
                ),
            ),
            crate::synthetic_class::s_array_assign(
                "items",
                e_self_call_decode(e_var("entry")),
                e_self_call_decode(e_binop(e_var("entry"), BinOp::Add, e_int(24))),
            ),
        ],
    ));
    body.push(crate::synthetic_class::s_return(e_var("items")));
    ClassMethod {
        type_params: Vec::new(),
        name: "__decode".to_string(),
        visibility: Visibility::Private,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("node".to_string(), Some(TypeExpr::Int), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(super::ast_literals::mixed_type()),
        by_ref_return: false,
        body,
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

fn e_self_call_decode(node: Expr) -> Expr {
    e_method_call(e_this(), "__decode", vec![node])
}
