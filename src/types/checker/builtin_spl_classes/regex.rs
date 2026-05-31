//! Purpose:
//! Injects RegexIterator and RecursiveRegexIterator metadata and synthetic bodies.
//! Keeps regex filtering, mode-specific current/key behavior, and recursive child wrapping together.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - Capture-producing modes reuse `preg_replace_callback()` to materialize match arrays.
//! - Recursive children preserve regex mode, flags, preg flags, and replacement state.

use std::collections::HashMap;

use crate::parser::ast::{
    BinOp, CastType, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, Stmt, TypeExpr,
    Visibility,
};
use crate::types::traits::FlattenedClass;

use super::common::*;
use super::forwarding::{inner_call, iterator_iterator_construct_body};
use super::recursive_array::assume_recursive_iterator_expr;

const REGEX_USE_KEY: i64 = 1;
const REGEX_INVERT_MATCH: i64 = 2;
const REGEX_MATCH: i64 = 0;
const REGEX_GET_MATCH: i64 = 1;
const REGEX_ALL_MATCHES: i64 = 2;
const REGEX_SPLIT: i64 = 3;
const REGEX_REPLACE: i64 = 4;
const REGEX_CAPTURE_LIMIT: usize = 100;
const PREG_SET_ORDER: i64 = 2;
const PREG_OFFSET_CAPTURE: i64 = 256;

/// Inserts regex iterator classes into the supplied builtin metadata registry.
pub(super) fn insert_classes(class_map: &mut HashMap<String, FlattenedClass>) {
    class_map.insert(
        "RegexIterator".to_string(),
        FlattenedClass {
            name: "RegexIterator".to_string(),
            extends: Some("FilterIterator".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: regex_iterator_properties(),
            methods: spl_regex_iterator_methods("RegexIterator", named_type("Iterator"), false),
            attributes: Vec::new(),
            constants: regex_iterator_constants(),
            used_traits: Vec::new(),
        },
    );

    class_map.insert(
        "RecursiveRegexIterator".to_string(),
        FlattenedClass {
            name: "RecursiveRegexIterator".to_string(),
            extends: Some("RegexIterator".to_string()),
            implements: vec!["RecursiveIterator".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: recursive_regex_iterator_properties(),
            methods: spl_recursive_regex_iterator_methods(),
            attributes: Vec::new(),
            constants: regex_iterator_constants(),
            used_traits: Vec::new(),
        },
    );
}

/// Builds the property list for RecursiveRegexIterator.
fn recursive_regex_iterator_properties() -> Vec<ClassProperty> {
    vec![storage_property("recursiveInner", named_type("RecursiveIterator"))]
}

/// Builds the property list for RegexIterator.
fn regex_iterator_properties() -> Vec<ClassProperty> {
    vec![
        storage_property("regex", TypeExpr::Str),
        storage_property("mode", TypeExpr::Int),
        storage_property("flags", TypeExpr::Int),
        storage_property("pregFlags", TypeExpr::Int),
        storage_property_with_visibility(
            "replacement",
            Some(TypeExpr::Nullable(Box::new(TypeExpr::Str))),
            Some(null_expr()),
            Visibility::Public,
        ),
    ]
}

/// Builds the shared class constants exposed by regex iterator classes.
fn regex_iterator_constants() -> Vec<ClassConst> {
    vec![
        class_const("USE_KEY", REGEX_USE_KEY),
        class_const("INVERT_MATCH", REGEX_INVERT_MATCH),
        class_const("MATCH", REGEX_MATCH),
        class_const("GET_MATCH", REGEX_GET_MATCH),
        class_const("ALL_MATCHES", REGEX_ALL_MATCHES),
        class_const("SPLIT", REGEX_SPLIT),
        class_const("REPLACE", REGEX_REPLACE),
    ]
}

/// Builds the method list for RegexIterator-like classes.
fn spl_regex_iterator_methods(
    class_name: &str,
    iterator_type: TypeExpr,
    is_recursive: bool,
) -> Vec<ClassMethod> {
    vec![
        method_with_body(
            "__construct",
            vec![
                param("iterator", iterator_type),
                param("pattern", TypeExpr::Str),
                param_default("mode", TypeExpr::Int, int_expr(REGEX_MATCH)),
                param_default("flags", TypeExpr::Int, int_expr(0)),
                param_default("pregFlags", TypeExpr::Int, int_expr(0)),
            ],
            Some(TypeExpr::Void),
            regex_construct_body(class_name, is_recursive),
        ),
        method_with_body("accept", Vec::new(), Some(TypeExpr::Bool), regex_accept_body()),
        method_with_body("current", Vec::new(), Some(mixed_type()), regex_current_body()),
        method_with_body("key", Vec::new(), Some(mixed_type()), regex_key_body()),
        method_with_body("getMode", Vec::new(), Some(TypeExpr::Int), return_body(regex_mode_expr())),
        method_with_body(
            "setMode",
            vec![param("mode", TypeExpr::Int)],
            Some(TypeExpr::Void),
            regex_set_mode_body(class_name),
        ),
        method_with_body("getFlags", Vec::new(), Some(TypeExpr::Int), return_body(regex_flags_expr())),
        method_with_body(
            "setFlags",
            vec![param("flags", TypeExpr::Int)],
            Some(TypeExpr::Void),
            vec![property_assign_stmt(this_expr(), "flags", var_expr("flags"))],
        ),
        method_with_body("getRegex", Vec::new(), Some(TypeExpr::Str), return_body(regex_pattern_expr())),
        method_with_body("getPregFlags", Vec::new(), Some(TypeExpr::Int), return_body(regex_preg_flags_expr())),
        method_with_body(
            "setPregFlags",
            vec![param("pregFlags", TypeExpr::Int)],
            Some(TypeExpr::Void),
            vec![property_assign_stmt(this_expr(), "pregFlags", var_expr("pregFlags"))],
        ),
        protected_method_with_body(
            "__elephcRegexTarget",
            Vec::new(),
            Some(mixed_type()),
            regex_target_body(),
        ),
        protected_method_with_body(
            "__elephcFirstMatch",
            vec![param("subject", TypeExpr::Str)],
            Some(array_type()),
            regex_first_match_body(),
        ),
        protected_method_with_body(
            "__elephcAllMatches",
            vec![param("subject", TypeExpr::Str)],
            Some(array_type()),
            regex_all_matches_body(),
        ),
        protected_method_with_body(
            "__elephcSplit",
            vec![param("subject", TypeExpr::Str)],
            Some(array_type()),
            regex_split_body(),
        ),
    ]
}

/// Builds the method list for RecursiveRegexIterator.
fn spl_recursive_regex_iterator_methods() -> Vec<ClassMethod> {
    let mut methods = spl_regex_iterator_methods(
        "RecursiveRegexIterator",
        named_type("RecursiveIterator"),
        true,
    );
    methods.extend(vec![
        method_with_body(
            "hasChildren",
            Vec::new(),
            Some(TypeExpr::Bool),
            return_body(method_call(recursive_regex_inner_expr(), "hasChildren", Vec::new())),
        ),
        method_with_body(
            "getChildren",
            Vec::new(),
            Some(TypeExpr::Nullable(Box::new(named_type("RecursiveIterator")))),
            recursive_regex_get_children_body(),
        ),
        method_with_body(
            "__elephcAssumeRecursiveIterator",
            vec![param("iterator", mixed_type())],
            Some(named_type("RecursiveIterator")),
            Vec::new(),
        ),
    ]);
    methods
}

/// Builds a protected concrete synthetic method.
fn protected_method_with_body(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
) -> ClassMethod {
    let mut method = method_with_body(name, params, return_type, body);
    method.visibility = Visibility::Protected;
    method
}

/// Builds the expression for `$this->regex`.
fn regex_pattern_expr() -> Expr {
    property_access(this_expr(), "regex")
}

/// Builds the expression for `$this->mode`.
fn regex_mode_expr() -> Expr {
    property_access(this_expr(), "mode")
}

/// Builds the expression for `$this->flags`.
fn regex_flags_expr() -> Expr {
    property_access(this_expr(), "flags")
}

/// Builds the expression for `$this->pregFlags`.
fn regex_preg_flags_expr() -> Expr {
    property_access(this_expr(), "pregFlags")
}

/// Builds the expression for `$this->replacement`.
fn regex_replacement_expr() -> Expr {
    property_access(this_expr(), "replacement")
}

/// Builds the expression for `$this->recursiveInner`.
fn recursive_regex_inner_expr() -> Expr {
    property_access(this_expr(), "recursiveInner")
}

/// Builds an expression that tests whether a RegexIterator flag is enabled.
fn regex_flag_enabled_expr(bit: i64) -> Expr {
    binary_expr(
        binary_expr(regex_flags_expr(), BinOp::BitAnd, int_expr(bit)),
        BinOp::NotEq,
        int_expr(0),
    )
}

/// Builds an expression that tests whether a preg flag bit is enabled in a local variable.
fn preg_flag_enabled_expr(local: &str, bit: i64) -> Expr {
    binary_expr(
        binary_expr(var_expr(local), BinOp::BitAnd, int_expr(bit)),
        BinOp::NotEq,
        int_expr(0),
    )
}

/// Builds an expression that tests the current RegexIterator mode.
fn regex_mode_is_expr(mode: i64) -> Expr {
    binary_expr(regex_mode_expr(), BinOp::StrictEq, int_expr(mode))
}

/// Builds an expression that casts the current regex target to a string.
fn regex_subject_string_expr() -> Expr {
    cast_expr(
        CastType::String,
        method_call(this_expr(), "__elephcRegexTarget", Vec::new()),
    )
}

/// Builds an expression that casts the replacement property to a string.
fn regex_replacement_string_expr() -> Expr {
    cast_expr(CastType::String, regex_replacement_expr())
}

/// Builds a preg_match expression against the current target.
fn regex_match_expr() -> Expr {
    binary_expr(
        function_call("preg_match", vec![regex_pattern_expr(), regex_subject_string_expr()]),
        BinOp::NotEq,
        int_expr(0),
    )
}

/// Builds a preg_replace expression for a supplied subject.
fn regex_replace_expr(subject: Expr) -> Expr {
    function_call(
        "preg_replace",
        vec![regex_pattern_expr(), regex_replacement_string_expr(), subject],
    )
}

/// Builds a closure expression used to collect preg_replace_callback match arrays.
fn regex_capture_closure_expr(capture_refs: Vec<String>, body: Vec<Stmt>) -> Expr {
    expr(ExprKind::Closure {
        params: vec![param("matches", array_type())],
        variadic: None,
        return_type: Some(TypeExpr::Str),
        body,
        is_arrow: false,
        is_static: false,
        captures: capture_refs.clone(),
        capture_refs,
    })
}

/// Builds a preg_replace_callback expression used to populate capture scratch locals.
fn regex_capture_replace_callback_expr(callback: Expr, subject: Expr) -> Expr {
    function_call(
        "preg_replace_callback",
        vec![regex_pattern_expr(), callback, subject],
    )
}

/// Builds the synthetic constructor body.
fn regex_construct_body(class_name: &str, is_recursive: bool) -> Vec<Stmt> {
    let mut body = regex_validate_mode_body(class_name, "__construct", 3);
    body.extend(iterator_iterator_construct_body());
    if is_recursive {
        body.push(property_assign_stmt(
            this_expr(),
            "recursiveInner",
            var_expr("iterator"),
        ));
    }
    body.extend(vec![
        property_assign_stmt(this_expr(), "regex", var_expr("pattern")),
        property_assign_stmt(this_expr(), "mode", var_expr("mode")),
        property_assign_stmt(this_expr(), "flags", var_expr("flags")),
        property_assign_stmt(this_expr(), "pregFlags", var_expr("pregFlags")),
        property_assign_stmt(this_expr(), "replacement", null_expr()),
    ]);
    body
}

/// Builds mode validation statements for constructor and setMode.
fn regex_validate_mode_body(class_name: &str, method_name: &str, arg_number: i64) -> Vec<Stmt> {
    vec![if_stmt(
        binary_expr(
            binary_expr(var_expr("mode"), BinOp::Lt, int_expr(REGEX_MATCH)),
            BinOp::Or,
            binary_expr(var_expr("mode"), BinOp::Gt, int_expr(REGEX_REPLACE)),
        ),
        vec![throw_stmt(new_object_expr(
            "ValueError",
            vec![string_expr(&format!(
                "{}::{}(): Argument #{} ($mode) must be {}::MATCH, {}::GET_MATCH, {}::ALL_MATCHES, {}::SPLIT, or {}::REPLACE",
                class_name,
                method_name,
                arg_number,
                class_name,
                class_name,
                class_name,
                class_name,
                class_name,
            ))],
        ))],
        None,
    )]
}

/// Builds the synthetic setMode body.
fn regex_set_mode_body(class_name: &str) -> Vec<Stmt> {
    let mut body = regex_validate_mode_body(class_name, "setMode", 1);
    body.push(property_assign_stmt(this_expr(), "mode", var_expr("mode")));
    body
}

/// Builds the synthetic target-selection body.
fn regex_target_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            regex_flag_enabled_expr(REGEX_USE_KEY),
            return_body(inner_call("key")),
            None,
        ),
        return_stmt(inner_call("current")),
    ]
}

/// Builds the synthetic accept body.
fn regex_accept_body() -> Vec<Stmt> {
    vec![
        assign_stmt("matched", regex_match_expr()),
        if_stmt(
            regex_flag_enabled_expr(REGEX_INVERT_MATCH),
            return_body(not_expr(var_expr("matched"))),
            None,
        ),
        return_stmt(var_expr("matched")),
    ]
}

/// Builds the synthetic current body for mode-specific output.
fn regex_current_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            regex_mode_is_expr(REGEX_MATCH),
            return_body(inner_call("current")),
            None,
        ),
        if_stmt(
            regex_mode_is_expr(REGEX_GET_MATCH),
            return_body(method_call(
                this_expr(),
                "__elephcFirstMatch",
                vec![regex_subject_string_expr()],
            )),
            None,
        ),
        if_stmt(
            regex_mode_is_expr(REGEX_ALL_MATCHES),
            return_body(method_call(
                this_expr(),
                "__elephcAllMatches",
                vec![regex_subject_string_expr()],
            )),
            None,
        ),
        if_stmt(
            regex_mode_is_expr(REGEX_SPLIT),
            return_body(method_call(
                this_expr(),
                "__elephcSplit",
                vec![regex_subject_string_expr()],
            )),
            None,
        ),
        if_stmt(
            binary_expr(
                regex_mode_is_expr(REGEX_REPLACE),
                BinOp::And,
                not_expr(regex_flag_enabled_expr(REGEX_USE_KEY)),
            ),
            return_body(regex_replace_expr(regex_subject_string_expr())),
            None,
        ),
        return_stmt(inner_call("current")),
    ]
}

/// Builds the synthetic key body for REPLACE mode with USE_KEY.
fn regex_key_body() -> Vec<Stmt> {
    vec![
        if_stmt(
            binary_expr(
                regex_mode_is_expr(REGEX_REPLACE),
                BinOp::And,
                regex_flag_enabled_expr(REGEX_USE_KEY),
            ),
            return_body(regex_replace_expr(cast_expr(CastType::String, inner_call("key")))),
            None,
        ),
        return_stmt(inner_call("key")),
    ]
}

/// Builds the synthetic first-match array body.
fn regex_first_match_body() -> Vec<Stmt> {
    let mut body = vec![
        assign_stmt("firstMatch", empty_array_expr()),
        assign_stmt("matchSeen", bool_expr(false)),
        assign_stmt("searchOffset", int_expr(0)),
        assign_stmt("pregFlags", regex_preg_flags_expr()),
    ];
    body.push(expr_stmt(regex_capture_replace_callback_expr(
        regex_capture_closure_expr(
            vec![
                "firstMatch".to_string(),
                "matchSeen".to_string(),
                "searchOffset".to_string(),
                "pregFlags".to_string(),
                "subject".to_string(),
            ],
            regex_capture_first_match_body(),
        ),
        var_expr("subject"),
    )));
    body.push(return_stmt(var_expr("firstMatch")));
    body
}

/// Builds the synthetic all-matches array body.
fn regex_all_matches_body() -> Vec<Stmt> {
    let mut body = vec![assign_stmt("pregFlags", regex_preg_flags_expr())];
    body.push(if_stmt(
        preg_flag_enabled_expr("pregFlags", PREG_SET_ORDER),
        regex_all_matches_set_order_body(),
        Some(vec![if_stmt(
            preg_flag_enabled_expr("pregFlags", PREG_OFFSET_CAPTURE),
            regex_all_matches_pattern_order_body(true),
            Some(regex_all_matches_pattern_order_body(false)),
        )]),
    ));
    body
}

/// Builds the ALL_MATCHES branch for PREG_SET_ORDER.
fn regex_all_matches_set_order_body() -> Vec<Stmt> {
    let mut body = vec![
        assign_stmt("allMatches", empty_array_expr()),
        assign_stmt("searchOffset", int_expr(0)),
    ];
    body.push(expr_stmt(regex_capture_replace_callback_expr(
        regex_capture_closure_expr(
            vec![
                "allMatches".to_string(),
                "searchOffset".to_string(),
                "pregFlags".to_string(),
                "subject".to_string(),
            ],
            regex_capture_all_matches_body(),
        ),
        var_expr("subject"),
    )));
    body.push(return_stmt(var_expr("allMatches")));
    body
}

/// Builds the ALL_MATCHES branch for PREG_PATTERN_ORDER.
fn regex_all_matches_pattern_order_body(offset_capture: bool) -> Vec<Stmt> {
    let bucket_names = regex_all_match_bucket_names();
    let mut captures = vec![
        "allGroupCount".to_string(),
        "searchOffset".to_string(),
        "subject".to_string(),
    ];
    captures.extend(bucket_names.iter().cloned());

    let mut body = vec![
        assign_stmt("allGroupCount", int_expr(0)),
        assign_stmt("searchOffset", int_expr(0)),
    ];
    for bucket_name in &bucket_names {
        let initial_bucket = if offset_capture {
            empty_array_expr()
        } else {
            seeded_string_array_expr()
        };
        body.push(assign_stmt(bucket_name, initial_bucket));
    }
    body.push(expr_stmt(regex_capture_replace_callback_expr(
        regex_capture_closure_expr(
            captures,
            regex_capture_pattern_order_matches_body(offset_capture),
        ),
        var_expr("subject"),
    )));
    body.extend(regex_build_pattern_order_matches_array_body(offset_capture));
    body
}

/// Builds the closure body that stores the first preg_replace_callback match array.
fn regex_capture_first_match_body() -> Vec<Stmt> {
    let mut first_body = vec![
        assign_stmt(
            "fullMatch",
            cast_expr(CastType::String, array_access(var_expr("matches"), int_expr(0))),
        ),
        assign_stmt("fullOffset", regex_match_offset_expr("searchOffset")),
    ];
    first_body.push(if_stmt(
        preg_flag_enabled_expr("pregFlags", PREG_OFFSET_CAPTURE),
        regex_build_offset_row_body("firstMatch"),
        Some(vec![assign_stmt("firstMatch", var_expr("matches"))]),
    ));
    first_body.push(assign_stmt("matchSeen", bool_expr(true)));

    let mut body = vec![if_stmt(not_expr(var_expr("matchSeen")), first_body, None)];
    body.extend(regex_advance_search_offset_body("searchOffset"));
    body.push(return_stmt(string_expr("")));
    body
}

/// Builds the closure body that transposes match arrays into preg_match_all shape.
fn regex_capture_all_matches_body() -> Vec<Stmt> {
    let mut body = vec![
        assign_stmt(
            "fullMatch",
            cast_expr(CastType::String, array_access(var_expr("matches"), int_expr(0))),
        ),
        assign_stmt("fullOffset", regex_match_offset_expr("searchOffset")),
    ];
    body.push(if_stmt(
        preg_flag_enabled_expr("pregFlags", PREG_OFFSET_CAPTURE),
        regex_build_offset_row_body("matchRow"),
        Some(vec![assign_stmt("matchRow", var_expr("matches"))]),
    ));
    body.push(if_stmt(
        preg_flag_enabled_expr("pregFlags", PREG_SET_ORDER),
        vec![array_push_stmt("allMatches", var_expr("matchRow"))],
        Some(Vec::new()),
    ));
    body.extend(regex_advance_search_offset_body("searchOffset"));
    body.push(return_stmt(string_expr("")));
    body
}

/// Builds the callback body that stores one match into static pattern-order buckets.
fn regex_capture_pattern_order_matches_body(offset_capture: bool) -> Vec<Stmt> {
    let mut body = vec![
        assign_stmt("allGroupCount", count_expr(var_expr("matches"))),
        assign_stmt(
            "fullMatch",
            cast_expr(CastType::String, array_access(var_expr("matches"), int_expr(0))),
        ),
        assign_stmt("fullOffset", regex_match_offset_expr("searchOffset")),
    ];
    for group in 0..REGEX_CAPTURE_LIMIT {
        body.push(if_stmt(
            binary_expr(count_expr(var_expr("matches")), BinOp::Gt, int_expr(group as i64)),
            regex_push_pattern_order_group_body(group, offset_capture),
            None,
        ));
    }
    body.extend(regex_advance_search_offset_body("searchOffset"));
    body.push(return_stmt(string_expr("")));
    body
}

/// Builds statements that push one capture group into its pattern-order bucket.
fn regex_push_pattern_order_group_body(group: usize, offset_capture: bool) -> Vec<Stmt> {
    let bucket_name = regex_all_match_bucket_name(group);
    if !offset_capture {
        return vec![array_push_stmt(
            &bucket_name,
            array_access(var_expr("matches"), int_expr(group as i64)),
        )];
    }

    vec![
        assign_stmt(
            "capture",
            cast_expr(
                CastType::String,
                array_access(var_expr("matches"), int_expr(group as i64)),
            ),
        ),
        assign_stmt(
            "captureOffset",
            binary_expr(
                var_expr("fullOffset"),
                BinOp::Add,
                function_call(
                    "intval",
                    vec![function_call(
                        "strpos",
                        vec![var_expr("fullMatch"), var_expr("capture")],
                    )],
                ),
            ),
        ),
        array_push_stmt(
            &bucket_name,
            expr(ExprKind::ArrayLiteral(vec![
                var_expr("capture"),
                var_expr("captureOffset"),
            ])),
        ),
    ]
}

/// Builds one non-empty string array used to keep empty capture buckets typed as strings.
fn seeded_string_array_expr() -> Expr {
    expr(ExprKind::ArrayLiteral(vec![string_expr("")]))
}

/// Builds the names of per-group match bucket arrays.
fn regex_all_match_bucket_names() -> Vec<String> {
    (0..REGEX_CAPTURE_LIMIT)
        .map(regex_all_match_bucket_name)
        .collect()
}

/// Builds one per-group match bucket variable name.
fn regex_all_match_bucket_name(group: usize) -> String {
    format!("match{group}")
}

/// Builds statements that reconstruct a pattern-order ALL_MATCHES result array.
fn regex_build_pattern_order_matches_array_body(offset_capture: bool) -> Vec<Stmt> {
    let mut body = vec![assign_stmt("out", empty_array_expr())];
    let first_occurrence = if offset_capture { 0 } else { 1 };
    for group in 0..REGEX_CAPTURE_LIMIT {
        let bucket_name = regex_all_match_bucket_name(group);
        body.push(if_stmt(
            binary_expr(var_expr("allGroupCount"), BinOp::Gt, int_expr(group as i64)),
            vec![
                assign_stmt("bucket", empty_array_expr()),
                assign_stmt("occurrence", int_expr(first_occurrence)),
                while_stmt(
                    binary_expr(var_expr("occurrence"), BinOp::Lt, count_expr(var_expr(&bucket_name))),
                    vec![
                        array_push_stmt(
                            "bucket",
                            array_access(var_expr(&bucket_name), var_expr("occurrence")),
                        ),
                        increment_stmt("occurrence"),
                    ],
                ),
                array_push_stmt("out", var_expr("bucket")),
            ],
            None,
        ));
    }
    body.push(return_stmt(var_expr("out")));
    body
}

/// Builds an expression for the current full match offset relative to the subject.
fn regex_match_offset_expr(cursor_name: &str) -> Expr {
    binary_expr(
        var_expr(cursor_name),
        BinOp::Add,
        function_call(
            "intval",
            vec![function_call(
                "strpos",
                vec![
                    function_call("substr", vec![var_expr("subject"), var_expr(cursor_name)]),
                    var_expr("fullMatch"),
                ],
            )],
        ),
    )
}

/// Builds statements that advance the regex search cursor after a callback match.
fn regex_advance_search_offset_body(cursor_name: &str) -> Vec<Stmt> {
    vec![if_stmt(
        binary_expr(
            function_call("strlen", vec![var_expr("fullMatch")]),
            BinOp::Gt,
            int_expr(0),
        ),
        vec![assign_stmt(
            cursor_name,
            binary_expr(
                var_expr("fullOffset"),
                BinOp::Add,
                function_call("strlen", vec![var_expr("fullMatch")]),
            ),
        )],
        Some(vec![assign_stmt(
            cursor_name,
            binary_expr(var_expr("fullOffset"), BinOp::Add, int_expr(1)),
        )]),
    )]
}

/// Builds statements that transform `$matches` into `[[string, offset], ...]`.
fn regex_build_offset_row_body(target: &str) -> Vec<Stmt> {
    vec![
        assign_stmt("offsetRow", empty_array_expr()),
        assign_stmt("group", int_expr(0)),
        while_stmt(
            binary_expr(var_expr("group"), BinOp::Lt, count_expr(var_expr("matches"))),
            vec![
                assign_stmt(
                    "capture",
                    cast_expr(CastType::String, array_access(var_expr("matches"), var_expr("group"))),
                ),
                assign_stmt(
                    "captureOffset",
                    binary_expr(
                        var_expr("fullOffset"),
                        BinOp::Add,
                        function_call(
                            "intval",
                            vec![function_call(
                                "strpos",
                                vec![var_expr("fullMatch"), var_expr("capture")],
                            )],
                        ),
                    ),
                ),
                array_push_stmt(
                    "offsetRow",
                    expr(ExprKind::ArrayLiteral(vec![
                        var_expr("capture"),
                        var_expr("captureOffset"),
                    ])),
                ),
                increment_stmt("group"),
            ],
        ),
        assign_stmt(target, var_expr("offsetRow")),
    ]
}

/// Builds the SPLIT mode body, applying preg split flags from `$this->pregFlags`.
fn regex_split_body() -> Vec<Stmt> {
    vec![
        assign_stmt("pregFlags", regex_preg_flags_expr()),
        return_stmt(function_call(
            "preg_split",
            vec![
                regex_pattern_expr(),
                var_expr("subject"),
                int_expr(-1),
                var_expr("pregFlags"),
            ],
        )),
    ]
}

/// Builds the synthetic RecursiveRegexIterator getChildren body.
fn recursive_regex_get_children_body() -> Vec<Stmt> {
    vec![
        assign_stmt(
            "child",
            method_call(recursive_regex_inner_expr(), "getChildren", Vec::new()),
        ),
        if_stmt(
            function_call("is_null", vec![var_expr("child")]),
            return_body(null_expr()),
            None,
        ),
        assign_stmt(
            "childIterator",
            assume_recursive_iterator_expr(var_expr("child")),
        ),
        assign_stmt("childPattern", regex_pattern_expr()),
        assign_stmt("childMode", regex_mode_expr()),
        assign_stmt("childFlags", regex_flags_expr()),
        assign_stmt("childPregFlags", regex_preg_flags_expr()),
        assign_stmt("childReplacement", regex_replacement_expr()),
        assign_stmt(
            "next",
            new_object_expr(
                "RecursiveRegexIterator",
                vec![
                    var_expr("childIterator"),
                    var_expr("childPattern"),
                    var_expr("childMode"),
                    var_expr("childFlags"),
                    var_expr("childPregFlags"),
                ],
            ),
        ),
        property_assign_stmt(var_expr("next"), "replacement", var_expr("childReplacement")),
        return_stmt(var_expr("next")),
    ]
}
