//! Purpose:
//! Builds and injects the managed ICU-backed `ext/intl` surface as synthetic PHP AST.
//!
//! Called from:
//! - `crate::pipeline` after include resolution and before name resolution.
//!
//! Key details:
//! - Every declaration is a real `Stmt::FunctionDecl` / `Stmt::ClassDecl` / extern node. No PHP
//!   source is tokenized or parsed here; the AST is what the parser would have produced.
//! - The scalar-only procedural entry points are shared registry builtins; their EIR lowering
//!   calls the `__elephc_intl_invoke_*` helpers injected here.
//! - Object-bearing and by-reference entry points are synthetic functions whose bodies cross the
//!   same fixed C ABI and decode a compact typed value stream.
//! - The bridge archive is linked through `BuiltinRequirement::Bridge("elephc_intl")` for the
//!   registry builtins and through the extern declaration's library for the synthetic functions.
//! - Injection is pay-for-use: an Intl-free program gets no declarations, no helpers, and no ICU.

mod class_constants;
mod functions;

use crate::optimize::reachability::PreludeInventory;
use crate::parser::ast::{
    BinOp, CastType, Expr, Stmt, StmtKind, TypeExpr,
};
use crate::prelude_prune::usage;
use crate::synthetic_class::{
    class, e_array, e_array_assoc, e_binop, e_call, e_cast, e_class_const, e_concat, e_concat_all, e_const,
    e_float, e_index, e_instance_of, e_int, e_not, e_method_call, e_new, e_new_static, e_null, e_prop, e_self_call, e_static_call, e_str, e_ternary, e_this_prop, e_var, function,
    method, s_array_assign, s_array_push, s_assign, s_const, s_expr, s_for, s_if, s_prop_assign,
    s_return, s_static, s_throw, s_try, s_while, t_array, t_class, t_mixed,
    t_nullable, t_union, FunctionBuilder,
};
use crate::synthetic_class;

use self::class_constants::{constants_for, IntlConstValue};
use self::functions::{IntlFunction, IntlParam, FUNCTIONS};

/// Returns whether the program names any PHP-visible `ext/intl` symbol.
pub fn program_uses_intl(program: &[Stmt]) -> bool {
    let usage = usage::collect(program);
    let function_names = elephc_builtin_contract::contracts()
        .iter()
        .filter(|contract| contract.module == elephc_builtin_contract::PhpModule::Intl)
        .map(|contract| contract.name);
    if function_names.clone().any(|name| usage.references(name)) {
        return true;
    }
    let literal_or_class = |name: &str| {
        let key = crate::names::php_symbol_key(name);
        usage.classes.contains(&key) || usage.literals.contains(&key)
    };
    if CLASS_NAMES.iter().any(|name| literal_or_class(name)) {
        return true;
    }
    let constant_names = intl_constant_names();
    usage.constants.iter().any(|name| constant_names.iter().any(|known| known == name))
        || usage.literals.iter().any(|name| constant_names.iter().any(|known| known == name))
}

/// Returns whether `name` is one of the synthetic Intl classes.
#[cfg(test)]
pub(crate) fn is_intl_class(name: &str) -> bool {
    CLASS_NAMES.contains(&name)
}

/// Returns whether the program names an Intl function or class (not only a constant).
fn program_needs_bridge(program: &[Stmt]) -> bool {
    let usage = usage::collect(program);
    if elephc_builtin_contract::contracts()
        .iter()
        .filter(|contract| contract.module == elephc_builtin_contract::PhpModule::Intl)
        .any(|contract| usage.references(contract.name))
    {
        return true;
    }
    CLASS_NAMES.iter().any(|name| {
        let key = crate::names::php_symbol_key(name);
        usage.classes.contains(&key) || usage.literals.contains(&key)
    })
}

/// Builds every Intl declaration and prepends it when the program references the surface.
pub fn inject_if_used(
    program: Vec<Stmt>,
    force: bool,
    inventory: &mut PreludeInventory,
) -> Vec<Stmt> {
    let needs_bridge = force || program_needs_bridge(&program);
    let uses_any = force || program_uses_intl(&program);
    if !uses_any {
        return program;
    }
    let mut declarations = synthetic_class::internal_declarations(|| build_surface(needs_bridge));
    inventory.record_program("intl", &declarations);
    declarations.append(&mut program.clone());
    declarations
}

/// Returns the injected Intl surface for prelude-parity auditing.
///
/// Mirrors `inject_if_used`'s build so the builtin parity tests can compare every
/// `PreludeProvided` Intl contract against the synthetic declaration it ships.
#[cfg(test)]
pub(crate) fn declaration_program() -> Vec<Stmt> {
    synthetic_class::internal_declarations(|| build_surface(true))
}

/// Builds the synthetic Intl surface: constants, bridge helpers, functions, and classes.
fn build_surface(include_bridge: bool) -> Vec<Stmt> {
    let mut declarations = intl_constants();
    if include_bridge {
        declarations.extend(extern_declarations());
        declarations.extend(helper_functions());
        declarations.extend(function_declarations());
        declarations.extend(class_declarations());
    }
    declarations
}

/// Returns the integer value of one global constant the Intl prelude declares.
///
/// Used by the builtin parity tests to resolve named defaults (`ULOC_ACTUAL_LOCALE`, ...)
/// against the folded literal the neutral contract records.
#[cfg(test)]
pub(crate) fn constant_int_value(name: &str) -> Option<i64> {
    intl_constants().into_iter().find_map(|stmt| match stmt.kind {
        crate::parser::ast::StmtKind::ConstDecl { name: declared, value } if declared == name => {
            match value.kind {
                crate::parser::ast::ExprKind::IntLiteral(value) => Some(value),
                _ => None,
            }
        }
        _ => None,
    })
}

/// Returns true for one of the global constants the Intl prelude declares.
///
/// `defined()` is folded at lowering time from the checker's constant map, which is built
/// before the Intl prelude is injected. Consult this predicate as a fallback so the
/// `INTL_*`/`IDNA_*`/`GRAPHEME_*`/`ULOC_*`/`LC_*` constants report as defined.
pub fn is_intl_constant(name: &str) -> bool {
    ["INTL_", "IDNA_", "GRAPHEME_", "ULOC_", "LC_"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// Builds every global constant PHP's `ext/intl` publishes.
fn intl_constants() -> Vec<Stmt> {
    let mut constants = vec![
        s_const("LC_ALL", e_int(0)),
        s_const("LC_COLLATE", e_int(1)),
        s_const("LC_CTYPE", e_int(2)),
        s_const("LC_MONETARY", e_int(3)),
        s_const("LC_NUMERIC", e_int(4)),
        s_const("LC_TIME", e_int(5)),
        s_const("LC_MESSAGES", e_int(6)),
        s_const("INTL_MAX_LOCALE_LEN", e_int(156)),
        s_const("INTL_ICU_VERSION", e_str("78.3")),
        s_const("INTL_ICU_DATA_VERSION", e_str("78.3")),
        s_const("GRAPHEME_EXTR_COUNT", e_int(0)),
        s_const("GRAPHEME_EXTR_MAXBYTES", e_int(1)),
        s_const("GRAPHEME_EXTR_MAXCHARS", e_int(2)),
        s_const("IDNA_DEFAULT", e_int(48)),
        s_const("IDNA_ALLOW_UNASSIGNED", e_int(1)),
        s_const("IDNA_USE_STD3_RULES", e_int(2)),
        s_const("IDNA_CHECK_BIDI", e_int(4)),
        s_const("IDNA_CHECK_CONTEXTJ", e_int(8)),
        s_const("IDNA_NONTRANSITIONAL_TO_ASCII", e_int(16)),
        s_const("IDNA_NONTRANSITIONAL_TO_UNICODE", e_int(32)),
        s_const("INTL_IDNA_VARIANT_UTS46", e_int(1)),
        s_const("IDNA_ERROR_EMPTY_LABEL", e_int(1)),
        s_const("IDNA_ERROR_LABEL_TOO_LONG", e_int(2)),
        s_const("IDNA_ERROR_DOMAIN_NAME_TOO_LONG", e_int(4)),
        s_const("IDNA_ERROR_LEADING_HYPHEN", e_int(8)),
        s_const("IDNA_ERROR_TRAILING_HYPHEN", e_int(16)),
        s_const("IDNA_ERROR_HYPHEN_3_4", e_int(32)),
        s_const("IDNA_ERROR_LEADING_COMBINING_MARK", e_int(64)),
        s_const("IDNA_ERROR_DISALLOWED", e_int(128)),
        s_const("IDNA_ERROR_PUNYCODE", e_int(256)),
        s_const("IDNA_ERROR_LABEL_HAS_DOT", e_int(512)),
        s_const("IDNA_ERROR_INVALID_ACE_LABEL", e_int(1024)),
        s_const("IDNA_ERROR_BIDI", e_int(2048)),
        s_const("IDNA_ERROR_CONTEXTJ", e_int(4096)),
        s_const("ULOC_ACTUAL_LOCALE", e_int(0)),
        s_const("ULOC_VALID_LOCALE", e_int(1)),
    ];
    // ICU status names are PHP-visible in ext/intl. The bridge owns the actual values and the
    // Rust unit test pins the symbolic lookup; these declarations keep source constants usable
    // without linking ICU for a constants-only program.
    for (name, value) in INTL_STATUS_CONSTANTS {
        constants.push(s_const(name, e_int(*value)));
    }
    constants
}

/// Returns the canonical names of every PHP-visible Intl global constant.
fn intl_constant_names() -> Vec<&'static str> {
    let mut names = vec![
        "INTL_MAX_LOCALE_LEN", "INTL_ICU_VERSION", "INTL_ICU_DATA_VERSION",
        "GRAPHEME_EXTR_COUNT", "GRAPHEME_EXTR_MAXBYTES", "GRAPHEME_EXTR_MAXCHARS",
        "IDNA_DEFAULT", "IDNA_ALLOW_UNASSIGNED", "IDNA_USE_STD3_RULES", "IDNA_CHECK_BIDI",
        "IDNA_CHECK_CONTEXTJ", "IDNA_NONTRANSITIONAL_TO_ASCII",
        "IDNA_NONTRANSITIONAL_TO_UNICODE", "INTL_IDNA_VARIANT_UTS46", "IDNA_ERROR_EMPTY_LABEL",
        "IDNA_ERROR_LABEL_TOO_LONG", "IDNA_ERROR_DOMAIN_NAME_TOO_LONG",
        "IDNA_ERROR_LEADING_HYPHEN", "IDNA_ERROR_TRAILING_HYPHEN", "IDNA_ERROR_HYPHEN_3_4",
        "IDNA_ERROR_LEADING_COMBINING_MARK", "IDNA_ERROR_DISALLOWED", "IDNA_ERROR_PUNYCODE",
        "IDNA_ERROR_LABEL_HAS_DOT", "IDNA_ERROR_INVALID_ACE_LABEL", "IDNA_ERROR_BIDI",
        "IDNA_ERROR_CONTEXTJ", "ULOC_ACTUAL_LOCALE", "ULOC_VALID_LOCALE",
    ];
    names.extend(INTL_STATUS_CONSTANTS.iter().map(|(name, _)| *name));
    names
}

/// Returns the synthetic helpers a scalar-only Intl builtin lowering calls directly.
///
/// Declaration reachability cannot see an EIR-only call edge, so the usage scan roots these
/// helpers from the source call site. The list includes every typed entry point because the
/// lowering picks one from the declared result type, which the declaration scan does not carry.
pub fn prelude_helpers_for(builtin: &str) -> &'static [&'static str] {
    let is_intl = elephc_builtin_contract::contracts().iter().any(|contract| {
        contract.module == elephc_builtin_contract::PhpModule::Intl
            && contract.name == builtin
    });
    if is_intl { INTL_HELPERS } else { &[] }
}

/// Synthetic helpers called from scalar Intl builtin lowering.
const INTL_HELPERS: &[&str] = &[
    "__elephc_intl_invoke",
    "__elephc_intl_invoke_string",
    "__elephc_intl_invoke_int",
    "__elephc_intl_invoke_float",
    "__elephc_intl_invoke_bool",
    "__elephc_intl_invoke_array",
    "__elephc_intl_invoke_void",
    "__elephc_intl_invoke_mixed",
    "__elephc_intl_invoke_locale",
];

/// Returns every synthetic Intl function that must survive declaration pruning.
///
/// The EIR lowering calls typed helpers directly, an edge the source reachability scan cannot
/// see. Rooting the closed helper set keeps those declarations available to codegen.
pub fn root_function_names() -> Vec<String> {
    let mut names: Vec<String> = INTL_HELPERS.iter().map(|name| (*name).to_string()).collect();
    names.extend([
        "__elephc_intl_value",
        "__elephc_intl_pack_values",
        "__elephc_intl_u32",
        "__elephc_intl_decode_value",
        "__elephc_intl_wrap",
        "__elephc_intl_invoke_outputs",
        "__elephc_intl_invoke_locale",
        "__elephc_intl_call",
        "__elephc_intl_open",
        "__elephc_intl_handle_arg",
    ].map(str::to_string));
    names.extend(FUNCTIONS.iter().map(|function| function.name.to_string()));
    names
}

/// Builds the extern declarations for the managed C ABI.
fn extern_declarations() -> Vec<Stmt> {
    let mut declarations = Vec::new();
    for (name, params, returns) in [
        ("elephc_intl_open", vec![("kind", "string"), ("spec", "string")], "int"),
        ("elephc_intl_close", vec![("handle", "int")], "void"),
        ("elephc_intl_clone", vec![("handle", "int")], "int"),
        ("elephc_intl_call", vec![("handle", "int"), ("method", "string"), ("args", "string")], "string"),
        ("elephc_intl_static", vec![("kind", "string"), ("method", "string"), ("args", "string")], "string"),
        ("elephc_intl_error_code", vec![("handle", "int")], "int"),
        ("elephc_intl_error_message", vec![("handle", "int")], "string"),
        ("elephc_intl_global_error_code", vec![], "int"),
        ("elephc_intl_global_error_message", vec![], "string"),
        ("elephc_intl_live_count", vec![], "int"),
        ("elephc_intl_version", vec![], "string"),
        ("elephc_intl_data_version", vec![], "string"),
    ] {
        declarations.push(Stmt::new(
            StmtKind::ExternFunctionDecl {
                name: name.to_string(),
                params: params.into_iter().map(|(name, ty)| crate::parser::ast::ExternParam {
                    name: name.to_string(),
                    c_type: match ty {
                        "string" => crate::parser::ast::CType::Str,
                        "int" => crate::parser::ast::CType::Int,
                        "void" => crate::parser::ast::CType::Void,
                        _ => crate::parser::ast::CType::Str,
                    },
                }).collect(),
                return_type: match returns {
                    "string" => crate::parser::ast::CType::Str,
                    "int" => crate::parser::ast::CType::Int,
                    "void" => crate::parser::ast::CType::Void,
                    _ => crate::parser::ast::CType::Str,
                },
                library: Some("elephc_intl".to_string()),
            },
            crate::span::Span::dummy(),
        ));
    }
    declarations
}

/// Builds the synthetic helper functions used by both builtin lowering and class bodies.
fn helper_functions() -> Vec<Stmt> {
    let mut helpers = Vec::new();
    helpers.push(
        function("__elephc_intl_utf8_codepoint")
            .param("text", TypeExpr::Str)
            .param("offset", TypeExpr::Int)
            .returns(TypeExpr::Int)
            .body(vec![
                s_assign("_length", e_call("strlen", vec![e_var("text")])),
                s_if(e_binop(e_var("offset"), BinOp::GtEq, e_var("_length")), vec![s_return(e_int(-1))], vec![], None),
                s_assign("_b0", e_call("ord", vec![e_index(e_var("text"), e_var("offset"))])),
                s_if(e_binop(e_var("_b0"), BinOp::Lt, e_int(0x80)), vec![s_return(e_var("_b0"))], vec![], None),
                s_if(e_binop(e_binop(e_var("_b0"), BinOp::BitAnd, e_int(0xE0)), BinOp::StrictEq, e_int(0xC0)), vec![
                    s_return(e_binop(
                        e_binop(e_binop(e_var("_b0"), BinOp::BitAnd, e_int(0x1F)), BinOp::ShiftLeft, e_int(6)),
                        BinOp::BitOr,
                        e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("offset"), BinOp::Add, e_int(1)))]), BinOp::BitAnd, e_int(0x3F)),
                    )),
                ], vec![], None),
                s_if(e_binop(e_binop(e_var("_b0"), BinOp::BitAnd, e_int(0xF0)), BinOp::StrictEq, e_int(0xE0)), vec![
                    s_return(e_binop(
                        e_binop(
                            e_binop(e_binop(e_var("_b0"), BinOp::BitAnd, e_int(0x0F)), BinOp::ShiftLeft, e_int(12)),
                            BinOp::BitOr,
                            e_binop(e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("offset"), BinOp::Add, e_int(1)))]), BinOp::BitAnd, e_int(0x3F)), BinOp::ShiftLeft, e_int(6)),
                        ),
                        BinOp::BitOr,
                        e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("offset"), BinOp::Add, e_int(2)))]), BinOp::BitAnd, e_int(0x3F)),
                    )),
                ], vec![], None),
                s_return(e_binop(
                    e_binop(
                        e_binop(e_binop(e_var("_b0"), BinOp::BitAnd, e_int(0x07)), BinOp::ShiftLeft, e_int(18)),
                        BinOp::BitOr,
                        e_binop(e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("offset"), BinOp::Add, e_int(1)))]), BinOp::BitAnd, e_int(0x3F)), BinOp::ShiftLeft, e_int(12)),
                    ),
                    BinOp::BitOr,
                    e_binop(
                        e_binop(e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("offset"), BinOp::Add, e_int(2)))]), BinOp::BitAnd, e_int(0x3F)), BinOp::ShiftLeft, e_int(6)),
                        BinOp::BitOr,
                        e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("offset"), BinOp::Add, e_int(3)))]), BinOp::BitAnd, e_int(0x3F)),
                    ),
                )),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_u32be")
            .param("value", TypeExpr::Int)
            .returns(TypeExpr::Str)
            .body(vec![
                s_return(e_concat_all(vec![
                    e_call("chr", vec![e_binop(e_binop(e_var("value"), BinOp::ShiftRight, e_int(24)), BinOp::BitAnd, e_int(0xFF))]),
                    e_call("chr", vec![e_binop(e_binop(e_var("value"), BinOp::ShiftRight, e_int(16)), BinOp::BitAnd, e_int(0xFF))]),
                    e_call("chr", vec![e_binop(e_binop(e_var("value"), BinOp::ShiftRight, e_int(8)), BinOp::BitAnd, e_int(0xFF))]),
                    e_call("chr", vec![e_binop(e_var("value"), BinOp::BitAnd, e_int(0xFF))]),
                ])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_value")
            .param("value", t_mixed())
            .returns(TypeExpr::Str)
            .body(vec![
                s_if(e_call("is_null", vec![e_var("value")]), vec![s_return(e_str("n"))], vec![], None),
                s_if(e_binop(e_var("value"), BinOp::StrictEq, crate::synthetic_class::e_bool(true)), vec![s_return(e_str("t"))], vec![], None),
                s_if(e_binop(e_var("value"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![s_return(e_str("f"))], vec![], None),
                s_if(e_call("is_int", vec![e_var("value")]), vec![
                    s_assign("_text", e_cast(CastType::String, e_var("value"))),
                    s_return(e_concat_all(vec![e_str("i"), e_call("__elephc_intl_u32be", vec![e_call("strlen", vec![e_var("_text")])]), e_var("_text")])),
                ], vec![], None),
                s_if(e_call("is_float", vec![e_var("value")]), vec![
                    // 17 significant digits round-trips every double; PHP's display precision
                    // (14) rounds 2147483643.12345 up and changes ICU's formatted result.
                    s_assign("_text", e_call("sprintf", vec![e_str("%.17G"), e_var("value")])),
                    s_return(e_concat_all(vec![e_str("d"), e_call("__elephc_intl_u32be", vec![e_call("strlen", vec![e_var("_text")])]), e_var("_text")])),
                ], vec![], None),
                // Resources are a distinct bridge tag: php's message formatter refuses them
                // with "No strategy to convert..." instead of stringifying them.
                s_if(e_call("is_resource", vec![e_var("value")]), vec![s_return(e_str("r"))], vec![], None),
                s_if(e_call("is_object", vec![e_var("value")]), vec![
                    // Intl wrappers carry the ICU handle as a public property. Pass that handle
                    // through as an integer so procedural entry points can unwrap the object
                    // exactly like the OO method path does.
                    s_if(intl_wrapper_check(), vec![
                        s_assign("_text", e_cast(CastType::String, e_prop(e_var("value"), "__elephc_handle"))),
                        s_return(e_concat_all(vec![e_str("i"), e_call("__elephc_intl_u32be", vec![e_call("strlen", vec![e_var("_text")])]), e_var("_text")])),
                    ], vec![], None),
                    // DateTime values become their timestamp BEFORE the generic __toString
                    // probe: a DateTime has no __toString, so the probe would otherwise
                    // swallow every date argument as null.
                    s_if(
                        e_binop(
                            e_instance_of(e_var("value"), "DateTime"),
                            BinOp::Or,
                            e_instance_of(e_var("value"), "DateTimeImmutable"),
                        ),
                        vec![
                            s_assign("_text", e_cast(CastType::String, e_method_call(e_var("value"), "getTimestamp", vec![]))),
                            s_return(e_concat_all(vec![e_str("d"), e_call("__elephc_intl_u32be", vec![e_call("strlen", vec![e_var("_text")])]), e_var("_text")])),
                        ],
                        vec![],
                        None,
                    ),
                    // Non-Intl objects reach the bridge as their PHP string form when they
                    // provide `__toString` (IntlListFormatter accepts those). A cast failure
                    // keeps the null encoding used for unsupported objects.
                    s_try(
                        vec![
                            s_assign("_text", e_cast(CastType::String, e_var("value"))),
                            s_return(e_concat_all(vec![e_str("s"), e_call("__elephc_intl_u32be", vec![e_call("strlen", vec![e_var("_text")])]), e_var("_text")])),
                        ],
                        vec![(vec!["Throwable"], Some("_error"), vec![s_return(e_str("n"))])],
                        None,
                    ),
                    s_return(e_str("n")),
                ], vec![], None),
                s_if(e_call("is_array", vec![e_var("value")]), vec![
                    s_assign("_out", e_concat(e_str("a"), e_call("__elephc_intl_u32be", vec![e_call("count", vec![e_var("value")])]))),
                    crate::synthetic_class::s_foreach(e_var("value"), Some("_key"), "_item", vec![
                        s_assign("_out", e_concat_all(vec![e_var("_out"), e_call("__elephc_intl_value", vec![e_var("_key")]), e_call("__elephc_intl_value", vec![e_var("_item")])])),
                    ]),
                    s_return(e_var("_out")),
                ], vec![], None),
                s_assign("_text", e_cast(CastType::String, e_var("value"))),
                s_return(e_concat_all(vec![e_str("s"), e_call("__elephc_intl_u32be", vec![e_call("strlen", vec![e_var("_text")])]), e_var("_text")])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_pack_values")
            .param("values", t_mixed())
            .returns(TypeExpr::Str)
            .body(vec![
                s_assign("_out", e_call("__elephc_intl_u32be", vec![e_call("count", vec![e_var("values")])])),
                crate::synthetic_class::s_foreach(e_var("values"), None, "_value", vec![
                    s_assign("_out", e_concat(e_var("_out"), e_call("__elephc_intl_value", vec![e_var("_value")]))),
                ]),
                s_return(e_var("_out")),
            ])
            .build(),
    );
    helpers.push(
        // The bridge argument arrays must carry boxed values: a nullable scalar parameter is
        // an inline tagged scalar, and storing one directly into an array literal keeps its
        // raw two-word representation instead of a Mixed cell the packer can classify.
        function("__elephc_intl_args")
            .variadic("values", Some(t_mixed()))
            .returns(t_array())
            .body(vec![
                s_assign("_out", e_array(vec![])),
                crate::synthetic_class::s_foreach(e_var("values"), None, "_value", vec![
                    s_array_push("_out", e_var("_value")),
                ]),
                s_return(e_var("_out")),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_last_error_message")
            .returns(TypeExpr::Str)
            .body(vec![s_return(e_call("__elephc_intl_invoke_string", vec![
                e_str("intl"), e_str("intl_get_error_message"), e_array(vec![]),
            ]))])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_u32")
            .param("bytes", TypeExpr::Str)
            .param_by_ref("offset", None)
            .returns(TypeExpr::Int)
            .body(vec![
                s_assign("_a", e_call("ord", vec![e_call("substr", vec![e_var("bytes"), e_var("offset"), e_int(1)])])),
                s_assign("_b", e_call("ord", vec![e_call("substr", vec![e_var("bytes"), e_binop(e_var("offset"), BinOp::Add, e_int(1)), e_int(1)])])),
                s_assign("_c", e_call("ord", vec![e_call("substr", vec![e_var("bytes"), e_binop(e_var("offset"), BinOp::Add, e_int(2)), e_int(1)])])),
                s_assign("_d", e_call("ord", vec![e_call("substr", vec![e_var("bytes"), e_binop(e_var("offset"), BinOp::Add, e_int(3)), e_int(1)])])),
                s_assign("offset", e_binop(e_var("offset"), BinOp::Add, e_int(4))),
                s_return(e_binop(e_binop(e_binop(e_binop(e_var("_a"), BinOp::ShiftLeft, e_int(24)), BinOp::BitOr, e_binop(e_var("_b"), BinOp::ShiftLeft, e_int(16))), BinOp::BitOr, e_binop(e_var("_c"), BinOp::ShiftLeft, e_int(8))), BinOp::BitOr, e_var("_d"))),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_decode_value")
            .param("bytes", TypeExpr::Str)
            .param_by_ref("offset", None)
            .returns(t_mixed())
            .body(vec![
                s_assign("_tag", e_call("substr", vec![e_var("bytes"), e_var("offset"), e_int(1)])),
                s_assign("offset", e_binop(e_var("offset"), BinOp::Add, e_int(1))),
                s_if(e_binop(e_var("_tag"), BinOp::StrictEq, e_str("n")), vec![s_return(e_null())], vec![], None),
                s_if(e_binop(e_var("_tag"), BinOp::StrictEq, e_str("t")), vec![s_return(crate::synthetic_class::e_bool(true))], vec![], None),
                s_if(e_binop(e_var("_tag"), BinOp::StrictEq, e_str("f")), vec![s_return(crate::synthetic_class::e_bool(false))], vec![], None),
                s_if(e_binop(e_var("_tag"), BinOp::StrictEq, e_str("i")), vec![
                    s_assign("_length", e_call("__elephc_intl_u32", vec![e_var("bytes"), e_var("offset")])),
                    s_assign("_value", e_cast(CastType::Int, e_call("substr", vec![e_var("bytes"), e_var("offset"), e_var("_length")]))),
                    s_assign("offset", e_binop(e_var("offset"), BinOp::Add, e_var("_length"))),
                    s_return(e_var("_value")),
                ], vec![], None),
                s_if(e_binop(e_var("_tag"), BinOp::StrictEq, e_str("d")), vec![
                    s_assign("_length", e_call("__elephc_intl_u32", vec![e_var("bytes"), e_var("offset")])),
                    s_assign("_value", e_cast(CastType::Float, e_call("substr", vec![e_var("bytes"), e_var("offset"), e_var("_length")]))),
                    s_assign("offset", e_binop(e_var("offset"), BinOp::Add, e_var("_length"))),
                    s_return(e_var("_value")),
                ], vec![], None),
                s_if(e_binop(e_var("_tag"), BinOp::StrictEq, e_str("s")), vec![
                    s_assign("_length", e_call("__elephc_intl_u32", vec![e_var("bytes"), e_var("offset")])),
                    s_assign("_value", e_call("substr", vec![e_var("bytes"), e_var("offset"), e_var("_length")])),
                    s_assign("offset", e_binop(e_var("offset"), BinOp::Add, e_var("_length"))),
                    s_return(e_var("_value")),
                ], vec![], None),
                s_if(e_binop(e_var("_tag"), BinOp::StrictEq, e_str("a")), vec![
                    s_assign("_count", e_call("__elephc_intl_u32", vec![e_var("bytes"), e_var("offset")])),
                    s_assign("_result", e_array(vec![])),
                    s_for(
                        Some(s_assign("_i", e_int(0))),
                        Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_count"))),
                        Some(s_expr(crate::synthetic_class::e_post_inc("_i"))),
                        vec![
                            s_assign("_key", e_call("__elephc_intl_decode_value", vec![e_var("bytes"), e_var("offset")])),
                            s_assign("_item", e_call("__elephc_intl_decode_value", vec![e_var("bytes"), e_var("offset")])),
                            s_array_assign("_result", e_var("_key"), e_var("_item")),
                        ],
                    ),
                    s_return(e_var("_result")),
                ], vec![], None),
                s_return(e_null()),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_wrap")
            .param("value", t_mixed())
            .returns(t_mixed())
            .body(vec![
                s_if(
                    e_binop(
                        e_call("is_array", vec![e_var("value")]),
                        BinOp::And,
                        e_call("isset", vec![e_index(e_var("value"), e_str("__elephc_object"))]),
                    ),
                    vec![
                        s_assign("_handle", e_index(e_var("value"), e_str("__elephc_object"))),
                        s_assign("_class", e_index(e_var("value"), e_str("__elephc_class"))),
                        s_if(e_binop(e_var("_class"), BinOp::StrictEq, e_str("IntlDateFormatter")), vec![s_return(e_static_call("IntlDateFormatter", "__elephc_from_handle", vec![e_var("_handle")]))], vec![], None),
                        s_if(e_binop(e_var("_class"), BinOp::StrictEq, e_str("IntlTimeZone")), vec![s_return(e_static_call("IntlTimeZone", "__elephc_from_handle", vec![e_var("_handle")]))], vec![], None),
                        s_if(e_binop(e_var("_class"), BinOp::StrictEq, e_str("IntlCalendar")), vec![s_return(e_static_call("IntlGregorianCalendar", "__elephc_from_handle", vec![e_var("_handle")]))], vec![], None),
                        s_if(e_binop(e_var("_class"), BinOp::StrictEq, e_str("IntlGregorianCalendar")), vec![s_return(e_static_call("IntlGregorianCalendar", "__elephc_from_handle", vec![e_var("_handle")]))], vec![], None),
                        s_if(e_binop(e_var("_class"), BinOp::StrictEq, e_str("NumberFormatter")), vec![s_return(e_static_call("NumberFormatter", "__elephc_from_handle", vec![e_var("_handle")]))], vec![], None),
                        s_if(e_binop(e_var("_class"), BinOp::StrictEq, e_str("MessageFormatter")), vec![s_return(e_static_call("MessageFormatter", "__elephc_from_handle", vec![e_var("_handle")]))], vec![], None),
                        s_if(e_binop(e_var("_class"), BinOp::StrictEq, e_str("Collator")), vec![s_return(e_static_call("Collator", "__elephc_from_handle", vec![e_var("_handle")]))], vec![], None),
                        s_if(e_binop(e_var("_class"), BinOp::StrictEq, e_str("Transliterator")), vec![s_return(e_static_call("Transliterator", "__elephc_from_handle", vec![e_var("_handle")]))], vec![], None),
                        s_if(e_binop(e_var("_class"), BinOp::StrictEq, e_str("Spoofchecker")), vec![s_return(e_static_call("Spoofchecker", "__elephc_from_handle", vec![e_var("_handle")]))], vec![], None),
                        s_if(e_binop(e_var("_class"), BinOp::StrictEq, e_str("ResourceBundle")), vec![s_return(e_static_call("ResourceBundle", "__elephc_from_handle", vec![e_var("_handle")]))], vec![], None),
                        s_if(e_binop(e_var("_class"), BinOp::StrictEq, e_str("UConverter")), vec![s_return(e_static_call("UConverter", "__elephc_from_handle", vec![e_var("_handle")]))], vec![], None),
                    ],
                    vec![],
                    None,
                ),
                s_return(e_var("value")),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_invoke")
            .param("family", TypeExpr::Str)
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![
                s_assign("_encoded", e_call("elephc_intl_static", vec![e_var("family"), e_var("method"), e_call("bin2hex", vec![e_call("__elephc_intl_pack_values", vec![e_var("args")])])])),
                s_if(e_binop(e_var("_encoded"), BinOp::StrictEq, e_str("")), vec![s_return(crate::synthetic_class::e_bool(false))], vec![], None),
                s_assign("_offset", e_int(0)),
                s_return(e_call("__elephc_intl_wrap", vec![e_call("__elephc_intl_decode_value", vec![e_call("hex2bin", vec![e_var("_encoded")]), e_var("_offset")])])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_invoke_checked")
            .param("family", TypeExpr::Str)
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .param("handleIndex", TypeExpr::Int)
            .returns(t_mixed())
            .body(vec![
                s_assign("_result", e_call("__elephc_intl_invoke", vec![e_var("family"), e_var("method"), e_var("args")])),
                s_if(e_binop(e_var("_result"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![
                    s_assign("_message", e_call("elephc_intl_error_message", vec![e_index(e_var("args"), e_var("handleIndex"))])),
                    s_if(
                        e_binop(
                            e_binop(e_call("str_contains", vec![e_var("_message"), e_str("must be a valid")]), BinOp::Or,
                                    e_call("str_contains", vec![e_var("_message"), e_str("must be between")])),
                            BinOp::Or,
                            e_call("str_contains", vec![e_var("_message"), e_str("no variant")]),
                        ),
                        vec![s_throw(e_new("ValueError", vec![e_var("_message")]))],
                        vec![],
                        None,
                    ),
                ], vec![], None),
                s_return(e_var("_result")),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_call")
            .param("handle", TypeExpr::Int)
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![
                s_assign("_encoded", e_call("elephc_intl_call", vec![e_var("handle"), e_var("method"), e_call("bin2hex", vec![e_call("__elephc_intl_pack_values", vec![e_var("args")])])])),
                s_if(e_binop(e_var("_encoded"), BinOp::StrictEq, e_str("")), vec![s_return(crate::synthetic_class::e_bool(false))], vec![], None),
                s_assign("_offset", e_int(0)),
                s_return(e_call("__elephc_intl_wrap", vec![e_call("__elephc_intl_decode_value", vec![e_call("hex2bin", vec![e_var("_encoded")]), e_var("_offset")])])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_invoke_outputs")
            .param("family", TypeExpr::Str)
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_array())
            .body(vec![
                s_assign("_result", e_call("__elephc_intl_invoke", vec![e_var("family"), e_var("method"), e_var("args")])),
                s_if(e_binop(e_var("_result"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![s_return(e_array(vec![]))], vec![], None),
                s_return(e_cast(CastType::Array, e_var("_result"))),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_open")
            .param("family", TypeExpr::Str)
            .param("spec", t_mixed())
            .returns(TypeExpr::Int)
            .body(vec![
                s_return(e_call("elephc_intl_open", vec![e_var("family"), e_call("bin2hex", vec![e_call("__elephc_intl_pack_values", vec![e_var("spec")])])])),
            ])
            .build(),
    );
    for (name, ty, cast) in [
        ("__elephc_intl_invoke_string", TypeExpr::Str, CastType::String),
        ("__elephc_intl_invoke_int", TypeExpr::Int, CastType::Int),
        ("__elephc_intl_invoke_float", TypeExpr::Float, CastType::Float),
        ("__elephc_intl_invoke_bool", TypeExpr::Bool, CastType::Bool),
    ] {
        helpers.push(
            function(name)
                .param("family", TypeExpr::Str)
                .param("method", TypeExpr::Str)
                .param("args", t_mixed())
                .returns(ty)
                .body(vec![
                    s_return(e_cast(
                        cast,
                        e_call("__elephc_intl_invoke", vec![e_var("family"), e_var("method"), e_var("args")]),
                    )),
                ])
                .build(),
        );
    }
    // A `(array)` cast of a boxed `mixed` keeps the boxed storage shape; rebuild the array
    // element by element so the caller sees a real PHP array (keys preserved).
    helpers.push(
        function("__elephc_intl_invoke_array")
            .param("family", TypeExpr::Str)
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_array())
            .body(vec![
                s_assign("_value", e_call("__elephc_intl_invoke", vec![e_var("family"), e_var("method"), e_var("args")])),
                s_assign("_result", e_array(vec![])),
                s_if(e_call("is_array", vec![e_var("_value")]), vec![
                    crate::synthetic_class::s_foreach(e_var("_value"), Some("_key"), "_item", vec![
                        s_array_assign("_result", e_var("_key"), e_var("_item")),
                    ]),
                ], vec![], None),
                s_return(e_var("_result")),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_invoke_void")
            .param("family", TypeExpr::Str)
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .body(vec![
                s_expr(e_call("__elephc_intl_invoke", vec![e_var("family"), e_var("method"), e_var("args")])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_invoke_mixed")
            .param("family", TypeExpr::Str)
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![
                s_return(e_call("__elephc_intl_invoke", vec![e_var("family"), e_var("method"), e_var("args")])),
            ])
            .build(),
    );
    let mut handle_test = e_instance_of(e_var("value"), "IntlDateFormatter");
    for class in [
        "IntlTimeZone", "IntlCalendar", "IntlGregorianCalendar", "Collator", "NumberFormatter",
        "MessageFormatter", "Transliterator", "Spoofchecker", "ResourceBundle", "UConverter",
        "IntlIterator", "IntlBreakIterator", "IntlRuleBasedBreakIterator",
        "IntlCodePointBreakIterator", "IntlPartsIterator", "IntlListFormatter",
        "IntlDatePatternGenerator", "IntlChar",
    ] {
        handle_test = e_binop(handle_test, BinOp::Or, e_instance_of(e_var("value"), class));
    }
    helpers.push(
        function("__elephc_intl_invoke_locale")
            .param("family", TypeExpr::Str)
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .param("names", t_mixed())
            .returns(t_mixed())
            .body(vec![
                s_if(
                    e_binop(e_var("method"), BinOp::StrictEq, e_str("locale_lookup")),
                    vec![s_expr(e_call("__elephc_intl_lookup_tags_check", vec![
                        e_index(e_var("args"), e_int(0)), e_var("method"),
                    ]))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(
                        e_binop(e_var("method"), BinOp::StrictEq, e_str("locale_compose")),
                        BinOp::And,
                        e_not(call_isset(e_index(e_index(e_var("args"), e_int(0)), e_str("language")))),
                    ),
                    vec![s_throw(e_new("ValueError", vec![e_concat_all(vec![
                        e_var("method"), e_str("(): Argument #1 ($subtags) must contain a \"language\" key"),
                    ])]))],
                    vec![],
                    None,
                ),
                crate::synthetic_class::s_for(
                    Some(s_assign("_i", e_int(0))),
                    Some(e_binop(e_var("_i"), BinOp::Lt, e_call("count", vec![e_var("args")]))),
                    Some(s_expr(crate::synthetic_class::e_post_inc("_i"))),
                    vec![
                        s_if(e_not(e_call("is_string", vec![e_index(e_var("args"), e_var("_i"))])), vec![crate::synthetic_class::s_continue(1)], vec![], None),
                        s_if(e_not(e_call("str_contains", vec![e_index(e_var("args"), e_var("_i")), e_str("\0")])), vec![crate::synthetic_class::s_continue(1)], vec![], None),
                        s_assign("_name", e_index(e_var("names"), e_var("_i"))),
                        s_throw(e_new("ValueError", vec![e_concat_all(vec![
                            e_var("method"), e_str("(): Argument #"), e_binop(e_var("_i"), BinOp::Add, e_int(1)),
                            e_str(" ($"), e_var("_name"), e_str(") must not contain any null bytes"),
                        ])])),
                    ],
                ),
                s_return(e_call("__elephc_intl_global_fail", vec![
                    e_call("__elephc_intl_invoke", vec![e_str("locale"), e_var("method"), e_var("args")]),
                    e_concat(e_var("method"), e_str("()")),
                ])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_grapheme_pos")
            .param("family", TypeExpr::Str)
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![
                s_assign("_haystack", e_cast(CastType::String, e_index(e_var("args"), e_int(0)))),
                s_assign("_offset", e_ternary(
                    call_isset(e_index(e_var("args"), e_int(2))),
                    e_cast(CastType::Int, e_index(e_var("args"), e_int(2))),
                    e_int(0),
                )),
                s_assign("_message", e_concat(e_var("method"), e_str("(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"))),
                s_if(
                    e_binop(
                        e_binop(e_binop(e_var("_offset"), BinOp::Add, e_call("strlen", vec![e_var("_haystack")])), BinOp::Lt, e_int(0)),
                        BinOp::Or,
                        e_binop(e_var("_offset"), BinOp::Gt, e_call("strlen", vec![e_var("_haystack")])),
                    ),
                    vec![s_throw(e_new("ValueError", vec![e_var("_message")]))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(
                        e_binop(e_binop(e_var("_offset"), BinOp::Add, e_call("grapheme_strlen", vec![e_var("_haystack")])), BinOp::Lt, e_int(0)),
                        BinOp::Or,
                        e_binop(e_var("_offset"), BinOp::Gt, e_call("grapheme_strlen", vec![e_var("_haystack")])),
                    ),
                    vec![s_throw(e_new("ValueError", vec![e_var("_message")]))],
                    vec![],
                    None,
                ),
                s_return(e_call("__elephc_intl_invoke", vec![e_var("family"), e_var("method"), e_var("args")])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_grapheme_extract")
            .param("family", TypeExpr::Str)
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![
                s_assign("_type", e_ternary(
                    call_isset(e_index(e_var("args"), e_int(2))),
                    e_cast(CastType::Int, e_index(e_var("args"), e_int(2))),
                    e_int(0),
                )),
                s_if(
                    e_binop(e_binop(e_var("_type"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("_type"), BinOp::Gt, e_int(2))),
                    vec![s_throw(e_new("ValueError", vec![e_str(
                        "grapheme_extract(): Argument #3 ($type) must be one of GRAPHEME_EXTR_COUNT, GRAPHEME_EXTR_MAXBYTES, or GRAPHEME_EXTR_MAXCHARS",
                    )]))],
                    vec![],
                    None,
                ),
                s_return(e_call("__elephc_intl_invoke", vec![e_var("family"), e_var("method"), e_var("args")])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_grapheme_levenshtein")
            .param("family", TypeExpr::Str)
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![
                s_assign("_insertion", e_ternary(
                    call_isset(e_index(e_var("args"), e_int(2))),
                    e_cast(CastType::Int, e_index(e_var("args"), e_int(2))),
                    e_int(1),
                )),
                s_assign("_replacement", e_ternary(
                    call_isset(e_index(e_var("args"), e_int(3))),
                    e_cast(CastType::Int, e_index(e_var("args"), e_int(3))),
                    e_int(1),
                )),
                s_assign("_deletion", e_ternary(
                    call_isset(e_index(e_var("args"), e_int(4))),
                    e_cast(CastType::Int, e_index(e_var("args"), e_int(4))),
                    e_int(1),
                )),
                s_if(
                    e_binop(e_binop(e_var("_insertion"), BinOp::LtEq, e_int(0)), BinOp::Or, e_binop(e_var("_insertion"), BinOp::Gt, e_int(1_073_741_823))),
                    vec![s_throw(e_new("ValueError", vec![e_str(
                        "grapheme_levenshtein(): Argument #3 ($insertion_cost) must be greater than 0 and less than or equal to 1073741823",
                    )]))],
                    vec![], None,
                ),
                s_if(
                    e_binop(e_binop(e_var("_replacement"), BinOp::LtEq, e_int(0)), BinOp::Or, e_binop(e_var("_replacement"), BinOp::Gt, e_int(1_073_741_823))),
                    vec![s_throw(e_new("ValueError", vec![e_str(
                        "grapheme_levenshtein(): Argument #4 ($replacement_cost) must be greater than 0 and less than or equal to 1073741823",
                    )]))],
                    vec![], None,
                ),
                s_if(
                    e_binop(e_binop(e_var("_deletion"), BinOp::LtEq, e_int(0)), BinOp::Or, e_binop(e_var("_deletion"), BinOp::Gt, e_int(1_073_741_823))),
                    vec![s_throw(e_new("ValueError", vec![e_str(
                        "grapheme_levenshtein(): Argument #5 ($deletion_cost) must be greater than 0 and less than or equal to 1073741823",
                    )]))],
                    vec![], None,
                ),
                s_return(e_call("__elephc_intl_invoke", vec![e_var("family"), e_var("method"), e_var("args")])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_timezone_valid")
            .param("timezone", t_mixed())
            .returns(TypeExpr::Bool)
            .body(vec![
                s_if(e_instance_of(e_var("timezone"), "IntlTimeZone"), vec![
                    s_return(e_binop(e_prop(e_var("timezone"), "__elephc_handle"), BinOp::StrictNotEq, e_int(0))),
                ], vec![], None),
                s_return(crate::synthetic_class::e_bool(true)),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_timezone_name")
            .param("timezone", t_mixed())
            .returns(TypeExpr::Str)
            .body(vec![
                s_if(e_binop(e_var("timezone"), BinOp::StrictEq, e_null()), vec![
                    s_return(e_call("date_default_timezone_get", vec![])),
                ], vec![], None),
                s_if(e_instance_of(e_var("timezone"), "IntlTimeZone"), vec![
                    s_return(e_cast(CastType::String, e_method_call(e_var("timezone"), "getID", vec![]))),
                ], vec![], None),
                s_if(e_instance_of(e_var("timezone"), "DateTimeZone"), vec![
                    s_return(e_cast(CastType::String, e_method_call(e_var("timezone"), "getName", vec![]))),
                ], vec![], None),
                s_return(e_cast(CastType::String, e_var("timezone"))),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_is_timezone")
            .param("zone", TypeExpr::Str)
            .returns(TypeExpr::Bool)
            .body(vec![
                s_assign("_length", e_call("strlen", vec![e_var("zone")])),
                s_if(e_binop(e_var("_length"), BinOp::StrictEq, e_int(0)), vec![s_return(crate::synthetic_class::e_bool(false))], vec![], None),
                s_if(e_call("str_contains", vec![e_var("zone"), e_str("/")]), vec![s_return(crate::synthetic_class::e_bool(true))], vec![], None),
                s_assign("_upper", e_call("strtoupper", vec![e_var("zone")])),
                s_if(e_binop(e_binop(e_var("_upper"), BinOp::StrictEq, e_str("UTC")), BinOp::Or, e_binop(e_var("_upper"), BinOp::StrictEq, e_str("GMT"))), vec![s_return(crate::synthetic_class::e_bool(true))], vec![], None),
                s_assign("_first", e_index(e_var("zone"), e_int(0))),
                s_if(e_binop(e_binop(e_var("_first"), BinOp::StrictEq, e_str("+")), BinOp::Or, e_binop(e_var("_first"), BinOp::StrictEq, e_str("-"))), vec![
                    s_return(e_call("str_contains", vec![e_var("zone"), e_str(":")])),
                ], vec![], None),
                s_if(e_binop(e_binop(e_var("_length"), BinOp::Lt, e_int(2)), BinOp::Or, e_binop(e_var("_length"), BinOp::Gt, e_int(5))), vec![s_return(crate::synthetic_class::e_bool(false))], vec![], None),
                s_for(
                    Some(s_assign("_i", e_int(0))),
                    Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_length"))),
                    Some(s_expr(crate::synthetic_class::e_post_inc("_i"))),
                    vec![
                        s_if(e_not(e_call("ctype_alpha", vec![e_index(e_var("zone"), e_var("_i"))])), vec![s_return(crate::synthetic_class::e_bool(false))], vec![], None),
                    ],
                ),
                s_return(crate::synthetic_class::e_bool(true)),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_no_null")
            .param("value", t_mixed())
            .param("caller", TypeExpr::Str)
            .param("index", TypeExpr::Int)
            .param("parameter", TypeExpr::Str)
            .body(vec![
                s_if(e_binop(e_call("is_string", vec![e_var("value")]), BinOp::And, e_call("str_contains", vec![e_var("value"), e_str("\0")])), vec![
                    s_throw(e_new("ValueError", vec![e_concat_all(vec![
                        e_var("caller"), e_str("(): Argument #"), e_var("index"), e_str(" ($"),
                        e_var("parameter"), e_str(") must not contain any null bytes"),
                    ])])),
                ], vec![], None),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_object_result")
            .param("object", t_mixed())
            .param("result", t_mixed())
            .returns(t_mixed())
            .body(vec![
                s_if(e_binop(e_var("result"), BinOp::StrictNotEq, crate::synthetic_class::e_bool(false)), vec![
                    s_return(e_var("result")),
                ], vec![], None),
                s_assign("_message", e_method_call(e_var("object"), "__elephc_call", vec![
                    e_str("error_context"), e_array(vec![]),
                ])),
                s_if(e_call("__elephc_intl_use_exceptions", vec![]), vec![
                    s_throw(e_new("IntlException", vec![e_var("_message")])),
                ], vec![], None),
                s_return(crate::synthetic_class::e_bool(false)),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_global_result")
            .param("result", t_mixed())
            .returns(t_mixed())
            .body(vec![
                s_if(e_binop(e_var("result"), BinOp::StrictNotEq, crate::synthetic_class::e_bool(false)), vec![
                    s_return(e_var("result")),
                ], vec![], None),
                s_assign("_message", e_call("__elephc_intl_invoke_string", vec![
                    e_str("intl"), e_str("intl_get_error_context"), e_array(vec![]),
                ])),
                s_if(e_call("__elephc_intl_use_exceptions", vec![]), vec![
                    s_throw(e_new("IntlException", vec![e_var("_message")])),
                ], vec![], None),
                s_return(crate::synthetic_class::e_bool(false)),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_object_fail")
            .param("object", t_mixed())
            .param("result", t_mixed())
            .param("prefix", TypeExpr::Str)
            .returns(t_mixed())
            .body(vec![
                s_return(e_call("__elephc_intl_handle_fail", vec![
                    e_prop(e_var("object"), "__elephc_handle"),
                    e_var("result"),
                    e_var("prefix"),
                ])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_handle_fail")
            .param("handle", TypeExpr::Int)
            .param("result", t_mixed())
            .param("prefix", TypeExpr::Str)
            .returns(t_mixed())
            .body(vec![
                s_if(e_binop(
                    e_binop(e_var("result"), BinOp::StrictNotEq, crate::synthetic_class::e_bool(false)),
                    BinOp::And,
                    e_binop(e_var("result"), BinOp::StrictNotEq, e_null()),
                ), vec![
                    s_return(e_var("result")),
                ], vec![], None),
                s_if(e_binop(e_call("__elephc_intl_invoke_int", vec![
                    e_str("intl"), e_str("intl_get_error_code"), e_array(vec![]),
                ]), BinOp::StrictEq, e_int(0)), vec![
                    s_return(e_var("result")),
                ], vec![], None),
                s_expr(e_call("__elephc_intl_invoke", vec![
                    e_str("intl"), e_str("intl_reprefix_error"),
                    e_array(vec![e_var("handle"), e_var("prefix")]),
                ])),
                s_assign("_message", e_call("__elephc_intl_invoke_string", vec![
                    e_str("intl"), e_str("intl_get_error_context"), e_array(vec![]),
                ])),
                s_if(e_call("__elephc_intl_use_exceptions", vec![]), vec![
                    s_throw(e_new("IntlException", vec![e_var("_message")])),
                ], vec![], None),
                s_return(crate::synthetic_class::e_bool(false)),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_global_fail")
            .param("result", t_mixed())
            .param("prefix", TypeExpr::Str)
            .returns(t_mixed())
            .body(vec![
                s_if(e_binop(
                    e_binop(e_var("result"), BinOp::StrictNotEq, crate::synthetic_class::e_bool(false)),
                    BinOp::And,
                    e_binop(e_var("result"), BinOp::StrictNotEq, e_null()),
                ), vec![
                    s_return(e_var("result")),
                ], vec![], None),
                s_assign("_code", e_call("__elephc_intl_invoke_int", vec![
                    e_str("intl"), e_str("intl_get_error_code"), e_array(vec![]),
                ])),
                s_assign("_context", e_call("__elephc_intl_invoke_string", vec![
                    e_str("intl"), e_str("intl_get_error_context"), e_array(vec![]),
                ])),
                // A custom message can carry U_ZERO_ERROR (php reports it with : U_ZERO_ERROR);
                // the empty/"U_ZERO_ERROR" contexts are the successful-call sentinels.
                s_if(
                    e_binop(
                        e_binop(e_var("_code"), BinOp::StrictEq, e_int(0)),
                        BinOp::And,
                        e_binop(
                            e_binop(e_var("_context"), BinOp::StrictEq, e_str("")),
                            BinOp::Or,
                            e_binop(e_var("_context"), BinOp::StrictEq, e_str("U_ZERO_ERROR")),
                        ),
                    ),
                    vec![s_return(e_var("result"))],
                    vec![],
                    None,
                ),
                s_assign("_message", e_concat_all(vec![
                    e_var("prefix"), e_str(": "), e_var("_context"),
                ])),
                s_expr(e_call("__elephc_intl_invoke", vec![
                    e_str("intl"), e_str("intl_set_error"),
                    e_array(vec![e_var("_code"), e_var("_message")]),
                ])),
                s_if(e_call("__elephc_intl_use_exceptions", vec![]), vec![
                    s_throw(e_new("IntlException", vec![e_var("_message")])),
                ], vec![], None),
                s_return(e_var("result")),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_offset_zone")
            .param("value", TypeExpr::Str)
            .returns(TypeExpr::Str)
            .body(vec![
                s_assign("_plus", e_call("strpos", vec![e_var("value"), e_str("+"), e_int(1)])),
                s_assign("_minus", e_call("strpos", vec![e_var("value"), e_str("-"), e_int(1)])),
                s_assign("_pos", crate::synthetic_class::e_bool(false)),
                s_if(e_binop(
                    e_binop(e_var("_plus"), BinOp::StrictNotEq, crate::synthetic_class::e_bool(false)),
                    BinOp::And,
                    e_binop(
                        e_binop(e_var("_minus"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)),
                        BinOp::Or,
                        e_binop(e_var("_plus"), BinOp::Lt, e_var("_minus")),
                    ),
                ), vec![
                    s_assign("_pos", e_var("_plus")),
                ], vec![], Some(vec![
                    s_if(e_binop(e_var("_minus"), BinOp::StrictNotEq, crate::synthetic_class::e_bool(false)), vec![
                        s_assign("_pos", e_var("_minus")),
                    ], vec![], None),
                ])),
                s_if(e_binop(e_var("_pos"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![
                    s_return(e_str("")),
                ], vec![], None),
                s_assign("_offset", e_call("substr", vec![e_var("value"), e_var("_pos")])),
                s_if(e_binop(e_call("strlen", vec![e_var("_offset")]), BinOp::StrictNotEq, e_int(6)), vec![
                    s_return(e_str("")),
                ], vec![], None),
                s_return(e_concat(e_str("GMT"), e_var("_offset"))),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_lookup_tags_check")
            .param("tags", t_mixed())
            .param("caller", TypeExpr::Str)
            .returns(TypeExpr::Void)
            .body(vec![
                crate::synthetic_class::s_foreach(e_var("tags"), None, "_tag", vec![
                    s_if(
                        e_binop(
                            e_call("is_string", vec![e_var("_tag")]),
                            BinOp::And,
                            e_call("str_contains", vec![e_var("_tag"), e_str("\0")]),
                        ),
                        vec![s_throw(e_new("ValueError", vec![e_concat_all(vec![
                            e_var("caller"), e_str("(): Argument #2 ($locale) must not contain any null bytes"),
                        ])]))],
                        vec![],
                        None,
                    ),
                ]),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_datetime_initialized")
            .param("value", t_mixed())
            .returns(TypeExpr::Bool)
            .body(vec![
                // Reading the flag through a mixed parameter keeps the dynamic property lookup
                // away from the DateTimeInterface narrowing at the call site.
                s_if(e_not(e_prop(e_var("value"), "__elephc_initialized")), vec![
                    s_return(crate::synthetic_class::e_bool(false)),
                ], vec![], None),
                s_return(crate::synthetic_class::e_bool(true)),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_debug_type")
            .param("value", t_mixed())
            .returns(TypeExpr::Str)
            .body(vec![
                s_if(e_call("is_object", vec![e_var("value")]), vec![s_return(e_call("get_class", vec![e_var("value")]))], vec![], None),
                s_if(e_call("is_int", vec![e_var("value")]), vec![s_return(e_str("int"))], vec![], None),
                s_if(e_call("is_float", vec![e_var("value")]), vec![s_return(e_str("float"))], vec![], None),
                s_if(e_call("is_bool", vec![e_var("value")]), vec![s_return(e_str("bool"))], vec![], None),
                s_if(e_call("is_string", vec![e_var("value")]), vec![s_return(e_str("string"))], vec![], None),
                s_if(e_call("is_array", vec![e_var("value")]), vec![s_return(e_str("array"))], vec![], None),
                s_if(e_call("is_null", vec![e_var("value")]), vec![s_return(e_str("null"))], vec![], None),
                s_return(e_call("gettype", vec![e_var("value")])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_number_argument")
            .param("value", t_mixed())
            .param("caller", TypeExpr::Str)
            .param("position", TypeExpr::Int)
            .param("name", TypeExpr::Str)
            .returns(t_union(vec![TypeExpr::Int, TypeExpr::Float]))
            .body(vec![
                s_if(e_call("is_int", vec![e_var("value")]), vec![s_return(e_var("value"))], vec![], None),
                s_if(e_call("is_float", vec![e_var("value")]), vec![s_return(e_var("value"))], vec![], None),
                s_if(e_call("is_null", vec![e_var("value")]), vec![
                    s_expr(e_call("trigger_error", vec![
                        e_concat_all(vec![
                            e_var("caller"),
                            e_str(": Passing null to parameter #"),
                            e_var("position"),
                            e_str(" ($"),
                            e_var("name"),
                            e_str(") of type int|float is deprecated"),
                        ]),
                        e_const("E_USER_DEPRECATED"),
                    ])),
                    s_return(e_int(0)),
                ], vec![], None),
                s_if(e_binop(e_call("is_string", vec![e_var("value")]), BinOp::And, e_call("is_numeric", vec![e_var("value")])), vec![
                    s_return(e_binop(e_var("value"), BinOp::Add, e_int(0))),
                ], vec![], None),
                s_if(e_call("is_bool", vec![e_var("value")]), vec![
                    s_return(e_ternary(e_var("value"), e_int(1), e_int(0))),
                ], vec![], None),
                s_throw(e_new("TypeError", vec![e_concat_all(vec![
                    e_var("caller"),
                    e_str(": Argument #"),
                    e_var("position"),
                    e_str(" ($"),
                    e_var("name"),
                    e_str(") must be of type int|float, "),
                    e_call("__elephc_intl_debug_type", vec![e_var("value")]),
                    e_str(" given"),
                ])])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_date_zone")
            .param("timezone", t_mixed())
            .returns(TypeExpr::Str)
            .body(vec![
                s_if(e_binop(e_var("timezone"), BinOp::StrictEq, e_null()), vec![
                    s_return(e_call("date_default_timezone_get", vec![])),
                ], vec![], None),
                s_if(e_instance_of(e_var("timezone"), "DateTimeZone"), vec![
                    s_return(e_cast(CastType::String, e_method_call(e_var("timezone"), "getName", vec![]))),
                ], vec![], None),
                s_if(e_instance_of(e_var("timezone"), "IntlTimeZone"), vec![
                    s_return(e_cast(CastType::String, e_method_call(e_var("timezone"), "getID", vec![]))),
                ], vec![], None),
                s_return(e_cast(CastType::String, e_var("timezone"))),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_msgfmt_parse_message")
            .param("family", TypeExpr::Str)
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![
                s_assign("_result", e_call("__elephc_intl_invoke", vec![
                    e_var("family"), e_var("method"), e_var("args"),
                ])),
                s_return(e_call("__elephc_intl_global_fail", vec![
                    e_var("_result"), e_str("msgfmt_parse_message()"),
                ])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_normalizer_raw")
            .param("family", TypeExpr::Str)
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![
                s_assign("_result", e_call("__elephc_intl_invoke", vec![
                    e_var("family"), e_var("method"), e_var("args"),
                ])),
                s_return(e_call("__elephc_intl_global_fail", vec![
                    e_var("_result"), e_str("normalizer_get_raw_decomposition()"),
                ])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_locale_valid")
            .param("locale", t_mixed())
            .returns(TypeExpr::Bool)
            .body(vec![s_return(e_call("__elephc_intl_invoke_bool", vec![
                e_str("intl"), e_str("locale_valid"), e_array(vec![e_cast(CastType::String, e_var("locale"))]),
            ]))])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_validate_timezone")
            .param("zone", TypeExpr::Str)
            .param("prefix", TypeExpr::Str)
            .returns(TypeExpr::Void)
            .body(vec![
                s_if(e_not(e_call("mb_check_encoding", vec![e_var("zone"), e_str("UTF-8")])), vec![
                    s_throw(e_new("IntlException", vec![e_concat_all(vec![
                        e_var("prefix"), e_str(": Time zone identifier given is not a valid UTF-8 string"),
                    ])])),
                ], vec![], None),
                s_if(e_not(e_call("__elephc_intl_timezone_id_valid", vec![e_var("zone")])), vec![
                    s_throw(e_new("IntlException", vec![e_concat_all(vec![
                        e_var("prefix"), e_str(": No such time zone: \""), e_var("zone"), e_str("\""),
                    ])])),
                ], vec![], None),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_validate_pattern")
            .param("pattern", TypeExpr::Str)
            .param("prefix", TypeExpr::Str)
            .returns(TypeExpr::Void)
            .body(vec![
                s_if(
                    e_binop(
                        e_binop(e_var("pattern"), BinOp::StrictNotEq, e_str("")),
                        BinOp::And,
                        e_not(e_call("mb_check_encoding", vec![e_var("pattern"), e_str("UTF-8")])),
                    ),
                    vec![s_throw(e_new("IntlException", vec![e_concat_all(vec![
                        e_var("prefix"), e_str(": error converting pattern to UTF-16"),
                    ])])),],
                    vec![],
                    None,
                ),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_timezone_id_valid")
            .param("zone", TypeExpr::Str)
            .returns(TypeExpr::Bool)
            .body(vec![s_return(e_call("__elephc_intl_invoke_bool", vec![
                e_str("intl"), e_str("timezone_valid"), e_array(vec![e_var("zone")]),
            ]))])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_use_exceptions")
            .param_default("value", t_mixed(), e_null())
            .returns(TypeExpr::Bool)
            .body(vec![
                s_static("enabled", crate::synthetic_class::e_bool(false)),
                s_if(e_binop(e_var("value"), BinOp::StrictNotEq, e_null()), vec![
                    s_assign("enabled", e_cast(CastType::Bool, e_var("value"))),
                ], vec![], None),
                s_return(e_var("enabled")),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_error_level")
            .param_default("value", t_mixed(), e_null())
            .returns(TypeExpr::Int)
            .body(vec![
                s_static("level", e_int(0)),
                s_if(e_binop(e_var("value"), BinOp::StrictNotEq, e_null()), vec![
                    s_assign("level", e_cast(CastType::Int, e_var("value"))),
                ], vec![], None),
                s_return(e_var("level")),
            ])
            .build(),
    );
    // Reports one Transliterator failure exactly as php-src's intl error layer does: the
    // object and the process-wide error are updated, `intl.use_exceptions` decides whether the
    // error is thrown, and a configured `intl.error_level` decides whether it is displayed.
    helpers.push(
        function("__elephc_intl_transliterator_error")
            .param("transliterator", t_mixed())
            .param("code", TypeExpr::Int)
            .param("message", TypeExpr::Str)
            .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![
                s_expr(e_method_call(e_var("transliterator"), "__elephc_call", vec![
                    e_str("__elephc_set_error"), e_array(vec![e_var("code"), e_var("message")]),
                ])),
                s_if(e_call("__elephc_intl_use_exceptions", vec![]), vec![
                    s_throw(e_new("IntlException", vec![e_var("message")])),
                ], vec![], None),
                s_assign("_level", e_call("__elephc_intl_error_level", vec![])),
                s_if(e_binop(e_var("_level"), BinOp::StrictEq, e_const("E_WARNING")), vec![
                    s_expr(e_call("trigger_error", vec![e_var("message"), e_const("E_USER_WARNING")])),
                ], vec![
                    (e_binop(e_var("_level"), BinOp::StrictEq, e_const("E_NOTICE")), vec![
                        s_expr(e_call("trigger_error", vec![e_var("message"), e_const("E_USER_NOTICE")])),
                    ]),
                    (e_binop(e_var("_level"), BinOp::StrictEq, e_const("E_DEPRECATED")), vec![
                        s_expr(e_call("trigger_error", vec![e_var("message"), e_const("E_USER_DEPRECATED")])),
                    ]),
                ], None),
                s_return(crate::synthetic_class::e_bool(false)),
            ])
            .build(),
    );
    // Shared body of `Transliterator::transliterate()` and `transliterator_transliterate()`.
    // php-src validates the range before converting the string, and the procedural entry point
    // numbers the arguments one place later and prefixes its own name in diagnostics.
    helpers.push(
        function("__elephc_intl_transliterate")
            .param("transliterator", t_mixed())
            .param("string", t_mixed())
            .param("start", TypeExpr::Int)
            .param("end", TypeExpr::Int)
            .param("is_method", TypeExpr::Bool)
            .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![
                s_assign("_name", crate::synthetic_class::e_ternary(
                    e_var("is_method"),
                    e_str("Transliterator::transliterate"),
                    e_str("transliterator_transliterate"),
                )),
                s_assign("_start_index", crate::synthetic_class::e_ternary(
                    e_var("is_method"), e_int(2), e_int(3),
                )),
                s_assign("_end_index", crate::synthetic_class::e_ternary(
                    e_var("is_method"), e_int(3), e_int(4),
                )),
                s_if(e_binop(e_var("end"), BinOp::Lt, e_int(-1)), vec![
                    s_throw(e_new("ValueError", vec![e_concat_all(vec![
                        e_var("_name"), e_str("(): Argument #"), e_var("_end_index"),
                        e_str(" ($end) must be greater than or equal to -1"),
                    ])])),
                ], vec![], None),
                s_if(e_binop(e_var("start"), BinOp::Lt, e_int(0)), vec![
                    s_throw(e_new("ValueError", vec![e_concat_all(vec![
                        e_var("_name"), e_str("(): Argument #"), e_var("_start_index"),
                        e_str(" ($start) must be greater than or equal to 0"),
                    ])])),
                ], vec![], None),
                s_if(e_binop(
                    e_binop(e_var("end"), BinOp::StrictNotEq, e_int(-1)),
                    BinOp::And,
                    e_binop(e_var("start"), BinOp::Gt, e_var("end")),
                ), vec![
                    s_throw(e_new("ValueError", vec![e_concat_all(vec![
                        e_var("_name"), e_str("(): Argument #"), e_var("_start_index"),
                        e_str(" ($start) must be less than or equal to argument #"),
                        e_var("_end_index"), e_str(" ($end)"),
                    ])])),
                ], vec![], None),
                s_assign("_string", e_cast(CastType::String, e_var("string"))),
                s_if(e_not(e_call("mb_check_encoding", vec![e_var("_string"), e_str("UTF-8")])), vec![
                    s_return(e_call("__elephc_intl_transliterator_error", vec![
                        e_var("transliterator"), e_int(10),
                        e_concat_all(vec![e_var("_name"), e_str("(): String conversion of string to UTF-16 failed")]),
                    ])),
                ], vec![], None),
                s_assign("_length", e_call("__elephc_intl_invoke_int", vec![
                    e_str("transliterator"), e_str("transliterator_utf16_length"),
                    e_array(vec![e_var("_string")]),
                ])),
                s_if(e_binop(
                    e_binop(e_var("start"), BinOp::Gt, e_var("_length")),
                    BinOp::Or,
                    e_binop(
                        e_binop(e_var("end"), BinOp::StrictNotEq, e_int(-1)),
                        BinOp::And,
                        e_binop(e_var("end"), BinOp::Gt, e_var("_length")),
                    ),
                ), vec![
                    s_return(e_call("__elephc_intl_transliterator_error", vec![
                        e_var("transliterator"), e_int(1),
                        e_concat_all(vec![
                            e_var("_name"),
                            e_str("(): Neither \"start\" nor the \"end\" arguments can exceed the number of UTF-16 code units (in this case, "),
                            e_var("_length"), e_str(")"),
                        ]),
                    ])),
                ], vec![], None),
                s_return(e_method_call(e_var("transliterator"), "__elephc_call", vec![
                    e_str("transliterate"), e_array(vec![e_var("_string"), e_var("start"), e_var("end")]),
                ])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_intl_handle_arg")
            .param("value", t_mixed())
            .returns(t_mixed())
            .body(vec![
                s_if(handle_test, vec![s_return(e_prop(e_var("value"), "__elephc_handle"))], vec![], None),
                s_return(e_var("value")),
            ])
            .build(),
    );
    helpers
}

/// Builds every synthetic procedural Intl function that crosses the bridge.
fn function_declarations() -> Vec<Stmt> {
    let mut declarations = Vec::new();
    for function_spec in FUNCTIONS {
        declarations.push(function_from_spec(function_spec));
    }
    // The php-src Intl corpus changes the default locale through `ini_set()` and exercises
    // `setlocale()` only for its return value (ICU keeps its own locale). Provide the two
    // process-facing entry points without a separate core builtin surface.
    declarations.push(
        function("ini_set")
            .param("option", TypeExpr::Str)
            .param("value", t_union(vec![
                TypeExpr::Str,
                TypeExpr::Int,
                TypeExpr::Float,
                TypeExpr::Bool,
                TypeExpr::Void,
            ]))
            .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![
                s_if(e_binop(e_var("option"), BinOp::StrictEq, e_str("intl.default_locale")), vec![
                    s_expr(e_call("locale_set_default", vec![e_cast(CastType::String, e_var("value"))])),
                ], vec![], None),
                s_if(e_binop(e_var("option"), BinOp::StrictEq, e_str("intl.use_exceptions")), vec![
                    s_expr(e_call("__elephc_intl_use_exceptions", vec![e_var("value")])),
                ], vec![], None),
                s_if(e_binop(e_var("option"), BinOp::StrictEq, e_str("intl.error_level")), vec![
                    s_assign("_level", e_cast(CastType::Int, e_var("value"))),
                    s_if(e_binop(e_var("_level"), BinOp::StrictNotEq, e_int(0)), vec![
                        s_expr(e_call("trigger_error", vec![
                            e_str("ini_set(): Using a value different than 0 for intl.error_level is deprecated, as the intl.error_level INI setting is deprecated. Instead the intl.use_exceptions INI setting should be enabled to throw exceptions on errors or intl_get_error_code()/intl_get_error_message() should be used to manually deal with errors"),
                            e_const("E_USER_DEPRECATED"),
                        ])),
                    ], vec![], None),
                    s_expr(e_call("__elephc_intl_error_level", vec![e_var("_level")])),
                ], vec![], None),
                s_return(e_str("")),
            ])
            .build(),
    );
    declarations.push(
        function("setlocale")
            .param("category", TypeExpr::Int)
            .variadic("locales", Some(t_mixed()))
            .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![
                s_static("current", e_str("C")),
                s_if(e_binop(e_call("count", vec![e_var("locales")]), BinOp::StrictEq, e_int(0)), vec![
                    s_return(e_var("current")),
                ], vec![], None),
                s_if(
                    e_binop(
                        e_binop(e_index(e_var("locales"), e_int(0)), BinOp::StrictEq, e_str("0")),
                        BinOp::Or,
                        e_binop(e_index(e_var("locales"), e_int(0)), BinOp::StrictEq, e_int(0)),
                    ),
                    vec![s_return(e_var("current"))],
                    vec![],
                    None,
                ),
                s_assign("current", e_cast(CastType::String, e_index(e_var("locales"), e_int(0)))),
                s_return(e_var("current")),
            ])
            .build(),
    );
    declarations
}

/// Builds one synthetic function declaration and its bridge body.
fn function_from_spec(spec: &IntlFunction) -> Stmt {
    let mut builder = function(spec.name);
    for param in spec.params {
        builder = apply_param(builder, param);
    }
    if let Some((returns, body)) = typed_intl_override(spec) {
        return builder.returns(returns).body(body).build();
    }
    // The synthetic bridge surface is intentionally untyped on return: the catalog owns the
    // PHP-visible signature for registry builtins, while these declarations exist to reach the
    // C ABI. Declaring `mixed` keeps every union/nullable result representable.
    // By-reference parameters are results, not inputs: the bridge reports them after the
    // return value. Reading an uninitialized caller variable here would hand the encoder a
    // raw null sentinel instead of a Mixed cell (and the callee would receive a meaningless
    // value for an argument it only writes).
    let args: Vec<Expr> = spec.params.iter().filter(|param| {
        // Collator sorting passes its array through the bridge: the operation returns the
        // sorted copy, but it needs the caller's entries as input first.
        // The parse entry points also READ the initial offset before overwriting it.
        !param.by_ref
            || (param.name == "array" && spec.name.starts_with("collator_"))
            || param.name == "offset"
    }).map(|param| {
        let value = if param.by_ref && param.name == "offset" {
            // A by-reference parameter is not evaluated by the caller when omitted, so read it
            // only under `isset()` and let a missing initial offset start at zero.
            e_ternary(
                call_isset(e_var(param.name)),
                e_cast(CastType::Int, e_var(param.name)),
                e_int(0),
            )
        } else {
            e_var(param.name)
        };
        // Instance entry points take the wrapper handle; factory/setter parameters take a zone
        // id or name, which `__elephc_intl_timezone_name` produces from every accepted form.
        if param.ty.trim().trim_start_matches('?') == "IntlTimeZone" {
            e_call("__elephc_intl_handle_arg", vec![value])
        } else if matches!(param.name, "timezone" | "timezoneOrYear" | "timezone_id" | "timezoneId") {
            e_call("__elephc_intl_timezone_name", vec![value])
        } else if has_object_type(param.ty) {
            e_call("__elephc_intl_handle_arg", vec![value])
        } else {
            value
        }
    }).collect();
    let outputs: Vec<(usize, &str)> = spec.params.iter().enumerate()
        .filter(|(_, param)| param.by_ref)
        .map(|(index, param)| (index, param.name))
        .collect();
    let mut body = Vec::new();
    if matches!(spec.name, "intlcal_equals" | "intlcal_before" | "intlcal_after" | "intlcal_is_equivalent_to") {
        let message = format!("{}(): Argument #2 ($other) is uninitialized", spec.name);
        body.push(s_if(
            e_binop(e_call("__elephc_intl_handle_arg", vec![e_var("other")]), BinOp::StrictEq, e_int(0)),
            vec![s_throw(e_new("Error", vec![e_str(&message)]))],
            vec![],
            None,
        ));
    }
    if spec.name == "numfmt_create" {
        body.push(s_if(
            e_binop(
                e_binop(e_var("locale"), BinOp::StrictNotEq, e_null()),
                BinOp::And,
                e_binop(
                    e_binop(e_var("locale"), BinOp::StrictNotEq, e_str("")),
                    BinOp::And,
                    e_not(e_call("__elephc_intl_locale_valid", vec![e_var("locale")])),
                ),
            ),
            vec![s_throw(e_new("ValueError", vec![e_concat_all(vec![
                e_str("numfmt_create(): Argument #1 ($locale) \""),
                e_var("locale"), e_str("\" is invalid"),
            ])]))],
            vec![],
            None,
        ));
    }
    if spec.name == "grapheme_extract" {
        body.push(s_if(
            e_binop(
                e_binop(e_var("type"), BinOp::Lt, e_int(0)),
                BinOp::Or,
                e_binop(e_var("type"), BinOp::Gt, e_int(2)),
            ),
            vec![s_throw(e_new("ValueError", vec![e_str(
                "grapheme_extract(): Argument #3 ($type) must be one of GRAPHEME_EXTR_COUNT, GRAPHEME_EXTR_MAXBYTES, or GRAPHEME_EXTR_MAXCHARS",
            )]))],
            vec![],
            None,
        ));
    }
    if matches!(spec.name, "grapheme_strpos" | "grapheme_stripos" | "grapheme_strrpos" | "grapheme_strripos") {
        let message = format!(
            "{}(): Argument #3 ($offset) must be contained in argument #1 ($haystack)",
            spec.name
        );
        body.push(s_if(
            e_binop(
                e_binop(e_binop(e_var("offset"), BinOp::Add, e_call("strlen", vec![e_var("haystack")])), BinOp::Lt, e_int(0)),
                BinOp::Or,
                e_binop(e_var("offset"), BinOp::Gt, e_call("strlen", vec![e_var("haystack")])),
            ),
            vec![s_throw(e_new("ValueError", vec![e_str(&message)]))],
            vec![],
            None,
        ));
        body.push(s_if(
            e_binop(
                e_binop(e_binop(e_var("offset"), BinOp::Add, e_call("grapheme_strlen", vec![e_var("haystack")])), BinOp::Lt, e_int(0)),
                BinOp::Or,
                e_binop(e_var("offset"), BinOp::Gt, e_call("grapheme_strlen", vec![e_var("haystack")])),
            ),
            vec![s_throw(e_new("ValueError", vec![e_str(&message)]))],
            vec![],
            None,
        ));
    }
    if spec.name == "numfmt_format" || spec.name == "numfmt_parse" {
        let caller = format!("{}()", spec.name);
        let currency_hint = if spec.name == "numfmt_format" {
            "numfmt_format_currency() function"
        } else {
            "numfmt_parse_currency() function"
        };
        body.push(s_if(
            e_binop(e_binop(e_var("type"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("type"), BinOp::Gt, e_int(4))),
            vec![s_throw(e_new("ValueError", vec![e_str(&format!(
                "{caller}: Argument #3 ($type) must be a NumberFormatter::TYPE_* constant"
            ))]))],
            vec![],
            None,
        ));
        body.push(s_if(
            e_binop(e_var("type"), BinOp::StrictEq, e_int(4)),
            vec![
                s_expr(e_call("trigger_error", vec![
                    e_str("Constant NumberFormatter::TYPE_CURRENCY is deprecated since 8.3"),
                    e_const("E_USER_DEPRECATED"),
                ])),
                s_throw(e_new("ValueError", vec![e_str(&format!(
                    "{caller}: Argument #3 ($type) cannot be NumberFormatter::TYPE_CURRENCY constant, use {currency_hint} instead"
                ))])),
            ],
            vec![],
            None,
        ));
    }
    if spec.family == "locale" {
        for (index, param) in spec.params.iter().enumerate() {
            if matches!(param.ty.trim().trim_start_matches('?'), "string") {
                body.push(s_expr(e_call("__elephc_intl_no_null", vec![
                    e_var(param.name), e_str(spec.name), e_int(index as i64 + 1),
                    e_str(param.name),
                ])));
            }
        }
    }
    if spec.name == "locale_compose" {
        body.push(s_if(
            e_not(call_isset(e_index(e_index(e_var("args"), e_int(0)), e_str("language")))),
            vec![s_throw(e_new("ValueError", vec![e_str("locale_compose(): Argument #1 ($subtags) must contain a \"language\" key")]))],
            vec![],
            None,
        ));
    }
    let checked_calendar = spec.family == "intlcal"
        && spec.params.first().is_some_and(|param| param.name == "calendar");
    if outputs.is_empty() {
        if spec.family == "locale" {
            body.push(s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str(spec.family), e_str(spec.method), e_array(args),
            ])));
            body.push(s_return(e_call("__elephc_intl_global_fail", vec![
                e_var("_result"), e_str(&format!("{}()", spec.name)),
            ])));
        } else {
            body.push(s_return(if checked_calendar {
                e_call("__elephc_intl_invoke_checked", vec![
                    e_str(spec.family), e_str(spec.method), e_array(args), e_int(0),
                ])
            } else {
                e_call("__elephc_intl_invoke", vec![
                    e_str(spec.family), e_str(spec.method), e_array(args),
                ])
            }));
        }
    } else {
        body.push(s_assign("_result", e_call("__elephc_intl_invoke_outputs", vec![
            e_str(spec.family), e_str(spec.method), e_array(args),
        ])));
        body.push(s_if(
            e_binop(e_var("_result"), BinOp::StrictEq, e_array(vec![])),
            vec![s_return(crate::synthetic_class::e_bool(false))],
            vec![],
            None,
        ));
        for (output_index, (_, name)) in outputs.iter().enumerate() {
            body.push(s_assign(*name, e_index(e_var("_result"), e_int(output_index as i64 + 1))));
        }
        body.push(s_return(e_index(e_var("_result"), e_int(0))));
    }
    // The bridge returns a boxed payload; the shared contract owns the PHP-visible return
    // type. Without this, the syntactic fallback types every untagged bridge function as int
    // and narrows a parsed float to zero.
    builder = builder.returns(t_mixed()).body(body);
    builder.build()
}

/// Returns a typed body for the procedural Intl entry points whose result is consumed by
/// another typed PHP function (`iterator_to_array`, `count`, method calls).
fn typed_intl_override(spec: &IntlFunction) -> Option<(TypeExpr, Vec<Stmt>)> {
    let args = || spec.params.iter().map(|param| e_var(param.name)).collect::<Vec<_>>();
    match spec.name {
        "collator_get_attribute" => Some((t_union(vec![TypeExpr::Int, TypeExpr::False]), vec![
            s_assign("_handle", e_call("__elephc_intl_handle_arg", vec![e_var("object")])),
            s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("collator"), e_str("collator_get_attribute"),
                e_array(vec![e_var("_handle"), e_var("attribute")]),
            ])),
            s_return(e_call("__elephc_intl_handle_fail", vec![
                e_var("_handle"), e_var("_result"), e_str("collator_get_attribute()"),
            ])),
        ])),
        "numfmt_parse_currency" => Some((t_union(vec![TypeExpr::Float, TypeExpr::False]), vec![
            s_assign("_handle", e_call("__elephc_intl_handle_arg", vec![e_var("formatter")])),
            s_assign("_result", e_call("__elephc_intl_invoke_outputs", vec![
                e_str("numfmt"), e_str("numfmt_parse_currency"),
                e_array(vec![e_var("_handle"), e_var("string"), e_var("offset")]),
            ])),
            s_if(e_binop(e_var("_result"), BinOp::StrictEq, e_array(vec![])), vec![
                s_return(e_call("__elephc_intl_handle_fail", vec![
                    e_var("_handle"), crate::synthetic_class::e_bool(false), e_str("numfmt_parse_currency()"),
                ])),
            ], vec![], None),
            s_assign("currency", e_index(e_var("_result"), e_int(1))),
            s_assign("offset", e_index(e_var("_result"), e_int(2))),
            s_return(e_call("__elephc_intl_handle_fail", vec![
                e_var("_handle"), e_index(e_var("_result"), e_int(0)), e_str("numfmt_parse_currency()"),
            ])),
        ])),
        "collator_get_locale" => Some((t_union(vec![TypeExpr::Str, TypeExpr::False, TypeExpr::Void]), vec![
            s_assign("_handle", e_call("__elephc_intl_handle_arg", vec![e_var("object")])),
            s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("collator"), e_str("collator_get_locale"),
                e_array(vec![e_var("_handle"), e_var("type")]),
            ])),
            s_return(e_call("__elephc_intl_handle_fail", vec![
                e_var("_handle"), e_var("_result"), e_str("collator_get_locale()"),
            ])),
        ])),
        "locale_get_display_name" => Some((t_union(vec![TypeExpr::Str, TypeExpr::False]), vec![
            s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("locale"), e_str("locale_get_display_name"), e_array(args()),
            ])),
            s_return(e_call("__elephc_intl_global_fail", vec![
                e_var("_result"), e_str("locale_get_display_name()"),
            ])),
        ])),
        "locale_accept_from_http" => Some((t_union(vec![TypeExpr::Str, TypeExpr::False]), vec![
            s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("locale"), e_str("locale_accept_from_http"), e_array(args()),
            ])),
            s_return(e_call("__elephc_intl_global_fail", vec![
                e_var("_result"), e_str("locale_accept_from_http()"),
            ])),
        ])),
        "numfmt_set_symbol" => Some((t_mixed(), vec![
            s_assign("_handle", e_call("__elephc_intl_handle_arg", vec![e_var("formatter")])),
            s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("numfmt"), e_str("numfmt_set_symbol"),
                e_array(vec![e_var("_handle"), e_var("symbol"), e_var("value")]),
            ])),
            s_return(e_call("__elephc_intl_handle_fail", vec![
                e_var("_handle"), e_var("_result"), e_str("numfmt_set_symbol()"),
            ])),
        ])),
        "numfmt_set_pattern" => Some((t_mixed(), vec![
            s_assign("_handle", e_call("__elephc_intl_handle_arg", vec![e_var("formatter")])),
            s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("numfmt"), e_str("numfmt_set_pattern"),
                e_array(vec![e_var("_handle"), e_var("pattern")]),
            ])),
            s_return(e_call("__elephc_intl_handle_fail", vec![
                e_var("_handle"), e_var("_result"), e_str("numfmt_set_pattern()"),
            ])),
        ])),
        "resourcebundle_get" => Some((t_mixed(), vec![
            s_assign("_handle", e_call("__elephc_intl_handle_arg", vec![e_var("bundle")])),
            s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("resourcebundle"), e_str("resourcebundle_get"),
                e_array(vec![e_var("_handle"), e_var("index"), e_var("fallback")]),
            ])),
            s_assign("_checked", e_call("__elephc_intl_handle_fail", vec![
                e_var("_handle"), e_var("_result"), e_str("resourcebundle_get()"),
            ])),
            // A failed lookup is a null PHP result, not false.
            s_if(e_binop(
                e_binop(e_var("_checked"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)),
                BinOp::And,
                e_binop(e_call("__elephc_intl_invoke_int", vec![
                    e_str("intl"), e_str("intl_get_error_code"), e_array(vec![]),
                ]), BinOp::StrictNotEq, e_int(0)),
            ), vec![s_return(e_null())], vec![], None),
            s_return(e_var("_checked")),
        ])),
        "datefmt_format" => Some((t_union(vec![TypeExpr::Str, TypeExpr::False]), vec![
            s_if(
                e_binop(
                    e_binop(
                        e_binop(e_not(e_instance_of(e_var("datetime"), "IntlCalendar")), BinOp::And,
                                e_not(e_instance_of(e_var("datetime"), "DateTimeInterface"))),
                        BinOp::And,
                        e_not(e_call("is_array", vec![e_var("datetime")])),
                    ),
                    BinOp::And,
                    e_not(e_binop(
                        e_binop(
                            e_binop(e_call("is_int", vec![e_var("datetime")]), BinOp::Or,
                                    e_call("is_float", vec![e_var("datetime")])),
                            BinOp::Or,
                            e_binop(e_call("is_string", vec![e_var("datetime")]), BinOp::And,
                                    e_call("is_numeric", vec![e_var("datetime")])),
                        ),
                        BinOp::And,
                        e_not(e_call("is_array", vec![e_var("datetime")])),
                    )),
                ),
                vec![
                    s_expr(e_call("__elephc_intl_invoke", vec![
                        e_str("intl"), e_str("intl_set_error"),
                        e_array(vec![e_int(1), e_str("datefmt_format(): invalid object type for date/time (only IntlCalendar and DateTimeInterface permitted)")]),
                    ])),
                    s_return(crate::synthetic_class::e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("formatter"), "format", vec![e_var("datetime")])),
        ])),
        "datefmt_format_object" => Some((t_union(vec![TypeExpr::Str, TypeExpr::False]), vec![
            s_return(e_static_call("IntlDateFormatter", "formatObject", args())),
        ])),
        "msgfmt_format_message" => Some((t_union(vec![TypeExpr::Str, TypeExpr::False]), vec![
            s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("msgfmt"), e_str("msgfmt_format_message"), e_array({
                    let mut values = args();
                    values.push(e_call("date_default_timezone_get", vec![]));
                    values
                }),
            ])),
            s_return(e_call("__elephc_intl_global_fail", vec![
                e_var("_result"), e_str("msgfmt_format_message()"),
            ])),
        ])),
        "msgfmt_parse_message" => Some((t_union(vec![t_array(), TypeExpr::False]), vec![
            s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("msgfmt"), e_str("msgfmt_parse_message"), e_array(args()),
            ])),
            s_return(e_call("__elephc_intl_global_fail", vec![
                e_var("_result"), e_str("msgfmt_parse_message()"),
            ])),
        ])),
        "msgfmt_set_pattern" => Some((t_mixed(), vec![
            s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("msgfmt"), e_str("msgfmt_set_pattern"),
                e_array(vec![
                    e_call("__elephc_intl_handle_arg", vec![e_var("formatter")]),
                    e_var("pattern"),
                ]),
            ])),
            s_return(e_call("__elephc_intl_handle_fail", vec![
                e_call("__elephc_intl_handle_arg", vec![e_var("formatter")]),
                e_var("_result"),
                e_str("msgfmt_set_pattern()"),
            ])),
        ])),
        "transliterator_transliterate" => Some((t_union(vec![TypeExpr::Str, TypeExpr::False]), vec![
            s_if(e_not(e_instance_of(e_var("transliterator"), "Transliterator")), vec![
                s_assign("_id", e_cast(CastType::String, e_var("transliterator"))),
                s_if(e_not(e_call("mb_check_encoding", vec![e_var("_id"), e_str("UTF-8")])), vec![
                    s_if(e_call("__elephc_intl_use_exceptions", vec![]), vec![
                        s_throw(e_new("IntlException", vec![e_str("transliterator_transliterate(): String conversion of id to UTF-16 failed")])),
                    ], vec![], None),
                    s_return(crate::synthetic_class::e_bool(false)),
                ], vec![], None),
                s_assign("_created", e_static_call("Transliterator", "create", vec![e_var("_id")])),
                s_if(e_binop(e_var("_created"), BinOp::StrictEq, e_null()), vec![
                    s_assign("_message", e_concat_all(vec![
                        e_str("transliterator_transliterate(): unable to open ICU transliterator with id \""),
                        e_var("_id"),
                        e_str("\""),
                    ])),
                    s_if(e_call("__elephc_intl_use_exceptions", vec![]), vec![
                        s_throw(e_new("IntlException", vec![e_var("_message")])),
                    ], vec![], None),
                    s_return(crate::synthetic_class::e_bool(false)),
                ], vec![], None),
                s_assign("transliterator", e_var("_created")),
            ], vec![], None),
            s_return(e_call("__elephc_intl_transliterate", vec![
                e_var("transliterator"), e_var("string"), e_var("start"), e_var("end"),
                crate::synthetic_class::e_bool(false),
            ])),
        ])),
        "intlcal_get_now" => Some((TypeExpr::Float, vec![
            s_return(e_static_call("IntlCalendar", "getNow", vec![])),
        ])),
        "intlcal_get_available_locales" => Some((t_array(), vec![
            s_return(e_static_call("IntlCalendar", "getAvailableLocales", vec![])),
        ])),
        "intlcal_get_keyword_values_for_locale" => Some((t_class("IntlIterator"), vec![
            s_return(e_static_call("IntlCalendar", "getKeywordValuesForLocale", args())),
        ])),
        "intlcal_create_instance" => Some((t_class("IntlCalendar"), vec![
            s_if(e_not(e_call("__elephc_intl_timezone_valid", vec![e_var("timezone")])), vec![
                s_throw(e_new("IntlException", vec![e_str("intlcal_create_instance(): passed IntlTimeZone is not properly constructed")])),
            ], vec![], None),
            s_return(e_static_call("IntlCalendar", "createInstance", args())),
        ])),
        "intlcal_from_date_time" => Some((t_class("IntlCalendar"), vec![
            s_return(e_static_call("IntlCalendar", "fromDateTime", args())),
        ])),
        "intlcal_to_date_time" => Some((t_class("DateTime"), vec![
            s_return(e_method_call(e_var("calendar"), "toDateTime", vec![])),
        ])),
        "intlcal_clear" => Some((TypeExpr::Bool, vec![
            s_if(
                e_binop(
                    e_binop(e_var("field"), BinOp::StrictNotEq, e_null()),
                    BinOp::And,
                    e_binop(
                        e_binop(e_var("field"), BinOp::Lt, e_int(0)),
                        BinOp::Or,
                        e_binop(e_var("field"), BinOp::GtEq, e_class_const("IntlCalendar", "FIELD_FIELD_COUNT")),
                    ),
                ),
                vec![s_throw(e_new("ValueError", vec![e_str(
                    "intlcal_clear(): Argument #2 ($field) must be a valid field",
                )]))],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("calendar"), "clear", vec![e_var("field")])),
        ])),
        "intlcal_set_minimal_days_in_first_week" => Some((TypeExpr::Bool, vec![
            s_if(
                e_binop(
                    e_binop(e_var("days"), BinOp::Lt, e_int(1)),
                    BinOp::Or,
                    e_binop(e_var("days"), BinOp::Gt, e_int(7)),
                ),
                vec![s_throw(e_new("ValueError", vec![e_str(
                    "intlcal_set_minimal_days_in_first_week(): Argument #2 ($days) must be between 1 and 7",
                )]))],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("calendar"), "setMinimalDaysInFirstWeek", vec![e_var("days")])),
        ])),
        "intlcal_set" => Some((TypeExpr::Bool, vec![
            s_expr(e_call("trigger_error", vec![
                e_str("Function intlcal_set() is deprecated since 8.4, use IntlCalendar::set(), IntlCalendar::setDate(), or IntlCalendar::setDateTime() instead"),
                e_const("E_USER_DEPRECATED"),
            ])),
            s_assign("_count", e_call("func_num_args", vec![])),
            s_if(e_binop(e_var("_count"), BinOp::StrictEq, e_int(3)), vec![
                s_if(e_binop(
                    e_binop(e_var("year"), BinOp::Lt, e_int(0)),
                    BinOp::Or,
                    e_binop(e_var("year"), BinOp::GtEq, e_class_const("IntlCalendar", "FIELD_FIELD_COUNT")),
                ), vec![s_throw(e_new("ValueError", vec![e_str("intlcal_set(): Argument #2 ($year) must be a valid field")]))], vec![], None),
                s_if(e_binop(
                    e_binop(e_var("month"), BinOp::Lt, e_int(-2_147_483_648)),
                    BinOp::Or,
                    e_binop(e_var("month"), BinOp::Gt, e_int(2_147_483_647)),
                ), vec![s_throw(e_new("ValueError", vec![e_str("intlcal_set(): Argument #3 ($month) must be between -2147483648 and 2147483647")]))], vec![], None),
                s_return(e_method_call(e_var("calendar"), "set", vec![e_var("year"), e_var("month")])),
            ], vec![], None),
            s_if(e_binop(e_var("_count"), BinOp::StrictEq, e_int(5)), vec![
                s_throw(e_new("ArgumentCountError", vec![e_str("IntlCalendar::set() has no variant with exactly 4 parameters")])),
            ], vec![], None),
            s_if(e_binop(e_var("_count"), BinOp::StrictEq, e_int(4)), vec![
                int32_guard(e_var("year"), "intlcal_set()", 2, "year"),
                int32_guard(e_var("month"), "intlcal_set()", 3, "month"),
                int32_guard(e_var("dayOfMonth"), "intlcal_set()", 4, "dayOfMonth"),
                s_expr(e_method_call(e_var("calendar"), "setDate", vec![e_var("year"), e_var("month"), e_var("dayOfMonth")])),
            ], vec![], None),
            s_if(e_binop(e_var("_count"), BinOp::StrictEq, e_int(6)), vec![
                int32_guard(e_var("year"), "intlcal_set()", 2, "year"),
                int32_guard(e_var("month"), "intlcal_set()", 3, "month"),
                int32_guard(e_var("dayOfMonth"), "intlcal_set()", 4, "dayOfMonth"),
                int32_guard(e_var("hour"), "intlcal_set()", 5, "hour"),
                int32_guard(e_var("minute"), "intlcal_set()", 6, "minute"),
                s_expr(e_method_call(e_var("calendar"), "setDateTime", vec![e_var("year"), e_var("month"), e_var("dayOfMonth"), e_var("hour"), e_var("minute")])),
            ], vec![], Some(vec![
                int32_guard(e_var("year"), "intlcal_set()", 2, "year"),
                int32_guard(e_var("month"), "intlcal_set()", 3, "month"),
                int32_guard(e_var("dayOfMonth"), "intlcal_set()", 4, "dayOfMonth"),
                int32_guard(e_var("hour"), "intlcal_set()", 5, "hour"),
                int32_guard(e_var("minute"), "intlcal_set()", 6, "minute"),
                int32_guard(e_var("second"), "intlcal_set()", 7, "second"),
                s_expr(e_method_call(e_var("calendar"), "setDateTime", vec![e_var("year"), e_var("month"), e_var("dayOfMonth"), e_var("hour"), e_var("minute"), e_var("second")])),
            ])),
            s_return(crate::synthetic_class::e_bool(true)),
        ])),
        "intlcal_roll" => Some((TypeExpr::Bool, vec![
            s_if(e_call("is_bool", vec![e_var("value")]), vec![
                s_expr(e_call("trigger_error", vec![
                    e_str("intlcal_roll(): Passing bool is deprecated, use 1 or -1 instead"),
                    e_const("E_USER_DEPRECATED"),
                ])),
                s_assign("value", e_ternary(e_var("value"), e_int(1), e_int(-1))),
            ], vec![], None),
            s_return(e_method_call(e_var("calendar"), "roll", vec![e_var("field"), e_var("value")])),
        ])),
        "intlcal_get_type" => Some((TypeExpr::Str, vec![
            s_return(e_method_call(e_var("calendar"), "getType", vec![])),
        ])),
        "intlcal_get_error_code" => Some((TypeExpr::Int, vec![
            s_return(e_method_call(e_var("calendar"), "getErrorCode", vec![])),
        ])),
        "intltz_from_date_time_zone" => Some((t_class("IntlTimeZone"), vec![
            s_assign("_object", e_static_call("IntlTimeZone", "fromDateTimeZone", vec![e_var("timezone")])),
            s_if(e_binop(e_var("_object"), BinOp::StrictEq, e_null()), vec![
                s_throw(e_new("IntlException", vec![e_str("IntlTimeZone::fromDateTimeZone(): time zone creation failed")])),
            ], vec![], None),
            s_return(e_var("_object")),
        ])),
        "intltz_to_date_time_zone" => Some((t_union(vec![t_class("DateTimeZone"), TypeExpr::False]), vec![
            s_return(e_method_call(e_var("timezone"), "toDateTimeZone", vec![])),
        ])),
        "intltz_create_enumeration" => Some((t_class("IntlIterator"), vec![
            s_assign("_items", e_call("__elephc_intl_invoke_array", vec![
                e_str("timezone"), e_str("intltz_create_enumeration"), e_array(args()),
            ])),
            s_return(e_static_call("IntlIterator", "__elephc_from_array", vec![e_var("_items")])),
        ])),
        "intltz_create_time_zone_id_enumeration" => Some((t_class("IntlIterator"), vec![
            s_assign("_items", e_call("__elephc_intl_invoke_array", vec![
                e_str("timezone"), e_str("intltz_create_time_zone_id_enumeration"), e_array(args()),
            ])),
            s_return(e_static_call("IntlIterator", "__elephc_from_array", vec![e_var("_items")])),
        ])),        "intltz_create_time_zone" => Some((t_class("IntlTimeZone"), vec![
            s_assign("_object", e_static_call("IntlTimeZone", "createTimeZone", vec![e_var("timezoneId")])),
            s_if(e_binop(e_var("_object"), BinOp::StrictEq, e_null()), vec![
                s_throw(e_new("IntlException", vec![e_str("IntlTimeZone::createTimeZone(): time zone creation failed")])),
            ], vec![], None),
            s_return(e_var("_object")),
        ])),
        "intltz_create_default" => Some((t_class("IntlTimeZone"), vec![
            s_return(e_static_call("IntlTimeZone", "createDefault", vec![])),
        ])),
        "intltz_get_gmt" => Some((t_class("IntlTimeZone"), vec![
            s_return(e_static_call("IntlTimeZone", "getGMT", vec![])),
        ])),
        "intltz_get_unknown" => Some((t_class("IntlTimeZone"), vec![
            s_return(e_static_call("IntlTimeZone", "getUnknown", vec![])),
        ])),
        "datefmt_create" => Some((t_nullable(t_class("IntlDateFormatter")), vec![
            s_if(
                e_binop(
                    e_binop(e_var("locale"), BinOp::StrictNotEq, e_null()),
                    BinOp::And,
                    e_binop(
                        e_binop(e_var("locale"), BinOp::StrictNotEq, e_str("")),
                        BinOp::And,
                        e_not(e_call("__elephc_intl_locale_valid", vec![e_var("locale")])),
                    ),
                ),
                vec![s_throw(e_new("ValueError", vec![e_concat_all(vec![
                    e_str("datefmt_create(): Argument #1 ($locale) \""),
                    e_var("locale"), e_str("\" is invalid"),
                ])]))],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_call("is_int", vec![e_var("calendar")]),
                    BinOp::And,
                    e_binop(
                        e_binop(e_var("calendar"), BinOp::StrictNotEq, e_int(0)),
                        BinOp::And,
                        e_binop(e_var("calendar"), BinOp::StrictNotEq, e_int(1)),
                    ),
                ),
                vec![
                    s_assign("_message", e_str("datefmt_create(): Invalid value for calendar type; it must be one of IntlDateFormatter::TRADITIONAL (locale's default calendar) or IntlDateFormatter::GREGORIAN. Alternatively, it can be an IntlCalendar object")),
                    s_expr(e_call("__elephc_intl_invoke", vec![
                        e_str("intl"), e_str("intl_set_error"),
                        e_array(vec![e_int(1), e_var("_message")]),
                    ])),
                    s_if(e_call("__elephc_intl_use_exceptions", vec![]), vec![
                        s_throw(e_new("IntlException", vec![e_var("_message")])),
                    ], vec![], None),
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_assign("_valid_date", e_binop(
                e_binop(
                    e_binop(e_var("dateType"), BinOp::StrictEq, e_class_const("IntlDateFormatter", "PATTERN")),
                    BinOp::Or,
                    e_binop(e_var("dateType"), BinOp::StrictEq, e_class_const("IntlDateFormatter", "NONE")),
                ),
                BinOp::Or,
                e_binop(
                    e_binop(e_var("dateType"), BinOp::GtEq, e_class_const("IntlDateFormatter", "FULL")),
                    BinOp::And,
                    e_binop(e_var("dateType"), BinOp::LtEq, e_class_const("IntlDateFormatter", "SHORT")),
                ),
            )),
            s_if(e_not(e_var("_valid_date")), vec![
                s_assign("_message", e_str("datefmt_create(): invalid date format style")),
                s_expr(e_call("__elephc_intl_invoke", vec![
                    e_str("intl"), e_str("intl_set_error"),
                    e_array(vec![e_int(1), e_var("_message")]),
                ])),
                s_if(e_call("__elephc_intl_use_exceptions", vec![]), vec![
                    s_throw(e_new("IntlException", vec![e_var("_message")])),
                ], vec![], None),
                s_return(e_null()),
            ], vec![], None),
            s_assign("_valid_time", e_binop(
                e_binop(
                    e_binop(e_var("timeType"), BinOp::StrictEq, e_class_const("IntlDateFormatter", "PATTERN")),
                    BinOp::Or,
                    e_binop(e_var("timeType"), BinOp::StrictEq, e_class_const("IntlDateFormatter", "NONE")),
                ),
                BinOp::Or,
                e_binop(
                    e_binop(e_var("timeType"), BinOp::GtEq, e_class_const("IntlDateFormatter", "FULL")),
                    BinOp::And,
                    e_binop(e_var("timeType"), BinOp::LtEq, e_class_const("IntlDateFormatter", "SHORT")),
                ),
            )),
            s_if(e_not(e_var("_valid_time")), vec![
                s_assign("_message", e_str("datefmt_create(): invalid time format style")),
                s_expr(e_call("__elephc_intl_invoke", vec![
                    e_str("intl"), e_str("intl_set_error"),
                    e_array(vec![e_int(1), e_var("_message")]),
                ])),
                s_if(e_call("__elephc_intl_use_exceptions", vec![]), vec![
                    s_throw(e_new("IntlException", vec![e_var("_message")])),
                ], vec![], None),
                s_return(e_null()),
            ], vec![], None),
            s_assign("_zone", e_call("__elephc_intl_date_zone", vec![e_var("timezone")])),
            s_expr(e_call("__elephc_intl_validate_timezone", vec![
                e_var("_zone"), e_str("datefmt_create()"),
            ])),
            s_assign("_pattern_text", e_ternary(
                e_binop(e_var("pattern"), BinOp::StrictEq, e_null()), e_str(""), e_var("pattern"),
            )),
            s_expr(e_call("__elephc_intl_validate_pattern", vec![
                e_var("_pattern_text"), e_str("datefmt_create()"),
            ])),
            s_assign("_object", e_static_call("IntlDateFormatter", "create", args())),
            s_if(e_binop(e_var("_object"), BinOp::StrictEq, e_null()), vec![
                s_throw(e_new("IntlException", vec![e_str("IntlDateFormatter::create(): date formatter creation failed")])),
            ], vec![], None),
            s_return(e_var("_object")),
        ])),
        "collator_create" => Some((t_nullable(t_class("Collator")), vec![
            s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("collator"), e_str("collator_create"), e_array(vec![e_var("locale")]),
            ])),
            s_if(e_instance_of(e_var("_result"), "Collator"), vec![
                s_return(e_var("_result")),
            ], vec![], None),
            s_expr(e_call("__elephc_intl_global_fail", vec![
                e_var("_result"), e_str("collator_create()"),
            ])),
            s_return(e_null()),
        ])),
        "numfmt_create" => Some((t_class("NumberFormatter"), vec![
            s_assign("_object", e_static_call("NumberFormatter", "create", args())),
            s_if(e_binop(e_var("_object"), BinOp::StrictEq, e_null()), vec![
                s_throw(e_new("IntlException", vec![e_str("NumberFormatter::create(): unable to create formatter")])),
            ], vec![], None),
            s_return(e_var("_object")),
        ])),
        "msgfmt_create" => Some((t_class("MessageFormatter"), vec![
            s_assign("_object", e_static_call("MessageFormatter", "create", args())),
            s_if(e_binop(e_var("_object"), BinOp::StrictEq, e_null()), vec![
                s_throw(e_new("IntlException", vec![e_str("MessageFormatter::create(): message formatter creation failed")])),
            ], vec![], None),
            s_return(e_var("_object")),
        ])),
        "resourcebundle_create" => Some((t_nullable(t_class("ResourceBundle")), vec![
            s_assign("_object", e_static_call("ResourceBundle", "create", args())),
            s_return(e_var("_object")),
        ])),        "intlcal_get_error_message" => Some((TypeExpr::Str, vec![
            s_return(e_method_call(e_var("calendar"), "getErrorMessage", vec![])),
        ])),
        "intlgregcal_create_instance" => Some((t_class("IntlGregorianCalendar"), vec![
            s_expr(e_call("trigger_error", vec![
                e_str("Function intlgregcal_create_instance() is deprecated since 8.4, use IntlGregorianCalendar::__construct(), IntlGregorianCalendar::createFromDate(), or IntlGregorianCalendar::createFromDateTime() instead"),
                e_const("E_USER_DEPRECATED"),
            ])),
            s_if(e_call("is_int", vec![e_var("timezoneOrYear")]), vec![
                s_if(e_binop(e_var("timezoneOrYear"), BinOp::StrictNotEq, e_null()), vec![int32_guard(e_cast(CastType::Int, e_var("timezoneOrYear")), "intlgregcal_create_instance()", 1, "timezoneOrYear")], vec![], None),
                s_if(e_binop(e_var("localeOrMonth"), BinOp::StrictNotEq, e_null()), vec![int32_guard(e_cast(CastType::Int, e_var("localeOrMonth")), "intlgregcal_create_instance()", 2, "localeOrMonth")], vec![], None),
                s_if(e_binop(e_var("day"), BinOp::StrictNotEq, e_null()), vec![int32_guard(e_cast(CastType::Int, e_var("day")), "intlgregcal_create_instance()", 3, "day")], vec![], None),
                s_if(e_binop(e_var("hour"), BinOp::StrictNotEq, e_null()), vec![int32_guard(e_cast(CastType::Int, e_var("hour")), "intlgregcal_create_instance()", 4, "hour")], vec![], None),
                s_if(e_binop(e_var("minute"), BinOp::StrictNotEq, e_null()), vec![int32_guard(e_cast(CastType::Int, e_var("minute")), "intlgregcal_create_instance()", 5, "minute")], vec![], None),
                s_if(e_binop(e_var("second"), BinOp::StrictNotEq, e_null()), vec![int32_guard(e_cast(CastType::Int, e_var("second")), "intlgregcal_create_instance()", 6, "second")], vec![], None),
                s_assign("_object", e_new("IntlGregorianCalendar", vec![e_null(), e_null()])),
                s_expr(e_method_call(e_var("_object"), "setDateTime", vec![
                    e_cast(CastType::Int, e_var("timezoneOrYear")),
                    e_cast(CastType::Int, e_var("localeOrMonth")),
                    e_ternary(e_binop(e_var("day"), BinOp::StrictEq, e_null()), e_int(1), e_cast(CastType::Int, e_var("day"))),
                    e_ternary(e_binop(e_var("hour"), BinOp::StrictEq, e_null()), e_int(0), e_cast(CastType::Int, e_var("hour"))),
                    e_ternary(e_binop(e_var("minute"), BinOp::StrictEq, e_null()), e_int(0), e_cast(CastType::Int, e_var("minute"))),
                    e_ternary(e_binop(e_var("second"), BinOp::StrictEq, e_null()), e_int(0), e_cast(CastType::Int, e_var("second"))),
                ])),
                s_return(e_var("_object")),
            ], vec![], Some(vec![
                s_return(e_new("IntlGregorianCalendar", vec![e_var("timezoneOrYear"), e_var("localeOrMonth")])),
            ])),
        ])),
        _ => None,
    }
}

/// Applies one generated parameter to a free-function builder.
fn apply_param(builder: FunctionBuilder, param: &IntlParam) -> FunctionBuilder {
    let ty = type_from_str(param.ty);
    let default = param.default.map(default_from_str);
    match (ty, default, param.by_ref) {
        (Some(ty), Some(default), true) => builder.param_by_ref_default(param.name, Some(ty), default),
        (Some(ty), Some(default), false) => builder.param_default(param.name, ty, default),
        (Some(ty), None, true) => builder.param_by_ref(param.name, Some(ty)),
        (Some(ty), None, false) => builder.param(param.name, ty),
        (None, Some(default), true) => builder.param_by_ref_default(param.name, None, default),
        (None, Some(default), false) => builder.param_untyped_default(param.name, default),
        (None, None, true) => builder.param_by_ref(param.name, None),
        (None, None, false) => builder.param_untyped(param.name),
    }
}

/// Returns whether a declared PHP type mentions an Intl object.
fn has_object_type(ty: &str) -> bool {
    OBJECT_TYPES.iter().any(|name| {
        ty.split('|').any(|part| part.trim().trim_start_matches('?') == *name)
    })
}

/// Converts one stub type spelling into the AST type expression the parser would build.
fn type_from_str(ty: &str) -> Option<TypeExpr> {
    let ty = ty.trim();
    if ty.is_empty() {
        return None;
    }
    if let Some(inner) = ty.strip_prefix('?') {
        return Some(t_nullable(type_from_str(inner)?));
    }
    let mut members = Vec::new();
    let mut null_count = 0;
    for part in ty.split('|') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if part == "null" {
            null_count += 1;
            continue;
        }
        members.push(match part {
            "int" => TypeExpr::Int,
            "float" => TypeExpr::Float,
            "string" => TypeExpr::Str,
            "bool" => TypeExpr::Bool,
            "false" => TypeExpr::False,
            "true" => TypeExpr::Bool,
            "void" => TypeExpr::Void,
            "array" => t_array(),
            "mixed" => t_mixed(),
            "callable" => TypeExpr::CallableSig { params: Vec::new(), ret: Box::new(t_mixed()) },
            other => t_class(other),
        });
    }
    if null_count > 0 && !members.is_empty() {
        if members.len() == 1 {
            return Some(t_nullable(members.pop().expect("non-null member")));
        }
        members.push(TypeExpr::Void);
    }
    Some(match members.len() {
        0 => t_mixed(),
        1 => members.pop().expect("one member"),
        _ => t_union(members),
    })
}

/// Converts one stub default spelling into the AST expression the parser would build.
fn default_from_str(default: &str) -> Expr {
    let default = default.trim();
    match default {
        "null" => e_null(),
        "true" => crate::synthetic_class::e_bool(true),
        "false" => crate::synthetic_class::e_bool(false),
        "[]" => e_array(Vec::new()),
        "\"\"" | "''" => e_str(""),
        "UNKNOWN" => e_null(),
        value if value.parse::<i64>().is_ok() => e_int(value.parse().expect("integer")),
        value if value.parse::<f64>().is_ok() => e_float(value.parse().expect("float")),
        value if value.contains("::") => {
            let mut parts = value.split("::");
            let class = parts.next().unwrap_or_default();
            let name = parts.next().unwrap_or_default();
            e_class_const(class, name)
        }
        value => e_const(value),
    }
}

/// Returns the class names whose declarations this module injects.
const CLASS_NAMES: &[&str] = &[
    "IntlException", "IntlDateFormatter", "IntlTimeZone", "IntlCalendar",
    "IntlGregorianCalendar", "Locale", "Normalizer", "Collator", "NumberFormatter",
    "MessageFormatter", "Transliterator", "Spoofchecker", "ResourceBundle", "UConverter",
    "IntlBreakIterator", "IntlRuleBasedBreakIterator", "IntlCodePointBreakIterator",
    "IntlPartsIterator", "IntlIterator", "IntlListFormatter", "IntlDatePatternGenerator",
    "IntlChar",
];

/// PHP-visible object classes that cross the bridge.
const OBJECT_TYPES: &[&str] = &[
    "IntlCalendar", "IntlGregorianCalendar", "IntlTimeZone", "IntlDateFormatter",
    "NumberFormatter", "Collator", "MessageFormatter", "Transliterator", "Spoofchecker",
    "ResourceBundle", "UConverter", "IntlIterator", "IntlBreakIterator",
    "IntlRuleBasedBreakIterator", "IntlCodePointBreakIterator", "IntlPartsIterator",
    "IntlChar", "DateTime", "DateTimeZone", "DateTimeInterface",
];

/// ICU status constants PHP exposes through ext/intl.
const INTL_STATUS_CONSTANTS: &[(&str, i64)] = &[
    ("U_ILLEGAL_ARGUMENT_ERROR", 1),
    ("U_INVALID_FORMAT_ERROR", 3),
    ("U_FILE_ACCESS_ERROR", 4),
    ("U_INTERNAL_PROGRAM_ERROR", 5),
    ("U_MESSAGE_PARSE_ERROR", 6),
    ("U_MEMORY_ALLOCATION_ERROR", 7),
    ("U_INDEX_OUTOFBOUNDS_ERROR", 8),
    ("U_PARSE_ERROR", 9),
    ("U_INVALID_CHAR_FOUND", 10),
    ("U_INVALID_TABLE_FORMAT", 11),
    ("U_INVALID_TABLE_FILE", 12),
    ("U_BUFFER_OVERFLOW_ERROR", 15),
    ("U_UNSUPPORTED_ERROR", 16),
    ("U_RESOURCE_TYPE_MISMATCH", 17),
    ("U_ILLEGAL_ESCAPE_SEQUENCE", 18),
    ("U_UNSUPPORTED_ESCAPE_SEQUENCE", 19),
    ("U_NO_SPACE_AVAILABLE", 20),
    ("U_CE_NOT_FOUND_ERROR", 21),
    ("U_PRIMARY_TOO_LONG_ERROR", 22),
    ("U_STATE_TOO_OLD_ERROR", 23),
    ("U_ENUM_OUT_OF_SYNC_ERROR", 25),
    ("U_INVARIANT_CONVERSION_ERROR", 26),
    ("U_INVALID_STATE_ERROR", 27),
    ("U_COLLATOR_VERSION_MISMATCH", 28),
    ("U_USELESS_COLLATOR_ERROR", 29),
    ("U_NO_WRITE_PERMISSION", 30),
    ("U_STANDARD_ERROR_LIMIT", 32),
    ("U_BAD_VARIABLE_DEFINITION", 65536),
    ("U_PARSE_ERROR_START", 65536),
    ("U_MALFORMED_RULE", 65537),
    ("U_MALFORMED_SET", 65538),
    ("U_MALFORMED_SYMBOL_REFERENCE", 65539),
    ("U_MALFORMED_UNICODE_ESCAPE", 65540),
    ("U_MALFORMED_VARIABLE_DEFINITION", 65541),
    ("U_MALFORMED_VARIABLE_REFERENCE", 65542),
    ("U_MISMATCHED_SEGMENT_DELIMITERS", 65543),
    ("U_MISPLACED_ANCHOR_START", 65544),
    ("U_MISPLACED_CURSOR_OFFSET", 65545),
    ("U_MISPLACED_QUANTIFIER", 65546),
    ("U_MISSING_OPERATOR", 65547),
    ("U_MISSING_SEGMENT_CLOSE", 65548),
    ("U_MULTIPLE_ANTE_CONTEXTS", 65549),
    ("U_MULTIPLE_CURSORS", 65550),
    ("U_MULTIPLE_POST_CONTEXTS", 65551),
    ("U_TRAILING_BACKSLASH", 65552),
    ("U_UNDEFINED_SEGMENT_REFERENCE", 65553),
    ("U_UNDEFINED_VARIABLE", 65554),
    ("U_UNQUOTED_SPECIAL", 65555),
    ("U_UNTERMINATED_QUOTE", 65556),
    ("U_RULE_MASK_ERROR", 65557),
    ("U_MISPLACED_COMPOUND_FILTER", 65558),
    ("U_MULTIPLE_COMPOUND_FILTERS", 65559),
    ("U_INVALID_RBT_SYNTAX", 65560),
    ("U_INVALID_PROPERTY_PATTERN", 65561),
    ("U_MALFORMED_PRAGMA", 65562),
    ("U_UNCLOSED_SEGMENT", 65563),
    ("U_ILLEGAL_CHAR_IN_SEGMENT", 65564),
    ("U_VARIABLE_RANGE_EXHAUSTED", 65565),
    ("U_VARIABLE_RANGE_OVERLAP", 65566),
    ("U_ILLEGAL_CHARACTER", 65567),
    ("U_INTERNAL_TRANSLITERATOR_ERROR", 65568),
    ("U_INVALID_ID", 65569),
    ("U_PARSE_ERROR_LIMIT", 65570),
    ("U_UNEXPECTED_TOKEN", 65792),
    ("U_FMT_PARSE_ERROR_START", 65792),
    ("U_MULTIPLE_DECIMAL_SEPARATORS", 65793),
    ("U_MULTIPLE_DECIMAL_SEPERATORS", 65793),
    ("U_MULTIPLE_EXPONENTIAL_SYMBOLS", 65794),
    ("U_MALFORMED_EXPONENTIAL_PATTERN", 65795),
    ("U_MULTIPLE_PERCENT_SYMBOLS", 65796),
    ("U_MULTIPLE_PERMILL_SYMBOLS", 65797),
    ("U_MULTIPLE_PAD_SPECIFIERS", 65798),
    ("U_PATTERN_SYNTAX_ERROR", 65799),
    ("U_ILLEGAL_PAD_POSITION", 65800),
    ("U_UNMATCHED_BRACES", 65801),
    ("U_UNSUPPORTED_PROPERTY", 65802),
    ("U_UNSUPPORTED_ATTRIBUTE", 65803),
    ("U_FMT_PARSE_ERROR_LIMIT", 65825),
    ("U_BRK_INTERNAL_ERROR", 66048),
    ("U_BRK_ERROR_START", 66048),
    ("U_BRK_HEX_DIGITS_EXPECTED", 66049),
    ("U_BRK_SEMICOLON_EXPECTED", 66050),
    ("U_BRK_RULE_SYNTAX", 66051),
    ("U_BRK_UNCLOSED_SET", 66052),
    ("U_BRK_ASSIGN_ERROR", 66053),
    ("U_BRK_VARIABLE_REDFINITION", 66054),
    ("U_BRK_MISMATCHED_PAREN", 66055),
    ("U_BRK_NEW_LINE_IN_QUOTED_STRING", 66056),
    ("U_BRK_UNDEFINED_VARIABLE", 66057),
    ("U_BRK_INIT_ERROR", 66058),
    ("U_BRK_RULE_EMPTY_SET", 66059),
    ("U_BRK_UNRECOGNIZED_OPTION", 66060),
    ("U_BRK_MALFORMED_RULE_TAG", 66061),
    ("U_BRK_ERROR_LIMIT", 66062),
    ("U_REGEX_INTERNAL_ERROR", 66304),
    ("U_REGEX_ERROR_START", 66304),
    ("U_REGEX_RULE_SYNTAX", 66305),
    ("U_REGEX_INVALID_STATE", 66306),
    ("U_REGEX_BAD_ESCAPE_SEQUENCE", 66307),
    ("U_REGEX_PROPERTY_SYNTAX", 66308),
    ("U_REGEX_UNIMPLEMENTED", 66309),
    ("U_REGEX_MISMATCHED_PAREN", 66310),
    ("U_REGEX_NUMBER_TOO_BIG", 66311),
    ("U_REGEX_BAD_INTERVAL", 66312),
    ("U_REGEX_MAX_LT_MIN", 66313),
    ("U_REGEX_INVALID_BACK_REF", 66314),
    ("U_REGEX_INVALID_FLAG", 66315),
    ("U_REGEX_LOOK_BEHIND_LIMIT", 66316),
    ("U_REGEX_SET_CONTAINS_STRING", 66317),
    ("U_REGEX_ERROR_LIMIT", 66318),
    ("U_IDNA_PROHIBITED_ERROR", 66560),
    ("U_IDNA_ERROR_START", 66560),
    ("U_IDNA_UNASSIGNED_ERROR", 66561),
    ("U_IDNA_CHECK_BIDI_ERROR", 66562),
    ("U_IDNA_STD3_ASCII_RULES_ERROR", 66563),
    ("U_IDNA_ACE_PREFIX_ERROR", 66564),
    ("U_IDNA_VERIFICATION_ERROR", 66565),
    ("U_IDNA_LABEL_TOO_LONG_ERROR", 66566),
    ("U_IDNA_ZERO_LENGTH_LABEL_ERROR", 66567),
    ("U_IDNA_DOMAIN_NAME_TOO_LONG_ERROR", 66568),
    ("U_IDNA_ERROR_LIMIT", 66569),
    ("U_STRINGPREP_PROHIBITED_ERROR", 66560),
    ("U_STRINGPREP_UNASSIGNED_ERROR", 66561),
    ("U_STRINGPREP_CHECK_BIDI_ERROR", 66562),
    ("U_PLUGIN_ERROR_START", 66816),
    ("U_PLUGIN_TOO_HIGH", 66816),
    ("U_PLUGIN_DIDNT_SET_LEVEL", 66817),
    ("U_PLUGIN_ERROR_LIMIT", 66818),
    ("U_ERROR_LIMIT", 66818),
    ("U_ERROR_WARNING_START", -128),
    ("U_USING_FALLBACK_WARNING", -128),
    ("U_USING_DEFAULT_WARNING", -127),
    ("U_SAFECLONE_ALLOCATED_WARNING", -126),
    ("U_STATE_OLD_WARNING", -125),
    ("U_STRING_NOT_TERMINATED_WARNING", -124),
    ("U_SORT_KEY_TOO_SHORT_WARNING", -123),
    ("U_AMBIGUOUS_ALIAS_WARNING", -122),
    ("U_DIFFERENT_UCA_VERSION", -121),
    ("U_PLUGIN_CHANGED_LEVEL_WARNING", -120),
    ("U_ZERO_ERROR", 0),
    ("U_ERROR_WARNING_LIMIT", -119),
];

/// Builds the synthetic Intl class declarations.
fn class_declarations() -> Vec<Stmt> {
    let mut declarations = Vec::new();
    declarations.push(crate::synthetic_class::class("IntlException").extends("Exception").build());
    declarations.push(build_locale_class());
    declarations.push(build_normalizer_class());
    declarations.push(build_date_formatter_class());
    declarations.push(build_iterator_class());
    declarations.push(build_time_zone_class());
    declarations.push(build_calendar_class());
    declarations.push(build_gregorian_calendar_class());
    declarations.push(build_collator_class());
    declarations.push(build_number_formatter_class());
    declarations.push(build_message_formatter_class());
    declarations.push(build_resource_bundle_class());
    declarations.push(build_spoofchecker_class());
    declarations.push(build_uconverter_class());
    declarations.push(build_transliterator_class());
    declarations.push(build_list_formatter_class());
    declarations.push(build_date_pattern_generator_class());
    // The break-iterator family is mutually referential: factories return the concrete
    // subclasses, `getPartsIterator()` returns `IntlPartsIterator`, and its
    // `getBreakIterator()` returns the base class. PHP resolves forward class references in
    // declarations, and ordering the subclasses first keeps the factory and parts-iterator
    // result types resolved instead of degrading to `mixed`.
    declarations.push(build_parts_iterator_class());
    declarations.push(build_code_point_break_iterator_class());
    declarations.push(build_rule_based_break_iterator_class());
    declarations.push(build_break_iterator_class());
    declarations.push(build_intl_char_class());
    declarations
}

/// Builds the static-only `IntlChar` class (constants plus the `u_char*` helpers).
fn build_intl_char_class() -> Stmt {
    let mut builder = class("IntlChar");
    for (constant, value) in constants_for("IntlChar") {
        builder = builder.constant(constant, match value {
            IntlConstValue::Int(value) => e_int(*value),
            IntlConstValue::Float(value) => e_float(*value),
            IntlConstValue::Str(value) => e_str(value),
            IntlConstValue::Bool(value) => crate::synthetic_class::e_bool(*value),
            IntlConstValue::Null => e_null(),
        });
    }
    let choice_default = e_class_const("IntlChar", "UNICODE_CHAR_NAME");
    builder
        .method(method("__construct").private().body(vec![]))
        .method(method("charFromName")
            .static_()
            .param("name", TypeExpr::Str)
            .param_default("nameChoice", TypeExpr::Int, choice_default.clone())
            .returns(t_mixed())
            .body(vec![
                s_assign("_result", e_call("__elephc_intl_invoke", vec![
                    e_str("char"), e_str("char_from_name"),
                    e_array(vec![e_var("name"), e_var("nameChoice")]),
                ])),
                // The bridge reports a missing name as a null payload, which the generic
                // invoke helper surfaces as `false`; PHP answers null here.
                s_if(e_binop(e_var("_result"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![
                    s_return(e_null()),
                ], vec![], None),
                s_return(e_var("_result")),
            ]))
        .method(method("charName")
            .static_()
            .param("codepoint", t_mixed())
            .param_default("nameChoice", TypeExpr::Int, choice_default)
            .returns(t_mixed())
            .body(vec![
                s_assign("_result", e_call("__elephc_intl_invoke", vec![
                    e_str("char"), e_str("char_name"),
                    e_array(vec![e_var("codepoint"), e_var("nameChoice")]),
                ])),
                s_if(e_binop(e_var("_result"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![
                    s_return(e_null()),
                ], vec![], None),
                s_return(e_var("_result")),
            ]))
        .method(method("digit")
            .static_()
            .param("codepoint", t_mixed())
            .param_default("radix", TypeExpr::Int, e_int(10))
            .returns(t_mixed())
            .body(vec![s_return(e_call("__elephc_intl_invoke", vec![
                e_str("char"), e_str("digit"),
                e_array(vec![e_var("codepoint"), e_var("radix")]),
            ]))]))
        .method(method("forDigit")
            .static_()
            .param("digit", TypeExpr::Int)
            .param_default("radix", TypeExpr::Int, e_int(10))
            .returns(t_mixed())
            .body(vec![s_return(e_call("__elephc_intl_invoke", vec![
                e_str("char"), e_str("for_digit"),
                e_array(vec![e_var("digit"), e_var("radix")]),
            ]))]))
        .build()
}

/// Builds a `__clone()` method that duplicates the ICU handle held by a wrapper.
///
/// An unconstructed wrapper (the common `class MyZone extends IntlTimeZone { function
/// __construct() {} }` case) has no ICU object to clone; PHP reports that before any property
/// copying happens.
fn clone_handle_method(class_name: &str) -> crate::synthetic_class::MethodBuilder {
    method("__clone").body(vec![
        s_if(
            e_binop(e_this_prop("__elephc_handle"), BinOp::StrictEq, e_int(0)),
            vec![s_throw(e_new("Error", vec![e_str(&format!(
                "Cannot clone uninitialized {class_name}",
            ))]))],
            vec![],
            None,
        ),
        s_prop_assign(
            e_var("this"),
            "__elephc_handle",
            e_call("elephc_intl_clone", vec![e_this_prop("__elephc_handle")]),
        ),
    ])
}


/// Builds the static-only `Locale` class.
fn build_locale_class() -> Stmt {
    let mut builder = class("Locale")
        .constant("ACTUAL_LOCALE", e_int(0))
        .constant("VALID_LOCALE", e_int(1))
        .constant("DEFAULT_LOCALE", e_null())
        .constant("LANG_TAG", e_str("language"))
        .constant("EXTLANG_TAG", e_str("extlang"))
        .constant("SCRIPT_TAG", e_str("script"))
        .constant("REGION_TAG", e_str("region"))
        .constant("VARIANT_TAG", e_str("variant"))
        .constant("GRANDFATHERED_LANG_TAG", e_str("grandfathered"))
        .constant("PRIVATE_TAG", e_str("private"));
    for (name, params) in [
        ("getDefault", vec![]),
        ("setDefault", vec![("locale", TypeExpr::Str, None, false)]),
        ("getPrimaryLanguage", vec![("locale", TypeExpr::Str, None, false)]),
        ("getScript", vec![("locale", TypeExpr::Str, None, false)]),
        ("getRegion", vec![("locale", TypeExpr::Str, None, false)]),
        ("getKeywords", vec![("locale", TypeExpr::Str, None, false)]),
        ("getDisplayScript", vec![("locale", TypeExpr::Str, None, false), ("displayLocale", t_nullable(TypeExpr::Str), Some(e_null()), false)]),
        ("getDisplayRegion", vec![("locale", TypeExpr::Str, None, false), ("displayLocale", t_nullable(TypeExpr::Str), Some(e_null()), false)]),
        ("getDisplayName", vec![("locale", TypeExpr::Str, None, false), ("displayLocale", t_nullable(TypeExpr::Str), Some(e_null()), false)]),
        ("getDisplayLanguage", vec![("locale", TypeExpr::Str, None, false), ("displayLocale", t_nullable(TypeExpr::Str), Some(e_null()), false)]),
        ("getDisplayVariant", vec![("locale", TypeExpr::Str, None, false), ("displayLocale", t_nullable(TypeExpr::Str), Some(e_null()), false)]),
        ("composeLocale", vec![("subtags", t_array(), None, false)]),
        ("parseLocale", vec![("locale", TypeExpr::Str, None, false)]),
        ("getAllVariants", vec![("locale", TypeExpr::Str, None, false)]),
        ("filterMatches", vec![("languageTag", TypeExpr::Str, None, false), ("locale", TypeExpr::Str, None, false), ("canonicalize", TypeExpr::Bool, Some(crate::synthetic_class::e_bool(false)), false)]),
        ("lookup", vec![("languageTag", t_array(), None, false), ("locale", TypeExpr::Str, None, false), ("canonicalize", TypeExpr::Bool, Some(crate::synthetic_class::e_bool(false)), false), ("defaultLocale", t_nullable(TypeExpr::Str), Some(e_null()), false)]),
        ("canonicalize", vec![("locale", TypeExpr::Str, None, false)]),
        ("acceptFromHttp", vec![("header", TypeExpr::Str, None, false)]),
        ("isRightToLeft", vec![("locale", TypeExpr::Str, None, false)]),
        ("addLikelySubtags", vec![("locale", t_union(vec![TypeExpr::Str, TypeExpr::False]), None, false)]),
        ("minimizeSubtags", vec![("locale", t_union(vec![TypeExpr::Str, TypeExpr::False]), None, false)]),
    ] {
        let args: Vec<Expr> = params.iter().map(|(param, _, _, _)| e_var(param)).collect();
        let mut body = Vec::new();
        for (index, (param, ty, _, _)) in params.iter().enumerate() {
            if matches!(ty, TypeExpr::Str | TypeExpr::Nullable(_)) {
                body.push(s_expr(e_call("__elephc_intl_no_null", vec![
                    e_var(param), e_str(&format!("Locale::{name}")), e_int(index as i64 + 1),
                    e_str(param),
                ])));
            }
        }
        if name == "lookup" {
            body.push(s_expr(e_call("__elephc_intl_lookup_tags_check", vec![
                e_var("languageTag"), e_str("Locale::lookup"),
            ])));
        }
        if name == "composeLocale" {
            body.push(s_if(
                e_not(call_isset(e_index(e_var("subtags"), e_str("language")))),
                vec![s_throw(e_new("ValueError", vec![e_str("Locale::composeLocale(): Argument #1 ($subtags) must contain a \"language\" key")]))],
                vec![],
                None,
            ));
        }
        if name == "addLikelySubtags" || name == "minimizeSubtags" {
            body.push(s_if(e_binop(e_var("locale"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![
                s_assign("locale", e_str("")),
            ], vec![], None));
            body.push(s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("locale"), e_str(locale_bridge_method(name)), e_array(args),
            ])));
            body.push(s_return(e_call("__elephc_intl_global_fail", vec![
                e_var("_result"), e_str(&format!("Locale::{name}()")),
            ])));
            builder = builder.method(static_method_typed(name, params, t_union(vec![TypeExpr::Str, TypeExpr::False]), body));
        } else {
            body.push(s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("locale"), e_str(locale_bridge_method(name)), e_array(args),
            ])));
            body.push(s_return(e_call("__elephc_intl_global_fail", vec![
                e_var("_result"), e_str(&format!("Locale::{name}()")),
            ])));
            builder = builder.method(static_method(name, params, body));
        }
    }
    builder.build()
}

/// Builds the static-only `Normalizer` class.
fn build_normalizer_class() -> Stmt {
    let mut builder = class("Normalizer")
        .constant("FORM_D", e_int(4))
        .constant("NFD", e_int(4))
        .constant("FORM_KD", e_int(8))
        .constant("NFKD", e_int(8))
        .constant("FORM_C", e_int(16))
        .constant("NFC", e_int(16))
        .constant("FORM_KC", e_int(32))
        .constant("NFKC", e_int(32))
        .constant("FORM_KC_CF", e_int(48))
        .constant("NFKC_CF", e_int(48));
    for name in ["normalize", "isNormalized", "getRawDecomposition"] {
        let params = vec![
            ("string", TypeExpr::Str, None, false),
            ("form", TypeExpr::Int, Some(e_int(16)), false),
        ];
        if name == "getRawDecomposition" {
            builder = builder.method(static_method(name, params, vec![
                s_assign("_result", e_call("__elephc_intl_invoke", vec![
                    e_str("normalizer"), e_str(normalizer_bridge_method(name)),
                    e_array(vec![e_var("string"), e_var("form")]),
                ])),
                s_return(e_call("__elephc_intl_global_fail", vec![
                    e_var("_result"), e_str("Normalizer::getRawDecomposition()"),
                ])),
            ]));
            continue;
        }
        builder = builder.method(static_method(name, params, bridge_static_body("normalizer", normalizer_bridge_method(name), vec![e_var("string"), e_var("form")])));
    }
    builder.build()
}

/// Builds `isset(<value>)` as the function call the parser emits.
fn call_isset(value: Expr) -> Expr {
    e_call("isset", vec![value])
}

/// Maps a `Locale` class method to its procedural bridge operation.
fn locale_bridge_method(name: &str) -> &str {
    match name {
        "getDefault" => "locale_get_default",
        "setDefault" => "locale_set_default",
        "getPrimaryLanguage" => "locale_get_primary_language",
        "getScript" => "locale_get_script",
        "getRegion" => "locale_get_region",
        "getKeywords" => "locale_get_keywords",
        "getDisplayScript" => "locale_get_display_script",
        "getDisplayRegion" => "locale_get_display_region",
        "getDisplayName" => "locale_get_display_name",
        "getDisplayLanguage" => "locale_get_display_language",
        "getDisplayVariant" => "locale_get_display_variant",
        "composeLocale" => "locale_compose",
        "parseLocale" => "locale_parse",
        "getAllVariants" => "locale_get_all_variants",
        "filterMatches" => "locale_filter_matches",
        "lookup" => "locale_lookup",
        "canonicalize" => "locale_canonicalize",
        "acceptFromHttp" => "locale_accept_from_http",
        "isRightToLeft" => "locale_is_right_to_left",
        "addLikelySubtags" => "locale_add_likely_subtags",
        "minimizeSubtags" => "locale_minimize_subtags",
        other => other,
    }
}

/// Maps a `Normalizer` class method to its procedural bridge operation.
fn normalizer_bridge_method(name: &str) -> &str {
    match name {
        "normalize" => "normalizer_normalize",
        "isNormalized" => "normalizer_is_normalized",
        "getRawDecomposition" => "normalizer_get_raw_decomposition",
        other => other,
    }
}

/// Builds a static method from a generic `(name, type, default, by_ref)` parameter list.
fn static_method(name: &str, params: Vec<(&str, TypeExpr, Option<Expr>, bool)>, body: Vec<Stmt>) -> crate::synthetic_class::MethodBuilder {
    let mut builder = method(name).static_();
    for (param, ty, default, by_ref) in params {
        builder = apply_method_param(builder, param, Some(ty), default, by_ref);
    }
    // An undeclared PHP return type is `mixed`; leaving it inferred from the bridge call would
    // make the syntactic fallback report `int` for every untyped Locale/magic helper.
    builder.returns(t_mixed()).body(body)
}

/// Builds a static method with an explicit return type.
fn static_method_typed(
    name: &str,
    params: Vec<(&str, TypeExpr, Option<Expr>, bool)>,
    returns: TypeExpr,
    body: Vec<Stmt>,
) -> crate::synthetic_class::MethodBuilder {
    static_method(name, params, body).returns(returns)
}

/// Applies one parameter to a method builder.
fn apply_method_param(
    builder: crate::synthetic_class::MethodBuilder,
    name: &str,
    ty: Option<TypeExpr>,
    default: Option<Expr>,
    by_ref: bool,
) -> crate::synthetic_class::MethodBuilder {
    match (ty, default, by_ref) {
        (Some(ty), Some(default), true) => builder.param_by_ref_default(name, Some(ty), default),
        (Some(ty), Some(default), false) => builder.param_default(name, ty, default),
        (Some(ty), None, true) => builder.param_by_ref(name, Some(ty)),
        (Some(ty), None, false) => builder.param(name, ty),
        (None, Some(default), true) => builder.param_by_ref_default(name, None, default),
        (None, Some(default), false) => builder.param_untyped_default(name, default),
        (None, None, true) => builder.param_by_ref(name, None),
        (None, None, false) => builder.param_untyped(name),
    }
}

/// Builds `return __elephc_intl_invoke(family, method, args);`.
fn bridge_static_body(family: &str, method_name: &str, args: Vec<Expr>) -> Vec<Stmt> {
    vec![s_return(e_call("__elephc_intl_invoke", vec![
        e_str(family), e_str(method_name), e_array(args),
    ]))]
}

/// Builds `IntlDateFormatter` with its constructor, factories, and ICU-backed methods.
fn build_date_formatter_class() -> Stmt {
    let mut builder = class("IntlDateFormatter")
        .prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)));
    for (constant, value) in constants_for("IntlDateFormatter") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .method(method("__construct")
            .param("locale", t_nullable(TypeExpr::Str))
            .param_default("dateType", TypeExpr::Int, e_int(0))
            .param_default("timeType", TypeExpr::Int, e_int(0))
            .param_default("timezone", t_union(vec![t_class("IntlTimeZone"), t_class("DateTimeZone"), TypeExpr::Str, TypeExpr::Void]), e_null())
            .param_default("calendar", t_mixed(), e_null())
            .param_default("pattern", t_nullable(TypeExpr::Str), e_null())
            .body(date_formatter_constructor_body()))
        .method(method("__elephc_from_handle")
            .static_()
            .param("handle", TypeExpr::Int)
            .returns(t_class("static"))
            .body(vec![
                s_assign("_object", e_new("IntlDateFormatter", vec![e_null(), e_int(0), e_int(0)])),
                s_assign("_dummy", e_prop(e_var("_object"), "__elephc_handle")),
                s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
                s_expr(e_call("elephc_intl_close", vec![e_var("_dummy")])),
                s_return(e_var("_object")),
            ]))
        .method(method("create")
            .static_()
            .param("locale", t_nullable(TypeExpr::Str))
            .param_default("dateType", TypeExpr::Int, e_int(0))
            .param_default("timeType", TypeExpr::Int, e_int(0))
            .param_default("timezone", t_union(vec![t_class("IntlTimeZone"), t_class("DateTimeZone"), TypeExpr::Str, TypeExpr::Void]), e_null())
            .param_default("calendar", t_union(vec![t_class("IntlCalendar"), TypeExpr::Int, TypeExpr::Void]), e_null())
            .param_default("pattern", t_nullable(TypeExpr::Str), e_null())
            .returns(t_nullable(t_class("IntlDateFormatter")))
            .body(vec![
                s_if(
                    e_binop(
                        e_binop(e_var("locale"), BinOp::StrictNotEq, e_null()),
                        BinOp::And,
                        e_binop(
                            e_binop(e_var("locale"), BinOp::StrictNotEq, e_str("")),
                            BinOp::And,
                            e_not(e_call("__elephc_intl_locale_valid", vec![e_var("locale")])),
                        ),
                    ),
                    vec![s_throw(e_new("ValueError", vec![e_concat_all(vec![
                        e_str("IntlDateFormatter::create(): Argument #1 ($locale) \""),
                        e_var("locale"), e_str("\" is invalid"),
                    ])]))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(
                        e_call("is_int", vec![e_var("calendar")]),
                        BinOp::And,
                        e_binop(
                            e_binop(e_var("calendar"), BinOp::StrictNotEq, e_int(0)),
                            BinOp::And,
                            e_binop(e_var("calendar"), BinOp::StrictNotEq, e_int(1)),
                        ),
                    ),
                    vec![
                        s_assign("_message", e_str("IntlDateFormatter::create(): Invalid value for calendar type; it must be one of IntlDateFormatter::TRADITIONAL (locale's default calendar) or IntlDateFormatter::GREGORIAN. Alternatively, it can be an IntlCalendar object")),
                        s_expr(e_call("__elephc_intl_invoke", vec![
                            e_str("intl"), e_str("intl_set_error"),
                            e_array(vec![e_int(1), e_var("_message")]),
                        ])),
                        s_if(e_call("__elephc_intl_use_exceptions", vec![]), vec![
                            s_throw(e_new("IntlException", vec![e_var("_message")])),
                        ], vec![], None),
                        s_return(e_null()),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_zone", e_call("__elephc_intl_date_zone", vec![e_var("timezone")])),
                s_expr(e_call("__elephc_intl_validate_timezone", vec![
                    e_var("_zone"), e_str("IntlDateFormatter::create()"),
                ])),
                s_assign("_pattern_text", e_ternary(
                    e_binop(e_var("pattern"), BinOp::StrictEq, e_null()), e_str(""), e_var("pattern"),
                )),
                s_expr(e_call("__elephc_intl_validate_pattern", vec![
                    e_var("_pattern_text"), e_str("IntlDateFormatter::create()"),
                ])),
                s_return(e_new("IntlDateFormatter", vec![
                    e_var("locale"), e_var("dateType"), e_var("timeType"),
                    e_var("timezone"), e_var("calendar"), e_var("pattern"),
                ])),
            ]));
    for (name, params, outputs) in [
        ("getDateType", vec![], vec![]),
        ("getTimeType", vec![], vec![]),
        ("getCalendar", vec![], vec![]),
        ("getPattern", vec![], vec![]),
        ("getLocale", vec![("type", TypeExpr::Int, Some(e_int(0)), false)], vec![]),
        ("format", vec![("datetime", t_mixed(), None, false)], vec![]),
        ("setCalendar", vec![("calendar", t_union(vec![t_class("IntlCalendar"), TypeExpr::Int, TypeExpr::Void]), None, false)], vec![]),
        ("getTimeZoneId", vec![], vec![]),
        ("getTimeZone", vec![], vec![]),
        ("setTimeZone", vec![("timezone", t_mixed(), None, false)], vec![]),
        ("setPattern", vec![("pattern", TypeExpr::Str, None, false)], vec![]),
        ("setLenient", vec![("lenient", TypeExpr::Bool, None, false)], vec![]),
        ("isLenient", vec![], vec![]),
        ("parse", vec![("string", TypeExpr::Str, None, false), ("offset", TypeExpr::Int, Some(e_int(0)), true)], vec!["offset"]),
        ("parseToCalendar", vec![("string", TypeExpr::Str, None, false), ("offset", TypeExpr::Int, Some(e_int(0)), true)], vec!["offset"]),
        ("localtime", vec![("string", TypeExpr::Str, None, false), ("offset", TypeExpr::Int, Some(e_int(0)), true)], vec!["offset"]),
        ("getErrorCode", vec![], vec![]),
        ("getErrorMessage", vec![], vec![]),
    ] {
        let body = date_formatter_method_body(name, &params, &outputs);
        builder = builder.method(instance_method(name, params, body));
    }
    builder = builder.method(method("getCalendarObject")
        .returns(t_class("IntlCalendar"))
        .body(vec![
            s_assign("_object", e_method_call(e_var("this"), "__elephc_call", vec![e_str("calendar_object"), e_array(vec![])])),
            s_if(e_instance_of(e_var("_object"), "IntlCalendar"), vec![s_return(e_var("_object"))], vec![], None),
            s_throw(e_new("IntlException", vec![e_str("IntlDateFormatter::getCalendarObject(): calendar creation failed")])),
        ]));
    builder = builder.method(method("formatObject")
        .static_()
        .param("datetime", t_mixed())
        .param_default("format", t_mixed(), e_null())
        .param_default("locale", t_nullable(TypeExpr::Str), e_null())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(date_formatter_format_object_body()));
    builder = builder.method(method("__elephc_call")
        .param("method", TypeExpr::Str)
        .param("args", t_mixed())
        .body(vec![s_return(e_call("__elephc_intl_call", vec![
            e_this_prop("__elephc_handle"), e_var("method"), e_var("args"),
        ]))]));
    builder = builder.method(clone_handle_method("IntlDateFormatter"));
    builder.build()
}

/// Builds the `IntlDateFormatter::__construct` body.
fn date_formatter_constructor_body() -> Vec<Stmt> {
    vec![
        s_if(e_binop(e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0)), vec![
            s_throw(e_new("IntlException", vec![e_str("IntlDateFormatter::__construct(): cannot call constructor twice")])),
        ], vec![], None),
        s_if(
            e_binop(
                e_binop(e_var("locale"), BinOp::StrictNotEq, e_null()),
                BinOp::And,
                e_binop(
                    e_binop(e_var("locale"), BinOp::StrictNotEq, e_str("")),
                    BinOp::And,
                    e_not(e_call("__elephc_intl_locale_valid", vec![e_var("locale")])),
                ),
            ),
            vec![s_throw(e_new("ValueError", vec![e_concat_all(vec![
                e_str("IntlDateFormatter::__construct(): Argument #1 ($locale) \""),
                e_var("locale"), e_str("\" is invalid"),
            ])]))],
            vec![],
            None,
        ),
        s_assign("_zone", e_str("")),
        s_if(e_binop(e_var("timezone"), BinOp::StrictEq, e_null()), vec![
            s_assign("_zone", e_call("date_default_timezone_get", vec![])),
        ], vec![], Some(vec![
            s_if(e_instance_of(e_var("timezone"), "DateTimeZone"), vec![
                s_assign("_zone", e_cast(CastType::String, e_method_call(e_var("timezone"), "getName", vec![]))),
            ], vec![], Some(vec![
                s_if(e_instance_of(e_var("timezone"), "IntlTimeZone"), vec![
                    s_assign("_zone", e_cast(CastType::String, e_method_call(e_var("timezone"), "getID", vec![]))),
                ], vec![], Some(vec![
                    s_assign("_zone", e_cast(CastType::String, e_var("timezone"))),
                ])),
            ])),
        ])),
        s_expr(e_call("__elephc_intl_validate_timezone", vec![
            e_var("_zone"), e_str("IntlDateFormatter::__construct()"),
        ])),
        s_if(
            e_binop(
                e_binop(e_var("calendar"), BinOp::StrictNotEq, e_null()),
                BinOp::And,
                e_binop(
                    e_not(e_call("is_int", vec![e_var("calendar")])),
                    BinOp::And,
                    e_not(e_instance_of(e_var("calendar"), "IntlCalendar")),
                ),
            ),
            vec![s_throw(e_new("TypeError", vec![e_concat_all(vec![
                e_str("IntlDateFormatter::__construct(): Argument #5 ($calendar) must be of type IntlCalendar|int|null, "),
                e_call("__elephc_intl_debug_type", vec![e_var("calendar")]),
                e_str(" given"),
            ])]))],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_call("is_int", vec![e_var("calendar")]),
                BinOp::And,
                e_binop(
                    e_binop(e_var("calendar"), BinOp::StrictNotEq, e_int(0)),
                    BinOp::And,
                    e_binop(e_var("calendar"), BinOp::StrictNotEq, e_int(1)),
                ),
            ),
            vec![s_throw(e_new("IntlException", vec![e_str("IntlDateFormatter::__construct(): Invalid value for calendar type; it must be one of IntlDateFormatter::TRADITIONAL (locale's default calendar) or IntlDateFormatter::GREGORIAN. Alternatively, it can be an IntlCalendar object")]))],
            vec![],
            None,
        ),
        s_assign("_calendar", e_int(1)),
        s_assign("_calendar_handle", e_int(0)),
        s_if(e_binop(e_var("calendar"), BinOp::StrictNotEq, e_null()), vec![
            s_if(e_instance_of(e_var("calendar"), "IntlCalendar"), vec![
                s_assign("_calendar", e_int(1)),
                s_assign("_calendar_handle", e_prop(e_var("calendar"), "__elephc_handle")),
            ], vec![], Some(vec![
                s_assign("_calendar", e_cast(CastType::Int, e_var("calendar"))),
            ])),
        ], vec![], None),
        s_assign("_pattern", e_ternary(
            e_binop(e_var("pattern"), BinOp::StrictEq, e_null()),
            e_str(""),
            e_var("pattern"),
        )),
        s_expr(e_call("__elephc_intl_validate_pattern", vec![
            e_var("_pattern"), e_str("IntlDateFormatter::__construct()"),
        ])),
        s_assign("_locale", e_ternary(
            e_binop(e_var("locale"), BinOp::StrictEq, e_null()),
            e_str(""),
            e_var("locale"),
        )),
        s_assign("_handle", e_call("__elephc_intl_open", vec![
            e_str("date"),
            e_array(vec![
                e_var("_locale"), e_var("dateType"), e_var("timeType"),
                e_var("_zone"), e_var("_calendar"), e_var("_pattern"),
                e_var("_calendar_handle"),
                e_binop(e_var("timezone"), BinOp::StrictNotEq, e_null()),
                e_str("IntlDateFormatter::__construct()"),
            ]),
        ])),
        s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![
            s_throw(e_new("IntlException", vec![e_call("__elephc_intl_last_error_message", vec![])])),
        ], vec![], None),
        s_prop_assign(e_var("this"), "__elephc_handle", e_var("_handle")),
    ]
}

/// Builds one generic `IntlDateFormatter` instance method body.
fn date_formatter_method_body(name: &str, params: &[(&str, TypeExpr, Option<Expr>, bool)], outputs: &[&str]) -> Vec<Stmt> {
    if name == "format" {
        return date_formatter_format_body();
    }
    if name == "setCalendar" {
        return vec![
            s_if(e_binop(e_var("calendar"), BinOp::StrictEq, e_null()), vec![
                s_return(crate::synthetic_class::e_bool(true)),
            ], vec![], None),
            s_if(e_instance_of(e_var("calendar"), "IntlCalendar"), vec![
                s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str("set_calendar_object"),
                    e_array(vec![e_call("__elephc_intl_handle_arg", vec![e_var("calendar")])]),
                ])),
            ], vec![], None),
            s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                e_str("set_calendar"),
                e_array(vec![e_cast(CastType::Int, e_var("calendar"))]),
            ])),
        ];
    }
    if name == "setTimeZone" {
        return vec![
            s_if(
                e_binop(
                    e_binop(e_var("timezone"), BinOp::StrictNotEq, e_null()),
                    BinOp::And,
                    e_binop(
                        e_not(e_call("is_object", vec![e_var("timezone")])),
                        BinOp::And,
                        e_not(e_call("is_string", vec![e_var("timezone")])),
                    ),
                ),
                vec![s_throw(e_new("TypeError", vec![e_concat_all(vec![
                    e_str("IntlDateFormatter::setTimeZone(): Argument #1 ($timezone) must be of type object|string|null, "),
                    e_call("__elephc_intl_debug_type", vec![e_var("timezone")]),
                    e_str(" given"),
                ])]))],
                vec![],
                None,
            ),
            s_assign("_zone", e_call("__elephc_intl_date_zone", vec![e_var("timezone")])),
            s_expr(e_call("__elephc_intl_validate_timezone", vec![
                e_var("_zone"), e_str("IntlDateFormatter::setTimeZone()"),
            ])),
            s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                e_str("set_timezone"),
                e_array(vec![e_var("_zone")]),
            ])),
        ];
    }
    if matches!(name, "getDateType" | "getTimeType" | "getCalendar" | "getPattern" | "getLocale" |
        "getTimeZoneId" | "setPattern" | "setLenient" | "isLenient" | "parse" |
        "parseToCalendar" | "localtime" | "getErrorCode" | "getErrorMessage") {
        let method_name = match name {
            "getDateType" => "datetype",
            "getTimeType" => "timetype",
            "getCalendar" => "calendar_kind",
            "getTimeZoneId" => "timezone_id",
            "getLocale" => "locale",
            "setPattern" => "set_pattern",
            "setLenient" => "set_lenient",
            "isLenient" => "is_lenient",
            "getPattern" => "pattern",
            "getErrorCode" => "error_code",
            "getErrorMessage" => "error_message",
            other => other,
        };
        let args: Vec<Expr> = params.iter().map(|(param, _, _, _)| e_var(param)).collect();
        if method_name == "error_code" {
            return vec![s_return(e_call("elephc_intl_error_code", vec![e_this_prop("__elephc_handle")]))];
        }
        if method_name == "error_message" {
            return vec![s_return(e_call("elephc_intl_error_message", vec![e_this_prop("__elephc_handle")]))];
        }
        let mut body = vec![s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![
            e_str(method_name), e_array(args),
        ]))];
        if outputs.is_empty() {
            body.push(s_return(e_var("_result")));
        } else {
            body.push(s_if(e_binop(e_var("_result"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![s_return(crate::synthetic_class::e_bool(false))], vec![], None));
            for (index, output) in outputs.iter().enumerate() {
                body.push(s_assign(*output, e_index(e_var("_result"), e_int(index as i64 + 1))));
            }
            body.push(s_return(e_index(e_var("_result"), e_int(0))));
        }
        return body;
    }
    if name == "getTimeZone" {
        return vec![
            s_assign("_zone_id", e_cast(CastType::String, e_method_call(e_var("this"), "getTimeZoneId", vec![]))),
            s_assign("_object", e_static_call("IntlTimeZone", "createTimeZone", vec![e_var("_zone_id")])),
            s_if(e_binop(e_var("_object"), BinOp::StrictEq, e_null()), vec![
                s_throw(e_new("IntlException", vec![e_str("IntlDateFormatter::getTimeZone(): time zone creation failed")])),
            ], vec![], None),
            s_return(e_var("_object")),
        ];
    }
    vec![s_return(crate::synthetic_class::e_bool(false))]
}

/// Builds `IntlDateFormatter::format`, accepting timestamps, arrays, `DateTimeInterface`, and calendars.
fn date_formatter_format_body() -> Vec<Stmt> {
    vec![
        s_if(e_instance_of(e_var("datetime"), "IntlCalendar"), vec![
            s_assign("_seconds", e_binop(e_cast(CastType::Float, e_method_call(e_var("datetime"), "getTime", vec![])), BinOp::Div, e_float(1000.0))),
        ], vec![], Some(vec![
            s_if(e_instance_of(e_var("datetime"), "DateTimeInterface"), vec![
                s_assign("_seconds", e_binop(
                    e_cast(CastType::Float, e_method_call(e_var("datetime"), "getTimestamp", vec![])),
                    BinOp::Add,
                    e_binop(
                        e_cast(CastType::Float, e_method_call(e_var("datetime"), "format", vec![e_str("u")])),
                        BinOp::Div,
                        e_float(1_000_000.0),
                    ),
                )),
            ], vec![], Some(vec![
                s_if(
                    e_binop(
                        e_binop(e_call("is_int", vec![e_var("datetime")]), BinOp::Or, e_call("is_float", vec![e_var("datetime")])),
                        BinOp::Or,
                        e_binop(e_call("is_string", vec![e_var("datetime")]), BinOp::And, e_call("is_numeric", vec![e_var("datetime")])),
                    ),
                    vec![s_assign("_seconds", e_cast(CastType::Float, e_var("datetime")))],
                    vec![],
                    Some(vec![
                        s_if(e_call("is_array", vec![e_var("datetime")]), vec![
                            // A tm array is a wall time in the FORMATTER's calendar; letting ICU
                            // resolve the fields keeps DST and fixed-offset zones exact.
                            s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                                e_str("format_localtime"),
                                e_array(vec![
                                    e_cast(CastType::Int, e_index(e_var("datetime"), e_str("tm_hour"))),
                                    e_cast(CastType::Int, e_index(e_var("datetime"), e_str("tm_min"))),
                                    e_cast(CastType::Int, e_index(e_var("datetime"), e_str("tm_sec"))),
                                    e_cast(CastType::Int, e_index(e_var("datetime"), e_str("tm_mon"))),
                                    e_cast(CastType::Int, e_index(e_var("datetime"), e_str("tm_mday"))),
                                    e_cast(CastType::Int, e_index(e_var("datetime"), e_str("tm_year"))),
                                ]),
                            ])),
                        ], vec![], Some(vec![
                            s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                                e_str("invalid_datetime"), e_array(vec![]),
                            ])),
                        ])),
                    ]),
                ),
            ])),
        ])),
        s_return(e_method_call(e_var("this"), "__elephc_call", vec![
            e_str("format"), e_array(vec![e_var("_seconds")]),
        ])),
    ]
}

/// Emits the global error and `false` result for `IntlDateFormatter::formatObject()`.
fn format_object_error(code: i64, message: &str) -> Vec<Stmt> {
    vec![
        s_expr(e_call("__elephc_intl_invoke", vec![
            e_str("intl"), e_str("intl_set_error"),
            e_array(vec![e_int(code), e_str(message)]),
        ])),
        s_return(crate::synthetic_class::e_bool(false)),
    ]
}

/// Builds `IntlDateFormatter::formatObject()` using a temporary formatter.
fn date_formatter_format_object_body() -> Vec<Stmt> {
    let valid_style = |value: Expr| e_call("in_array", vec![
        value,
        e_array(vec![
            e_int(-1), e_int(0), e_int(1), e_int(2), e_int(3),
            e_int(128), e_int(129), e_int(130), e_int(131),
        ]),
        crate::synthetic_class::e_bool(true),
    ]);
    let invalid_message = "IntlDateFormatter::formatObject(): the date/time format type is invalid";
    let empty_message = "IntlDateFormatter::formatObject(): the format is empty";
    let element_message = "IntlDateFormatter::formatObject(): bad format; if array, it must have two elements";
    let date_message = "IntlDateFormatter::formatObject(): bad format; the date format (first element of the array) is not valid";
    let time_message = "IntlDateFormatter::formatObject(): bad format; the time format (second element of the array) is not valid";
    let object_message = "IntlDateFormatter::formatObject(): the passed object must be an instance of either IntlCalendar or DateTimeInterface";
    let calendar_message = "IntlDateFormatter::formatObject(): bad IntlCalendar instance: not initialized properly";

    let mut body = vec![
        s_assign("_locale", e_ternary(e_binop(e_var("locale"), BinOp::StrictEq, e_null()), e_str(""), e_var("locale"))),
        // php's formatObject() with no format uses DateFormat::kDefault (MEDIUM) for both.
        s_assign("_date_style", e_int(2)),
        s_assign("_time_style", e_int(2)),
        s_assign("_pattern", e_str("")),
    ];
    body.push(s_if(e_call("is_array", vec![e_var("format")]), vec![
        s_if(e_binop(e_call("count", vec![e_var("format")]), BinOp::StrictNotEq, e_int(2)), format_object_error(1, element_message), vec![], None),
        s_assign("_style", e_index(e_var("format"), e_int(0))),
        s_if(
            e_binop(
                e_not(e_call("is_int", vec![e_var("_style")])),
                BinOp::Or,
                e_not(valid_style(e_var("_style"))),
            ),
            format_object_error(1, date_message),
            vec![],
            None,
        ),
        s_assign("_date_style", e_cast(CastType::Int, e_var("_style"))),
        s_assign("_style", e_index(e_var("format"), e_int(1))),
        s_if(
            e_binop(
                e_not(e_call("is_int", vec![e_var("_style")])),
                BinOp::Or,
                e_not(valid_style(e_var("_style"))),
            ),
            format_object_error(1, time_message),
            vec![],
            None,
        ),
        s_assign("_time_style", e_cast(CastType::Int, e_var("_style"))),
    ], vec![], None));
    body.push(s_if(
        e_binop(
            e_not(e_call("is_array", vec![e_var("format")])),
            BinOp::And,
            e_binop(e_var("format"), BinOp::StrictNotEq, e_null()),
        ),
        vec![
            s_if(e_call("is_int", vec![e_var("format")]), vec![
                s_if(e_not(valid_style(e_var("format"))), format_object_error(1, invalid_message), vec![], None),
                s_assign("_date_style", e_cast(CastType::Int, e_var("format"))),
                s_assign("_time_style", e_cast(CastType::Int, e_var("format"))),
            ], vec![], Some(vec![
                s_assign("_pattern", e_cast(CastType::String, e_var("format"))),
                s_if(e_binop(e_var("_pattern"), BinOp::StrictEq, e_str("")), format_object_error(1, empty_message), vec![], None),
            ])),
        ],
        vec![],
        None,
    ));
    body.extend(vec![
        s_if(
            e_instance_of(e_var("datetime"), "IntlCalendar"),
            vec![
                s_if(
                    e_binop(e_call("__elephc_intl_handle_arg", vec![e_var("datetime")]), BinOp::StrictEq, e_int(0)),
                    format_object_error(1, calendar_message),
                    vec![],
                    None,
                ),
            ],
            vec![],
            Some(vec![
                s_if(e_not(e_instance_of(e_var("datetime"), "DateTimeInterface")), format_object_error(0, object_message), vec![], None),
                s_if(
                    e_not(e_call("__elephc_intl_datetime_initialized", vec![e_var("datetime")])),
                    vec![s_throw(e_new("DateObjectError", vec![e_concat_all(vec![
                        e_str("Object of type "),
                        e_call("get_class", vec![e_var("datetime")]),
                        e_str(" (inheriting DateTime) has not been correctly initialized by calling parent::__construct() in its constructor"),
                    ])]))],
                    vec![],
                    None,
                ),
            ]),
        ),
        s_assign("_zone", e_ternary(
            e_instance_of(e_var("datetime"), "IntlCalendar"),
            e_method_call(e_var("datetime"), "getTimeZone", vec![]),
            e_method_call(e_var("datetime"), "getTimezone", vec![]),
        )),
        s_assign("_seconds", e_ternary(
            e_instance_of(e_var("datetime"), "IntlCalendar"),
            e_binop(e_cast(CastType::Float, e_method_call(e_var("datetime"), "getTime", vec![])), BinOp::Div, e_float(1000.0)),
            e_cast(CastType::Float, e_method_call(e_var("datetime"), "getTimestamp", vec![])),
        )),
        s_assign("_calendar_arg", e_ternary(
            e_instance_of(e_var("datetime"), "IntlCalendar"),
            e_var("datetime"),
            e_null(),
        )),
        s_assign("_formatter", e_new("IntlDateFormatter", vec![
            e_var("_locale"), e_var("_date_style"), e_var("_time_style"),
            e_var("_zone"), e_var("_calendar_arg"), e_var("_pattern"),
        ])),
        s_return(e_method_call(e_var("_formatter"), "format", vec![e_var("_seconds")])),
    ]);
    body
}

/// Maps one `IntlTimeZone` static method to its snake_case bridge operation.
fn timezone_bridge_name(name: &str) -> &str {
    match name {
        "countEquivalentIDs" => "intltz_count_equivalent_ids",
        "createEnumeration" => "intltz_create_enumeration",
        "createTimeZoneIDEnumeration" => "intltz_create_time_zone_id_enumeration",
        "getCanonicalID" => "intltz_get_canonical_id",
        "getEquivalentID" => "intltz_get_equivalent_id",
        "getRegion" => "intltz_get_region",
        "getTZDataVersion" => "intltz_get_tz_data_version",
        "getWindowsID" => "intltz_get_windows_id",
        "getIDForWindowsID" => "intltz_get_id_for_windows_id",
        "getIanaID" => "intltz_get_iana_id",
        other => other,
    }
}

/// Builds the `Transliterator` class and its ICU-backed surface.
fn build_transliterator_class() -> Stmt {
    // php-src declares `public readonly string $id`; a child may redeclare it, and
    // `newInstanceWithoutConstructor()` leaves it uninitialized for the first write.
    let mut builder = class("Transliterator")
        .prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)))
        .readonly_prop("id", TypeExpr::Str);
    for (constant, value) in constants_for("Transliterator") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .method(method("__construct")
            .private()
            .param("handle", TypeExpr::Int)
            .body(vec![
                // A readonly property initializes once from the declaring scope; the bridge
                // object is created through this private constructor, never by user code.
                s_prop_assign(e_var("this"), "__elephc_handle", e_var("handle")),
                s_prop_assign(e_var("this"), "id", e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str("id"), e_array(vec![]),
                ])),
            ]))
        .method(method("__elephc_from_handle")
            .static_()
            .param("handle", TypeExpr::Int)
            .returns(t_class("static"))
            .body(vec![
                s_return(e_new("Transliterator", vec![e_var("handle")])),
            ]))
        .method(method("__elephc_call")
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![s_return(e_call("__elephc_intl_call", vec![
                e_this_prop("__elephc_handle"), e_var("method"), e_var("args"),
            ]))]))
        .method(method("create")
            .static_()
            .param("id", TypeExpr::Str)
            .param_default("direction", TypeExpr::Int, e_int(0))
            .returns(t_nullable(t_class("Transliterator")))
            .body(vec![
                s_assign("_result", e_call("__elephc_intl_invoke", vec![
                    e_str("transliterator"), e_str("transliterator_create"),
                    e_array(vec![e_var("id"), e_var("direction")]),
                ])),
                s_if(e_not(e_instance_of(e_var("_result"), "Transliterator")), vec![s_return(e_null())], vec![], None),
                s_return(e_var("_result")),
            ]))
        .method(method("createFromRules")
            .static_()
            .param("rules", TypeExpr::Str)
            .param_default("direction", TypeExpr::Int, e_int(0))
            .returns(t_nullable(t_class("Transliterator")))
            .body(vec![
                s_assign("_result", e_call("__elephc_intl_invoke", vec![
                    e_str("transliterator"), e_str("transliterator_create_from_rules"),
                    e_array(vec![e_var("rules"), e_var("direction")]),
                ])),
                s_if(e_not(e_instance_of(e_var("_result"), "Transliterator")), vec![s_return(e_null())], vec![], None),
                s_return(e_var("_result")),
            ]))
        .method(method("listIDs").static_().returns(t_union(vec![t_array(), TypeExpr::False]))
            .body(vec![s_return(e_call("__elephc_intl_invoke_array", vec![
                e_str("transliterator"), e_str("transliterator_list_ids"), e_array(vec![]),
            ]))]))
        .method(method("createInverse").returns(t_nullable(t_class("Transliterator")))
            .body(vec![
                s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str("create_inverse"), e_array(vec![]),
                ])),
                s_if(e_not(e_instance_of(e_var("_result"), "Transliterator")), vec![s_return(e_null())], vec![], None),
                s_return(e_var("_result")),
            ]))
        .method(method("transliterate")
            .param("string", t_mixed())
            .param_default("start", TypeExpr::Int, e_int(0))
            .param_default("end", TypeExpr::Int, e_int(-1))
            .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![s_return(e_call("__elephc_intl_transliterate", vec![
                e_var("this"), e_var("string"), e_var("start"), e_var("end"),
                crate::synthetic_class::e_bool(true),
            ]))]))
        .method(method("__debugInfo")
            .returns(t_array())
            .body(vec![
                // The compiler-owned handle is not a PHP property. The static projection fold
                // turns this body into the class's var_dump descriptor, hiding it from dumps.
                s_return(e_array_assoc(vec![(e_str("id"), e_prop(e_var("this"), "id"))])),
            ]))
        .method(method("getErrorCode").returns(TypeExpr::Int)
            .body(vec![s_return(e_call("elephc_intl_error_code", vec![e_this_prop("__elephc_handle")]))]))
        .method(method("getErrorMessage").returns(TypeExpr::Str)
            .body(vec![s_return(e_call("elephc_intl_error_message", vec![e_this_prop("__elephc_handle")]))]))
        .method(clone_handle_method("Transliterator"));
    builder.build()
}

/// Builds the `IntlDatePatternGenerator` class.
fn build_date_pattern_generator_class() -> Stmt {
    class("IntlDatePatternGenerator")
        .prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)))
        .method(method("__construct")
            .param_default("locale", t_nullable(TypeExpr::Str), e_null())
            .body(vec![
                s_if(e_binop(e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0)), vec![
                    s_throw(e_new("Error", vec![e_str("IntlDatePatternGenerator object is already constructed")])),
                ], vec![], None),
                s_assign("_handle", e_static_call("IntlDatePatternGenerator", "__elephc_open", vec![e_var("locale")])),
                s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![
                    s_throw(e_new("IntlException", vec![e_str("IntlDatePatternGenerator::__construct(): unable to create pattern generator")])),
                ], vec![], None),
                s_prop_assign(e_var("this"), "__elephc_handle", e_var("_handle")),
            ]))
        .method(method("__elephc_open")
            .static_()
            .param("locale", t_nullable(TypeExpr::Str))
            .returns(TypeExpr::Int)
            .body(vec![s_return(e_call("__elephc_intl_open", vec![
                e_str("datepatterngenerator"), e_array(vec![e_var("locale")]),
            ]))]))
        .method(method("create")
            .static_()
            .param_default("locale", t_nullable(TypeExpr::Str), e_null())
            .returns(t_nullable(t_class("IntlDatePatternGenerator")))
            .body(vec![
                s_assign("_handle", e_static_call("IntlDatePatternGenerator", "__elephc_open", vec![e_var("locale")])),
                s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![s_return(e_null())], vec![], None),
                s_return(e_static_call("IntlDatePatternGenerator", "__elephc_from_handle", vec![e_var("_handle")])),
            ]))
        .method(method("__elephc_from_handle")
            .static_()
            .param("handle", TypeExpr::Int)
            .returns(t_class("static"))
            .body(vec![
                s_assign("_object", e_new("IntlDatePatternGenerator", vec![])),
                s_assign("_dummy", e_prop(e_var("_object"), "__elephc_handle")),
                s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
                s_expr(e_call("elephc_intl_close", vec![e_var("_dummy")])),
                s_return(e_var("_object")),
            ]))
        .method(method("__elephc_call")
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![s_return(e_call("__elephc_intl_call", vec![
                e_this_prop("__elephc_handle"), e_var("method"), e_var("args"),
            ]))]))
        .method(method("getBestPattern")
            .param("skeleton", TypeExpr::Str)
            .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                e_str("best_pattern"), e_array(vec![e_var("skeleton")]),
            ]))]))
        .method(clone_handle_method("IntlDatePatternGenerator"))
        .build()
}

/// Builds the `IntlListFormatter` class and its ICU-backed list formatting.
fn build_list_formatter_class() -> Stmt {
    let mut builder = class("IntlListFormatter").prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)));
    for (constant, value) in constants_for("IntlListFormatter") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .method(method("__construct")
            .param("locale", TypeExpr::Str)
            .param_default("type", TypeExpr::Int, e_int(0))
            .param_default("width", TypeExpr::Int, e_int(0))
            .body(vec![
                s_if(e_binop(e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0)), vec![
                    s_throw(e_new("Error", vec![e_str("IntlListFormatter object is already constructed")])),
                ], vec![], None),
                s_if(e_binop(e_call("strlen", vec![e_var("locale")]), BinOp::Gt, e_int(156)), vec![
                    s_throw(e_new("ValueError", vec![e_str(
                        "IntlListFormatter::__construct(): Argument #1 ($locale) must be less than or equal to 156 characters",
                    )])),
                ], vec![], None),
                s_if(e_binop(e_call("strlen", vec![e_var("locale")]), BinOp::Lt, e_int(2)), vec![
                    s_throw(e_new("ValueError", vec![e_concat_all(vec![
                        e_str("IntlListFormatter::__construct(): Argument #1 ($locale) \""),
                        e_var("locale"),
                        e_str("\" is invalid"),
                    ])])),
                ], vec![], None),
                s_if(
                    e_binop(
                        e_binop(e_var("type"), BinOp::StrictNotEq, e_int(0)),
                        BinOp::And,
                        e_binop(
                            e_binop(e_var("type"), BinOp::StrictNotEq, e_int(1)),
                            BinOp::And,
                            e_binop(e_var("type"), BinOp::StrictNotEq, e_int(2)),
                        ),
                    ),
                    vec![s_throw(e_new("ValueError", vec![e_str(
                        "IntlListFormatter::__construct(): Argument #2 ($type) must be one of IntlListFormatter::TYPE_AND, IntlListFormatter::TYPE_OR, or IntlListFormatter::TYPE_UNITS",
                    )]))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(
                        e_binop(e_var("width"), BinOp::StrictNotEq, e_int(0)),
                        BinOp::And,
                        e_binop(
                            e_binop(e_var("width"), BinOp::StrictNotEq, e_int(1)),
                            BinOp::And,
                            e_binop(e_var("width"), BinOp::StrictNotEq, e_int(2)),
                        ),
                    ),
                    vec![s_throw(e_new("ValueError", vec![e_str(
                        "IntlListFormatter::__construct(): Argument #3 ($width) must be one of IntlListFormatter::WIDTH_WIDE, IntlListFormatter::WIDTH_SHORT, or IntlListFormatter::WIDTH_NARROW",
                    )]))],
                    vec![],
                    None,
                ),
                s_assign("_handle", e_call("__elephc_intl_open", vec![
                    e_str("listformatter"), e_array(vec![e_var("locale"), e_var("type"), e_var("width")]),
                ])),
                s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![
                    s_throw(e_new("IntlException", vec![e_str("IntlListFormatter::__construct(): unable to create list formatter")])),
                ], vec![], None),
                s_prop_assign(e_var("this"), "__elephc_handle", e_var("_handle")),
            ]))
        .method(method("__elephc_from_handle")
            .static_()
            .param("handle", TypeExpr::Int)
            .returns(t_class("static"))
            .body(vec![
                s_assign("_object", e_new("IntlListFormatter", vec![e_str("en_US")])),
                s_assign("_dummy", e_prop(e_var("_object"), "__elephc_handle")),
                s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
                s_expr(e_call("elephc_intl_close", vec![e_var("_dummy")])),
                s_return(e_var("_object")),
            ]))
        .method(method("__elephc_call")
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![s_return(e_call("__elephc_intl_call", vec![
                e_this_prop("__elephc_handle"), e_var("method"), e_var("args"),
            ]))]))
        .method(method("format")
            .param("strings", t_mixed())
            .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![
                crate::synthetic_class::s_foreach(e_var("strings"), None, "_item", vec![
                    s_if(e_call("is_object", vec![e_var("_item")]), vec![
                        s_try(
                            vec![s_assign("_text", e_cast(CastType::String, e_var("_item")))],
                            vec![(vec!["Throwable"], Some("_error"), vec![
                                s_throw(e_new("Error", vec![e_concat_all(vec![
                                    e_str("Object of class "),
                                    e_call("get_class", vec![e_var("_item")]),
                                    e_str(" could not be converted to string"),
                                ])])),
                            ])],
                            None,
                        ),
                    ], vec![], None),
                ]),
                s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str("format"), e_array(vec![e_var("strings")]),
                ])),
            ]))
        .method(method("getErrorCode").returns(TypeExpr::Int)
            .body(vec![s_return(e_call("elephc_intl_error_code", vec![e_this_prop("__elephc_handle")]))]))
        .method(method("getErrorMessage").returns(TypeExpr::Str)
            .body(vec![s_return(e_call("elephc_intl_error_message", vec![e_this_prop("__elephc_handle")]))]))
        .method(method("__clone").body(vec![
            s_throw(e_new("Error", vec![e_str("Trying to clone an uncloneable object of class IntlListFormatter")])),
        ]));
    builder.build()
}

/// Builds the `UConverter` class and its ICU-backed conversion surface.
fn build_uconverter_class() -> Stmt {
    let mut builder = class("UConverter").prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)));
    for (constant, value) in constants_for("UConverter") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .method(method("__construct")
            .param_untyped_default("destination_encoding", e_null())
            .param_untyped_default("source_encoding", e_null())
            .body(vec![
                s_if(e_binop(e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0)), vec![
                    s_throw(e_new("Error", vec![e_str("UConverter object is already constructed")])),
                ], vec![], None),
                s_assign("_handle", e_call("__elephc_intl_open", vec![
                    e_str("converter"), e_array(vec![e_var("destination_encoding"), e_var("source_encoding")]),
                ])),
                s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![
                    s_throw(e_new("IntlException", vec![e_call("__elephc_intl_last_error_message", vec![])])),
                ], vec![], None),
                s_prop_assign(e_var("this"), "__elephc_handle", e_var("_handle")),
            ]))
        .method(method("__elephc_from_handle")
            .static_()
            .param("handle", TypeExpr::Int)
            .returns(t_class("static"))
            .body(vec![
                s_assign("_object", e_new("UConverter", vec![])),
                s_assign("_dummy", e_prop(e_var("_object"), "__elephc_handle")),
                s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
                s_expr(e_call("elephc_intl_close", vec![e_var("_dummy")])),
                s_return(e_var("_object")),
            ]))
        .method(method("__elephc_call")
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![s_return(e_call("__elephc_intl_call", vec![
                e_this_prop("__elephc_handle"), e_var("method"), e_var("args"),
            ]))]))
        .method(method("convert")
            .param("str", TypeExpr::Str)
            .param_default("reverse", TypeExpr::Bool, crate::synthetic_class::e_bool(false))
            .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                e_str("convert"), e_array(vec![e_var("str"), e_var("reverse")]),
            ]))]))
        .method(method("getSourceEncoding").returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![e_str("get_source_encoding"), e_array(vec![])]))]))
        .method(method("getDestinationEncoding").returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![e_str("get_destination_encoding"), e_array(vec![])]))]))
        .method(method("setSourceEncoding").param("encoding", TypeExpr::Str).returns(TypeExpr::Bool)
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![e_str("set_source_encoding"), e_array(vec![e_var("encoding")])]))]))
        .method(method("setDestinationEncoding").param("encoding", TypeExpr::Str).returns(TypeExpr::Bool)
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![e_str("set_destination_encoding"), e_array(vec![e_var("encoding")])]))]))
        .method(method("getSourceType").returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![e_str("get_source_type"), e_array(vec![])]))]))
        .method(method("getDestinationType").returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![e_str("get_destination_type"), e_array(vec![])]))]))
        .method(method("setSubstChars").param("chars", TypeExpr::Str).returns(TypeExpr::Bool)
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![e_str("set_subst_chars"), e_array(vec![e_var("chars")])]))]))
        .method(method("getSubstChars").returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![e_str("get_subst_chars"), e_array(vec![])]))]))
        .method(method("getErrorCode").returns(TypeExpr::Int)
            .body(vec![s_return(e_call("elephc_intl_error_code", vec![e_this_prop("__elephc_handle")]))]))
        .method(method("getErrorMessage").returns(t_nullable(TypeExpr::Str))
            .body(vec![s_return(e_call("elephc_intl_error_message", vec![e_this_prop("__elephc_handle")]))]))
        .method(method("getAliases").static_().param("name", TypeExpr::Str)
            .returns(t_union(vec![t_array(), TypeExpr::False]))
            .body(vec![s_return(e_call("__elephc_intl_invoke_array", vec![
                e_str("uconverter"), e_str("get_aliases"), e_array(vec![e_var("name")]),
            ]))]))
        .method(method("getAvailable").static_().returns(t_array())
            .body(vec![s_return(e_call("__elephc_intl_invoke_array", vec![
                e_str("uconverter"), e_str("get_available"), e_array(vec![]),
            ]))]))
        .method(method("getStandards").static_().returns(t_array())
            .body(vec![s_return(e_call("__elephc_intl_invoke_array", vec![
                e_str("uconverter"), e_str("get_standards"), e_array(vec![]),
            ]))]))
        .method(method("reasonText").static_().param("reason", TypeExpr::Int).returns(TypeExpr::Str)
            .body(vec![s_return(e_call("__elephc_intl_invoke_string", vec![
                e_str("uconverter"), e_str("reason_text"), e_array(vec![e_var("reason")]),
            ]))]))
        .method(method("transcode")
            .static_()
            .param("str", TypeExpr::Str)
            .param("toEncoding", TypeExpr::Str)
            .param("fromEncoding", TypeExpr::Str)
            .param_default("options", t_nullable(t_array()), e_null())
            .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![
                s_assign("_result", e_call("__elephc_intl_invoke", vec![
                    e_str("uconverter"), e_str("transcode"),
                    e_array(vec![e_var("str"), e_var("toEncoding"), e_var("fromEncoding"), e_var("options")]),
                ])),
                s_if(e_binop(e_var("_result"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![
                    s_return(crate::synthetic_class::e_bool(false)),
                ], vec![], None),
                s_return(e_cast(CastType::String, e_var("_result"))),
            ]))
        .method(method("toUCallback")
            .param("reason", TypeExpr::Int)
            .param("source", t_mixed())
            .param("codePoint", TypeExpr::Int)
            .param_by_ref("error", Some(t_mixed()))
            .returns(t_nullable(t_mixed()))
            .body(vec![s_return(e_null())]))
        .method(method("fromUCallback")
            .param("reason", TypeExpr::Int)
            .param("source", t_mixed())
            .param("codePoint", TypeExpr::Int)
            .param_by_ref("error", Some(t_mixed()))
            .returns(t_nullable(t_mixed()))
            .body(vec![s_return(e_null())]))
        .method(clone_handle_method("UConverter"));
    builder.build()
}

/// Builds the `Spoofchecker` class and its ICU-backed checks.
fn build_spoofchecker_class() -> Stmt {
    let mut builder = class("Spoofchecker").prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)));
    for (constant, value) in constants_for("Spoofchecker") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .method(method("__construct").body(vec![
            s_if(e_binop(e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0)), vec![
                s_throw(e_new("Error", vec![e_str("Spoofchecker object is already constructed")])),
            ], vec![], None),
            s_assign("_handle", e_call("__elephc_intl_open", vec![
                e_str("spoof"), e_array(vec![]),
            ])),
            s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![
                s_throw(e_new("IntlException", vec![e_str("Spoofchecker::__construct(): unable to create spoof checker")])),
            ], vec![], None),
            s_prop_assign(e_var("this"), "__elephc_handle", e_var("_handle")),
        ]))
        .method(method("__elephc_from_handle")
            .static_()
            .param("handle", TypeExpr::Int)
            .returns(t_class("static"))
            .body(vec![
                s_assign("_object", e_new("Spoofchecker", vec![])),
                s_assign("_dummy", e_prop(e_var("_object"), "__elephc_handle")),
                s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
                s_expr(e_call("elephc_intl_close", vec![e_var("_dummy")])),
                s_return(e_var("_object")),
            ]))
        .method(method("__elephc_call")
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![s_return(e_call("__elephc_intl_call", vec![
                e_this_prop("__elephc_handle"), e_var("method"), e_var("args"),
            ]))]))
        .method(method("isSuspicious")
            .param("string", TypeExpr::Str)
            .param_by_ref_default("errorCode", Some(t_mixed()), e_null())
            .returns(TypeExpr::Bool)
            .body(vec![
                s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str("is_suspicious"), e_array(vec![e_var("string")]),
                ])),
                s_if(e_not(e_call("is_array", vec![e_var("_result")])), vec![s_return(crate::synthetic_class::e_bool(false))], vec![], None),
                s_assign("errorCode", e_index(e_var("_result"), e_int(1))),
                s_return(e_index(e_var("_result"), e_int(0))),
            ]))
        .method(method("areConfusable")
            .param("string1", TypeExpr::Str)
            .param("string2", TypeExpr::Str)
            .param_by_ref_default("errorCode", Some(t_mixed()), e_null())
            .returns(TypeExpr::Bool)
            .body(vec![
                s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str("are_confusable"), e_array(vec![e_var("string1"), e_var("string2")]),
                ])),
                s_if(e_not(e_call("is_array", vec![e_var("_result")])), vec![s_return(crate::synthetic_class::e_bool(false))], vec![], None),
                s_assign("errorCode", e_index(e_var("_result"), e_int(1))),
                s_return(e_index(e_var("_result"), e_int(0))),
            ]))
        .method(method("setAllowedLocales")
            .param("locales", TypeExpr::Str)
            .returns(TypeExpr::Void)
            .body(vec![s_expr(e_method_call(e_var("this"), "__elephc_call", vec![
                e_str("set_allowed_locales"), e_array(vec![e_var("locales")]),
            ]))]))
        .method(method("setChecks")
            .param("checks", TypeExpr::Int)
            .returns(TypeExpr::Void)
            .body(vec![s_expr(e_method_call(e_var("this"), "__elephc_call", vec![
                e_str("set_checks"), e_array(vec![e_var("checks")]),
            ]))]))
        .method(method("setRestrictionLevel")
            .param("level", TypeExpr::Int)
            .returns(TypeExpr::Void)
            .body(vec![
                s_if(
                    e_binop(
                        e_binop(e_binop(e_var("level"), BinOp::StrictNotEq, e_class_const("Spoofchecker", "ASCII")), BinOp::And,
                            e_binop(e_var("level"), BinOp::StrictNotEq, e_class_const("Spoofchecker", "SINGLE_SCRIPT_RESTRICTIVE"))),
                        BinOp::And,
                        e_binop(
                            e_binop(e_binop(e_var("level"), BinOp::StrictNotEq, e_class_const("Spoofchecker", "HIGHLY_RESTRICTIVE")), BinOp::And,
                                e_binop(e_var("level"), BinOp::StrictNotEq, e_class_const("Spoofchecker", "MODERATELY_RESTRICTIVE"))),
                            BinOp::And,
                            e_binop(
                                e_binop(e_var("level"), BinOp::StrictNotEq, e_class_const("Spoofchecker", "MINIMALLY_RESTRICTIVE")),
                                BinOp::And,
                                e_binop(e_var("level"), BinOp::StrictNotEq, e_class_const("Spoofchecker", "UNRESTRICTIVE")),
                            ),
                        ),
                    ),
                    vec![s_throw(e_new("ValueError", vec![e_str(
                        "Spoofchecker::setRestrictionLevel(): Argument #1 ($level) must be one of Spoofchecker::ASCII, Spoofchecker::SINGLE_SCRIPT_RESTRICTIVE, Spoofchecker::HIGHLY_RESTRICTIVE, Spoofchecker::MODERATELY_RESTRICTIVE, Spoofchecker::MINIMALLY_RESTRICTIVE, or Spoofchecker::UNRESTRICTIVE",
                    )]))],
                    vec![],
                    None,
                ),
                s_expr(e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str("set_restriction_level"), e_array(vec![e_var("level")]),
                ])),
            ]))
        .method(method("setAllowedChars")
            .param("pattern", TypeExpr::Str)
            .param_default("patternOptions", TypeExpr::Int, e_int(0))
            .returns(TypeExpr::Void)
            .body(vec![
                s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str("set_allowed_chars"), e_array(vec![e_var("pattern"), e_var("patternOptions")]),
                ])),
                s_if(e_call("is_string", vec![e_var("_result")]), vec![
                    s_throw(e_new("ValueError", vec![e_cast(CastType::String, e_var("_result"))])),
                ], vec![], None),
            ]))
        .method(clone_handle_method("Spoofchecker"));
    builder.build()
}

/// Builds the `IntlTimeZone` class.
fn build_time_zone_class() -> Stmt {
    let mut builder = class("IntlTimeZone")
        .prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)))
        .prop("valid", TypeExpr::Bool, Some(crate::synthetic_class::e_bool(true)))
        .prop("id", TypeExpr::Str, Some(e_str("")))
        .prop("rawOffset", TypeExpr::Int, Some(e_int(0)))
        .prop("currentOffset", TypeExpr::Int, Some(e_int(0)));
    for (constant, value) in constants_for("IntlTimeZone") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .method(method("__construct").private().body(vec![
            s_if(e_binop(e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0)), vec![
                s_throw(e_new("IntlException", vec![e_str("IntlTimeZone::__construct(): cannot call constructor twice")])),
            ], vec![], None),
        ]))
        .method(method("__elephc_sync_debug").body(vec![
            s_prop_assign(e_var("this"), "valid", crate::synthetic_class::e_bool(true)),
            s_prop_assign(e_var("this"), "id", e_cast(CastType::String, e_method_call(e_var("this"), "getID", vec![]))),
            s_assign("_offset", e_method_call(e_var("this"), "__elephc_call", vec![e_str("debug_offset"), e_array(vec![])])),
            s_prop_assign(e_var("this"), "rawOffset", e_cast(CastType::Int, e_index(e_var("_offset"), e_int(0)))),
            s_prop_assign(e_var("this"), "currentOffset", e_binop(
                e_cast(CastType::Int, e_index(e_var("_offset"), e_int(0))),
                BinOp::Add,
                e_cast(CastType::Int, e_index(e_var("_offset"), e_int(1))),
            )),
        ]))
        .method(method("__elephc_from_handle")
            .static_()
            .param("handle", TypeExpr::Int)
            .returns(t_class("static"))
            .body(vec![
                s_assign("_object", e_new_static(vec![])),
                s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
                s_expr(e_method_call(e_var("_object"), "__elephc_sync_debug", vec![])),
                s_return(e_var("_object")),
            ]))
        .method(method("__elephc_open")
            .static_()
            .param("id", TypeExpr::Str)
            .returns(t_nullable(t_class("static")))
            .body(vec![
                s_assign("_handle", e_call("__elephc_intl_open", vec![e_str("timezone"), e_array(vec![e_var("id")])])),
                s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![s_return(e_null())], vec![], None),
                s_assign("_object", e_new_static(vec![])),
                s_prop_assign(e_var("_object"), "__elephc_handle", e_var("_handle")),
                s_expr(e_method_call(e_var("_object"), "__elephc_sync_debug", vec![])),
                s_return(e_var("_object")),
            ]));
    builder = builder
        .method(method("createTimeZone").static_().param("timezoneId", TypeExpr::Str)
            .returns(t_nullable(t_class("IntlTimeZone")))
            .body(vec![s_return(e_self_call("__elephc_open", vec![e_var("timezoneId")]))]))
        .method(method("createDefault").static_().returns(t_class("IntlTimeZone"))
            .body(vec![
                s_assign("_zone", e_call("date_default_timezone_get", vec![])),
                s_assign("_object", e_self_call("createTimeZone", vec![e_var("_zone")])),
                s_if(e_binop(e_var("_object"), BinOp::StrictEq, e_null()), vec![s_throw(e_new("IntlException", vec![e_str("IntlTimeZone::createDefault(): time zone creation failed")]))], vec![], None),
                s_return(e_var("_object")),
            ]))
        .method(method("getGMT").static_().returns(t_class("IntlTimeZone"))
            .body(vec![
                s_assign("_object", e_self_call("createTimeZone", vec![e_str("GMT")])),
                s_if(e_binop(e_var("_object"), BinOp::StrictEq, e_null()), vec![s_throw(e_new("IntlException", vec![e_str("IntlTimeZone::getGMT(): time zone creation failed")]))], vec![], None),
                s_return(e_var("_object")),
            ]))
        .method(method("getUnknown").static_().returns(t_class("IntlTimeZone"))
            .body(vec![
                s_assign("_object", e_self_call("createTimeZone", vec![e_str("Etc/Unknown")])),
                s_if(e_binop(e_var("_object"), BinOp::StrictEq, e_null()), vec![s_throw(e_new("IntlException", vec![e_str("IntlTimeZone::getUnknown(): time zone creation failed")]))], vec![], None),
                s_return(e_var("_object")),
            ]))
        .method(method("fromDateTimeZone").static_().param("timezone", t_class("DateTimeZone"))
            .returns(t_nullable(t_class("IntlTimeZone")))
            .body(vec![s_return(e_self_call(
                "createTimeZone",
                vec![e_method_call(e_var("timezone"), "getName", vec![])],
            ))]))
        .method(method("toDateTimeZone").returns(t_union(vec![t_class("DateTimeZone"), TypeExpr::False]))
            .body(vec![
                s_assign("_id", e_method_call(e_var("this"), "getID", vec![])),
                s_if(e_binop(e_var("_id"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![s_return(crate::synthetic_class::e_bool(false))], vec![], None),
                // ICU reports a fixed-offset zone as `GMT+04:05`; PHP's DateTimeZone keeps the
                // offset spelling (`+04:05`) as its name.
                s_if(
                    e_binop(
                        e_binop(e_call("substr", vec![e_var("_id"), e_int(0), e_int(4)]), BinOp::StrictEq, e_str("GMT+")),
                        BinOp::Or,
                        e_binop(e_call("substr", vec![e_var("_id"), e_int(0), e_int(4)]), BinOp::StrictEq, e_str("GMT-")),
                    ),
                    vec![s_assign("_id", e_call("substr", vec![e_var("_id"), e_int(3)]))],
                    vec![],
                    None,
                ),
                s_try(
                    vec![s_return(e_new("DateTimeZone", vec![e_var("_id")]))],
                    vec![(vec!["Throwable"], Some("_error"), vec![
                        s_throw(e_new("IntlException", vec![
                            e_str("DateTimeZone constructor threw exception"), e_int(0), e_var("_error"),
                        ])),
                    ])],
                    None,
                ),
            ]))
        .method(method("__elephc_call")
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![s_return(e_call("__elephc_intl_call", vec![e_this_prop("__elephc_handle"), e_var("method"), e_var("args")]))]))
        .method(method("__debugInfo")
            .returns(t_array())
            .body(vec![
                s_return(e_array_assoc(vec![
                    (e_str("valid"), e_this_prop("valid")),
                    (e_str("id"), e_this_prop("id")),
                    (e_str("rawOffset"), e_this_prop("rawOffset")),
                    (e_str("currentOffset"), e_this_prop("currentOffset")),
                ])),
            ]));
    for (name, params, outputs) in [
        ("countEquivalentIDs", vec![("timezoneId", TypeExpr::Str, None, false)], vec![]),
        ("createEnumeration", vec![("countryOrRawOffset", t_union(vec![TypeExpr::Str, TypeExpr::Int, TypeExpr::Void]), Some(e_null()), false)], vec![]),
        ("createTimeZoneIDEnumeration", vec![("type", TypeExpr::Int, None, false), ("region", t_nullable(TypeExpr::Str), Some(e_null()), false), ("rawOffset", t_nullable(TypeExpr::Int), Some(e_null()), false)], vec![]),
        ("getCanonicalID", vec![("timezoneId", TypeExpr::Str, None, false), ("isSystemId", TypeExpr::Bool, Some(e_null()), true)], vec!["isSystemId"]),
        ("getDisplayName", vec![("dst", TypeExpr::Bool, Some(crate::synthetic_class::e_bool(false)), false), ("style", TypeExpr::Int, Some(e_int(2)), false), ("locale", t_nullable(TypeExpr::Str), Some(e_null()), false)], vec![]),
        ("getDSTSavings", vec![], vec![]),
        ("getEquivalentID", vec![("timezoneId", TypeExpr::Str, None, false), ("offset", t_mixed(), None, false)], vec![]),
        ("getID", vec![], vec![]),
        ("getOffset", vec![("timestamp", TypeExpr::Float, None, false), ("local", TypeExpr::Bool, None, false), ("rawOffset", TypeExpr::Int, None, true), ("dstOffset", TypeExpr::Int, None, true)], vec!["rawOffset", "dstOffset"]),
        ("getRawOffset", vec![], vec![]),
        ("getRegion", vec![("timezoneId", TypeExpr::Str, None, false)], vec![]),
        ("getTZDataVersion", vec![], vec![]),
        ("getWindowsID", vec![("timezoneId", TypeExpr::Str, None, false)], vec![]),
        ("getIDForWindowsID", vec![("timezoneId", TypeExpr::Str, None, false), ("region", t_nullable(TypeExpr::Str), Some(e_null()), false)], vec![]),
        ("getIanaID", vec![("timezoneId", TypeExpr::Str, None, false)], vec![]),
        ("hasSameRules", vec![("other", t_mixed(), None, false)], vec![]),
        ("useDaylightTime", vec![], vec![]),
        ("getErrorCode", vec![], vec![]),
        ("getErrorMessage", vec![], vec![]),
    ] {
        let body = if name == "createEnumeration" || name == "createTimeZoneIDEnumeration" {
            let mut statements = Vec::new();
            if name == "createTimeZoneIDEnumeration" {
                statements.push(s_if(
                    e_binop(
                        e_binop(e_var("type"), BinOp::StrictNotEq, e_int(0)),
                        BinOp::And,
                        e_binop(
                            e_binop(e_var("type"), BinOp::StrictNotEq, e_int(1)),
                            BinOp::And,
                            e_binop(e_var("type"), BinOp::StrictNotEq, e_int(2)),
                        ),
                    ),
                    vec![s_throw(e_new("ValueError", vec![e_str(
                        "IntlTimeZone::createTimeZoneIDEnumeration(): Argument #1 ($type) must be one of IntlTimeZone::TYPE_ANY, IntlTimeZone::TYPE_CANONICAL, or IntlTimeZone::TYPE_CANONICAL_LOCATION",
                    )]))],
                    vec![],
                    None,
                ));
                statements.push(s_if(
                    e_binop(
                        e_binop(e_var("rawOffset"), BinOp::StrictNotEq, e_null()),
                        BinOp::And,
                        e_binop(
                            e_binop(e_var("rawOffset"), BinOp::Lt, e_int(-2147483648)),
                            BinOp::Or,
                            e_binop(e_var("rawOffset"), BinOp::Gt, e_int(2147483647)),
                        ),
                    ),
                    vec![s_throw(e_new("ValueError", vec![e_str(
                        "IntlTimeZone::createTimeZoneIDEnumeration(): Argument #3 ($rawOffset) must be between -2147483648 and 2147483647",
                    )]))],
                    vec![],
                    None,
                ));
            }
            statements.push(s_assign("_items", e_static_call("IntlTimeZone", "__elephc_static_call", vec![e_str(timezone_bridge_name(name)), e_array(params.iter().map(|(p,_,_,_)| e_var(p)).collect())])));
            statements.push(s_return(e_static_call("IntlIterator", "__elephc_from_array", vec![e_var("_items")])));
            statements
        } else if name == "getCanonicalID" {
            vec![
                s_assign("_result", e_call("__elephc_intl_invoke", vec![
                    e_str("timezone"), e_str("intltz_get_canonical_id"),
                    e_array(params.iter().map(|(p,_,_,_)| e_var(p)).collect()),
                ])),
                s_if(e_binop(e_var("_result"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![s_return(crate::synthetic_class::e_bool(false))], vec![], None),
                s_assign("isSystemId", e_index(e_var("_result"), e_int(1))),
                s_return(e_index(e_var("_result"), e_int(0))),
            ]
        } else if name == "getID" || name == "getRawOffset" || name == "getDSTSavings" || name == "getDisplayName" || name == "getOffset" || name == "hasSameRules" || name == "useDaylightTime" {
            let bridge_name = match name {
                "getID" => "get_id",
                "getRawOffset" => "get_raw_offset",
                "getDSTSavings" => "get_dst_savings",
                "getDisplayName" => "get_display_name",
                "getOffset" => "get_offset",
                "hasSameRules" => "has_same_rules",
                "useDaylightTime" => "use_daylight_time",
                other => other,
            };
            let mut body = vec![s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![e_str(bridge_name), e_array(params.iter().map(|(p,_,_,_)| e_var(p)).collect())]))];
            if outputs.is_empty() { body.push(s_return(e_var("_result"))); }
            else {
                body.push(s_if(e_binop(e_var("_result"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![s_return(crate::synthetic_class::e_bool(false))], vec![], None));
                for (index, output) in outputs.iter().enumerate() {
                    body.push(s_assign(*output, e_index(e_var("_result"), e_int(index as i64 + 1))));
                }
                body.push(s_return(e_index(e_var("_result"), e_int(0))));
            }
            body
        } else if name == "getErrorCode" {
            vec![s_return(e_call("elephc_intl_error_code", vec![e_this_prop("__elephc_handle")]))]
        } else if name == "getErrorMessage" {
            vec![s_return(e_call("elephc_intl_error_message", vec![e_this_prop("__elephc_handle")]))]
        } else {
            vec![s_return(e_call("__elephc_intl_invoke", vec![e_str("timezone"), e_str(timezone_bridge_name(name)), e_array(params.iter().map(|(p,_,_,_)| if *p == "offset" { e_cast(CastType::Int, e_var(p)) } else { e_var(p) }).collect())]))]
        };
        let method_builder = if name.starts_with("create") || name == "countEquivalentIDs" || name.starts_with("getCanonicalID") || name.starts_with("getEquivalentID") || name.starts_with("getRegion") || name.starts_with("getTZDataVersion") || name.starts_with("getWindowsID") || name.starts_with("getIDForWindowsID") || name.starts_with("getIanaID") {
            static_method(name, params, body)
        } else {
            instance_method(name, params, body)
        };
        builder = builder.method(method_builder.returns(timezone_method_return(name)));
    }
    builder = builder.method(method("__elephc_static_call").static_().param("method", TypeExpr::Str).param("args", t_mixed()).body(vec![s_return(e_call("__elephc_intl_invoke", vec![e_str("timezone"), e_var("method"), e_var("args")]))]));
    builder = builder.method(clone_handle_method("IntlTimeZone"));
    builder.build()
}

/// Builds a public instance method from a parameter table and body.
fn instance_method(name: &str, params: Vec<(&str, TypeExpr, Option<Expr>, bool)>, body: Vec<Stmt>) -> crate::synthetic_class::MethodBuilder {
    let mut builder = method(name);
    for (param, ty, default, by_ref) in params {
        builder = apply_method_param(builder, param, Some(ty), default, by_ref);
    }
    // See `static_method`: an undeclared PHP return type is `mixed`, not the syntactic
    // fallback's `int`.
    builder.returns(t_mixed()).body(body)
}

/// Returns the PHP-visible result type of one `IntlTimeZone` method.
fn timezone_method_return(name: &str) -> TypeExpr {
    match name {
        "getID" | "getDisplayName" | "getEquivalentID" | "getRegion" | "getTZDataVersion" |
        "getWindowsID" | "getIDForWindowsID" | "getIanaID" | "getErrorMessage" => {
            t_union(vec![TypeExpr::Str, TypeExpr::False])
        }
        "getRawOffset" | "getDSTSavings" | "countEquivalentIDs" => {
            t_union(vec![TypeExpr::Int, TypeExpr::False])
        }
        "getErrorCode" => t_union(vec![TypeExpr::Int, TypeExpr::False]),
        "getOffset" | "hasSameRules" | "useDaylightTime" => TypeExpr::Bool,
        "createEnumeration" | "createTimeZoneIDEnumeration" => t_class("IntlIterator"),
        "getCanonicalID" => t_union(vec![TypeExpr::Str, TypeExpr::False]),
        "toDateTimeZone" => t_union(vec![t_class("DateTimeZone"), TypeExpr::False]),
        _ => t_mixed(),
    }
}

/// Builds the `instanceof` disjunction that recognizes a generated Intl wrapper object.
fn intl_wrapper_check() -> Expr {
    let mut check: Option<Expr> = None;
    for class in [
        "IntlTimeZone", "IntlDateFormatter", "IntlCalendar", "IntlGregorianCalendar",
        "NumberFormatter", "MessageFormatter", "Collator", "Transliterator", "Spoofchecker",
        "ResourceBundle", "UConverter", "IntlIterator", "IntlBreakIterator",
        "IntlListFormatter", "IntlDatePatternGenerator", "IntlChar",
    ] {
        let current = e_instance_of(e_var("value"), class);
        check = Some(match check {
            Some(previous) => e_binop(previous, BinOp::Or, current),
            None => current,
        });
    }
    check.unwrap_or_else(|| crate::synthetic_class::e_bool(false))
}

/// Renders one captured class constant as an expression.
fn const_value_expr(value: &IntlConstValue) -> Expr {
    match value {
        IntlConstValue::Int(value) => e_int(*value),
        IntlConstValue::Float(value) => e_float(*value),
        IntlConstValue::Str(value) => e_str(value),
        IntlConstValue::Bool(value) => crate::synthetic_class::e_bool(*value),
        IntlConstValue::Null => e_null(),
    }
}

/// Builds `IntlIterator` with an array-backed iterator protocol.
fn build_iterator_class() -> Stmt {
    class("IntlIterator")
        .prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)))
        .prop("__elephc_items", t_array(), Some(e_array(vec![])))
        .prop("__elephc_keys", t_array(), Some(e_array(vec![])))
        .prop("__elephc_index", TypeExpr::Int, Some(e_int(0)))
        .implements("Iterator")
        .method(method("__construct").protected().body(vec![]))
        .method(method("__elephc_from_array").static_().param("items", t_mixed()).returns(t_class("static")).body(vec![
            s_assign("_object", e_new_static(vec![])),
            s_prop_assign(e_var("_object"), "__elephc_items", e_var("items")),
            s_prop_assign(e_var("_object"), "__elephc_keys", e_call("array_keys", vec![e_var("items")])),
            s_return(e_var("_object")),
        ]))
        .method(method("__elephc_from_handle").static_().param("handle", TypeExpr::Int).returns(t_class("static")).body(vec![
            s_assign("_object", e_new_static(vec![])),
            s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
            s_return(e_var("_object")),
        ]))
        .method(method("current").returns(t_mixed()).body(vec![s_return(e_index(e_this_prop("__elephc_items"), e_index(e_this_prop("__elephc_keys"), e_this_prop("__elephc_index"))))]))
        .method(method("key").returns(t_mixed()).body(vec![s_return(e_index(e_this_prop("__elephc_keys"), e_this_prop("__elephc_index")))]))
        .method(method("next").returns(TypeExpr::Void).body(vec![s_prop_assign(e_var("this"), "__elephc_index", e_binop(e_this_prop("__elephc_index"), BinOp::Add, e_int(1)))]))
        .method(method("rewind").returns(TypeExpr::Void).body(vec![s_prop_assign(e_var("this"), "__elephc_index", e_int(0))]))
        .method(method("valid").returns(TypeExpr::Bool).body(vec![s_return(e_binop(e_this_prop("__elephc_index"), BinOp::Lt, e_call("count", vec![e_this_prop("__elephc_keys")])))]))
        .build()
}

/// Builds the `IntlBreakIterator` class and its navigation surface.
fn build_break_iterator_class() -> Stmt {
    let mut builder = class("IntlBreakIterator")
        .prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)))
        .prop("valid", TypeExpr::Bool, Some(crate::synthetic_class::e_bool(false)))
        .prop("text", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("type", TypeExpr::Str, Some(e_str("")));
    for (constant, value) in constants_for("IntlBreakIterator") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .implements("IteratorAggregate")
        .method(method("__construct").private().body(vec![]))
        .method(method("__elephc_sync_debug").body(vec![
            s_prop_assign(e_var("this"), "valid", e_binop(
                e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0),
            )),
            s_prop_assign(e_var("this"), "text", e_method_call(e_var("this"), "__elephc_call", vec![
                e_str("get_text"), e_array(vec![]),
            ])),
            s_prop_assign(e_var("this"), "type", e_cast(CastType::String, e_method_call(e_var("this"), "__elephc_call", vec![
                e_str("debug_type"), e_array(vec![]),
            ]))),
        ]))
        .method(method("__elephc_from_handle")
            .static_()
            .param("handle", TypeExpr::Int)
            .returns(t_class("static"))
            .body(vec![
                s_assign("_object", e_new("IntlBreakIterator", vec![])),
                s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
                s_expr(e_method_call(e_var("_object"), "__elephc_sync_debug", vec![])),
                s_return(e_var("_object")),
            ]))
        .method(method("__elephc_open")
            .static_()
            .param("locale", t_nullable(TypeExpr::Str))
            .param("type", TypeExpr::Int)
            .returns(t_nullable(t_class("static")))
            .body(vec![
                s_assign("_locale", e_ternary(e_binop(e_var("locale"), BinOp::StrictEq, e_null()), e_str(""), e_var("locale"))),
                s_assign("_handle", e_call("__elephc_intl_open", vec![
                    e_str("breakiter"),
                    e_array(vec![e_var("_locale"), e_var("type"), e_null()]),
                ])),
                s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![s_return(e_null())], vec![], None),
                s_assign("_object", e_new("IntlBreakIterator", vec![])),
                s_prop_assign(e_var("_object"), "__elephc_handle", e_var("_handle")),
                s_expr(e_method_call(e_var("_object"), "__elephc_sync_debug", vec![])),
                s_return(e_var("_object")),
            ]))
        .method(method("__elephc_call")
            .param("method", TypeExpr::Str)
            .param("args", t_mixed())
            .returns(t_mixed())
            .body(vec![s_return(e_call("__elephc_intl_call", vec![
                e_this_prop("__elephc_handle"), e_var("method"), e_var("args"),
            ]))]));
    for (name, kind) in [
        ("createCharacterInstance", 0),
        ("createWordInstance", 1),
        ("createLineInstance", 2),
        ("createSentenceInstance", 3),
        ("createTitleInstance", 4),
    ] {
        builder = builder.method(method(name)
            .static_()
            .param_default("locale", t_nullable(TypeExpr::Str), e_null())
            .returns(t_class("IntlRuleBasedBreakIterator"))
            .body(vec![
                s_assign("_handle", e_call("__elephc_intl_open", vec![
                    e_str("breakiter"),
                    e_array(vec![
                        e_ternary(e_binop(e_var("locale"), BinOp::StrictEq, e_null()), e_str(""), e_var("locale")),
                        e_int(kind),
                        e_null(),
                    ]),
                ])),
                s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![
                    s_throw(e_new("IntlException", vec![e_str("IntlBreakIterator::create instance: unable to create break iterator")])),
                ], vec![], None),
                s_return(e_static_call("IntlRuleBasedBreakIterator", "__elephc_from_handle", vec![e_var("_handle")])),
            ]));
    }
    builder = builder.method(method("createCodePointInstance")
        .static_()
        .returns(t_class("IntlCodePointBreakIterator"))
        .body(vec![
            s_assign("_handle", e_call("__elephc_intl_open", vec![
                e_str("codepointbreakiter"), e_array(vec![e_str("")]),
            ])),
            s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![
                s_throw(e_new("IntlException", vec![e_str("IntlBreakIterator::createCodePointInstance(): unable to create break iterator")])),
            ], vec![], None),
            s_return(e_static_call("IntlCodePointBreakIterator", "__elephc_from_handle", vec![e_var("_handle")])),
        ]));
    for (name, params, returns, bridge) in [
        ("current", vec![], TypeExpr::Int, "current"),
        ("first", vec![], TypeExpr::Int, "first"),
        ("last", vec![], TypeExpr::Int, "last"),
        ("previous", vec![], TypeExpr::Int, "previous"),
        ("following", vec![("offset", TypeExpr::Int, None, false)], TypeExpr::Int, "following"),
        ("preceding", vec![("offset", TypeExpr::Int, None, false)], TypeExpr::Int, "preceding"),
        ("isBoundary", vec![("offset", TypeExpr::Int, None, false)], TypeExpr::Bool, "is_boundary"),
        ("getText", vec![], t_nullable(TypeExpr::Str), "get_text"),
        ("setText", vec![("text", t_mixed(), None, false)], TypeExpr::Bool, "set_text"),
        ("getErrorCode", vec![], TypeExpr::Int, "error_code"),
        ("getErrorMessage", vec![], TypeExpr::Str, "error_message"),
        // IntlCodePointBreakIterator exposes getLastCodePoint(); getPartsIterator() hands back
        // the iterator through the base type, so the method is reachable here as well.
        ("getLastCodePoint", vec![], TypeExpr::Int, "get_last_code_point"),
    ] {
        let body = if name == "getErrorCode" {
            vec![s_return(e_call("elephc_intl_error_code", vec![e_this_prop("__elephc_handle")]))]
        } else if name == "getErrorMessage" {
            vec![s_return(e_call("elephc_intl_error_message", vec![e_this_prop("__elephc_handle")]))]
        } else if name == "setText" {
            vec![
                s_assign("_text", e_cast(CastType::String, e_var("text"))),
                s_assign("_ok", e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str(bridge), e_array(vec![e_var("_text")]),
                ])),
                s_if(e_binop(e_var("_ok"), BinOp::StrictNotEq, crate::synthetic_class::e_bool(false)), vec![
                    s_expr(e_method_call(e_var("this"), "__elephc_sync_debug", vec![])),
                ], vec![], None),
                s_return(e_var("_ok")),
            ]
        } else {
            vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                e_str(bridge), e_array(params.iter().map(|(param, _, _, _)| e_var(param)).collect()),
            ]))]
        };
        builder = builder.method(instance_method(name, params, body).returns(returns));
    }
    builder = builder.method(method("next")
        .param_default("offset", t_nullable(TypeExpr::Int), e_null())
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                e_str("next"), e_call("__elephc_intl_args", vec![e_var("offset")]),
            ])),
        ]))
        .method(method("getLocale")
            .param("type", TypeExpr::Int)
            .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![e_str("get_locale"), e_array(vec![e_var("type")])]))]))
        .method(method("getIterator")
            .returns(t_class("IntlIterator"))
            .body(vec![
                s_assign("_items", e_array(vec![])),
                s_assign("_position", e_method_call(e_var("this"), "current", vec![])),
                s_assign("_p", e_method_call(e_var("this"), "first", vec![])),
                s_while(e_binop(e_var("_p"), BinOp::StrictNotEq, e_class_const("IntlBreakIterator", "DONE")), vec![
                    s_array_push("_items", e_var("_p")),
                    s_assign("_p", e_method_call(e_var("this"), "next", vec![e_null()])),
                ]),
                s_if(e_binop(e_var("_position"), BinOp::GtEq, e_int(0)), vec![
                    s_expr(e_method_call(e_var("this"), "following", vec![e_var("_position")])),
                ], vec![], None),
                s_return(e_static_call("IntlIterator", "__elephc_from_array", vec![e_var("_items")])),
            ]))
        .method(method("getPartsIterator")
            .param_default("type", TypeExpr::Int, e_int(0))
            .returns(t_class("IntlPartsIterator"))
            .body(vec![
                s_if(
                    e_binop(
                        e_binop(e_var("type"), BinOp::StrictNotEq, e_int(0)),
                        BinOp::And,
                        e_binop(
                            e_binop(e_var("type"), BinOp::StrictNotEq, e_int(1)),
                            BinOp::And,
                            e_binop(e_var("type"), BinOp::StrictNotEq, e_int(2)),
                        ),
                    ),
                    vec![s_throw(e_new("ValueError", vec![e_str(
                        "IntlBreakIterator::getPartsIterator(): Argument #1 ($type) must be one of IntlPartsIterator::KEY_SEQUENTIAL, IntlPartsIterator::KEY_LEFT, or IntlPartsIterator::KEY_RIGHT",
                    )]))],
                    vec![],
                    None,
                ),
                s_assign("_text", e_cast(CastType::String, e_method_call(e_var("this"), "getText", vec![]))),
                s_assign("_items", e_array(vec![])),
                s_assign("_position", e_method_call(e_var("this"), "current", vec![])),
                s_assign("_p", e_method_call(e_var("this"), "first", vec![])),
                s_assign("_prev", e_var("_p")),
                s_while(e_binop(e_var("_p"), BinOp::StrictNotEq, e_class_const("IntlBreakIterator", "DONE")), vec![
                    s_assign("_next", e_method_call(e_var("this"), "next", vec![e_null()])),
                    s_if(e_binop(e_var("_next"), BinOp::StrictEq, e_class_const("IntlBreakIterator", "DONE")), vec![
                        crate::synthetic_class::s_break(1),
                    ], vec![], None),
                    s_assign("_part", e_call("substr", vec![e_var("_text"), e_var("_prev"), e_binop(e_var("_next"), BinOp::Sub, e_var("_prev"))])),
                    s_if(e_binop(e_var("type"), BinOp::StrictEq, e_int(1)), vec![
                        s_array_assign("_items", e_var("_prev"), e_var("_part")),
                    ], vec![], Some(vec![
                        s_if(e_binop(e_var("type"), BinOp::StrictEq, e_int(2)), vec![
                            s_array_assign("_items", e_var("_next"), e_var("_part")),
                        ], vec![], Some(vec![
                            s_array_push("_items", e_var("_part")),
                        ])),
                    ])),
                    s_assign("_prev", e_var("_next")),
                    s_assign("_p", e_var("_next")),
                ]),
                s_if(e_binop(e_var("_position"), BinOp::GtEq, e_int(0)), vec![
                    s_expr(e_method_call(e_var("this"), "following", vec![e_var("_position")])),
                ], vec![], None),
                s_return(e_static_call("IntlPartsIterator", "__elephc_from_parts", vec![
                    e_var("_items"), e_var("this"), e_int(0),
                ])),
            ]));
    builder = builder.method(clone_handle_method("IntlBreakIterator"));
    builder.build()
}

/// Builds the `IntlRuleBasedBreakIterator` class.
fn build_rule_based_break_iterator_class() -> Stmt {
    let mut builder = class("IntlRuleBasedBreakIterator").extends("IntlBreakIterator");
    for (constant, value) in constants_for("IntlRuleBasedBreakIterator") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .method(method("__construct")
            .param_default("rules", t_mixed(), e_null())
            .param_default("compiled", t_mixed(), crate::synthetic_class::e_bool(false))
            .variadic("extra", Some(t_mixed()))
            .body(vec![
                s_if(e_binop(e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0)), vec![
                    s_throw(e_new("Error", vec![e_str("IntlRuleBasedBreakIterator object is already constructed")])),
                ], vec![], None),
                s_if(e_binop(e_call("count", vec![e_var("extra")]), BinOp::Gt, e_int(0)), vec![
                    s_throw(e_new("ArgumentCountError", vec![e_concat_all(vec![
                        e_str("IntlRuleBasedBreakIterator::__construct() expects at most 2 arguments, "),
                        e_cast(CastType::String, e_binop(e_call("count", vec![e_var("extra")]), BinOp::Add, e_int(2))),
                        e_str(" given"),
                    ])])),
                ], vec![], None),
                s_if(e_binop(e_var("rules"), BinOp::StrictEq, e_null()), vec![
                    s_throw(e_new("ArgumentCountError", vec![e_str("IntlRuleBasedBreakIterator::__construct() expects at least 1 argument, 0 given")])),
                ], vec![], None),
                s_if(
                    e_binop(
                        e_call("is_array", vec![e_var("compiled")]),
                        BinOp::Or,
                        e_call("is_object", vec![e_var("compiled")]),
                    ),
                    vec![s_throw(e_new("TypeError", vec![e_concat_all(vec![
                        e_str("IntlRuleBasedBreakIterator::__construct(): Argument #2 ($compiled) must be of type bool, "),
                        e_call("__elephc_intl_debug_type", vec![e_var("compiled")]),
                        e_str(" given"),
                    ])]))],
                    vec![],
                    None,
                ),
                s_assign("_rules", e_cast(CastType::String, e_var("rules"))),
                s_assign("_compiled", e_cast(CastType::Bool, e_var("compiled"))),
                // The internal `__elephc_from_handle` factory passes this sentinel so an
                // already-open ICU handle can be wrapped without compiling rules twice.
                s_if(e_binop(e_var("_rules"), BinOp::StrictEq, e_str("__elephc_internal_handle__")), vec![], vec![], Some(vec![
                    s_assign("_handle", e_call("__elephc_intl_open", vec![
                        e_str("rulebasedbreakiter"), e_array(vec![e_var("_rules"), e_var("_compiled")]),
                    ])),
                    s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![
                        s_assign("_message", e_call("__elephc_intl_invoke_string", vec![
                            e_str("intl"), e_str("intl_get_error_context"), e_array(vec![]),
                        ])),
                        s_throw(e_new("IntlException", vec![e_concat_all(vec![
                            e_str("IntlRuleBasedBreakIterator::__construct(): "),
                            e_var("_message"),
                        ])])),
                    ], vec![], None),
                    s_prop_assign(e_var("this"), "__elephc_handle", e_var("_handle")),
                ])),
            ]))
        .method(method("__elephc_from_handle")
            .static_()
            .param("handle", TypeExpr::Int)
            .returns(t_class("static"))
            .body(vec![
                s_assign("_object", e_new("IntlRuleBasedBreakIterator", vec![e_str("__elephc_internal_handle__"), crate::synthetic_class::e_bool(false)])),
                s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
                s_return(e_var("_object")),
            ]))
        .method(method("getRules")
            .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![e_str("get_rules"), e_array(vec![])]))]))
        .method(method("getBinaryRules")
            .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![e_str("get_binary_rules"), e_array(vec![])]))]))
        .method(method("getRuleStatus")
            .returns(TypeExpr::Int)
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![e_str("get_rule_status"), e_array(vec![])]))]))
        .method(method("getRuleStatusVec")
            .returns(t_union(vec![t_array(), TypeExpr::False]))
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![e_str("get_rule_status_vec"), e_array(vec![])]))]));
    builder.build()
}

/// Builds the `IntlCodePointBreakIterator` class.
fn build_code_point_break_iterator_class() -> Stmt {
    class("IntlCodePointBreakIterator")
        .extends("IntlBreakIterator")
        .method(method("__construct").private().body(vec![]))
        .method(method("__elephc_from_handle")
            .static_()
            .param("handle", TypeExpr::Int)
            .returns(t_class("static"))
            .body(vec![
                s_assign("_object", e_new("IntlCodePointBreakIterator", vec![])),
                s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
                s_expr(e_method_call(e_var("_object"), "__elephc_sync_debug", vec![])),
                s_return(e_var("_object")),
            ]))
        .method(method("getLastCodePoint")
            .returns(TypeExpr::Int)
            .body(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                e_str("get_last_code_point"), e_array(vec![]),
            ]))]))
        .build()
}

/// Builds the `IntlPartsIterator` class.
fn build_parts_iterator_class() -> Stmt {
    let mut builder = class("IntlPartsIterator")
        .extends("IntlIterator")
        .prop("__elephc_break_iterator", t_mixed(), Some(e_null()))
        .prop("__elephc_rule_status", TypeExpr::Int, Some(e_int(0)));
    for (constant, value) in constants_for("IntlPartsIterator") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .method(method("__elephc_from_parts")
            .static_()
            .param("items", t_mixed())
            .param("iterator", t_mixed())
            .param("status", TypeExpr::Int)
            .returns(t_class("static"))
            .body(vec![
                s_assign("_object", e_new("IntlPartsIterator", vec![])),
                s_prop_assign(e_var("_object"), "__elephc_items", e_var("items")),
                s_prop_assign(e_var("_object"), "__elephc_keys", e_call("array_keys", vec![e_var("items")])),
                s_prop_assign(e_var("_object"), "__elephc_break_iterator", e_var("iterator")),
                s_prop_assign(e_var("_object"), "__elephc_rule_status", e_var("status")),
                s_return(e_var("_object")),
            ]))
        .method(method("getBreakIterator")
            .returns(t_class("IntlBreakIterator"))
            .body(vec![
                s_assign("_iterator", e_this_prop("__elephc_break_iterator")),
                s_if(e_instance_of(e_var("_iterator"), "IntlBreakIterator"), vec![s_return(e_var("_iterator"))], vec![], None),
                s_throw(e_new("IntlException", vec![e_str("IntlPartsIterator::getBreakIterator(): no break iterator")])),
            ]))
        .method(method("getRuleStatus")
            .returns(TypeExpr::Int)
            .body(vec![s_return(e_this_prop("__elephc_rule_status"))]));
    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the usage gate sees a synthetic Intl class before injection.
    #[test]
    fn detects_intl_class_and_function_references() {
        let tokens = crate::lexer::tokenize("<?php $f = new IntlDateFormatter('en_US'); grapheme_strlen('x');").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        assert!(program_uses_intl(&program));
        assert!(program_needs_bridge(&program));
    }

    /// Verifies a scalar Intl builtin both selects the bridge and roots its helpers.
    #[test]
    fn scalar_builtin_roots_the_typed_helpers() {
        let tokens = crate::lexer::tokenize("<?php echo grapheme_strlen('x');").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        assert!(program_uses_intl(&program));
        assert!(program_needs_bridge(&program));
        assert!(prelude_helpers_for("grapheme_strlen").contains(&"__elephc_intl_invoke_mixed"));
        let declarations = build_surface(true);
        assert!(declarations.iter().any(|stmt| matches!(&stmt.kind, StmtKind::FunctionDecl { name, .. } if name == "__elephc_intl_invoke_mixed")));
    }

    /// Verifies every advertised Intl class receives one synthetic declaration.
    #[test]
    fn class_surface_declares_each_advertised_class_once() {
        let declarations = class_declarations();
        let names: Vec<&str> = declarations.iter().filter_map(|stmt| match &stmt.kind {
            StmtKind::ClassDecl { name, .. } => Some(name.as_str()),
            _ => None,
        }).collect();
        for name in CLASS_NAMES {
            assert_eq!(names.iter().filter(|seen| **seen == *name).count(), 1, "{name}");
        }
    }
}

/// Builds the `$zone` string expression for a PHP timezone argument.
fn timezone_arg_expr(name: &str) -> Expr {
    e_call("__elephc_intl_timezone_name", vec![e_var(name)])
}

/// Builds `IntlCalendar` with the factory, field accessors, and wall-time surface.
fn build_calendar_class() -> Stmt {
    let mut builder = class("IntlCalendar")
        .prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)));
    for (constant, value) in constants_for("IntlCalendar") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .method(method("__construct").protected().body(vec![
            s_if(e_binop(e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0)), vec![
                s_throw(e_new("IntlException", vec![e_str("IntlCalendar::__construct(): cannot call constructor twice")])),
            ], vec![], None),
        ]))
        .method(method("__elephc_from_handle").static_().param("handle", TypeExpr::Int).returns(t_class("static")).body(vec![
            s_assign("_object", e_new_static(vec![])),
            s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
            s_return(e_var("_object")),
        ]))
        .method(method("__elephc_open").static_().param("zone", TypeExpr::Str).param("locale", t_nullable(TypeExpr::Str)).returns(t_nullable(t_class("static"))).body(vec![
            s_assign("_locale", e_ternary(e_binop(e_var("locale"), BinOp::StrictEq, e_null()), e_str(""), e_var("locale"))),
            s_assign("_handle", e_call("__elephc_intl_open", vec![e_str("calendar"), e_array(vec![e_var("zone"), e_var("_locale")])])),
            s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![s_return(e_null())], vec![], None),
            s_assign("_object", e_new_static(vec![])),
            s_prop_assign(e_var("_object"), "__elephc_handle", e_var("_handle")),
            s_return(e_var("_object")),
        ]))
        .method(method("__elephc_call").param("method", TypeExpr::Str).param("args", t_mixed()).returns(t_mixed()).body(vec![
            s_return(e_call("__elephc_intl_call", vec![e_this_prop("__elephc_handle"), e_var("method"), e_var("args")])),
        ]))
        .method(method("createInstance").static_()
            .param_default("timezone", t_union(vec![t_class("IntlTimeZone"), t_class("DateTimeZone"), TypeExpr::Str, TypeExpr::Void]), e_null())
            .param_default("locale", t_nullable(TypeExpr::Str), e_null())
            .returns(t_class("IntlCalendar"))
            .body(vec![
                s_if(e_not(e_call("__elephc_intl_timezone_valid", vec![e_var("timezone")])), vec![
                    s_throw(e_new("IntlException", vec![e_str("IntlCalendar::createInstance(): passed IntlTimeZone is not properly constructed")])),
                ], vec![], None),
                s_assign("_zone", timezone_arg_expr("timezone")),
                s_assign("_object", e_static_call("IntlGregorianCalendar", "__elephc_open", vec![e_var("_zone"), e_var("locale")])),
                s_if(e_binop(e_var("_object"), BinOp::StrictEq, e_null()), vec![
                    s_throw(e_new("IntlException", vec![e_str("IntlCalendar::createInstance(): invalid timezone")])),
                ], vec![], None),
                s_return(e_var("_object")),
            ]))
        .method(method("fromDateTime").static_()
            .param("datetime", t_union(vec![t_class("DateTime"), TypeExpr::Str]))
            .param_default("locale", t_nullable(TypeExpr::Str), e_null())
            .returns(t_class("IntlCalendar"))
            .body(from_date_time_body()))
        .method(method("toDateTime").returns(t_class("DateTime")).body(vec![
            s_assign("_millis", e_method_call(e_var("this"), "getTime", vec![])),
            s_assign("_seconds", e_cast(CastType::Int, e_binop(e_cast(CastType::Float, e_var("_millis")), BinOp::Div, e_float(1000.0)))),
            s_assign("_zone", e_method_call(e_var("this"), "getTimeZone", vec![])),
            s_assign("_datetime", e_new("DateTime", vec![e_concat(e_str("@"), e_cast(CastType::String, e_var("_seconds")))])),
            s_try(
                vec![s_expr(e_method_call(e_var("_datetime"), "setTimezone", vec![e_new("DateTimeZone", vec![e_cast(CastType::String, e_method_call(e_var("_zone"), "getID", vec![]))])]))],
                vec![(vec!["Throwable"], Some("_error"), vec![
                    s_throw(e_new("IntlException", vec![
                        e_str("DateTimeZone constructor threw exception"), e_int(0), e_var("_error"),
                    ])),
                ])],
                None,
            ),
            s_return(e_var("_datetime")),
        ]))
        .method(method("getTimeZone").returns(t_class("IntlTimeZone")).body(vec![
            s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![e_str("get_timezone"), e_array(vec![])])),
            s_if(e_instance_of(e_var("_result"), "IntlTimeZone"), vec![s_return(e_var("_result"))], vec![], None),
            s_throw(e_new("IntlException", vec![e_str("IntlCalendar::getTimeZone(): could not clone TimeZone")])),
        ]))
        .method(method("getErrorCode").returns(TypeExpr::Int).body(vec![
            s_return(e_call("elephc_intl_error_code", vec![e_this_prop("__elephc_handle")])),
        ]))
        .method(method("getErrorMessage").returns(TypeExpr::Str).body(vec![
            s_return(e_call("elephc_intl_error_message", vec![e_this_prop("__elephc_handle")])),
        ]))
        .method(method("getKeywordValuesForLocale").static_()
            .param("keyword", TypeExpr::Str)
            .param("locale", TypeExpr::Str)
            .param("onlyCommon", TypeExpr::Bool)
            .returns(t_class("IntlIterator"))
            .body(vec![
                s_assign("_items", e_call("__elephc_intl_invoke_array", vec![
                    e_str("calendar"), e_str("intlcal_get_keyword_values_for_locale"),
                    e_array(vec![e_var("keyword"), e_var("locale"), e_var("onlyCommon")]),
                ])),
                s_return(e_static_call("IntlIterator", "__elephc_from_array", vec![e_var("_items")])),
            ]))
        .method(method("getAvailableLocales").static_().returns(t_array()).body(vec![
            s_return(e_call("__elephc_intl_invoke_array", vec![
                e_str("calendar"), e_str("intlcal_get_available_locales"), e_array(vec![]),
            ])),
        ]))
        .method(method("getNow").static_().returns(TypeExpr::Float).body(vec![
            s_return(e_call("__elephc_intl_invoke_float", vec![
                e_str("calendar"), e_str("intlcal_get_now"), e_array(vec![]),
            ])),
        ]))
        .method(method("set")
            .param("year", TypeExpr::Int)
            .param("month", TypeExpr::Int)
            .param_default("dayOfMonth", t_nullable(TypeExpr::Int), e_null())
            .param_default("hour", t_nullable(TypeExpr::Int), e_null())
            .param_default("minute", t_nullable(TypeExpr::Int), e_null())
            .param_default("second", t_nullable(TypeExpr::Int), e_null())
            .variadic("_extra", None)
            .body(calendar_set_body()))
        .method(method("setDate")
            .param("year", TypeExpr::Int)
            .param("month", TypeExpr::Int)
            .param("dayOfMonth", TypeExpr::Int)
            .body(vec![
                int32_guard(e_var("year"), "IntlCalendar::setDate()", 1, "year"),
                int32_guard(e_var("month"), "IntlCalendar::setDate()", 2, "month"),
                int32_guard(e_var("dayOfMonth"), "IntlCalendar::setDate()", 3, "dayOfMonth"),
                s_expr(e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str("set_date"),
                    e_array(vec![e_var("year"), e_var("month"), e_var("dayOfMonth")]),
                ])),
            ]))
        .method(method("setDateTime")
            .param("year", TypeExpr::Int)
            .param("month", TypeExpr::Int)
            .param("dayOfMonth", TypeExpr::Int)
            .param("hour", TypeExpr::Int)
            .param("minute", TypeExpr::Int)
            .param_default("second", t_nullable(TypeExpr::Int), e_null())
            .body(vec![
                int32_guard(e_var("year"), "IntlCalendar::setDateTime()", 1, "year"),
                int32_guard(e_var("month"), "IntlCalendar::setDateTime()", 2, "month"),
                int32_guard(e_var("dayOfMonth"), "IntlCalendar::setDateTime()", 3, "dayOfMonth"),
                int32_guard(e_var("hour"), "IntlCalendar::setDateTime()", 4, "hour"),
                int32_guard(e_var("minute"), "IntlCalendar::setDateTime()", 5, "minute"),
                s_assign("_second", e_ternary(e_binop(e_var("second"), BinOp::StrictEq, e_null()), e_int(0), e_var("second"))),
                int32_guard(e_var("_second"), "IntlCalendar::setDateTime()", 6, "second"),
                s_if(e_binop(e_call("func_num_args", vec![]), BinOp::StrictEq, e_int(5)), vec![
                    s_expr(e_method_call(e_var("this"), "__elephc_call", vec![
                        e_str("set_date_time"),
                        e_array(vec![e_var("year"), e_var("month"), e_var("dayOfMonth"), e_var("hour"), e_var("minute")]),
                    ])),
                ], vec![], Some(vec![
                    s_expr(e_method_call(e_var("this"), "__elephc_call", vec![
                        e_str("set_date_time"),
                        e_array(vec![e_var("year"), e_var("month"), e_var("dayOfMonth"), e_var("hour"), e_var("minute"), e_var("_second")]),
                    ])),
                ])),
            ]))
        .method(method("roll")
            .param("field", TypeExpr::Int)
            .param("value", t_mixed())
            .returns(TypeExpr::Bool)
            .body(calendar_roll_body()));
    for (name, params, bridge) in [
        ("get", vec![("field", TypeExpr::Int, None, false)], "get"),
        ("getTime", vec![], "get_time"),
        ("setTime", vec![("timestamp", t_mixed(), None, false)], "set_time"),
        ("add", vec![("field", TypeExpr::Int, None, false), ("value", TypeExpr::Int, None, false)], "add"),
        ("clear", vec![("field", t_nullable(TypeExpr::Int), Some(e_null()), false)], "clear"),
        ("getType", vec![], "get_type"),
        ("getLocale", vec![("type", TypeExpr::Int, Some(e_int(0)), false)], "locale"),
        ("setLenient", vec![("lenient", TypeExpr::Bool, None, false)], "set_lenient"),
        ("isLenient", vec![], "is_lenient"),
        ("inDaylightTime", vec![], "in_daylight_time"),
        ("isEquivalentTo", vec![("other", t_class("IntlCalendar"), None, false)], "is_equivalent_to"),
        ("equals", vec![("other", t_class("IntlCalendar"), None, false)], "equals"),
        ("after", vec![("other", t_class("IntlCalendar"), None, false)], "after"),
        ("before", vec![("other", t_class("IntlCalendar"), None, false)], "before"),
        ("isSet", vec![("field", TypeExpr::Int, None, false)], "is_set"),
        ("fieldDifference", vec![("timestamp", t_mixed(), None, false), ("field", TypeExpr::Int, None, false)], "field_difference"),
        ("getFirstDayOfWeek", vec![], "get_first_day_of_week"),
        ("setFirstDayOfWeek", vec![("dayOfWeek", TypeExpr::Int, None, false)], "set_first_day_of_week"),
        ("getMinimalDaysInFirstWeek", vec![], "get_minimal_days_in_first_week"),
        ("setMinimalDaysInFirstWeek", vec![("days", TypeExpr::Int, None, false)], "set_minimal_days_in_first_week"),
        ("getMinimum", vec![("field", TypeExpr::Int, None, false)], "get_minimum"),
        ("getMaximum", vec![("field", TypeExpr::Int, None, false)], "get_maximum"),
        ("getGreatestMinimum", vec![("field", TypeExpr::Int, None, false)], "get_greatest_minimum"),
        ("getLeastMaximum", vec![("field", TypeExpr::Int, None, false)], "get_least_maximum"),
        ("getActualMinimum", vec![("field", TypeExpr::Int, None, false)], "get_actual_minimum"),
        ("getActualMaximum", vec![("field", TypeExpr::Int, None, false)], "get_actual_maximum"),
        ("getDayOfWeekType", vec![("dayOfWeek", TypeExpr::Int, None, false)], "get_day_of_week_type"),
        ("getWeekendTransition", vec![("dayOfWeek", TypeExpr::Int, None, false)], "get_weekend_transition"),
        ("isWeekend", vec![("timestamp", t_nullable(TypeExpr::Float), Some(e_null()), false)], "is_weekend"),
        ("getRepeatedWallTimeOption", vec![], "get_repeated_wall_time_option"),
        ("setRepeatedWallTimeOption", vec![("option", TypeExpr::Int, None, false)], "set_repeated_wall_time_option"),
        ("getSkippedWallTimeOption", vec![], "get_skipped_wall_time_option"),
        ("setSkippedWallTimeOption", vec![("option", TypeExpr::Int, None, false)], "set_skipped_wall_time_option"),
        ("setTimeZone", vec![("timezone", t_union(vec![t_class("IntlTimeZone"), t_class("DateTimeZone"), TypeExpr::Str, TypeExpr::Void]), None, false)], "set_timezone"),
    ] {
        let args: Vec<Expr> = params.iter().map(|(param, _, _, _)| {
            if *param == "timezone" {
                timezone_arg_expr(param)
            } else if *param == "other" {
                e_call("__elephc_intl_handle_arg", vec![e_var(param)])
            } else {
                e_var(param)
            }
        }).collect();
        let mut body = Vec::new();
        if name == "clear" {
            // A nullable scalar packed into the bridge's Mixed array loses its tag; split the
            // call so each branch packs a concrete value.
            body.push(field_guard(e_var("field"), "IntlCalendar::clear()", 1, "field"));
            body.push(s_if(
                e_binop(e_var("field"), BinOp::StrictEq, e_null()),
                vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str("clear"), e_array(vec![]),
                ]))],
                vec![],
                Some(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str("clear"), e_array(vec![e_cast(CastType::Int, e_var("field"))]),
                ]))]),
            ));
            builder = builder.method(instance_method(name, params, body).returns(calendar_method_return(name)));
            continue;
        }
        if name == "isWeekend" {
            body.push(s_if(
                e_binop(e_var("timestamp"), BinOp::StrictEq, e_null()),
                vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str("is_weekend"), e_array(vec![]),
                ]))],
                vec![],
                Some(vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str("is_weekend"), e_array(vec![e_cast(CastType::Float, e_var("timestamp"))]),
                ]))]),
            ));
            builder = builder.method(instance_method(name, params, body).returns(calendar_method_return(name)));
            continue;
        }
        if name == "setTimeZone" {
            body.push(s_if(
                e_binop(e_var("timezone"), BinOp::StrictEq, e_null()),
                vec![s_return(crate::synthetic_class::e_bool(true))],
                vec![],
                None,
            ));
        }
        match name {
            "get" | "isSet" | "getActualMaximum" | "getActualMinimum" | "getMinimum" |
            "getMaximum" | "getGreatestMinimum" | "getLeastMaximum" | "fieldDifference" => {
                body.push(field_guard(e_var("field"), &format!("IntlCalendar::{name}()"), 1, "field"));
            }
            "getDayOfWeekType" | "getWeekendTransition" | "setFirstDayOfWeek" => {
                body.push(day_guard(e_var("dayOfWeek"), &format!("IntlCalendar::{name}()"), 1, "dayOfWeek"));
            }
            "setMinimalDaysInFirstWeek" => {
                let message = "IntlCalendar::setMinimalDaysInFirstWeek(): Argument #1 ($days) must be between 1 and 7".to_string();
                body.push(s_if(
                    e_binop(
                        e_binop(e_var("days"), BinOp::Lt, e_int(1)),
                        BinOp::Or,
                        e_binop(e_var("days"), BinOp::Gt, e_int(7)),
                    ),
                    vec![s_throw(e_new("ValueError", vec![e_str(&message)]))],
                    vec![],
                    None,
                ));
            }
            "setRepeatedWallTimeOption" => {
                let message = "IntlCalendar::setRepeatedWallTimeOption(): Argument #1 ($option) must be either IntlCalendar::WALLTIME_FIRST or IntlCalendar::WALLTIME_LAST".to_string();
                body.push(s_if(
                    e_binop(
                        e_binop(e_var("option"), BinOp::Lt, e_int(0)),
                        BinOp::Or,
                        e_binop(e_var("option"), BinOp::Gt, e_int(1)),
                    ),
                    vec![s_throw(e_new("ValueError", vec![e_str(&message)]))],
                    vec![],
                    None,
                ));
            }
            "equals" | "before" | "after" | "isEquivalentTo" => {
                let message = format!(
                    "IntlCalendar::{name}(): Argument #1 ($other) is uninitialized"
                );
                body.push(s_if(
                    e_binop(e_call("__elephc_intl_handle_arg", vec![e_var("other")]), BinOp::StrictEq, e_int(0)),
                    vec![s_throw(e_new("Error", vec![e_str(&message)]))],
                    vec![],
                    None,
                ));
            }
            "setSkippedWallTimeOption" => {
                let message = "IntlCalendar::setSkippedWallTimeOption(): Argument #1 ($option) must be one of IntlCalendar::WALLTIME_FIRST, IntlCalendar::WALLTIME_LAST, or IntlCalendar::WALLTIME_NEXT_VALID".to_string();
                body.push(s_if(
                    e_binop(
                        e_binop(e_var("option"), BinOp::Lt, e_int(0)),
                        BinOp::Or,
                        e_binop(e_var("option"), BinOp::Gt, e_int(2)),
                    ),
                    vec![s_throw(e_new("ValueError", vec![e_str(&message)]))],
                    vec![],
                    None,
                ));
            }
            _ => {}
        }
        body.push(s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![e_str(bridge), e_array(args)])));
        if name == "setFirstDayOfWeek" || name == "setMinimalDaysInFirstWeek" ||
            name == "setRepeatedWallTimeOption" || name == "setSkippedWallTimeOption" ||
            name == "setLenient" {
            body.push(s_return(crate::synthetic_class::e_bool(true)));
        } else {
            body.push(s_return(e_var("_result")));
        }
        builder = builder.method(instance_method(name, params, body).returns(calendar_method_return(name)));
    }
    builder = builder.method(clone_handle_method("IntlCalendar"));
    builder.build()
}

/// Returns the PHP-visible result type of one `IntlCalendar` method.
fn calendar_method_return(name: &str) -> TypeExpr {
    match name {
        "getTime" => t_union(vec![TypeExpr::Float, TypeExpr::False]),
        "get" | "fieldDifference" | "getMinimum" | "getMaximum" | "getGreatestMinimum" |
        "getLeastMaximum" | "getActualMinimum" | "getActualMaximum" | "getDayOfWeekType" |
        "getWeekendTransition" | "getFirstDayOfWeek" | "getMinimalDaysInFirstWeek" => {
            t_union(vec![TypeExpr::Int, TypeExpr::False])
        }
        "getType" => TypeExpr::Str,
        "getLocale" => t_union(vec![TypeExpr::Str, TypeExpr::False]),
        "getRepeatedWallTimeOption" | "getSkippedWallTimeOption" => TypeExpr::Int,
        "setTime" | "add" | "clear" | "setLenient" | "isLenient" | "inDaylightTime" |
        "isEquivalentTo" | "equals" | "after" | "before" | "isSet" | "setFirstDayOfWeek" |
        "setMinimalDaysInFirstWeek" | "isWeekend" | "setRepeatedWallTimeOption" |
        "setSkippedWallTimeOption" | "setTimeZone" => TypeExpr::Bool,
        _ => t_mixed(),
    }
}

/// Builds a PHP `ValueError` guard for an argument that must fit a signed 32-bit ICU integer.
fn int32_guard(expr: Expr, caller: &str, index: i64, name: &str) -> Stmt {
    let message = format!(
        "{caller}: Argument #{index} (${name}) must be between -2147483648 and 2147483647"
    );
    s_if(
        e_binop(
            e_binop(expr.clone(), BinOp::Lt, e_int(-2_147_483_648)),
            BinOp::Or,
            e_binop(expr, BinOp::Gt, e_int(2_147_483_647)),
        ),
        vec![s_throw(e_new("ValueError", vec![e_str(&message)]))],
        vec![],
        None,
    )
}

/// Builds a PHP `ValueError` guard for a calendar field number.
fn field_guard(expr: Expr, caller: &str, index: i64, name: &str) -> Stmt {
    let message = format!("{caller}: Argument #{index} (${name}) must be a valid field");
    s_if(
        e_binop(
            e_binop(expr.clone(), BinOp::Lt, e_int(0)),
            BinOp::Or,
            e_binop(
                expr,
                BinOp::GtEq,
                e_class_const("IntlCalendar", "FIELD_FIELD_COUNT"),
            ),
        ),
        vec![s_throw(e_new("ValueError", vec![e_str(&message)]))],
        vec![],
        None,
    )
}

/// Builds a PHP `ValueError` guard for a calendar day-of-week number.
fn day_guard(expr: Expr, caller: &str, index: i64, name: &str) -> Stmt {
    let message = format!(
        "{caller}: Argument #{index} (${name}) must be a valid day of the week"
    );
    s_if(
        e_binop(
            e_binop(expr.clone(), BinOp::Lt, e_int(1)),
            BinOp::Or,
            e_binop(expr, BinOp::Gt, e_int(7)),
        ),
        vec![s_throw(e_new("ValueError", vec![e_str(&message)]))],
        vec![],
        None,
    )
}

/// Builds `IntlCalendar::set()`, which is field/value with two args and date/time otherwise.
fn calendar_set_body() -> Vec<Stmt> {
    vec![
        s_assign("_count", e_call("func_num_args", vec![])),
        // The deprecation is emitted before the arity error, as php-src does.
        s_if(e_binop(e_var("_count"), BinOp::Gt, e_int(2)), vec![
            s_expr(e_call("trigger_error", vec![
                e_str("Calling IntlCalendar::set() with more than 2 arguments is deprecated, use either IntlCalendar::setDate() or IntlCalendar::setDateTime() instead"),
                e_const("E_USER_DEPRECATED"),
            ])),
        ], vec![], None),
        s_if(e_binop(e_var("_count"), BinOp::Gt, e_int(6)), vec![
            s_throw(e_new("ArgumentCountError", vec![e_concat_all(vec![
                e_str("IntlCalendar::set() expects at most 6 arguments, "),
                e_cast(CastType::String, e_var("_count")),
                e_str(" given"),
            ])])),
        ], vec![], None),
        s_if(e_binop(e_var("_count"), BinOp::StrictEq, e_int(2)), vec![
            field_guard(e_var("year"), "IntlCalendar::set()", 1, "year"),
            int32_guard(e_var("month"), "IntlCalendar::set()", 2, "month"),
            s_expr(e_method_call(e_var("this"), "__elephc_call", vec![
                e_str("set"), e_array(vec![e_var("year"), e_var("month")]),
            ])),
            s_return(crate::synthetic_class::e_bool(true)),
        ], vec![], None),
        s_if(e_binop(e_var("_count"), BinOp::StrictEq, e_int(4)), vec![
            s_throw(e_new("ArgumentCountError", vec![e_str("IntlCalendar::set() has no variant with exactly 4 parameters")])),
        ], vec![], None),
        int32_guard(e_var("year"), "IntlCalendar::set()", 1, "year"),
        int32_guard(e_var("month"), "IntlCalendar::set()", 2, "month"),
        s_assign("_day", e_ternary(e_binop(e_var("dayOfMonth"), BinOp::StrictEq, e_null()), e_int(1), e_var("dayOfMonth"))),
        s_assign("_hour", e_ternary(e_binop(e_var("hour"), BinOp::StrictEq, e_null()), e_int(0), e_var("hour"))),
        s_assign("_minute", e_ternary(e_binop(e_var("minute"), BinOp::StrictEq, e_null()), e_int(0), e_var("minute"))),
        s_assign("_second", e_ternary(e_binop(e_var("second"), BinOp::StrictEq, e_null()), e_int(0), e_var("second"))),
        int32_guard(e_var("_day"), "IntlCalendar::set()", 3, "dayOfMonth"),
        int32_guard(e_var("_hour"), "IntlCalendar::set()", 4, "hour"),
        int32_guard(e_var("_minute"), "IntlCalendar::set()", 5, "minute"),
        int32_guard(e_var("_second"), "IntlCalendar::set()", 6, "second"),
        s_if(e_binop(e_var("_count"), BinOp::StrictEq, e_int(5)), vec![
            s_expr(e_method_call(e_var("this"), "__elephc_call", vec![
                e_str("set_date_time"),
                e_array(vec![e_var("year"), e_var("month"), e_var("_day"), e_var("_hour"), e_var("_minute")]),
            ])),
        ], vec![], Some(vec![
            s_expr(e_method_call(e_var("this"), "__elephc_call", vec![
                e_str("set_date_time"),
                e_array(vec![e_var("year"), e_var("month"), e_var("_day"), e_var("_hour"), e_var("_minute"), e_var("_second")]),
            ])),
        ])),
        s_return(crate::synthetic_class::e_bool(true)),
    ]
}

/// Builds `IntlCalendar::roll()`, accepting PHP 8's deprecated boolean direction.
fn calendar_roll_body() -> Vec<Stmt> {
    vec![
        s_if(
            e_binop(
                e_binop(e_var("field"), BinOp::Lt, e_int(0)),
                BinOp::Or,
                e_binop(e_var("field"), BinOp::GtEq, e_class_const("IntlCalendar", "FIELD_FIELD_COUNT")),
            ),
            vec![s_throw(e_new("ValueError", vec![e_str(
                "IntlCalendar::roll(): Argument #1 ($field) must be a valid field",
            )]))],
            vec![],
            None,
        ),
        s_if(e_binop(e_call("is_bool", vec![e_var("value")]), BinOp::StrictEq, crate::synthetic_class::e_bool(true)), vec![
            s_expr(e_call("trigger_error", vec![
                e_str("IntlCalendar::roll(): Passing bool is deprecated, use 1 or -1 instead"),
                e_const("E_USER_DEPRECATED"),
            ])),
            s_assign("value", e_ternary(e_var("value"), e_int(1), e_int(-1))),
        ], vec![], None),
        int32_guard(e_cast(CastType::Int, e_var("value")), "IntlCalendar::roll()", 2, "value"),
        s_return(e_method_call(e_var("this"), "__elephc_call", vec![
            e_str("roll"), e_array(vec![e_var("field"), e_var("value")]),
        ])),
    ]
}

/// Builds `IntlCalendar::fromDateTime()`, preserving the source timezone and instant.
fn from_date_time_body() -> Vec<Stmt> {
    vec![
        s_assign("_zone", e_str("")),
        s_assign("_timestamp", e_int(0)),
        s_if(e_call("is_string", vec![e_var("datetime")]), vec![
            s_assign("_text", e_cast(CastType::String, e_call("trim", vec![e_var("datetime")]))),
            s_assign("_space", e_call("strrpos", vec![e_var("_text"), e_str(" ")])),
            s_if(e_binop(e_var("_space"), BinOp::StrictNotEq, crate::synthetic_class::e_bool(false)), vec![
                s_assign("_zone", e_cast(CastType::String, e_call("substr", vec![e_var("_text"), e_binop(e_var("_space"), BinOp::Add, e_int(1))]))),
                s_assign("_text", e_cast(CastType::String, e_call("substr", vec![e_var("_text"), e_int(0), e_var("_space")]))),
                s_if(e_not(e_call("__elephc_intl_is_timezone", vec![e_var("_zone")])), vec![
                    // A trailing numeric offset (`+03:00`) is the DateTime's fixed zone;
                    // keep the full text so strtotime() still applies the offset.
                    s_assign("_offset_zone", e_call("__elephc_intl_offset_zone", vec![e_var("_zone")])),
                    s_if(e_binop(e_var("_offset_zone"), BinOp::StrictNotEq, e_str("")), vec![
                        s_assign("_zone", e_var("_offset_zone")),
                        s_assign("_text", e_cast(CastType::String, e_call("trim", vec![e_var("datetime")]))),
                    ], vec![], Some(vec![
                        s_assign("_text", e_cast(CastType::String, e_call("trim", vec![e_var("datetime")]))),
                        s_assign("_zone", e_str("")),
                    ])),
                ], vec![], None),
            ], vec![], None),
            s_assign("_saved", e_call("date_default_timezone_get", vec![])),
            s_if(e_binop(e_var("_zone"), BinOp::StrictNotEq, e_str("")), vec![
                s_expr(e_call("date_default_timezone_set", vec![e_var("_zone")])),
            ], vec![], None),
            s_assign("_timestamp", e_cast(CastType::Int, e_call("strtotime", vec![e_var("_text")]))),
            s_expr(e_call("date_default_timezone_set", vec![e_var("_saved")])),
        ], vec![], Some(vec![
            s_assign("_timestamp", e_method_call(e_var("datetime"), "getTimestamp", vec![])),
            s_assign("_zone", e_cast(CastType::String, e_method_call(e_method_call(e_var("datetime"), "getTimezone", vec![]), "getName", vec![]))),
        ])),
        s_if(e_binop(e_var("_zone"), BinOp::StrictEq, e_str("")), vec![
            s_assign("_zone", e_call("date_default_timezone_get", vec![])),
        ], vec![], None),
        s_assign("_object", e_static_call("IntlGregorianCalendar", "__elephc_open", vec![e_var("_zone"), e_var("locale")])),
        s_if(e_binop(e_var("_object"), BinOp::StrictEq, e_null()), vec![
            s_throw(e_new("IntlException", vec![e_str("IntlCalendar::fromDateTime(): error creating ICU Calendar object")])),
        ], vec![], None),
        s_expr(e_method_call(e_var("_object"), "setTime", vec![e_binop(e_cast(CastType::Float, e_var("_timestamp")), BinOp::Mul, e_float(1000.0))])),
        s_return(e_var("_object")),
    ]
}

/// Builds `IntlGregorianCalendar`, including the public constructor overloads and factories.
fn build_gregorian_calendar_class() -> Stmt {
    let mut builder = class("IntlGregorianCalendar").extends("IntlCalendar");
    for (constant, value) in constants_for("IntlGregorianCalendar") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .method(method("__construct")
            .param_untyped_default("timezoneOrYear", e_null())
            .param_untyped_default("localeOrMonth", e_null())
            .param_untyped_default("day", e_null())
            .param_untyped_default("hour", e_null())
            .param_untyped_default("minute", e_null())
            .param_untyped_default("second", e_null())
            .body(vec![
                s_if(e_binop(e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0)), vec![
                    s_throw(e_new("IntlException", vec![e_str("IntlGregorianCalendar::__construct(): cannot call constructor twice")])),
                ], vec![], None),
                s_assign("_count", e_call("func_num_args", vec![])),
                s_assign("_zone", e_call("date_default_timezone_get", vec![])),
                s_assign("_locale", e_str("")),
                s_if(e_binop(e_call("is_int", vec![e_var("timezoneOrYear")]), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![
                    s_if(e_binop(e_var("timezoneOrYear"), BinOp::StrictNotEq, e_null()), vec![
                        s_assign("_zone", timezone_arg_expr("timezoneOrYear")),
                    ], vec![], None),
                    s_if(e_binop(e_var("localeOrMonth"), BinOp::StrictNotEq, e_null()), vec![
                        s_assign("_locale", e_cast(CastType::String, e_var("localeOrMonth"))),
                    ], vec![], None),
                ], vec![], None),
                s_assign("_handle", e_call("__elephc_intl_open", vec![e_str("calendar"), e_array(vec![e_var("_zone"), e_var("_locale")])])),
                s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![
                    s_throw(e_new("IntlException", vec![e_str("IntlGregorianCalendar::__construct(): calendar creation failed")])),
                ], vec![], None),
                s_prop_assign(e_var("this"), "__elephc_handle", e_var("_handle")),
                s_if(
                    e_binop(
                        e_binop(e_var("_count"), BinOp::Gt, e_int(2)),
                        BinOp::And,
                        e_binop(e_var("timezoneOrYear"), BinOp::StrictNotEq, e_null()),
                    ),
                    vec![
                    s_expr(e_call("trigger_error", vec![
                        e_str("Calling IntlGregorianCalendar::__construct() with more than 2 arguments is deprecated, use either IntlGregorianCalendar::createFromDate() or IntlGregorianCalendar::createFromDateTime() instead"),
                        e_const("E_USER_DEPRECATED"),
                    ])),
                ], vec![], None),
                s_if(e_binop(e_call("is_int", vec![e_var("timezoneOrYear")]), BinOp::StrictEq, crate::synthetic_class::e_bool(true)), vec![
                    s_if(e_binop(e_var("timezoneOrYear"), BinOp::StrictNotEq, e_null()), vec![int32_guard(e_cast(CastType::Int, e_var("timezoneOrYear")), "IntlGregorianCalendar::__construct()", 1, "timezoneOrYear")], vec![], None),
                    s_if(e_binop(e_var("localeOrMonth"), BinOp::StrictNotEq, e_null()), vec![int32_guard(e_cast(CastType::Int, e_var("localeOrMonth")), "IntlGregorianCalendar::__construct()", 2, "localeOrMonth")], vec![], None),
                    s_if(e_binop(e_var("day"), BinOp::StrictNotEq, e_null()), vec![int32_guard(e_cast(CastType::Int, e_var("day")), "IntlGregorianCalendar::__construct()", 3, "day")], vec![], None),
                    s_if(e_binop(e_var("hour"), BinOp::StrictNotEq, e_null()), vec![int32_guard(e_cast(CastType::Int, e_var("hour")), "IntlGregorianCalendar::__construct()", 4, "hour")], vec![], None),
                    s_if(e_binop(e_var("minute"), BinOp::StrictNotEq, e_null()), vec![int32_guard(e_cast(CastType::Int, e_var("minute")), "IntlGregorianCalendar::__construct()", 5, "minute")], vec![], None),
                    s_if(e_binop(e_var("second"), BinOp::StrictNotEq, e_null()), vec![int32_guard(e_cast(CastType::Int, e_var("second")), "IntlGregorianCalendar::__construct()", 6, "second")], vec![], None),
                    s_expr(e_method_call(e_var("this"), "__elephc_call", vec![
                        e_str("set_date_time"),
                        e_array(vec![
                            e_cast(CastType::Int, e_var("timezoneOrYear")),
                            e_cast(CastType::Int, e_var("localeOrMonth")),
                            e_ternary(e_binop(e_var("day"), BinOp::StrictEq, e_null()), e_int(1), e_cast(CastType::Int, e_var("day"))),
                            e_ternary(e_binop(e_var("hour"), BinOp::StrictEq, e_null()), e_int(0), e_cast(CastType::Int, e_var("hour"))),
                            e_ternary(e_binop(e_var("minute"), BinOp::StrictEq, e_null()), e_int(0), e_cast(CastType::Int, e_var("minute"))),
                            e_ternary(e_binop(e_var("second"), BinOp::StrictEq, e_null()), e_int(0), e_cast(CastType::Int, e_var("second"))),
                        ]),
                    ])),
                ], vec![], None),
            ]))
        .method(method("createFromDate").static_()
            .param("year", TypeExpr::Int).param("month", TypeExpr::Int).param("dayOfMonth", TypeExpr::Int)
            .param_default("locale", t_nullable(TypeExpr::Str), e_null())
            .returns(t_class("IntlGregorianCalendar"))
            .body(vec![
                int32_guard(e_var("year"), "IntlGregorianCalendar::createFromDate()", 1, "year"),
                int32_guard(e_var("month"), "IntlGregorianCalendar::createFromDate()", 2, "month"),
                int32_guard(e_var("dayOfMonth"), "IntlGregorianCalendar::createFromDate()", 3, "dayOfMonth"),
                s_assign("_object", e_self_call("__elephc_open", vec![e_call("date_default_timezone_get", vec![]), e_var("locale")])),
                s_if(e_binop(e_var("_object"), BinOp::StrictEq, e_null()), vec![s_throw(e_new("IntlException", vec![e_str("IntlGregorianCalendar::createFromDate(): calendar creation failed")]))], vec![], None),
                s_expr(e_method_call(e_var("_object"), "setDate", vec![e_var("year"), e_var("month"), e_var("dayOfMonth")])),
                s_return(e_var("_object")),
            ]))
        .method(method("createFromDateTime").static_()
            .param("year", TypeExpr::Int).param("month", TypeExpr::Int).param("dayOfMonth", TypeExpr::Int)
            .param("hour", TypeExpr::Int).param("minute", TypeExpr::Int)
            .param_default("second", t_nullable(TypeExpr::Int), e_null())
            .param_default("locale", t_nullable(TypeExpr::Str), e_null())
            .returns(t_class("IntlGregorianCalendar"))
            .body(vec![
                int32_guard(e_var("year"), "IntlGregorianCalendar::createFromDateTime()", 1, "year"),
                int32_guard(e_var("month"), "IntlGregorianCalendar::createFromDateTime()", 2, "month"),
                int32_guard(e_var("dayOfMonth"), "IntlGregorianCalendar::createFromDateTime()", 3, "dayOfMonth"),
                int32_guard(e_var("hour"), "IntlGregorianCalendar::createFromDateTime()", 4, "hour"),
                int32_guard(e_var("minute"), "IntlGregorianCalendar::createFromDateTime()", 5, "minute"),
                int32_guard(e_cast(CastType::Int, e_var("second")), "IntlGregorianCalendar::createFromDateTime()", 6, "second"),
                s_assign("_object", e_self_call("__elephc_open", vec![e_call("date_default_timezone_get", vec![]), e_var("locale")])),
                s_if(e_binop(e_var("_object"), BinOp::StrictEq, e_null()), vec![s_throw(e_new("IntlException", vec![e_str("IntlGregorianCalendar::createFromDateTime(): calendar creation failed")]))], vec![], None),
                s_expr(e_method_call(e_var("_object"), "setDateTime", vec![e_var("year"), e_var("month"), e_var("dayOfMonth"), e_var("hour"), e_var("minute"), e_var("second")])),
                s_return(e_var("_object")),
            ]))
        .method(method("getGregorianChange").returns(TypeExpr::Float).body(vec![s_return(e_call("__elephc_intl_call", vec![e_this_prop("__elephc_handle"), e_str("gregorian_change"), e_array(vec![])]))]))
        .method(method("setGregorianChange").param("timestamp", TypeExpr::Float).returns(TypeExpr::Bool).body(vec![s_return(e_call("__elephc_intl_call", vec![e_this_prop("__elephc_handle"), e_str("set_gregorian_change"), e_array(vec![e_var("timestamp")])]))]))
        .method(method("isLeapYear").param("year", TypeExpr::Int).returns(TypeExpr::Bool).body(vec![s_return(e_call("__elephc_intl_call", vec![e_this_prop("__elephc_handle"), e_str("is_leap_year"), e_array(vec![e_var("year")])]))]));
    builder = builder.method(clone_handle_method("IntlCalendar"));
    builder.build()
}

/// Builds the `Collator` class.
fn build_collator_class() -> Stmt {
    let mut builder = class("Collator").prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)));
    for (constant, value) in constants_for("Collator") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .method(method("__construct").param("locale", TypeExpr::Str).body(vec![
            s_if(e_binop(e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0)), vec![
                s_throw(e_new("Error", vec![e_str("Collator object is already constructed")])),
            ], vec![], None),
            s_assign("_handle", e_call("__elephc_intl_open", vec![e_str("collator"), e_array(vec![e_var("locale")])])),
            s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![
                s_assign("_message", e_call("__elephc_intl_invoke_string", vec![
                    e_str("intl"), e_str("intl_get_error_context"), e_array(vec![]),
                ])),
                s_throw(e_new("IntlException", vec![e_concat(e_str("Collator::__construct(): "), e_var("_message"))])),
            ], vec![], None),
            s_prop_assign(e_var("this"), "__elephc_handle", e_var("_handle")),
        ]))
        .method(method("__elephc_from_handle").static_().param("handle", TypeExpr::Int).returns(t_class("static")).body(vec![
            s_assign("_object", e_new("Collator", vec![e_str("en_US")])),
            s_assign("_dummy", e_prop(e_var("_object"), "__elephc_handle")),
            s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
            s_expr(e_call("elephc_intl_close", vec![e_var("_dummy")])),
            s_return(e_var("_object")),
        ]))
        .method(method("__elephc_call").param("method", TypeExpr::Str).param("args", t_mixed()).body(vec![
            s_if(e_binop(e_this_prop("__elephc_handle"), BinOp::StrictEq, e_int(0)), vec![
                s_throw(e_new("Error", vec![e_str("Object not initialized")])),
            ], vec![], None),
            s_return(e_call("__elephc_intl_call", vec![e_this_prop("__elephc_handle"), e_var("method"), e_var("args")])),
        ]))
        .method(method("create").static_().param("locale", TypeExpr::Str).returns(t_nullable(t_class("Collator"))).body(vec![
            s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("collator"), e_str("collator_create"), e_array(vec![e_var("locale")]),
            ])),
            s_if(e_instance_of(e_var("_result"), "Collator"), vec![
                s_return(e_var("_result")),
            ], vec![], None),
            s_expr(e_call("__elephc_intl_global_fail", vec![
                e_var("_result"), e_str("Collator::create()"),
            ])),
            s_return(e_null()),
        ]))
        .method(method("getDefault").static_().returns(t_nullable(t_class("Collator"))).body(vec![
            s_return(e_new("Collator", vec![e_call("locale_get_default", vec![])])),
        ]))
        .method(method("setDefault").static_().param("object", t_class("Collator")).returns(TypeExpr::Bool).body(vec![
            s_return(crate::synthetic_class::e_bool(true)),
        ]))
        .method(method("getDisplayName").static_().param("objectLocale", TypeExpr::Str).param_default("displayLocale", t_nullable(TypeExpr::Str), e_null()).returns(t_union(vec![TypeExpr::Str, TypeExpr::False])).body(vec![
            s_return(e_call("__elephc_intl_invoke", vec![e_str("collator"), e_str("getDisplayName"), e_array(vec![e_var("objectLocale"), e_var("displayLocale")])])),
        ]))
        .method(method("getAvailableLocales").static_().returns(t_array()).body(vec![
            s_return(e_call("__elephc_intl_invoke_array", vec![e_str("collator"), e_str("getAvailableLocales"), e_array(vec![])])),
        ]));
    for (name, params, outputs, bridge) in [
        ("compare", vec![("string1", TypeExpr::Str, None, false), ("string2", TypeExpr::Str, None, false)], Vec::<&str>::new(), "compare"),
        ("getAttribute", vec![("attribute", TypeExpr::Int, None, false)], vec![], "get_attribute"),
        ("setAttribute", vec![("attribute", TypeExpr::Int, None, false), ("value", TypeExpr::Int, None, false)], vec![], "set_attribute"),
        ("getStrength", vec![], vec![], "get_strength"),
        ("setStrength", vec![("strength", TypeExpr::Int, None, false)], vec![], "set_strength"),
        ("getSortKey", vec![("string", TypeExpr::Str, None, false)], vec![], "sort_key"),
        ("getLocale", vec![("type", TypeExpr::Int, Some(e_int(0)), false)], vec![], "locale"),
        ("getErrorCode", vec![], vec![], "error_code"),
        ("getErrorMessage", vec![], vec![], "error_message"),
        ("getVariableTop", vec![], vec![], "variable_top"),
        ("setVariableTop", vec![("variableTop", TypeExpr::Str, None, false)], vec![], "set_variable_top"),
        ("restoreVariableTop", vec![("variableTop", TypeExpr::Str, None, false)], vec![], "restore_variable_top"),
    ] {
        let body = if name == "getErrorCode" {
            vec![s_return(e_call("elephc_intl_error_code", vec![e_this_prop("__elephc_handle")]))]
        } else if name == "getErrorMessage" {
            vec![s_return(e_call("elephc_intl_error_message", vec![e_this_prop("__elephc_handle")]))]
        } else if name == "getVariableTop" {
            vec![s_return(e_int(0))]
        } else if name == "setVariableTop" || name == "restoreVariableTop" {
            vec![s_return(crate::synthetic_class::e_bool(true))]
        } else if name == "getAttribute" {
            vec![
                s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str(bridge), e_array(vec![e_var("attribute")]),
                ])),
                s_return(e_call("__elephc_intl_object_fail", vec![
                    e_var("this"), e_var("_result"), e_str("Collator::getAttribute()"),
                ])),
            ]
        } else if name == "getLocale" {
            vec![
                s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str("locale"), e_array(vec![e_var("type")]),
                ])),
                s_return(e_call("__elephc_intl_object_fail", vec![
                    e_var("this"), e_var("_result"), e_str("Collator::getLocale()"),
                ])),
            ]
        } else {
            let mut body = vec![s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![e_str(bridge), e_array(params.iter().map(|(p,_,_,_)| e_var(p)).collect())]))];
            if outputs.is_empty() { body.push(s_return(e_var("_result"))); } else { body.push(s_return(e_index(e_var("_result"), e_int(0)))); }
            body
        };
        builder = builder.method(instance_method(name, params, body));
    }
    // Sorting methods mutate their by-reference array. ICU performs the comparison in the
    // shim, so the generated method only publishes the sorted copy.
    for (php_name, bridge_name) in [
        ("sort", "sort"),
        ("asort", "asort"),
        ("sortWithSortKeys", "sort_with_sort_keys"),
    ] {
        let params = if php_name == "sortWithSortKeys" {
            vec![("array", t_mixed(), None, true)]
        } else {
            vec![("array", t_mixed(), None, true), ("flags", TypeExpr::Int, Some(e_int(0)), false)]
        };
        let mut bridge_args = vec![e_var("array")];
        if php_name != "sortWithSortKeys" {
            bridge_args.push(e_var("flags"));
        }
        builder = builder.method(instance_method(
            php_name,
            params,
            vec![
                s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str(bridge_name), e_array(bridge_args),
                ])),
                s_if(e_binop(e_var("_result"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![
                    s_return(crate::synthetic_class::e_bool(false)),
                ], vec![], None),
                s_assign("array", e_index(e_var("_result"), e_int(1))),
                s_return(e_index(e_var("_result"), e_int(0))),
            ],
        ));
    }
    builder = builder.method(clone_handle_method("Collator"));
    builder.build()
}

/// Builds the `NumberFormatter` class.
fn build_number_formatter_class() -> Stmt {
    let mut builder = class("NumberFormatter").prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)));
    for (constant, value) in constants_for("NumberFormatter") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .method(method("__construct")
            .param_default("locale", t_nullable(TypeExpr::Str), e_null())
            .param_default("style", TypeExpr::Int, e_class_const("NumberFormatter", "DECIMAL"))
            .param_default("pattern", t_nullable(TypeExpr::Str), e_null())
            .body(vec![
            s_if(e_binop(e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0)), vec![
                s_throw(e_new("IntlException", vec![e_str("NumberFormatter::__construct(): cannot call constructor twice")])),
            ], vec![], None),
            s_if(
                e_binop(
                    e_binop(e_var("locale"), BinOp::StrictNotEq, e_null()),
                    BinOp::And,
                    e_binop(
                        e_binop(e_var("locale"), BinOp::StrictNotEq, e_str("")),
                        BinOp::And,
                        e_not(e_call("__elephc_intl_locale_valid", vec![e_var("locale")])),
                    ),
                ),
                vec![s_throw(e_new("ValueError", vec![e_concat_all(vec![
                    e_str("NumberFormatter::__construct(): Argument #1 ($locale) \""),
                    e_var("locale"), e_str("\" is invalid"),
                ])]))],
                vec![],
                None,
            ),
            s_assign("_handle", e_call("__elephc_intl_open", vec![e_str("number"), e_array(vec![e_var("locale"), e_var("style"), e_var("pattern")])])),
            s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![
                s_throw(e_new("IntlException", vec![e_str("NumberFormatter::__construct(): unable to create formatter")])),
            ], vec![], None),
            s_prop_assign(e_var("this"), "__elephc_handle", e_var("_handle")),
        ]))
        .method(method("__elephc_from_handle").static_().param("handle", TypeExpr::Int).returns(t_class("static")).body(vec![
            s_assign("_object", e_new("NumberFormatter", vec![e_str("en_US"), e_int(0)])),
            s_assign("_dummy", e_prop(e_var("_object"), "__elephc_handle")),
            s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
            s_expr(e_call("elephc_intl_close", vec![e_var("_dummy")])),
            s_return(e_var("_object")),
        ]))
        .method(method("__elephc_call").param("method", TypeExpr::Str).param("args", t_mixed()).body(vec![
            s_return(e_call("__elephc_intl_call", vec![e_this_prop("__elephc_handle"), e_var("method"), e_var("args")])),
        ]))
        .method(method("create").static_().param("locale", TypeExpr::Str).param("style", TypeExpr::Int).param_default("pattern", t_nullable(TypeExpr::Str), e_null()).returns(t_nullable(t_class("NumberFormatter"))).body(vec![
            s_if(e_not(e_call("__elephc_intl_locale_valid", vec![e_var("locale")])), vec![
                s_throw(e_new("ValueError", vec![e_concat_all(vec![
                    e_str("NumberFormatter::create(): Argument #1 ($locale) \""),
                    e_var("locale"), e_str("\" is invalid"),
                ])]))],
                vec![], None),
            s_return(e_new("NumberFormatter", vec![e_var("locale"), e_var("style"), e_var("pattern")])),
        ]));
    for (name, params, outputs, bridge) in [
        ("format", vec![("num", t_union(vec![TypeExpr::Int, TypeExpr::Float]), None, false), ("type", TypeExpr::Int, Some(e_int(0)), false)], Vec::<&str>::new(), "format"),
        ("formatCurrency", vec![("amount", TypeExpr::Float, None, false), ("currency", TypeExpr::Str, None, false)], vec![], "format_currency"),
        ("setAttribute", vec![("attribute", TypeExpr::Int, None, false), ("value", t_union(vec![TypeExpr::Int, TypeExpr::Float]), None, false)], vec![], "set_attribute"),
        ("getAttribute", vec![("attribute", TypeExpr::Int, None, false)], vec![], "get_attribute"),
        ("setTextAttribute", vec![("attribute", TypeExpr::Int, None, false), ("value", TypeExpr::Str, None, false)], vec![], "set_text_attribute"),
        ("getTextAttribute", vec![("attribute", TypeExpr::Int, None, false)], vec![], "get_text_attribute"),
        ("setSymbol", vec![("symbol", TypeExpr::Int, None, false), ("value", TypeExpr::Str, None, false)], vec![], "set_symbol"),
        ("getSymbol", vec![("symbol", TypeExpr::Int, None, false)], vec![], "get_symbol"),
        ("setPattern", vec![("pattern", TypeExpr::Str, None, false)], vec![], "set_pattern"),
        ("getPattern", vec![], vec![], "pattern"),
        ("getLocale", vec![("type", TypeExpr::Int, Some(e_int(0)), false)], vec![], "locale"),
        ("getErrorCode", vec![], vec![], "error_code"),
        ("getErrorMessage", vec![], vec![], "error_message"),
    ] {
        let body = if name == "format" {
            let mut statements = vec![s_assign(
                "num",
                e_call(
                    "__elephc_intl_number_argument",
                    vec![
                        e_var("num"),
                        e_str("NumberFormatter::format()"),
                        e_int(1),
                        e_str("num"),
                    ],
                ),
            )];
            statements.extend(numfmt_type_guards("NumberFormatter::format()", 2, "NumberFormatter::formatCurrency() method"));
            statements.push(s_return(e_method_call(e_var("this"), "__elephc_call", vec![
                e_str(bridge), e_array(params.iter().map(|(p,_,_,_)| e_var(p)).collect()),
            ])));
            statements
        } else if name == "getErrorCode" {
            vec![s_return(e_call("elephc_intl_error_code", vec![e_this_prop("__elephc_handle")]))]
        } else if name == "getErrorMessage" {
            vec![s_return(e_call("elephc_intl_error_message", vec![e_this_prop("__elephc_handle")]))]
        } else if name == "setPattern" {
            vec![
                s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str(bridge), e_array(vec![e_var("pattern")]),
                ])),
                s_return(e_call("__elephc_intl_object_fail", vec![
                    e_var("this"), e_var("_result"), e_str("NumberFormatter::setPattern()"),
                ])),
            ]
        } else {
            let mut statements = vec![s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![e_str(bridge), e_array(params.iter().map(|(p,_,_,_)| e_var(p)).collect())]))];
            if outputs.is_empty() { statements.push(s_return(e_var("_result"))); } else { statements.push(s_return(e_index(e_var("_result"), e_int(0)))); }
            statements
        };
        builder = builder.method(instance_method(name, params, body));
    }
    builder = builder.method(instance_method(
        "parse",
        vec![("string", TypeExpr::Str, None, false), ("type", TypeExpr::Int, Some(e_int(0)), false), ("offset", TypeExpr::Int, Some(e_int(0)), true)],
        numfmt_parse_body("parse"),
    ));
    builder = builder.method(instance_method(
        "parseCurrency",
        vec![("string", TypeExpr::Str, None, false), ("currency", t_mixed(), None, true), ("offset", TypeExpr::Int, Some(e_int(0)), true)],
        numfmt_parse_body("parse_currency"),
    ));
    builder = builder.method(method("__serialize").returns(t_array()).body(vec![
        s_throw(e_new("Exception", vec![e_str("Serialization of 'NumberFormatter' is not allowed")])),
    ]));
    builder = builder.method(clone_handle_method("NumberFormatter"));
    builder.build()
}

/// Builds the PHP `ValueError` guards for a NumberFormatter format/parse type argument.
fn numfmt_type_guards(caller: &str, argument: i64, currency_hint: &str) -> Vec<Stmt> {
    vec![
        s_if(
            e_binop(e_binop(e_var("type"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("type"), BinOp::Gt, e_int(4))),
            vec![s_throw(e_new("ValueError", vec![e_str(&format!(
                "{caller}: Argument #{argument} ($type) must be a NumberFormatter::TYPE_* constant"
            ))]))],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("type"), BinOp::StrictEq, e_int(4)),
            vec![
                s_expr(e_call("trigger_error", vec![
                    e_str("Constant NumberFormatter::TYPE_CURRENCY is deprecated since 8.3"),
                    e_const("E_USER_DEPRECATED"),
                ])),
                s_throw(e_new("ValueError", vec![e_str(&format!(
                    "{caller}: Argument #{argument} ($type) cannot be NumberFormatter::TYPE_CURRENCY constant, use {currency_hint} instead"
                ))])),
            ],
            vec![],
            None,
        ),
    ]
}

/// Builds one `NumberFormatter::parse*` body with write-back outputs.
fn numfmt_parse_body(method_name: &str) -> Vec<Stmt> {
    let mut body = Vec::new();
    if method_name == "parse" {
        body.extend(numfmt_type_guards("NumberFormatter::parse()", 2, "NumberFormatter::parseCurrency() method"));
    }
    body.push(s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![e_str(method_name), e_array(if method_name == "parse_currency" { vec![e_var("string"), e_var("offset")] } else { vec![e_var("string"), e_var("type"), e_var("offset")] })])),);
    body.push(s_if(e_binop(e_var("_result"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)), vec![s_return(crate::synthetic_class::e_bool(false))], vec![], None));
    if method_name == "parse_currency" {
        body.push(s_assign("currency", e_index(e_var("_result"), e_int(1))));
        body.push(s_assign("offset", e_index(e_var("_result"), e_int(2))));
    } else {
        body.push(s_assign("offset", e_index(e_var("_result"), e_int(1))));
    }
    body.push(s_return(e_index(e_var("_result"), e_int(0))));
    body
}

/// Builds the `MessageFormatter` class.
fn build_message_formatter_class() -> Stmt {
    let mut builder = class("MessageFormatter").prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)));
    for (constant, value) in constants_for("MessageFormatter") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .method(method("__construct").param("locale", TypeExpr::Str).param("pattern", TypeExpr::Str).body(vec![
            s_if(e_binop(e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0)), vec![
                s_throw(e_new("IntlException", vec![e_str("MessageFormatter::__construct(): cannot call constructor twice")])),
            ], vec![], None),
            s_assign("_handle", e_call("__elephc_intl_open", vec![e_str("message"), e_array(vec![e_var("locale"), e_var("pattern")])])),
            s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![
                s_assign("_message", e_call("__elephc_intl_last_error_message", vec![])),
                s_if(
                    e_binop(
                        e_binop(e_var("_message"), BinOp::StrictEq, e_str("U_ZERO_ERROR")),
                        BinOp::Or,
                        e_binop(e_var("_message"), BinOp::StrictEq, e_str("")),
                    ),
                    vec![s_throw(e_new("IntlException", vec![e_str("MessageFormatter::__construct(): message formatter creation failed")]))],
                    vec![],
                    Some(vec![s_throw(e_new("IntlException", vec![e_concat(e_str("MessageFormatter::__construct(): "), e_var("_message"))]))]),
                ),
            ], vec![], None),
            s_prop_assign(e_var("this"), "__elephc_handle", e_var("_handle")),
        ]))
        .method(method("__elephc_from_handle").static_().param("handle", TypeExpr::Int).returns(t_class("static")).body(vec![
            s_assign("_object", e_new("MessageFormatter", vec![e_str("en_US"), e_str("{}")])),
            s_assign("_dummy", e_prop(e_var("_object"), "__elephc_handle")),
            s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
            s_expr(e_call("elephc_intl_close", vec![e_var("_dummy")])),
            s_return(e_var("_object")),
        ]))
        .method(method("__elephc_call").param("method", TypeExpr::Str).param("args", t_mixed()).body(vec![
            s_return(e_call("__elephc_intl_call", vec![e_this_prop("__elephc_handle"), e_var("method"), e_var("args")])),
        ]))
        .method(method("create").static_().param("locale", TypeExpr::Str).param("pattern", TypeExpr::Str).returns(t_nullable(t_class("MessageFormatter"))).body(vec![
            s_return(e_new("MessageFormatter", vec![e_var("locale"), e_var("pattern")])),
        ]))
        .method(method("formatMessage").static_().param("locale", TypeExpr::Str).param("pattern", TypeExpr::Str).param("values", t_mixed()).returns(t_union(vec![TypeExpr::Str, TypeExpr::False])).body(vec![
            s_assign("_result", e_call("__elephc_intl_invoke", vec![e_str("msgfmt"), e_str("msgfmt_format_message"), e_array(vec![e_var("locale"), e_var("pattern"), e_var("values"), e_call("date_default_timezone_get", vec![])])])),
            s_return(e_call("__elephc_intl_global_fail", vec![e_var("_result"), e_str("MessageFormatter::formatMessage()")])),
        ]))
        .method(method("parseMessage").static_().param("locale", TypeExpr::Str).param("pattern", TypeExpr::Str).param("message", TypeExpr::Str).returns(t_union(vec![t_array(), TypeExpr::False])).body(vec![
            s_assign("_result", e_call("__elephc_intl_invoke", vec![e_str("msgfmt"), e_str("parse_message"), e_array(vec![e_var("locale"), e_var("pattern"), e_var("message")])])),
            s_return(e_call("__elephc_intl_global_fail", vec![e_var("_result"), e_str("MessageFormatter::parseMessage()")])),
        ]));
    for (name, params, bridge) in [
        ("format", vec![("values", t_mixed(), None, false)], "format"),
        ("parse", vec![("string", TypeExpr::Str, None, false)], "parse"),
        ("setPattern", vec![("pattern", TypeExpr::Str, None, false)], "set_pattern"),
        ("getPattern", vec![], "pattern"),
        ("getLocale", vec![], "locale"),
        ("getErrorCode", vec![], "error_code"),
        ("getErrorMessage", vec![], "error_message"),
    ] {
        let body = if name == "getErrorCode" {
            vec![s_return(e_call("elephc_intl_error_code", vec![e_this_prop("__elephc_handle")]))]
        } else if name == "getErrorMessage" {
            vec![s_return(e_call("elephc_intl_error_message", vec![e_this_prop("__elephc_handle")]))]
        } else if name == "format" {
            // php's MessageFormatter::format() throws when use_exceptions is enabled and the
            // pattern/argument conversion failed; otherwise it returns false.
            vec![
                s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str(bridge), e_array(vec![
                        e_var("values"), e_call("date_default_timezone_get", vec![]),
                    ]),
                ])),
                s_return(e_call("__elephc_intl_object_result", vec![e_var("this"), e_var("_result")])),
            ]
        } else if name == "setPattern" {
            vec![
                s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![
                    e_str(bridge), e_array(params.iter().map(|(p,_,_,_)| e_var(p)).collect()),
                ])),
                s_return(e_call("__elephc_intl_object_fail", vec![
                    e_var("this"), e_var("_result"), e_str("MessageFormatter::setPattern()"),
                ])),
            ]
        } else {
            vec![s_return(e_method_call(e_var("this"), "__elephc_call", vec![e_str(bridge), e_array(params.iter().map(|(p,_,_,_)| e_var(p)).collect())]))]
        };
        builder = builder.method(instance_method(name, params, body));
    }
    builder = builder.method(clone_handle_method("MessageFormatter"));
    builder.build()
}

/// Builds the `ResourceBundle` class.
fn build_resource_bundle_class() -> Stmt {
    let mut builder = class("ResourceBundle").prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)));
    for (constant, value) in constants_for("ResourceBundle") {
        builder = builder.constant(constant, const_value_expr(value));
    }
    builder = builder
        .implements("Countable")
        .implements("IteratorAggregate")
        .implements("ArrayAccess")
        .method(method("__construct")
            .param("locale", t_nullable(TypeExpr::Str))
            .param("bundle", t_nullable(TypeExpr::Str))
            .param_default("fallback", TypeExpr::Bool, crate::synthetic_class::e_bool(true))
            .body(vec![
                s_if(e_binop(e_this_prop("__elephc_handle"), BinOp::StrictNotEq, e_int(0)), vec![
                    s_throw(e_new("Error", vec![e_str("ResourceBundle object is already constructed")])),
                ], vec![], None),
                s_assign("_handle", e_call("__elephc_intl_open", vec![e_str("bundle"), e_array(vec![e_var("locale"), e_var("bundle"), e_var("fallback")])])),
                s_if(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), vec![
                    // The bridge records the precise reason (missing bundle, fallback refusal).
                    s_assign("_message", e_call("__elephc_intl_invoke_string", vec![
                        e_str("intl"), e_str("intl_get_error_context"), e_array(vec![]),
                    ])),
                    s_throw(e_new("IntlException", vec![e_var("_message")])),
                ], vec![], None),
                s_prop_assign(e_var("this"), "__elephc_handle", e_var("_handle")),
            ]))
        .method(method("__elephc_from_handle").static_().param("handle", TypeExpr::Int).returns(t_class("static")).body(vec![
            // The dummy bundle construction resets the process-wide error; preserve the
            // warning a fallback bundle load just reported.
            s_assign("_error_code", e_call("__elephc_intl_invoke_int", vec![
                e_str("intl"), e_str("intl_get_error_code"), e_array(vec![]),
            ])),
            s_assign("_error_context", e_call("__elephc_intl_invoke_string", vec![
                e_str("intl"), e_str("intl_get_error_context"), e_array(vec![]),
            ])),
            s_assign("_object", e_new("ResourceBundle", vec![e_str("en_US"), e_str("ICUDATA")])),
            s_assign("_dummy", e_prop(e_var("_object"), "__elephc_handle")),
            s_prop_assign(e_var("_object"), "__elephc_handle", e_var("handle")),
            s_expr(e_call("elephc_intl_close", vec![e_var("_dummy")])),
            s_expr(e_call("__elephc_intl_invoke", vec![
                e_str("intl"), e_str("intl_restore_error"),
                e_array(vec![e_var("_error_code"), e_var("_error_context")]),
            ])),
            s_return(e_var("_object")),
        ]))
        .method(method("__elephc_call").param("method", TypeExpr::Str).param("args", t_mixed()).body(vec![
            s_return(e_call("__elephc_intl_call", vec![e_this_prop("__elephc_handle"), e_var("method"), e_var("args")])),
        ]))
        .method(method("create").static_().param("locale", t_nullable(TypeExpr::Str)).param("bundle", t_nullable(TypeExpr::Str)).param_default("fallback", TypeExpr::Bool, crate::synthetic_class::e_bool(true)).returns(t_nullable(t_class("ResourceBundle"))).body(vec![
            s_assign("_result", e_call("__elephc_intl_invoke", vec![
                e_str("bundle"), e_str("resourcebundle_create"),
                e_array(vec![e_var("locale"), e_var("bundle"), e_var("fallback")]),
            ])),
            s_if(e_not(e_instance_of(e_var("_result"), "ResourceBundle")), vec![s_return(e_null())], vec![], None),
            s_return(e_var("_result")),
        ]))
        .method(method("getLocales").static_().param("bundle", TypeExpr::Str).returns(t_union(vec![t_array(), TypeExpr::False])).body(vec![
            s_return(e_call("__elephc_intl_invoke", vec![e_str("bundle"), e_str("getLocales"), e_array(vec![e_var("bundle")])])),
        ]))
        .method(method("get")
            .param("index", t_union(vec![TypeExpr::Str, TypeExpr::Int]))
            .param_default("fallback", TypeExpr::Bool, crate::synthetic_class::e_bool(true))
            .returns(t_union(vec![
                t_class("ResourceBundle"), t_array(), TypeExpr::Str, TypeExpr::Int, TypeExpr::Void,
            ]))
            .body(vec![
            s_assign("_result", e_method_call(e_var("this"), "__elephc_call", vec![e_str("get"), e_array(vec![e_var("index"), e_var("fallback")])])),
            s_assign("_checked", e_call("__elephc_intl_object_fail", vec![
                e_var("this"), e_var("_result"), e_str("ResourceBundle::get()"),
            ])),
            s_if(
                e_binop(
                    e_binop(e_var("_checked"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)),
                    BinOp::And,
                    e_binop(e_call("__elephc_intl_invoke_int", vec![
                        e_str("intl"), e_str("intl_get_error_code"), e_array(vec![]),
                    ]), BinOp::StrictNotEq, e_int(0)),
                ),
                vec![s_return(e_null())],
                vec![],
                None,
            ),
            s_return(e_var("_checked")),
        ]))
        .method(method("count").returns(TypeExpr::Int).body(vec![
            s_return(e_cast(CastType::Int, e_call("__elephc_intl_call", vec![e_this_prop("__elephc_handle"), e_str("count"), e_array(vec![])]))),
        ]))
        .method(method("offsetExists").param("offset", t_mixed()).returns(TypeExpr::Bool).body(vec![
            s_return(e_binop(e_method_call(e_var("this"), "get", vec![e_var("offset")]), BinOp::StrictNotEq, e_null())),
        ]))
        .method(method("offsetGet").param("offset", t_mixed()).returns(t_mixed()).body(vec![
            s_return(e_method_call(e_var("this"), "get", vec![e_var("offset")])),
        ]))
        .method(method("offsetSet").param("offset", t_mixed()).param("value", t_mixed()).returns(TypeExpr::Void).body(vec![
            s_throw(e_new("Error", vec![e_str("Cannot use object of type ResourceBundle as array")])),
        ]))
        .method(method("offsetUnset").param("offset", t_mixed()).returns(TypeExpr::Void).body(vec![
            s_throw(e_new("Error", vec![e_str("Cannot use object of type ResourceBundle as array")])),
        ]))
        .method(method("getIterator").returns(t_class("IntlIterator")).body(vec![
            s_assign("_values", e_array(vec![])),
            crate::synthetic_class::s_foreach(
                e_method_call(e_var("this"), "__elephc_call", vec![e_str("keys"), e_array(vec![])]),
                Some("_index"),
                "_key",
                vec![
                    crate::synthetic_class::s_array_assign(
                        "_values",
                        e_var("_key"),
                        e_method_call(e_var("this"), "get", vec![e_var("_key")]),
                    ),
                ],
            ),
            s_return(e_static_call("IntlIterator", "__elephc_from_array", vec![e_var("_values")])),
        ]))
        .method(method("getErrorCode").body(vec![s_return(e_call("elephc_intl_error_code", vec![e_this_prop("__elephc_handle")]))]))
        .method(method("getErrorMessage").body(vec![s_return(e_call("elephc_intl_error_message", vec![e_this_prop("__elephc_handle")]))]));
    builder = builder.method(clone_handle_method("ResourceBundle"));
    builder.build()
}
