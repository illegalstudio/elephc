//! Purpose:
//! The table-reading constructors of the member and type reflectors: `ReflectionClassConstant`,
//! `ReflectionEnumUnitCase`, `ReflectionEnumBackedCase`, `ReflectionNamedType`,
//! `ReflectionUnionType` and `ReflectionIntersectionType`.
//!
//! Called from:
//! - `injection`, which installs them in place of the empty constructor bodies when the table
//!   reflectors are on ([`crate::reflection_table::enabled`]).
//!
//! Key details:
//! - A member constructor finds its row in the module reflection table by the key
//!   `(kind, lowercase class, member name)`, stores every plain-data slot from it, and runs the
//!   row's extras filler (attributes, a value the decoder lacks) when it has one. It is ONE body
//!   for every member of every class: nothing in it grows with the number of reflected classes.
//! - The layout is `crate::reflection_table`; the emitter (`member_table.rs`) and this file agree
//!   on it and on nothing else.

use crate::parser::ast::{BinOp, CastType, ClassMethod, Expr, Stmt, TypeExpr, Visibility};
use crate::reflection_table::{member, member_flag, member_kind, type_node, word};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_cast, e_concat, e_int, e_method_call, e_new, e_str, e_this,
    e_this_prop, e_var, s_array_push, s_assign, s_expr, s_for, s_if, s_prop_assign, s_return,
};

use super::class_relations::throw_new_reflection_exception;
use super::class_table_ctor::{decode_method, private_method};
use crate::types::traits::FlattenedClass;

/// Which class-constant reflector to build.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ConstantKind {
    Constant,
    UnitCase,
    BackedCase,
}

fn mw(at: usize) -> Expr {
    e_call("__elephc_refl_word", vec![e_var("row"), e_int(at as i64)])
}

fn flag_set(bit: i64) -> Expr {
    e_binop(
        e_binop(e_var("flags"), BinOp::BitAnd, e_int(bit)),
        BinOp::StrictNotEq,
        e_int(0),
    )
}

fn pool_one(pool: Expr) -> Expr {
    e_call("__elephc_refl_pool_str", vec![pool, e_int(0)])
}

/// `$class` as the lowercase name a table key holds: an object's class, or the string without a
/// leading separator.
fn class_key_statements() -> Vec<Stmt> {
    vec![
        s_if(
            e_call("is_object", vec![e_var("class_name")]),
            vec![s_assign("shown", e_call("get_class", vec![e_var("class_name")]))],
            vec![],
            Some(vec![s_assign("shown", e_cast(CastType::String, e_var("class_name")))]),
        ),
        s_assign(
            "lower",
            e_call("strtolower", vec![e_call("ltrim", vec![e_var("shown"), e_str("\\")])]),
        ),
    ]
}

/// `"<kind>\0" . $lower . "\0" . $member`
fn key_expression(kind: u8, member_variable: &str) -> Expr {
    e_concat(
        e_concat(
            e_concat(e_str(&format!("{}\0", kind as char)), e_var("lower")),
            e_str("\0"),
        ),
        e_var(member_variable),
    )
}

/// The message-raising tail shared by the member constructors: `Class "X" does not exist` when
/// the class has no row, else `<what>` (built by the caller).
fn missing_member(not_found: Expr) -> Vec<Stmt> {
    vec![
        s_if(
            e_binop(
                e_call("__elephc_refl_row", vec![e_var("lower")]),
                BinOp::StrictEq,
                e_int(0),
            ),
            vec![throw_new_reflection_exception(
                e_concat(
                    e_concat(e_str("Class \""), e_var("shown")),
                    e_str("\" does not exist"),
                ),
                crate::span::Span::dummy(),
            )],
            vec![],
            None,
        ),
        throw_new_reflection_exception(not_found, crate::span::Span::dummy()),
    ]
}

/// The class a message names: the row's canonical name, else what the caller passed.
fn canonical_class() -> Vec<Stmt> {
    vec![
        s_assign("class_row", e_call("__elephc_refl_row", vec![e_var("lower")])),
        s_if(
            e_binop(e_var("class_row"), BinOp::StrictNotEq, e_int(0)),
            vec![s_assign(
                "shown",
                pool_one(e_call(
                    "__elephc_refl_word",
                    vec![e_var("class_row"), e_int(word::NAME as i64)],
                )),
            )],
            vec![],
            None,
        ),
    ]
}

/// `$this-><type slot> = $this->__type_from(<type node>)`
fn type_from(node: Expr) -> Expr {
    e_method_call(e_this(), "__type_from", vec![node])
}

fn constant_constructor(kind: ConstantKind) -> ClassMethod {
    let key_kind = if kind == ConstantKind::Constant {
        member_kind::CONSTANT
    } else {
        member_kind::CASE
    };
    let mut body = vec![mark_from_table()];
    body.extend(class_key_statements());
    body.push(s_assign("row", e_call("__elephc_refl_row", vec![key_expression(key_kind, "constant_name")])));
    let mut missing = canonical_class();
    missing.extend(missing_member(e_concat(
        e_concat(
            e_concat(e_str("Constant "), e_var("shown")),
            e_str("::"),
        ),
        e_concat(e_var("constant_name"), e_str(" does not exist")),
    )));
    if kind != ConstantKind::Constant {
        // A plain constant of an enum is not a case.
        let mut plain = Vec::new();
        plain.push(s_if(
            e_binop(
                e_call(
                    "__elephc_refl_row",
                    vec![key_expression(member_kind::CONSTANT, "constant_name")],
                ),
                BinOp::StrictNotEq,
                e_int(0),
            ),
            {
                let mut stmts = canonical_class();
                stmts.push(throw_new_reflection_exception(
                    e_concat(
                        e_concat(
                            e_concat(e_str("Constant "), e_var("shown")),
                            e_str("::"),
                        ),
                        e_concat(e_var("constant_name"), e_str(" is not a case")),
                    ),
                    crate::span::Span::dummy(),
                ));
                stmts
            },
            vec![],
            None,
        ));
        plain.extend(missing);
        missing = plain;
    }
    body.push(s_if(
        e_binop(e_var("row"), BinOp::StrictEq, e_int(0)),
        missing,
        vec![],
        None,
    ));
    body.push(s_assign("flags", mw(member::FLAGS)));
    body.push(s_prop_assign(e_this(), "__name", pool_one(mw(member::NAME))));
    body.push(s_prop_assign(e_this(), "__class", pool_one(mw(member::CLASS))));
    body.push(s_prop_assign(e_this(), "__modifiers", mw(member::MODIFIERS)));
    body.push(s_prop_assign(e_this(), "__is_public", flag_set(member_flag::PUBLIC)));
    body.push(s_prop_assign(e_this(), "__is_protected", flag_set(member_flag::PROTECTED)));
    body.push(s_prop_assign(e_this(), "__is_private", flag_set(member_flag::PRIVATE)));
    body.push(s_prop_assign(e_this(), "__is_final", flag_set(member_flag::FINAL)));
    body.push(s_prop_assign(e_this(), "__is_enum_case", flag_set(member_flag::ENUM_CASE)));
    if kind == ConstantKind::Constant {
        body.push(s_prop_assign(e_this(), "__has_type", e_binop(mw(member::TYPE), BinOp::StrictNotEq, e_int(0))));
        body.push(s_prop_assign(e_this(), "__type", type_from(mw(member::TYPE))));
    }
    body.push(s_assign("value_node", mw(member::VALUE)));
    body.push(s_if(
        e_binop(e_var("value_node"), BinOp::StrictNotEq, e_int(0)),
        vec![s_prop_assign(
            e_this(),
            "__value",
            e_method_call(e_this(), "__decode", vec![e_var("value_node")]),
        )],
        vec![],
        None,
    ));
    if kind == ConstantKind::BackedCase {
        body.push(s_assign("backing_node", mw(member::BACKING_VALUE)));
        body.push(s_if(
            e_binop(e_var("backing_node"), BinOp::StrictNotEq, e_int(0)),
            vec![s_prop_assign(
                e_this(),
                "__backing_value",
                e_method_call(e_this(), "__decode", vec![e_var("backing_node")]),
            )],
            vec![],
            None,
        ));
    }
    body.push(s_assign("extras", mw(member::EXTRAS)));
    body.push(s_if(
        e_binop(e_var("extras"), BinOp::StrictNotEq, e_int(0)),
        vec![s_expr(e_call(
            "__elephc_reflection_fill_members",
            vec![e_var("extras"), e_this()],
        ))],
        vec![],
        None,
    ));
    let mut method = private_method(
        "__construct",
        vec![
            ("class_name".to_string(), Some(super::ast_literals::mixed_type()), None, false),
            ("constant_name".to_string(), Some(TypeExpr::Str), None, false),
        ],
        body,
    );
    method.visibility = Visibility::Public;
    method.return_type = None;
    method
}

fn property_constructor() -> ClassMethod {
    let mut body = vec![mark_from_table()];
    body.extend(class_key_statements());
    body.push(s_assign(
        "row",
        e_call("__elephc_refl_row", vec![key_expression(member_kind::PROPERTY, "property_name")]),
    ));
    // No row: a dynamic property of an object (php reflects those), else the exception.
    let mut absent = vec![s_if(
        e_binop(
            e_call("is_object", vec![e_var("class_name")]),
            BinOp::And,
            e_call("property_exists", vec![e_var("class_name"), e_var("property_name")]),
        ),
        vec![
            s_prop_assign(e_this(), "__name", e_var("property_name")),
            s_prop_assign(e_this(), "__class", e_var("shown")),
            s_prop_assign(e_this(), "__modifiers", e_int(1)),
            s_prop_assign(e_this(), "__is_public", e_bool(true)),
            s_prop_assign(e_this(), "__is_dynamic", e_bool(true)),
        ],
        vec![],
        Some({
            let mut missing = canonical_class();
            missing.extend(missing_member(e_concat(
                e_concat(
                    e_concat(e_str("Property "), e_var("shown")),
                    e_str("::$"),
                ),
                e_concat(e_var("property_name"), e_str(" does not exist")),
            )));
            missing
        }),
    )];
    let mut fill = vec![s_assign("flags", mw(member::FLAGS))];
    fill.push(s_prop_assign(e_this(), "__name", pool_one(mw(member::NAME))));
    fill.push(s_prop_assign(e_this(), "__class", pool_one(mw(member::CLASS))));
    fill.push(s_prop_assign(e_this(), "__modifiers", mw(member::MODIFIERS)));
    for (property, bit) in [
        ("__is_static", member_flag::STATIC),
        ("__is_public", member_flag::PUBLIC),
        ("__is_protected", member_flag::PROTECTED),
        ("__is_private", member_flag::PRIVATE),
        ("__is_final", member_flag::FINAL),
        ("__is_abstract", member_flag::ABSTRACT),
        ("__is_readonly", member_flag::READONLY),
        ("__is_promoted", member_flag::PROMOTED),
        ("__is_virtual", member_flag::VIRTUAL),
        ("__has_default_value", member_flag::HAS_DEFAULT),
        ("__has_hooks", member_flag::HAS_HOOKS),
    ] {
        fill.push(s_prop_assign(e_this(), property, flag_set(bit)));
    }
    fill.push(s_assign("type_object", type_from(mw(member::TYPE))));
    fill.push(s_prop_assign(e_this(), "__type", e_var("type_object")));
    fill.push(s_prop_assign(e_this(), "__settable_type", e_var("type_object")));
    fill.push(s_assign("value_node", mw(member::VALUE)));
    fill.push(s_if(
        e_binop(e_var("value_node"), BinOp::StrictNotEq, e_int(0)),
        vec![s_prop_assign(
            e_this(),
            "__default_value",
            e_method_call(e_this(), "__decode", vec![e_var("value_node")]),
        )],
        vec![],
        None,
    ));
    fill.push(s_prop_assign(e_this(), "__string", pool_one(mw(member::STRING))));
    fill.push(s_assign("extras", mw(member::EXTRAS)));
    fill.push(s_if(
        e_binop(e_var("extras"), BinOp::StrictNotEq, e_int(0)),
        vec![s_expr(e_call(
            "__elephc_reflection_fill_members",
            vec![e_var("extras"), e_this()],
        ))],
        vec![],
        None,
    ));
    body.push(s_if(
        e_binop(e_var("row"), BinOp::StrictEq, e_int(0)),
        std::mem::take(&mut absent),
        vec![],
        Some(fill),
    ));
    let mut method = private_method(
        "__construct",
        vec![
            ("class_name".to_string(), Some(super::ast_literals::mixed_type()), None, false),
            ("property_name".to_string(), Some(TypeExpr::Str), None, false),
        ],
        body,
    );
    method.visibility = Visibility::Public;
    method.return_type = None;
    method
}

/// Makes `ReflectionProperty` build itself from the table.
pub(super) fn apply_property_table(class: &mut FlattenedClass) {
    add_from_table_mark(class);
    if let Some(constructor) = class.methods.iter_mut().find(|method| method.name == "__construct") {
        *constructor = property_constructor();
    }
    class.methods.push(decode_method());
    class.methods.push(type_from_method());
    replace_body(
        class,
        "getDeclaringClass",
        vec![s_return(e_new("ReflectionClass", vec![e_this_prop("__class")]))],
    );
}

fn method_constructor() -> ClassMethod {
    let mut body = vec![mark_from_table()];
    // `new ReflectionMethod($closure, '__invoke')` reflects `Closure::__invoke`, whose signature
    // is the closure's own: Twig's `debug:twig` reads a closure filter's parameters this way.
    let shape = |at: i64| crate::synthetic_class::e_index(e_var("shape"), e_int(at));
    body.push(s_if(
        e_binop(
            crate::synthetic_class::e_instance_of(e_var("class_name"), "\\Closure"),
            BinOp::And,
            e_binop(
                e_call("strtolower", vec![e_cast(CastType::String, e_var("method_name"))]),
                BinOp::StrictEq,
                e_str("__invoke"),
            ),
        ),
        vec![
            s_assign(
                "invoked",
                crate::synthetic_class::e_new_fq("ReflectionFunction", vec![e_var("class_name")]),
            ),
            s_assign(
                "shape",
                crate::synthetic_class::e_method_call(e_var("invoked"), INVOKE_SHAPE_METHOD, Vec::new()),
            ),
            s_assign(
                "invoke_type",
                crate::synthetic_class::e_method_call(e_var("invoked"), INVOKE_TYPE_METHOD, Vec::new()),
            ),
            s_prop_assign(e_this(), "__name", e_str("__invoke")),
            s_prop_assign(e_this(), "__class", e_str("Closure")),
            s_prop_assign(e_this(), "__modifiers", e_int(1)),
            s_prop_assign(e_this(), "__is_public", e_bool(true)),
            s_prop_assign(e_this(), "__parameter_mode", e_int(INTERNAL_PARAMETER_MODE)),
            s_prop_assign(e_this(), "__parameters_row", shape(0)),
            s_prop_assign(e_this(), "__parameters_filler", shape(1)),
            s_prop_assign(e_this(), "__parameter_count", shape(2)),
            s_prop_assign(e_this(), "__required_parameter_count", shape(3)),
            s_prop_assign(e_this(), "__type", e_var("invoke_type")),
            s_prop_assign(
                e_this(),
                "__has_return_type",
                e_binop(e_var("invoke_type"), BinOp::StrictNotEq, crate::synthetic_class::e_null()),
            ),
            crate::synthetic_class::s_return_void(),
        ],
        vec![],
        None,
    ));
    // `new ReflectionMethod('Class::method')`: the name rides in the first argument.
    body.push(s_if(
        e_binop(e_var("method_name"), BinOp::StrictEq, crate::synthetic_class::e_null()),
        vec![
            s_assign("split", e_call("strpos", vec![e_cast(CastType::String, e_var("class_name")), e_str("::")])),
            s_if(
                e_binop(e_var("split"), BinOp::StrictEq, e_bool(false)),
                vec![throw_new_reflection_exception(
                    e_str("ReflectionMethod::__construct(): Argument #1 ($objectOrMethod) must be a valid method name"),
                    crate::span::Span::dummy(),
                )],
                vec![],
                None,
            ),
            s_assign(
                "method_name",
                e_call(
                    "substr",
                    vec![
                        e_cast(CastType::String, e_var("class_name")),
                        e_binop(e_cast(CastType::Int, e_var("split")), BinOp::Add, e_int(2)),
                    ],
                ),
            ),
            s_assign(
                "class_name",
                e_call(
                    "substr",
                    vec![
                        e_cast(CastType::String, e_var("class_name")),
                        e_int(0),
                        e_cast(CastType::Int, e_var("split")),
                    ],
                ),
            ),
        ],
        vec![],
        None,
    ));
    body.extend(class_key_statements());
    body.push(s_assign(
        "row",
        e_call(
            "__elephc_refl_row",
            vec![key_expression_lower(member_kind::METHOD, "method_name")],
        ),
    ));
    let mut missing = canonical_class();
    missing.extend(missing_member(e_concat(
        e_concat(
            e_concat(e_str("Method "), e_var("shown")),
            e_str("::"),
        ),
        e_concat(e_var("method_name"), e_str("() does not exist")),
    )));
    let mut fill = vec![s_assign("flags", mw(member::FLAGS))];
    fill.push(s_prop_assign(e_this(), "__name", pool_one(mw(member::NAME))));
    fill.push(s_prop_assign(e_this(), "__class", pool_one(mw(member::CLASS))));
    fill.push(s_prop_assign(e_this(), "__modifiers", mw(member::MODIFIERS)));
    for (property, bit) in [
        ("__is_static", member_flag::STATIC),
        ("__is_public", member_flag::PUBLIC),
        ("__is_protected", member_flag::PROTECTED),
        ("__is_private", member_flag::PRIVATE),
        ("__is_final", member_flag::FINAL),
        ("__is_abstract", member_flag::ABSTRACT),
        ("__is_deprecated", member_flag::DEPRECATED),
        ("__is_generator", member_flag::GENERATOR),
        ("__returns_reference", member_flag::RETURNS_REFERENCE),
        ("__has_prototype", member_flag::HAS_PROTOTYPE),
    ] {
        fill.push(s_prop_assign(e_this(), property, flag_set(bit)));
    }
    fill.push(s_assign("return_type", type_from(mw(member::TYPE))));
    fill.push(s_prop_assign(e_this(), "__type", e_var("return_type")));
    fill.push(s_prop_assign(
        e_this(),
        "__has_return_type",
        e_binop(e_var("return_type"), BinOp::StrictNotEq, crate::synthetic_class::e_null()),
    ));
    fill.push(s_prop_assign(e_this(), "__required_parameter_count", mw(member::COUNT)));
    fill.push(s_prop_assign(e_this(), "__parameter_count", mw(member::PARAMETER_COUNT)));
    fill.push(s_prop_assign(e_this(), "__parameters_row", mw(member::PARAMETERS)));
    fill.push(s_if(
        e_binop(mw(member::PARAMETER_COUNT), BinOp::Gt, e_int(0)),
        vec![s_prop_assign(e_this(), "__parameters_filler", e_int(-1))],
        vec![],
        None,
    ));
    fill.push(s_if(
        flag_set(member_flag::HAS_PROTOTYPE),
        vec![
            s_prop_assign(e_this(), "__prototype_class", pool_one(mw(member::PROTOTYPE_CLASS))),
            s_prop_assign(e_this(), "__prototype_name", pool_one(mw(member::PROTOTYPE_NAME))),
        ],
        vec![],
        None,
    ));
    fill.push(s_assign("extras", mw(member::EXTRAS)));
    fill.push(s_if(
        e_binop(e_var("extras"), BinOp::StrictNotEq, e_int(0)),
        vec![s_expr(e_call(
            "__elephc_reflection_fill_members",
            vec![e_var("extras"), e_this()],
        ))],
        vec![],
        None,
    ));
    body.push(s_if(
        e_binop(e_var("row"), BinOp::StrictEq, e_int(0)),
        missing,
        vec![],
        Some(fill),
    ));
    let mut method = private_method(
        "__construct",
        vec![
            ("class_name".to_string(), Some(super::ast_literals::mixed_type()), None, false),
            (
                "method_name".to_string(),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Str))),
                Some(crate::synthetic_class::e_null()),
                false,
            ),
        ],
        body,
    );
    method.visibility = Visibility::Public;
    method.return_type = None;
    method
}

/// `"<kind>\0" . $lower . "\0" . strtolower($member)`
fn key_expression_lower(kind: u8, member_variable: &str) -> Expr {
    e_concat(
        e_concat(
            e_concat(e_str(&format!("{}\0", kind as char)), e_var("lower")),
            e_str("\0"),
        ),
        e_call("strtolower", vec![e_var(member_variable)]),
    )
}

/// Makes `ReflectionMethod` build itself from the table.
pub(super) fn apply_method_table(class: &mut FlattenedClass) {
    add_from_table_mark(class);
    use super::{builtin_property, empty_string, int_lit};
    for (name, ty, default) in [
        ("__parameters_row", TypeExpr::Int, int_lit(0)),
        ("__parameter_mode", TypeExpr::Int, int_lit(0)),
        ("__prototype_class", TypeExpr::Str, empty_string()),
        ("__prototype_name", TypeExpr::Str, empty_string()),
    ] {
        class.properties.push(builtin_property(name, Visibility::Private, Some(ty), default));
    }
    if let Some(constructor) = class.methods.iter_mut().find(|method| method.name == "__construct") {
        *constructor = method_constructor();
    }
    class.methods.push(type_from_method());
    class.methods.push(parameters_fill_method());
    replace_body(
        class,
        "getDeclaringClass",
        vec![s_return(e_new("ReflectionClass", vec![e_this_prop("__class")]))],
    );
    replace_body(
        class,
        "getPrototype",
        vec![
            s_if(
                crate::synthetic_class::e_not(e_this_prop("__has_prototype")),
                vec![throw_new_reflection_exception(
                    e_str("Method does not have a prototype"),
                    crate::span::Span::dummy(),
                )],
                vec![],
                None,
            ),
            s_return(e_new(
                "ReflectionMethod",
                vec![e_this_prop("__prototype_class"), e_this_prop("__prototype_name")],
            )),
        ],
    );
}

/// `private function __fill_generic__parameters(): void` -- the method's parameter reflectors,
/// read from the parameter rows that follow `__parameters_row`.
fn parameters_fill_method() -> ClassMethod {
    let body = vec![
        s_assign("list", e_array(Vec::new())),
        s_for(
            Some(s_assign("i", e_int(0))),
            Some(e_binop(e_var("i"), BinOp::Lt, e_this_prop("__parameter_count"))),
            Some(s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1)))),
            vec![s_array_push(
                "list",
                e_new(
                    "ReflectionParameter",
                    vec![
                        e_binop(e_this_prop("__parameters_row"), BinOp::Add, e_var("i")),
                        e_this_prop("__parameter_mode"),
                    ],
                ),
            )],
        ),
        s_prop_assign(e_this(), "__parameters", e_var("list")),
    ];
    private_method("__fill_generic__parameters", Vec::new(), body)
}

fn parameter_constructor() -> ClassMethod {
    let mut body = vec![mark_from_table(), s_assign("row", e_cast(CastType::Int, e_var("function")))];
    body.push(s_assign("flags", mw(member::FLAGS)));
    body.push(s_prop_assign(e_this(), "__name", pool_one(mw(member::NAME))));
    body.push(s_prop_assign(e_this(), "name", pool_one(mw(member::NAME))));
    body.push(s_prop_assign(e_this(), "__position", mw(member::COUNT)));
    for (property, bit) in [
        ("__optional", member_flag::OPTIONAL),
        ("__variadic", member_flag::VARIADIC),
        ("__is_passed_by_reference", member_flag::BY_REFERENCE),
        ("__is_promoted", member_flag::PROMOTED),
        ("__has_type", member_flag::HAS_TYPE),
        ("__allows_null", member_flag::ALLOWS_NULL),
        ("__is_array_type", member_flag::ARRAY_TYPE),
        ("__is_callable_type", member_flag::CALLABLE_TYPE),
        ("__has_default_value", member_flag::HAS_DEFAULT),
        ("__is_default_value_constant", member_flag::DEFAULT_CONSTANT),
    ] {
        body.push(s_prop_assign(e_this(), property, flag_set(bit)));
    }
    body.push(s_prop_assign(e_this(), "__type", type_from(mw(member::TYPE))));
    body.push(s_if(
        flag_set(member_flag::DEFAULT_CONSTANT),
        vec![s_prop_assign(
            e_this(),
            "__default_value_constant_name",
            pool_one(mw(member::STRING)),
        )],
        vec![],
        None,
    ));
    body.push(s_assign("value_node", mw(member::VALUE)));
    body.push(s_if(
        e_binop(e_var("value_node"), BinOp::StrictNotEq, e_int(0)),
        vec![s_if(
            e_binop(
                e_call("__elephc_refl_at", vec![e_var("value_node"), e_int(0)]),
                BinOp::StrictEq,
                e_int(crate::reflection_table::value::OBJECT),
            ),
            // An object default is built when asked: the class name and the argument array wait.
            vec![
                s_prop_assign(
                    e_this(),
                    "__default_value_object_class",
                    pool_one(e_call("__elephc_refl_at", vec![e_var("value_node"), e_int(1)])),
                ),
                s_prop_assign(
                    e_this(),
                    "__default_value",
                    e_method_call(
                        e_this(),
                        "__decode",
                        vec![e_call("__elephc_refl_at", vec![e_var("value_node"), e_int(2)])],
                    ),
                ),
            ],
            vec![],
            Some(vec![s_prop_assign(
                e_this(),
                "__default_value",
                e_method_call(e_this(), "__decode", vec![e_var("value_node")]),
            )]),
        )],
        vec![],
        None,
    ));
    body.push(s_prop_assign(e_this(), "__function_name", pool_one(mw(member::FUNCTION))));
    body.push(s_prop_assign(e_this(), "__class_name", pool_one(mw(member::CLASS))));
    // A parameter of an internal method (`Closure::__invoke`) has no default php can show.
    body.push(s_if(
        e_binop(e_var("param"), BinOp::StrictEq, e_int(INTERNAL_PARAMETER_MODE)),
        vec![
            s_prop_assign(e_this(), "__has_default_value", e_bool(false)),
            s_prop_assign(e_this(), "__is_default_value_constant", e_bool(false)),
            s_prop_assign(e_this(), "__function_name", e_str("__invoke")),
            s_prop_assign(e_this(), "__class_name", e_str("Closure")),
        ],
        vec![],
        None,
    ));
    body.push(s_assign("extras", mw(member::EXTRAS)));
    body.push(s_if(
        e_binop(e_var("extras"), BinOp::StrictNotEq, e_int(0)),
        vec![s_expr(e_call(
            "__elephc_reflection_fill_members",
            vec![e_var("extras"), e_this()],
        ))],
        vec![],
        None,
    ));
    let mut method = private_method(
        "__construct",
        vec![
            ("function".to_string(), Some(super::ast_literals::mixed_type()), None, false),
            ("param".to_string(), Some(super::ast_literals::mixed_type()), None, false),
        ],
        body,
    );
    method.visibility = Visibility::Public;
    method.return_type = None;
    method
}

/// Makes `ReflectionParameter` build itself from a parameter row of the table.
pub(super) fn apply_parameter_table(class: &mut FlattenedClass) {
    add_from_table_mark(class);
    use super::{builtin_property, empty_string};
    for name in ["__function_name", "__class_name"] {
        class
            .properties
            .push(builtin_property(name, Visibility::Private, Some(TypeExpr::Str), empty_string()));
    }
    if let Some(constructor) = class.methods.iter_mut().find(|method| method.name == "__construct") {
        *constructor = parameter_constructor();
    }
    class.methods.push(decode_method());
    class.methods.push(type_from_method());
    replace_body(
        class,
        "getDeclaringClass",
        vec![
            s_if(
                e_binop(e_this_prop("__class_name"), BinOp::StrictEq, e_str("")),
                vec![s_return(crate::synthetic_class::e_null())],
                vec![],
                None,
            ),
            s_return(e_new("ReflectionClass", vec![e_this_prop("__class_name")])),
        ],
    );
    replace_body(
        class,
        "getDeclaringFunction",
        vec![
            s_if(
                e_binop(e_this_prop("__class_name"), BinOp::StrictEq, e_str("")),
                vec![s_return(e_new("ReflectionFunction", vec![e_this_prop("__function_name")]))],
                vec![],
                None,
            ),
            s_return(e_new(
                "ReflectionMethod",
                vec![e_this_prop("__class_name"), e_this_prop("__function_name")],
            )),
        ],
    );
}

fn function_constructor() -> ClassMethod {
    let mut body = vec![mark_from_table(), s_if(
        e_call("is_string", vec![e_var("function")]),
        vec![
            s_assign("shown", e_cast(CastType::String, e_var("function"))),
            s_assign(
                "key",
                e_call("strtolower", vec![e_call("ltrim", vec![e_var("shown"), e_str("\\")])]),
            ),
            s_assign("cut", e_bool(false)),
        ],
        vec![],
        Some(vec![
            s_assign("shown", e_call("__elephc_refl_callable_name", vec![e_var("function")])),
            s_assign("key", e_call("strtolower", vec![e_var("shown")])),
            // A closure over a method (`$object->method(...)`, `Class::method(...)`) is named
            // `Class::method`; its row is the method's.
            s_assign("cut", e_call("strpos", vec![e_var("shown"), e_str("::")])),
            // A closure literal written in a method is named `{closure:Class::method():LINE}`:
            // its `::` belongs to the location, and its row is the closure's own. Cutting there
            // looked up a method `method():LINE}` and threw "Function ... does not exist"
            // (symfony/dependency-injection's `ServicesBundle::loadExtension()` configurators).
            s_if(
                e_call("str_starts_with", vec![e_var("shown"), e_str("{closure")]),
                vec![s_assign("cut", e_bool(false))],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "__callable", e_var("function")),
            s_prop_assign(e_this(), "__callable_name", e_var("shown")),
        ]),
    )];
    body.push(s_assign(
        "row",
        e_call(
            "__elephc_refl_row",
            vec![e_concat(e_str(&format!("{}\0\0", member_kind::FUNCTION as char)), e_var("key"))],
        ),
    ));
    body.push(s_if(
        e_binop(e_var("cut"), BinOp::StrictNotEq, e_bool(false)),
        vec![s_assign(
            "row",
            e_call(
                "__elephc_refl_row",
                vec![e_concat(
                    e_concat(
                        e_concat(
                            e_str(&format!("{}\0", member_kind::METHOD as char)),
                            e_call(
                                "strtolower",
                                vec![e_call(
                                    "ltrim",
                                    vec![
                                        e_call(
                                            "substr",
                                            vec![e_var("shown"), e_int(0), e_cast(CastType::Int, e_var("cut"))],
                                        ),
                                        e_str("\\"),
                                    ],
                                )],
                            ),
                        ),
                        e_str("\0"),
                    ),
                    e_call(
                        "strtolower",
                        vec![e_call(
                            "substr",
                            vec![
                                e_var("shown"),
                                e_binop(e_cast(CastType::Int, e_var("cut")), BinOp::Add, e_int(2)),
                            ],
                        )],
                    ),
                )],
            ),
        )],
        vec![],
        None,
    ));
    let missing = vec![throw_new_reflection_exception(
        e_concat(
            e_concat(e_str("Function "), e_var("shown")),
            e_str("() does not exist"),
        ),
        crate::span::Span::dummy(),
    )];
    let mut fill = vec![s_assign("flags", mw(member::FLAGS))];
    fill.push(s_prop_assign(e_this(), "__name", pool_one(mw(member::NAME))));
    for (property, bit) in [
        ("__is_deprecated", member_flag::DEPRECATED),
        ("__is_generator", member_flag::GENERATOR),
        ("__returns_reference", member_flag::RETURNS_REFERENCE),
        ("__is_closure", member_flag::CLOSURE),
    ] {
        fill.push(s_prop_assign(e_this(), property, flag_set(bit)));
    }
    fill.push(s_if(
        e_binop(e_var("cut"), BinOp::StrictNotEq, e_bool(false)),
        vec![s_prop_assign(e_this(), "__is_closure", e_bool(true))],
        vec![],
        None,
    ));
    fill.push(s_assign("split", e_call("strrpos", vec![e_this_prop("__name"), e_str("\\")])));
    fill.push(s_if(
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
                        e_binop(e_cast(CastType::Int, e_var("split")), BinOp::Add, e_int(1)),
                    ],
                ),
            ),
            s_prop_assign(
                e_this(),
                "__namespace_name",
                e_call(
                    "substr",
                    vec![e_this_prop("__name"), e_int(0), e_cast(CastType::Int, e_var("split"))],
                ),
            ),
            s_prop_assign(e_this(), "__in_namespace", e_bool(true)),
        ]),
    ));
    fill.push(s_assign("return_type", type_from(mw(member::TYPE))));
    fill.push(s_prop_assign(e_this(), "__type", e_var("return_type")));
    fill.push(s_prop_assign(
        e_this(),
        "__has_return_type",
        e_binop(e_var("return_type"), BinOp::StrictNotEq, crate::synthetic_class::e_null()),
    ));
    fill.push(s_prop_assign(e_this(), "__required_parameter_count", mw(member::COUNT)));
    fill.push(s_prop_assign(e_this(), "__parameter_count", mw(member::PARAMETER_COUNT)));
    fill.push(s_prop_assign(e_this(), "__parameters_row", mw(member::PARAMETERS)));
    fill.push(s_if(
        e_binop(mw(member::PARAMETER_COUNT), BinOp::Gt, e_int(0)),
        vec![s_prop_assign(e_this(), "__parameters_filler", e_int(-1))],
        vec![],
        None,
    ));
    fill.push(s_if(
        flag_set(member_flag::HAS_FILE),
        vec![
            s_prop_assign(e_this(), "__file_name", pool_one(mw(member::DOC))),
            s_prop_assign(e_this(), "__start_line", mw(member::START_LINE)),
            s_prop_assign(e_this(), "__end_line", mw(member::END_LINE)),
        ],
        vec![],
        None,
    ));
    fill.push(s_assign("extras", mw(member::EXTRAS)));
    fill.push(s_if(
        e_binop(e_var("extras"), BinOp::StrictNotEq, e_int(0)),
        vec![s_expr(e_call(
            "__elephc_reflection_fill_members",
            vec![e_var("extras"), e_this()],
        ))],
        vec![],
        None,
    ));
    body.push(s_if(
        e_binop(e_var("row"), BinOp::StrictEq, e_int(0)),
        missing,
        vec![],
        Some(fill),
    ));
    let mut method = private_method(
        "__construct",
        vec![(
            "function".to_string(),
            Some(TypeExpr::Union(vec![
                TypeExpr::Named(crate::names::Name::unqualified("Closure")),
                TypeExpr::Str,
            ])),
            None,
            false,
        )],
        body,
    );
    method.visibility = Visibility::Public;
    method.return_type = None;
    method
}

/// Makes `ReflectionFunction` build itself from the table.
pub(super) fn apply_function_table(class: &mut FlattenedClass) {
    add_from_table_mark(class);
    use super::{bool_lit, builtin_property, int_lit};
    class
        .properties
        .push(builtin_property("__parameters_row", Visibility::Private, Some(TypeExpr::Int), int_lit(0)));
    class
        .properties
        .push(builtin_property("__parameter_mode", Visibility::Private, Some(TypeExpr::Int), int_lit(0)));
    class
        .properties
        .push(builtin_property("__is_closure", Visibility::Private, Some(TypeExpr::Bool), bool_lit(false)));
    if let Some(constructor) = class.methods.iter_mut().find(|method| method.name == "__construct") {
        *constructor = function_constructor();
    }
    class.methods.push(type_from_method());
    class.methods.push(parameters_fill_method());
    class.methods.push(invoke_shape_method());
    class.methods.push(invoke_type_method());
    replace_body(class, "isClosure", vec![s_return(e_this_prop("__is_closure"))]);
    class.properties.push(builtin_property(
        "__callable_name",
        Visibility::Private,
        Some(TypeExpr::Str),
        super::empty_string(),
    ));
    // A closure literal is named `{closure:...}`; a first-class callable carries its target's name
    // (`strlen`, `K::m`). Only the first is anonymous.
    replace_body(
        class,
        "isAnonymous",
        vec![s_return(e_call(
            "str_starts_with",
            vec![e_this_prop("__callable_name"), e_str("{closure")],
        ))],
    );
    for method in ["getClosureScopeClass", "getClosureCalledClass"] {
        replace_body(class, method, closure_class_body());
    }
    // A builtin's row carries no file; a user function's and a closure literal's do.
    let internal = || e_binop(e_this_prop("__file_name"), BinOp::StrictEq, e_str(""));
    replace_body(class, "isInternal", vec![s_return(internal())]);
    replace_body(
        class,
        "isUserDefined",
        vec![s_return(crate::synthetic_class::e_not(internal()))],
    );
}

/// The internal accessor `ReflectionMethod($closure, '__invoke')` copies its signature through.
const INVOKE_SHAPE_METHOD: &str = "__elephc_invoke_shape";

/// The `ReflectionParameter` mode of a parameter that belongs to an internal method.
const INTERNAL_PARAMETER_MODE: i64 = -1;

/// The accessor for the reflected closure's return type.
const INVOKE_TYPE_METHOD: &str = "__elephc_invoke_type";

/// `[parameters row, parameters filler, parameter count, required count]`.
fn invoke_shape_method() -> ClassMethod {
    let mut method = private_method(
        INVOKE_SHAPE_METHOD,
        Vec::new(),
        vec![s_return(crate::synthetic_class::e_array(vec![
            e_this_prop("__parameters_row"),
            e_this_prop("__parameters_filler"),
            e_this_prop("__parameter_count"),
            e_this_prop("__required_parameter_count"),
        ]))],
    );
    method.visibility = Visibility::Public;
    method.return_type = None;
    method
}

/// The return type object, apart from the integer shape so neither is coerced to the other.
fn invoke_type_method() -> ClassMethod {
    let mut method = private_method(INVOKE_TYPE_METHOD, Vec::new(), vec![s_return(e_this_prop("__type"))]);
    method.visibility = Visibility::Public;
    method.return_type = None;
    method
}

/// `getClosureScopeClass()`/`getClosureCalledClass()` from the callable's name: the class of a
/// method callable (`K::m`), or the class a closure literal was written in
/// (`{closure:Class::method():LINE}`). A function, a free closure, or a receiver whose class the
/// descriptor did not record (`object::m`) has none.
fn closure_class_body() -> Vec<Stmt> {
    let name = || e_this_prop("__callable_name");
    vec![
        s_assign("owner", name()),
        s_if(
            e_call("str_starts_with", vec![name(), e_str("{closure:")]),
            vec![s_assign("owner", e_call("substr", vec![name(), e_int(9)]))],
            vec![],
            None,
        ),
        s_assign("cut", e_call("strpos", vec![e_var("owner"), e_str("::")])),
        s_if(
            e_binop(e_var("cut"), BinOp::StrictEq, e_bool(false)),
            vec![s_return(crate::synthetic_class::e_null())],
            vec![],
            None,
        ),
        s_assign(
            "class",
            e_call("substr", vec![e_var("owner"), e_int(0), e_cast(CastType::Int, e_var("cut"))]),
        ),
        s_if(
            e_binop(e_var("class"), BinOp::StrictEq, e_str("object")),
            vec![s_return(crate::synthetic_class::e_null())],
            vec![],
            None,
        ),
        s_return(crate::synthetic_class::e_new_fq("ReflectionClass", vec![e_var("class")])),
    ]
}

/// `private function __type_from(int $node): mixed` -- the type reflector of a type node.
fn type_from_method() -> ClassMethod {
    let tag = || e_call("__elephc_refl_at", vec![e_var("node"), e_int(0)]);
    let is = |wanted: i64| e_binop(tag(), BinOp::StrictEq, e_int(wanted));
    let body = vec![
        s_if(
            e_binop(e_var("node"), BinOp::StrictEq, e_int(0)),
            vec![s_return(crate::synthetic_class::e_null())],
            vec![],
            None,
        ),
        s_if(
            is(type_node::NAMED),
            vec![s_return(e_new("ReflectionNamedType", vec![e_var("node")]))],
            vec![],
            None,
        ),
        s_if(
            is(type_node::UNION),
            vec![s_return(e_new("ReflectionUnionType", vec![e_var("node")]))],
            vec![],
            None,
        ),
        s_return(e_new("ReflectionIntersectionType", vec![e_var("node")])),
    ];
    let mut method = private_method(
        "__type_from",
        vec![("node".to_string(), Some(TypeExpr::Int), None, false)],
        body,
    );
    method.return_type = Some(super::ast_literals::mixed_type());
    method
}

/// Replaces the body of `name` in `class` (when it has such a method).
fn replace_body(class: &mut FlattenedClass, name: &str, body: Vec<Stmt>) {
    if let Some(method) = class.methods.iter_mut().find(|method| method.name == name) {
        // A reflector built some other way (the eval bridge, a per-class materializer) keeps the
        // method it always had; only a table-built one answers from its row.
        let original = std::mem::take(&mut method.body);
        let mut wrapped = vec![s_if(
            crate::synthetic_class::e_not(e_this_prop("__from_table")),
            original,
            vec![],
            None,
        )];
        wrapped.extend(body);
        method.body = wrapped;
    }
}

/// Adds the `__from_table` mark to `class`.
fn add_from_table_mark(class: &mut FlattenedClass) {
    class.properties.push(super::builtin_property(
        "__from_table",
        Visibility::Private,
        Some(TypeExpr::Bool),
        super::bool_lit(false),
    ));
}

/// The statement every table constructor starts with.
fn mark_from_table() -> Stmt {
    s_prop_assign(e_this(), "__from_table", e_bool(true))
}

/// Makes `class` (a class-constant or enum-case reflector) build itself from the table.
pub(super) fn apply_constant_table(class: &mut FlattenedClass, kind: ConstantKind) {
    add_from_table_mark(class);
    if let Some(constructor) = class.methods.iter_mut().find(|method| method.name == "__construct") {
        *constructor = constant_constructor(kind);
    }
    class.methods.push(decode_method());
    class.methods.push(type_from_method());
    // The declaring class is rebuilt from its name when asked: no per-member object graph.
    replace_body(
        class,
        "getDeclaringClass",
        vec![s_return(e_new("ReflectionClass", vec![e_this_prop("__class")]))],
    );
    if kind != ConstantKind::Constant {
        replace_body(
            class,
            "getEnum",
            vec![
                s_if(
                    e_binop(
                        e_this_prop("__enum"),
                        BinOp::StrictEq,
                        crate::synthetic_class::e_null(),
                    ),
                    vec![s_prop_assign(
                        e_this(),
                        "__enum",
                        e_new("ReflectionEnum", vec![e_this_prop("__class")]),
                    )],
                    vec![],
                    None,
                ),
                s_return(e_this_prop("__enum")),
            ],
        );
    }
}

/// Makes the three type reflectors build themselves from a type node.
pub(super) fn apply_type_tables(named: &mut FlattenedClass, union: &mut FlattenedClass, intersection: &mut FlattenedClass) {
    let at = |index: i64| e_call("__elephc_refl_at", vec![e_var("node"), e_int(index)]);
    let node_param = || vec![("node".to_string(), Some(TypeExpr::Int), Some(e_int(0)), false)];
    // Named: [tag, flags, name pool, 0]
    let named_body = vec![
        s_assign("flags", at(1)),
        s_prop_assign(e_this(), "__name", pool_one(at(2))),
        s_prop_assign(
            e_this(),
            "__allows_null",
            e_binop(
                e_binop(e_var("flags"), BinOp::BitAnd, e_int(1)),
                BinOp::StrictNotEq,
                e_int(0),
            ),
        ),
        s_prop_assign(
            e_this(),
            "__is_builtin",
            e_binop(
                e_binop(e_var("flags"), BinOp::BitAnd, e_int(2)),
                BinOp::StrictNotEq,
                e_int(0),
            ),
        ),
    ];
    install_constructor(named, node_param(), named_body);
    // Union / intersection: [tag, allows null, address of the child nodes, child count]
    for class in [union, intersection] {
        let body = vec![
            s_prop_assign(
                e_this(),
                "__allows_null",
                e_binop(at(1), BinOp::StrictNotEq, e_int(0)),
            ),
            s_assign("types", e_array(Vec::new())),
            s_for(
                Some(s_assign("i", e_int(0))),
                Some(e_binop(e_var("i"), BinOp::Lt, at(3))),
                Some(s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1)))),
                vec![s_array_push(
                    "types",
                    e_new(
                        "ReflectionNamedType",
                        vec![e_call(
                            "__elephc_refl_at",
                            vec![at(2), e_var("i")],
                        )],
                    ),
                )],
            ),
            s_prop_assign(e_this(), "__types", e_var("types")),
        ];
        install_constructor(class, node_param(), body);
    }
    let _ = e_bool;
}

fn install_constructor(
    class: &mut FlattenedClass,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    body: Vec<Stmt>,
) {
    if let Some(constructor) = class.methods.iter_mut().find(|method| method.name == "__construct") {
        let mut replacement = private_method("__construct", params, body);
        replacement.visibility = Visibility::Public;
        replacement.return_type = None;
        *constructor = replacement;
    }
}
