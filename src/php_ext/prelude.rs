//! Purpose:
//! Declares hosted extensions to the compiler: every function an extension
//! registers becomes an ordinary PHP function whose body calls it through the
//! engine, and its exception classes and constants are declared alongside.
//!
//! Called from:
//! - `crate::pipeline`, before name resolution, when the program's project
//!   declares extensions in `[extension.dependencies]`.
//!
//! Key details:
//! - Built as AST, never parsed from PHP source.
//! - The engine is reached only through `extern` declarations with no library
//!   (the symbols come from archives the link plan carries) and the zval
//!   bridge builtins, so this adds no assembly and runs on every target the FFI
//!   already supports.
//! - Signatures come from the extension's own arginfo. An optional parameter
//!   whose default is a literal (or a constant) is declared with that default
//!   and always passed, which an internal function cannot tell apart from an
//!   omitted argument. One whose default only the C code knows (`UNKNOWN`, or
//!   arginfo that states none, as simdjson's does) cannot be declared that way:
//!   from there on each parameter is declared untyped, defaulting to a
//!   sentinel no caller writes, so exactly the arguments the caller wrote are
//!   passed (each keeping its own by-reference mode).
//! - Every class an extension registers becomes a wrapper class holding the
//!   engine object by handle (see `wrapper_class`).
//! - Results are rebuilt by walking the engine's value (`__elephc_php_ext_value`)
//!   with indexed accessors, never with `foreach ($array as $k => $v)`: a
//!   string-keyed rebuild through a fresh `[]` is miscompiled on the current
//!   backend (it keeps the list layout statically while the runtime promotes it
//!   to a hash), so the walk writes string keys only, through `$map[$key]`.
//! - Object-free subtrees go through `zval_unpack` in one call; only stdClass
//!   objects (and the arrays holding them) are rebuilt piece by piece.

use std::collections::BTreeSet;

use crate::parser::ast::{BinOp, CType, CastType, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_assign, e_binop, e_bool, e_call, e_cast, e_const, e_dyn_prop, e_float, e_index, e_instance_of, e_int,
    e_method_call, e_new, e_not, e_null, e_null_coalesce, e_static_prop, e_str, e_this, e_var, extern_fn_unbound,
    function, interface, internal_declarations, method, s_array_assign, s_array_push, s_assign, s_const, s_expr,
    s_for, s_foreach, s_if, s_namespace, s_return, s_return_void, s_static, s_static_prop_array_assign, s_throw,
    t_array, t_class, t_mixed, t_nullable, t_ptr,
};

use super::install::{HostedExtensions, InstalledExtension};
use super::surface::{ConstantValue, SurfaceClass, SurfaceFunction, SurfaceParam};

/// Declares a plan's parameters on a function or method builder (both spell
/// the same calls).
macro_rules! declare_params {
    ($builder:expr, $plan:expr) => {{
        let mut builder = $builder;
        for param in &$plan.fixed {
            builder = match (param.by_ref, &param.default, &param.ty) {
                (true, _, ty) => builder.param_by_ref(&param.name, ty.clone()),
                (false, Some(default), Some(ty)) => builder.param_default(&param.name, ty.clone(), default.clone()),
                (false, Some(default), None) => builder.param_untyped_default(&param.name, default.clone()),
                (false, None, Some(ty)) => builder.param(&param.name, ty.clone()),
                (false, None, None) => builder.param_untyped(&param.name),
            };
        }
        match &$plan.tail {
            Some(Tail::Variadic(name)) => builder = builder.variadic(name, Some(t_mixed())),
            Some(Tail::Optional { params }) => {
                for param in params {
                    // Typed mixed: an untyped parameter would take the type of
                    // its default, and the sentinel is a string.
                    builder = if param.by_ref {
                        builder.param_by_ref_default(&param.name, Some(t_mixed()), e_str(OMITTED))
                    } else {
                        builder.param_default(&param.name, t_mixed(), e_str(OMITTED))
                    };
                }
            }
            None => {}
        }
        builder
    }};
}

/// The inventory group the declarations are recorded under, for pruning.
pub const PRELUDE_GROUP: &str = "php_ext";

/// Exception classes the engine registers and Elephc also declares, so an
/// extension class may extend them and a thrown one can be rebuilt. The
/// engine's `ErrorException`, `CompileError` and `ParseError` are absent: Elephc
/// declares none of them, so an exception of one of those classes is rebuilt as
/// its nearest declared ancestor.
const ENGINE_THROWABLES: &[&str] = &[
    "Exception", "Error", "TypeError",
    "ArgumentCountError", "ValueError", "ArithmeticError", "DivisionByZeroError",
    "UnhandledMatchError", "LogicException", "BadFunctionCallException", "BadMethodCallException",
    "DomainException", "InvalidArgumentException", "LengthException", "OutOfRangeException",
    "RuntimeException", "OutOfBoundsException", "OverflowException", "RangeException",
    "UnderflowException", "UnexpectedValueException",
];

/// Constants a default may name that Elephc itself defines.
const KNOWN_CONSTANTS: &[&str] = &[
    "PHP_INT_MAX", "PHP_INT_MIN", "PHP_INT_SIZE", "PHP_FLOAT_EPSILON", "PHP_FLOAT_MAX",
    "PHP_FLOAT_MIN", "PHP_EOL", "E_ALL", "E_ERROR", "E_WARNING", "E_NOTICE", "E_DEPRECATED",
    "M_PI", "INF", "NAN",
];

/// Builds every declaration for the hosted extensions.
pub fn declarations(hosted: &HostedExtensions) -> Program {
    NEXT_SITE.with(|next| next.set(0));
    internal_declarations(|| {
        let mut program = abi_externs();
        for extension in &hosted.extensions {
            program.push(
                extern_fn_unbound(&extension.record.module_accessor())
                    .returns(CType::Ptr)
                    .build(),
            );
        }
        let throwables = declared_throwables(&hosted.extensions);
        let wrapped = wrapped_classes(&hosted.extensions);
        program.push(setup_helper(&hosted.ini));
        program.extend(transfer_helpers(&throwables));
        program.extend(object_helpers(hosted, &wrapped));
        let has_ini = !hosted.ini.is_empty();
        for extension in &hosted.extensions {
            program.extend(class_declarations(extension));
            program.extend(wrapper_class_declarations(extension, &wrapped, has_ini));
            program.extend(constant_declarations(extension));
            for function in &extension.surface.functions {
                program.push(wrap_in_namespace(&function.name, |local| {
                    wrapper(extension, function, local, has_ini, &wrapped)
                }));
            }
        }
        program
    })
}

/// The hidden interface every wrapper of a hosted object implements, and the
/// table holding each wrapper's engine object. The table is keyed by
/// `spl_object_id` rather than a property, so the wrapper shows exactly the
/// properties PHP would (none) to `var_dump` and `print_r`.
pub(crate) const OBJECT_INTERFACE: &str = "__ElephcPhpExtObject";
const OBJECT_TABLE: &str = "__ElephcPhpExtObjects";
/// The wrapper method returning (creating on first use) its engine object.
const OBJECT_METHOD: &str = "__elephcObject";
/// The wrapper method adopting an engine object a call returned.
const ADOPT_METHOD: &str = "__elephcAdopt";
/// The static wrapper method building an instance around an engine object,
/// declared in the class so it may call a non-public constructor.
const WRAP_METHOD: &str = "__elephcWrap";
/// The protected wrapper property repeating its table slot. `clone` copies it
/// (the table is keyed by object id, which a clone does not share), so
/// `__clone` knows which engine object to clone.
pub(crate) const HANDLE_PROPERTY: &str = "__elephcHandle";

thread_local! {
    /// The next wrapper call site number: each site caches what its first call
    /// resolved (`elephc_php_ext_site_call`), so numbers are dense per program.
    static NEXT_SITE: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

/// A fresh call site number.
fn next_site() -> i64 {
    NEXT_SITE.with(|next| {
        let site = next.get();
        next.set(site + 1);
        site
    })
}

/// Hosted classes and interfaces declared as wrappers, by name: those whose
/// ancestors are themselves wrapped, so every parent is declared first.
/// Throwables keep their plain declarations (`class_declarations`).
fn wrapped_classes(extensions: &[InstalledExtension]) -> BTreeSet<String> {
    let all: Vec<&SurfaceClass> = extensions.iter().flat_map(|extension| &extension.surface.classes).collect();
    let throwables: BTreeSet<String> = declared_throwables(extensions).into_iter().collect();
    let mut wrapped = BTreeSet::new();
    loop {
        let before = wrapped.len();
        for class in &all {
            if wrapped.contains(&class.name) || throwables.contains(&class.name) || is_throwable_like(class, &all) {
                continue;
            }
            let parent_ok = class.parent.as_ref().is_none_or(|parent| wrapped.contains(parent));
            let interfaces_ok = class
                .interfaces
                .iter()
                .all(|name| wrapped.contains(name) || !all.iter().any(|other| &other.name == name));
            if parent_ok && interfaces_ok {
                wrapped.insert(class.name.clone());
            }
        }
        if wrapped.len() == before {
            return wrapped;
        }
    }
}

/// A class whose chain reaches an engine throwable: its instances are
/// exceptions, rebuilt by class name, never held by a wrapper.
fn is_throwable_like(class: &SurfaceClass, all: &[&SurfaceClass]) -> bool {
    let mut current = class.parent.clone();
    let mut depth = 0;
    while let Some(name) = current {
        if ENGINE_THROWABLES.contains(&name.as_str()) || depth > 32 {
            return true;
        }
        current = all.iter().find(|other| other.name == name).and_then(|other| other.parent.clone());
        depth += 1;
    }
    class.interfaces.iter().any(|name| name == "Throwable")
}

/// The engine's C ABI (`crates/elephc-zend/src/host.rs`).
fn abi_externs() -> Vec<Stmt> {
    let ptr = || CType::Ptr;
    vec![
        extern_fn_unbound("elephc_php_ext_ini").param("name", CType::Str).param("value", CType::Str).build(),
        extern_fn_unbound("elephc_php_ext_call_new_at")
            .param("module", ptr())
            .param("name", CType::Str)
            .param("argc", CType::Int)
            .param("site", CType::Int)
            .returns(ptr())
            .build(),
        extern_fn_unbound("elephc_php_ext_site_call")
            .param("site", CType::Int)
            .param("object", CType::Int)
            .param("argc", CType::Int)
            .returns(ptr())
            .build(),
        extern_fn_unbound("elephc_php_ext_call_arg_long").param("call", ptr()).param("index", CType::Int).param("value", CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_call_arg_double")
            .param("call", ptr())
            .param("index", CType::Int)
            .param("value", CType::Float)
            .build(),
        extern_fn_unbound("elephc_php_ext_call_arg_bool").param("call", ptr()).param("index", CType::Int).param("value", CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_call_result_is").param("call", ptr()).param("kind", CType::Int).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_call_result_double").param("call", ptr()).returns(CType::Float).build(),
        extern_fn_unbound("elephc_php_ext_call_result_bool").param("call", ptr()).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_call_arg")
            .param("call", ptr())
            .param("index", CType::Int)
            .param("value", ptr())
            .param("by_ref", CType::Int)
            .returns(CType::Int)
            .build(),
        extern_fn_unbound("elephc_php_ext_call_invoke").param("call", ptr()).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_call_result").param("call", ptr()).returns(ptr()).build(),
        extern_fn_unbound("elephc_php_ext_call_ref").param("call", ptr()).param("index", CType::Int).returns(ptr()).build(),
        extern_fn_unbound("elephc_php_ext_call_error_class").param("call", ptr()).returns(CType::Str).build(),
        extern_fn_unbound("elephc_php_ext_call_error_message").param("call", ptr()).returns(CType::Str).build(),
        // Decimal text: an extern `int` return keeps only 32 bits of a 64-bit code.
        extern_fn_unbound("elephc_php_ext_call_error_code").param("call", ptr()).returns(CType::Str).build(),
        extern_fn_unbound("elephc_php_ext_call_free").param("call", ptr()).build(),
        extern_fn_unbound("elephc_php_ext_value_check").param("call", ptr()).param("value", ptr()).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_value_kind").param("value", ptr()).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_value_export").param("call", ptr()).param("value", ptr()).returns(ptr()).build(),
        extern_fn_unbound("elephc_php_ext_value_count").param("value", ptr()).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_value_is_list").param("value", ptr()).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_value_key")
            .param("call", ptr())
            .param("value", ptr())
            .param("index", CType::Int)
            .returns(ptr())
            .build(),
        extern_fn_unbound("elephc_php_ext_value_name").param("value", ptr()).param("index", CType::Int).returns(CType::Str).build(),
        extern_fn_unbound("elephc_php_ext_value_at").param("value", ptr()).param("index", CType::Int).returns(ptr()).build(),
        // Hosted objects travel as their engine address, an int, so the wrapper
        // can keep it in an ordinary `array<int, int>`.
        extern_fn_unbound("elephc_php_ext_object_new").param("module", ptr()).param("class", CType::Str).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_object_release").param("object", CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_method_new_at")
            .param("module", ptr())
            .param("class", CType::Str)
            .param("name", CType::Str)
            .param("object", CType::Int)
            .param("argc", CType::Int)
            .param("site", CType::Int)
            .returns(ptr())
            .build(),
        extern_fn_unbound("elephc_php_ext_call_arg_object")
            .param("call", ptr())
            .param("index", CType::Int)
            .param("object", CType::Int)
            .param("by_ref", CType::Int)
            .returns(CType::Int)
            .build(),
        extern_fn_unbound("elephc_php_ext_call_returned").param("call", ptr()).param("object", CType::Int).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_object_class").param("value", ptr()).returns(CType::Str).build(),
        extern_fn_unbound("elephc_php_ext_object_take").param("value", ptr()).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_property_new")
            .param("object", CType::Int)
            .param("name", CType::Str)
            .param("argc", CType::Int)
            .returns(ptr())
            .build(),
        extern_fn_unbound("elephc_php_ext_property_invoke").param("call", ptr()).param("op", CType::Int).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_iter_new").param("object", CType::Int).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_iter_call").param("id", CType::Int).returns(ptr()).build(),
        extern_fn_unbound("elephc_php_ext_iter_step").param("id", CType::Int).param("op", CType::Int).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_iter_fetch").param("id", CType::Int).param("op", CType::Int).returns(ptr()).build(),
        extern_fn_unbound("elephc_php_ext_iter_free").param("id", CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_set_dispatcher").param("dispatcher", CType::Callable).build(),
        extern_fn_unbound("elephc_php_ext_call_arg_callable")
            .param("call", ptr())
            .param("index", CType::Int)
            .param("slot", CType::Int)
            .returns(CType::Int)
            .build(),
        extern_fn_unbound("elephc_php_ext_callback_threw").returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_callback_argc").param("call", ptr()).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_callback_arg").param("call", ptr()).param("index", CType::Int).returns(ptr()).build(),
        extern_fn_unbound("elephc_php_ext_callback_return").param("call", ptr()).param("value", ptr()).returns(CType::Int).build(),
        extern_fn_unbound("elephc_php_ext_callback_return_object")
            .param("call", ptr())
            .param("object", CType::Int)
            .returns(CType::Int)
            .build(),
    ]
}

/// Builds an AST call expression to the named function with the given arguments.
fn call(name: &str, args: Vec<Expr>) -> Expr {
    e_call(name, args)
}

/// Builds an AST strict-equality (`===`) expression.
fn strict_eq(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::StrictEq, right)
}

/// `__elephc_php_ext_setup()`: applies `[extension.ini]` once, before any
/// extension starts.
fn setup_helper(ini: &[(String, String)]) -> Stmt {
    let mut body = vec![
        s_static("__elephc_done", e_bool(false)),
        s_if(e_var("__elephc_done"), vec![s_return_void()], vec![], None),
        s_assign("__elephc_done", e_bool(true)),
    ];
    for (directive, value) in ini {
        body.push(s_expr(call("elephc_php_ext_ini", vec![e_str(directive), e_str(value)])));
    }
    function("__elephc_php_ext_setup").returns(TypeExpr::Void).body(body).build()
}

/// The helpers every wrapper shares: argument transfer, invocation with
/// exception rebuilding, and result walking.
fn transfer_helpers(throwables: &[String]) -> Vec<Stmt> {
    let call_var = || e_var("__elephc_call");
    let value_var = || e_var("__elephc_value");

    // function __elephc_php_ext_arg(ptr $call, int $index, mixed $value, int $byRef): void
    let arg = function("__elephc_php_ext_arg")
        .param("__elephc_call", t_ptr())
        .param("__elephc_index", TypeExpr::Int)
        .param("__elephc_value", t_mixed())
        .param("__elephc_by_ref", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            // A callable parameter's value is registered and passed as a
            // hosted Closure; a wrapper of a hosted object passes its object.
            s_if(
                e_binop(
                    e_binop(
                        e_binop(e_var("__elephc_by_ref"), BinOp::BitAnd, e_int(2)),
                        BinOp::StrictNotEq,
                        e_int(0),
                    ),
                    BinOp::And,
                    e_binop(value_var(), BinOp::StrictNotEq, e_null()),
                ),
                vec![s_assign(
                    "__elephc_status",
                    call(
                        "elephc_php_ext_call_arg_callable",
                        vec![call_var(), e_var("__elephc_index"), call("__elephc_php_ext_callable", vec![value_var()])],
                    ),
                )],
                vec![(
                e_instance_of(value_var(), &format!("\\{OBJECT_INTERFACE}")),
                vec![s_assign(
                    "__elephc_status",
                    call(
                        "elephc_php_ext_call_arg_object",
                        vec![
                            call_var(),
                            e_var("__elephc_index"),
                            e_method_call(value_var(), OBJECT_METHOD, vec![]),
                            e_binop(e_var("__elephc_by_ref"), BinOp::BitAnd, e_int(1)),
                        ],
                    ),
                )])],
                Some(vec![
                    s_assign("__elephc_zval", call("zval_pack", vec![value_var()])),
                    s_assign(
                        "__elephc_status",
                        call(
                            "elephc_php_ext_call_arg",
                            vec![
                                call_var(),
                                e_var("__elephc_index"),
                                e_var("__elephc_zval"),
                                e_binop(e_var("__elephc_by_ref"), BinOp::BitAnd, e_int(1)),
                            ],
                        ),
                    ),
                    s_expr(call("zval_free", vec![e_var("__elephc_zval")])),
                ]),
            ),
            s_if(
                e_binop(e_var("__elephc_status"), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_assign("__elephc_message", call("elephc_php_ext_call_error_message", vec![call_var()])),
                    s_expr(call("elephc_php_ext_call_free", vec![call_var()])),
                    s_throw(e_new("\\TypeError", vec![e_var("__elephc_message")])),
                ],
                vec![],
                None,
            ),
        ])
        .build();

    // function __elephc_php_ext_throw(string $chain, string $message, int $code): void
    // `$chain` is the thrown class and its ancestors, most derived first: the
    // first one declared here is rethrown.
    let mut arms: Vec<(Expr, Vec<Stmt>)> = throwables
        .iter()
        .map(|class_name| {
            (
                strict_eq(e_var("__elephc_class"), e_str(class_name)),
                vec![s_throw(e_new(
                    &format!("\\{class_name}"),
                    vec![e_var("__elephc_message"), e_var("__elephc_code")],
                ))],
            )
        })
        .collect();
    let mut throw_body = Vec::new();
    if !arms.is_empty() {
        let (first_condition, first_body) = arms.remove(0);
        throw_body.push(s_foreach(
            call("explode", vec![e_str(","), e_var("__elephc_chain")]),
            None,
            "__elephc_class",
            vec![s_if(first_condition, first_body, arms, None)],
        ));
    }
    throw_body.push(s_throw(e_new("\\Error", vec![e_var("__elephc_message"), e_var("__elephc_code")])));
    let throw = function("__elephc_php_ext_throw")
        .param("__elephc_chain", TypeExpr::Str)
        .param("__elephc_message", TypeExpr::Str)
        .param("__elephc_code", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(throw_body)
        .build();

    // function __elephc_php_ext_raise(ptr $call): void
    // Throws the call's error (the extension's exception, or the bridge's
    // refusal) after freeing the call.
    let raise = function("__elephc_php_ext_raise")
        .param("__elephc_call", t_ptr())
        .returns(TypeExpr::Void)
        .body(vec![
            // A program callable threw inside the call: rethrow that very
            // throwable, not the marker the extension unwound with.
            s_if(
                strict_eq(call("elephc_php_ext_callback_threw", vec![]), e_int(1)),
                vec![
                    s_assign("__elephc_pending", e_static_prop(&format!("\\{OBJECT_TABLE}"), "pending")),
                    crate::synthetic_class::s_static_prop_assign(&format!("\\{OBJECT_TABLE}"), "pending", e_null()),
                    s_if(
                        e_instance_of(e_var("__elephc_pending"), "\\Throwable"),
                        vec![
                            s_expr(call("elephc_php_ext_call_free", vec![call_var()])),
                            s_throw(e_var("__elephc_pending")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_class", call("elephc_php_ext_call_error_class", vec![call_var()])),
            s_assign("__elephc_message", call("elephc_php_ext_call_error_message", vec![call_var()])),
            s_assign("__elephc_code", e_cast(CastType::Int, call("elephc_php_ext_call_error_code", vec![call_var()]))),
            s_expr(call("elephc_php_ext_call_free", vec![call_var()])),
            s_expr(call(
                "__elephc_php_ext_throw",
                vec![e_var("__elephc_class"), e_var("__elephc_message"), e_var("__elephc_code")],
            )),
        ])
        .build();

    // function __elephc_php_ext_invoke(ptr $call): int
    // 0, or 1 when the extension threw: the wrapper still writes by-reference
    // arguments back first, since PHP keeps what the extension wrote into them.
    let invoke = function("__elephc_php_ext_invoke")
        .param("__elephc_call", t_ptr())
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("__elephc_status", call("elephc_php_ext_call_invoke", vec![call_var()])),
            // A fatal error was already reported by the engine, as PHP reports it.
            s_if(
                strict_eq(e_var("__elephc_status"), e_int(2)),
                vec![
                    s_expr(call("elephc_php_ext_call_free", vec![call_var()])),
                    s_expr(call("exit", vec![e_int(255)])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("__elephc_status")),
        ])
        .build();

    // function __elephc_php_ext_value(ptr $call, ptr $value): mixed
    let at = || call("elephc_php_ext_value_at", vec![value_var(), e_var("__elephc_i")]);
    let recurse = || call("__elephc_php_ext_value", vec![call_var(), at()]);
    let key = || {
        call(
            "zval_unpack",
            vec![call("elephc_php_ext_value_key", vec![call_var(), value_var(), e_var("__elephc_i")])],
        )
    };
    let counted_loop = |body: Vec<Stmt>| {
        s_for(
            Some(s_assign("__elephc_i", e_int(0))),
            Some(e_binop(e_var("__elephc_i"), BinOp::Lt, e_var("__elephc_count"))),
            Some(s_assign("__elephc_i", e_binop(e_var("__elephc_i"), BinOp::Add, e_int(1)))),
            body,
        )
    };
    let value = function("__elephc_php_ext_value")
        .param("__elephc_call", t_ptr())
        .param("__elephc_value", t_ptr())
        .returns(t_mixed())
        .body(vec![
            s_assign("__elephc_kind", call("elephc_php_ext_value_kind", vec![value_var()])),
            s_if(
                strict_eq(e_var("__elephc_kind"), e_int(0)),
                vec![s_return(call(
                    "zval_unpack",
                    vec![call("elephc_php_ext_value_export", vec![call_var(), value_var()])],
                ))],
                vec![],
                None,
            ),
            // A hosted object: a wrapper of its class adopts a new reference.
            s_if(
                strict_eq(e_var("__elephc_kind"), e_int(3)),
                vec![s_return(call(
                    "__elephc_php_ext_adopt",
                    vec![
                        call("elephc_php_ext_object_class", vec![value_var()]),
                        call("elephc_php_ext_object_take", vec![value_var()]),
                    ],
                ))],
                vec![],
                None,
            ),
            s_assign("__elephc_count", call("elephc_php_ext_value_count", vec![value_var()])),
            s_if(
                strict_eq(e_var("__elephc_kind"), e_int(1)),
                vec![
                    s_assign("__elephc_object", e_new("\\stdClass", vec![])),
                    counted_loop(vec![
                        // A C string, not the mixed key: a name cast from mixed
                        // leaks once per property on the current backend.
                        s_assign("__elephc_name", call("elephc_php_ext_value_name", vec![value_var(), e_var("__elephc_i")])),
                        s_expr(e_assign(
                            e_dyn_prop(e_var("__elephc_object"), e_var("__elephc_name")),
                            recurse(),
                        )),
                    ]),
                    s_return(e_var("__elephc_object")),
                ],
                vec![],
                None,
            ),
            s_if(
                strict_eq(call("elephc_php_ext_value_is_list", vec![value_var()]), e_int(1)),
                vec![
                    s_assign("__elephc_list", e_array(vec![])),
                    counted_loop(vec![s_array_push("__elephc_list", recurse())]),
                    s_return(e_var("__elephc_list")),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_map", e_array(vec![])),
            counted_loop(vec![
                s_assign("__elephc_key", key()),
                s_array_assign("__elephc_map", e_var("__elephc_key"), recurse()),
            ]),
            s_return(e_var("__elephc_map")),
        ])
        .build();

    // function __elephc_php_ext_rebuild(ptr $call, ptr $value): mixed
    // The one entry into __elephc_php_ext_value: the whole value is checked
    // first, so a refusal (a cycle, a class Elephc lacks) frees the call
    // instead of leaving a walk half done.
    let rebuild = function("__elephc_php_ext_rebuild")
        .param("__elephc_call", t_ptr())
        .param("__elephc_value", t_ptr())
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(call("elephc_php_ext_value_check", vec![call_var(), value_var()]), BinOp::StrictNotEq, e_int(0)),
                vec![s_expr(call("__elephc_php_ext_raise", vec![call_var()]))],
                vec![],
                None,
            ),
            s_return(call("__elephc_php_ext_value", vec![call_var(), value_var()])),
        ])
        .build();

    // function __elephc_php_ext_result(ptr $call): mixed
    let result = function("__elephc_php_ext_result")
        .param("__elephc_call", t_ptr())
        .returns(t_mixed())
        .body(vec![
            s_assign(
                "__elephc_result",
                call("__elephc_php_ext_rebuild", vec![call_var(), call("elephc_php_ext_call_result", vec![call_var()])]),
            ),
            s_expr(call("elephc_php_ext_call_free", vec![call_var()])),
            s_return(e_var("__elephc_result")),
        ])
        .build();

    vec![arg, throw, raise, invoke, value, rebuild, result]
}

/// Every throwable a wrapper may have to rebuild: the engine's own and each
/// hosted extension's exception classes, subclasses before their parents so
/// the first matching arm is the exact class.
fn declared_throwables(extensions: &[InstalledExtension]) -> Vec<String> {
    let mut classes: Vec<String> = Vec::new();
    for extension in extensions {
        for class_name in exception_classes(extension) {
            classes.push(class_name);
        }
    }
    classes.extend(ENGINE_THROWABLES.iter().map(|name| name.to_string()));
    classes
}

/// The extension classes Elephc can declare: exception classes whose parent
/// is a known throwable or another such class, with no methods of their own.
fn exception_classes(extension: &InstalledExtension) -> Vec<String> {
    let mut known: BTreeSet<String> = ENGINE_THROWABLES.iter().map(|name| name.to_string()).collect();
    let mut ordered = Vec::new();
    loop {
        let before = ordered.len();
        for class in &extension.surface.classes {
            let Some(parent) = &class.parent else { continue };
            if class.interface || !class.methods.is_empty() || known.contains(&class.name) {
                continue;
            }
            if known.contains(parent) {
                known.insert(class.name.clone());
                ordered.push(class.name.clone());
            }
        }
        if ordered.len() == before {
            break;
        }
    }
    // Deepest first, so a catch-all parent arm never shadows its subclass.
    ordered.reverse();
    ordered
}

/// Builds the AST class declarations for the extension's exception classes, parents before
/// children, each extending its surface parent (`Exception` by default) inside its own namespace
/// and marked final when the surface says so.
fn class_declarations(extension: &InstalledExtension) -> Vec<Stmt> {
    let mut declared = exception_classes(extension);
    declared.reverse(); // parents before children
    declared
        .iter()
        .filter_map(|name| extension.surface.classes.iter().find(|class| &class.name == name))
        .map(|class| {
            let parent = format!("\\{}", class.parent.as_deref().unwrap_or("Exception"));
            wrap_in_namespace(&class.name, |local| {
                let builder = crate::synthetic_class::class(local).extends(&parent);
                if class.is_final { builder.final_().build() } else { builder.build() }
            })
        })
        .collect()
}

/// The hidden interface every wrapper implements, the object table, and
/// `__elephc_php_ext_adopt()`, which wraps an engine object a call returned.
fn object_helpers(hosted: &HostedExtensions, wrapped: &BTreeSet<String>) -> Vec<Stmt> {
    let marker = interface(OBJECT_INTERFACE).method(method(OBJECT_METHOD).returns(TypeExpr::Int)).build();
    let table = crate::synthetic_class::class(OBJECT_TABLE)
        .final_()
        .static_prop("objects", t_array(), Some(e_array(vec![])))
        .static_prop(ADOPTING, TypeExpr::Int, Some(e_int(0)))
        .static_prop("callables", t_array(), Some(e_array(vec![])))
        .static_prop("next_callable", TypeExpr::Int, Some(e_int(1)))
        .static_prop("live_callables", TypeExpr::Int, Some(e_int(0)))
        .static_prop("pending", t_mixed(), Some(e_null()))
        .build();

    // function __elephc_php_ext_adopt(string $class, int $object): mixed
    // Wraps an object the extension made itself, so its constructor must not
    // run: `new` runs only the wrapper constructor's adoption branch, given
    // placeholder arguments its signature accepts. (Reflection's
    // `newInstanceWithoutConstructor()` would do, but costs heap per call.)
    let classes: Vec<(&InstalledExtension, &SurfaceClass)> = hosted
        .extensions
        .iter()
        .flat_map(|extension| extension.surface.classes.iter().map(move |class| (extension, class)))
        .collect();
    let mut arms: Vec<(Expr, Vec<Stmt>)> = classes
        .iter()
        .filter(|(_, class)| wrapped.contains(&class.name) && !class.interface && !class.is_abstract)
        .enumerate()
        .map(|(index, (_, class))| {
            let qualified = format!("\\{}", class.name);
            // One local per arm: the checker types a local by its first class.
            let wrapper = format!("__elephc_wrapper_{index}");
            let body = match placeholder_arguments(class, &classes, wrapped) {
                Some(_) => vec![s_return(crate::synthetic_class::e_static_call(
                    &qualified,
                    WRAP_METHOD,
                    vec![e_var("__elephc_object")],
                ))],
                None => vec![
                    s_assign(
                        &wrapper,
                        e_method_call(
                            e_new("\\ReflectionClass", vec![e_str(&class.name)]),
                            "newInstanceWithoutConstructor",
                            vec![],
                        ),
                    ),
                    s_expr(e_method_call(e_var(&wrapper), ADOPT_METHOD, vec![e_var("__elephc_object")])),
                    s_return(e_var(&wrapper)),
                ],
            };
            (strict_eq(e_var("__elephc_class"), e_str(&class.name)), body)
        })
        .collect();
    let mut body = Vec::new();
    if !arms.is_empty() {
        let (condition, first) = arms.remove(0);
        body.push(s_if(condition, first, arms, None));
    }
    body.push(s_expr(call("elephc_php_ext_object_release", vec![e_var("__elephc_object")])));
    body.push(s_throw(e_new(
        "\\Error",
        vec![e_binop(
            e_binop(e_str("a hosted PHP extension returned a value of type "), BinOp::Concat, e_var("__elephc_class")),
            BinOp::Concat,
            e_str(", which Elephc cannot represent yet"),
        )],
    )));
    // function __elephc_php_ext_property(int $object, string $name, int $op, mixed $value): mixed
    // A property access the engine object answers: 0 read, 1 write, 2 isset,
    // 3 unset (`elephc_php_ext_property_invoke`).
    let call_var = || e_var("__elephc_call");
    let property = function("__elephc_php_ext_property")
        .param("__elephc_object", TypeExpr::Int)
        .param("__elephc_name", TypeExpr::Str)
        .param("__elephc_op", TypeExpr::Int)
        .param("__elephc_value", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_assign(
                "__elephc_call",
                call(
                    "elephc_php_ext_property_new",
                    vec![
                        e_var("__elephc_object"),
                        e_var("__elephc_name"),
                        crate::synthetic_class::e_ternary(strict_eq(e_var("__elephc_op"), e_int(1)), e_int(1), e_int(0)),
                    ],
                ),
            ),
            s_if(call("ptr_is_null", vec![call_var()]), vec![s_expr(call("exit", vec![e_int(255)]))], vec![], None),
            s_if(
                strict_eq(e_var("__elephc_op"), e_int(1)),
                vec![s_expr(call("__elephc_php_ext_arg", vec![call_var(), e_int(0), e_var("__elephc_value"), e_int(0)]))],
                vec![],
                None,
            ),
            s_assign("__elephc_status", call("elephc_php_ext_property_invoke", vec![call_var(), e_var("__elephc_op")])),
            s_if(
                strict_eq(e_var("__elephc_status"), e_int(2)),
                vec![s_expr(call("elephc_php_ext_call_free", vec![call_var()])), s_expr(call("exit", vec![e_int(255)]))],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_status"), BinOp::StrictNotEq, e_int(0)),
                vec![s_expr(call("__elephc_php_ext_raise", vec![call_var()]))],
                vec![],
                None,
            ),
            s_return(call("__elephc_php_ext_result", vec![call_var()])),
        ])
        .build();
    let adopt = function("__elephc_php_ext_adopt")
        .param("__elephc_class", TypeExpr::Str)
        .param("__elephc_object", TypeExpr::Int)
        .returns(t_mixed())
        .body(body)
        .build();
    // function __elephc_php_ext_clone(int $object): int
    // The handle of a clone of a held object, through the property call's
    // error paths: an uncloneable class throws PHP's `Error`.
    let clone = function("__elephc_php_ext_clone")
        .param("__elephc_object", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("__elephc_call", call("elephc_php_ext_property_new", vec![e_var("__elephc_object"), e_str(""), e_int(0)])),
            s_if(call("ptr_is_null", vec![call_var()]), vec![s_expr(call("exit", vec![e_int(255)]))], vec![], None),
            s_assign("__elephc_status", call("elephc_php_ext_property_invoke", vec![call_var(), e_int(5)])),
            s_if(
                strict_eq(e_var("__elephc_status"), e_int(2)),
                vec![s_expr(call("elephc_php_ext_call_free", vec![call_var()])), s_expr(call("exit", vec![e_int(255)]))],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_status"), BinOp::StrictNotEq, e_int(0)),
                vec![s_expr(call("__elephc_php_ext_raise", vec![call_var()]))],
                vec![],
                None,
            ),
            s_assign(
                "__elephc_clone",
                call("elephc_php_ext_object_take", vec![call("elephc_php_ext_call_result", vec![call_var()])]),
            ),
            s_expr(call("elephc_php_ext_call_free", vec![call_var()])),
            s_return(e_var("__elephc_clone")),
        ])
        .build();
    vec![marker, table, property, clone, adopt, iterator_class(), callable_registration(), dispatcher()]
}

/// `__elephc_php_ext_callable(mixed $callable): int`: keeps a callable the
/// program passes to an extension and returns its slot; the first one also
/// hands the engine the dispatcher it calls back through.
fn callable_registration() -> Stmt {
    let table = format!("\\{OBJECT_TABLE}");
    function("__elephc_php_ext_callable")
        .param("__elephc_callable", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_static("__elephc_ready", e_bool(false)),
            s_if(
                e_not(e_var("__elephc_ready")),
                vec![
                    s_assign("__elephc_ready", e_bool(true)),
                    s_expr(call("elephc_php_ext_set_dispatcher", vec![e_str("__elephc_php_ext_dispatch")])),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_slot", e_static_prop(&table, "next_callable")),
            crate::synthetic_class::s_static_prop_assign(
                &table,
                "next_callable",
                e_binop(e_var("__elephc_slot"), BinOp::Add, e_int(1)),
            ),
            s_static_prop_array_assign(&table, "callables", e_var("__elephc_slot"), e_var("__elephc_callable")),
            crate::synthetic_class::s_static_prop_assign(
                &table,
                "live_callables",
                e_binop(e_static_prop(&table, "live_callables"), BinOp::Add, e_int(1)),
            ),
            s_return(e_var("__elephc_slot")),
        ])
        .build()
}

/// `__elephc_php_ext_dispatch(int $slot, ptr $call): int`, the function the
/// engine calls a program callable through: rebuilds the arguments, calls it,
/// and hands back its result. A throwable never crosses the C frames between:
/// it is kept, 1 is returned, and the wrapper rethrows it once the extension
/// returns. A NULL call releases the slot (the extension dropped the callable).
fn dispatcher() -> Stmt {
    let table = format!("\\{OBJECT_TABLE}");
    let call_var = || e_var("__elephc_call");
    let keep = |throwable: Expr| {
        vec![
            crate::synthetic_class::s_static_prop_assign(&table, "pending", throwable),
            s_return(e_int(1)),
        ]
    };
    let arguments = s_for(
        Some(s_assign("__elephc_i", e_int(0))),
        Some(e_binop(e_var("__elephc_i"), BinOp::Lt, call("elephc_php_ext_callback_argc", vec![call_var()]))),
        Some(s_assign("__elephc_i", e_binop(e_var("__elephc_i"), BinOp::Add, e_int(1)))),
        vec![
            s_assign("__elephc_arg", call("elephc_php_ext_callback_arg", vec![call_var(), e_var("__elephc_i")])),
            s_if(
                e_binop(call("elephc_php_ext_value_check", vec![call_var(), e_var("__elephc_arg")]), BinOp::StrictNotEq, e_int(0)),
                keep(e_new("\\Error", vec![call("elephc_php_ext_call_error_message", vec![call_var()])])),
                vec![],
                None,
            ),
            s_array_push("__elephc_args", call("__elephc_php_ext_value", vec![call_var(), e_var("__elephc_arg")])),
        ],
    );
    let invoke = crate::synthetic_class::s_try(
        vec![s_assign(
            "__elephc_result",
            call(
                "call_user_func_array",
                vec![e_index(e_static_prop(&table, "callables"), e_var("__elephc_slot")), e_var("__elephc_args")],
            ),
        )],
        vec![(vec!["\\Throwable"], Some("__elephc_thrown"), keep(e_var("__elephc_thrown")))],
        None,
    );
    function("__elephc_php_ext_dispatch")
        .param("__elephc_slot", TypeExpr::Int)
        .param("__elephc_call", t_ptr())
        .returns(TypeExpr::Int)
        .body(vec![
            // The extension dropped the callable: free its slot, and the whole table once no
            // callable is held, so a long-running program does not grow it call after call.
            s_if(
                call("ptr_is_null", vec![call_var()]),
                vec![
                    s_static_prop_array_assign(&table, "callables", e_var("__elephc_slot"), e_null()),
                    crate::synthetic_class::s_static_prop_assign(
                        &table,
                        "live_callables",
                        e_binop(e_static_prop(&table, "live_callables"), BinOp::Sub, e_int(1)),
                    ),
                    s_if(
                        e_binop(e_static_prop(&table, "live_callables"), BinOp::LtEq, e_int(0)),
                        vec![
                            crate::synthetic_class::s_static_prop_assign(&table, "callables", e_array(vec![])),
                            crate::synthetic_class::s_static_prop_assign(&table, "next_callable", e_int(1)),
                            crate::synthetic_class::s_static_prop_assign(&table, "live_callables", e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_int(0)),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_args", e_array(vec![])),
            arguments,
            invoke,
            s_if(
                e_instance_of(e_var("__elephc_result"), &format!("\\{OBJECT_INTERFACE}")),
                vec![s_assign(
                    "__elephc_status",
                    call(
                        "elephc_php_ext_callback_return_object",
                        vec![call_var(), e_method_call(e_var("__elephc_result"), OBJECT_METHOD, vec![])],
                    ),
                )],
                vec![],
                Some(vec![
                    s_assign("__elephc_zval", call("zval_pack", vec![e_var("__elephc_result")])),
                    s_assign(
                        "__elephc_status",
                        call("elephc_php_ext_callback_return", vec![call_var(), e_var("__elephc_zval")]),
                    ),
                    s_expr(call("zval_free", vec![e_var("__elephc_zval")])),
                ]),
            ),
            s_if(
                e_binop(e_var("__elephc_status"), BinOp::StrictNotEq, e_int(0)),
                keep(e_new(
                    "\\TypeError",
                    vec![e_str("a PHP callable returned an object a hosted PHP extension cannot receive yet")],
                )),
                vec![],
                None,
            ),
            s_return(e_int(0)),
        ])
        .build()
}

/// The hidden class `foreach` drives a hosted object's own iterator through.
const ITERATOR_CLASS: &str = "__ElephcPhpExtIterator";

/// `__ElephcPhpExtIterator`: an `\Iterator` over an iteration the engine
/// runs with the extension's `get_iterator` handler. Its destructor ends the
/// iteration, so a `foreach` left early releases the extension's iterator too.
fn iterator_class() -> Stmt {
    let id = || crate::synthetic_class::e_this_prop("id");
    let call_of = || call("elephc_php_ext_iter_call", vec![id()]);
    // $this->fail(): throws the iteration's error; the engine keeps the
    // iteration until the destructor ends it.
    let fail = method("fail").private().returns(TypeExpr::Void).body(vec![
        s_assign("__elephc_call", call_of()),
        s_expr(call(
            "__elephc_php_ext_throw",
            vec![
                call("elephc_php_ext_call_error_class", vec![e_var("__elephc_call")]),
                call("elephc_php_ext_call_error_message", vec![e_var("__elephc_call")]),
                e_cast(CastType::Int, call("elephc_php_ext_call_error_code", vec![e_var("__elephc_call")])),
            ],
        )),
    ]);
    let step = method("step").private().param("op", TypeExpr::Int).returns(TypeExpr::Int).body(vec![
        s_assign("__elephc_status", call("elephc_php_ext_iter_step", vec![id(), e_var("op")])),
        s_if(strict_eq(e_var("__elephc_status"), e_int(-2)), vec![s_expr(call("exit", vec![e_int(255)]))], vec![], None),
        s_if(
            strict_eq(e_var("__elephc_status"), e_int(-1)),
            vec![s_expr(e_method_call(e_this(), "fail", vec![]))],
            vec![],
            None,
        ),
        s_return(e_var("__elephc_status")),
    ]);
    let fetch = method("fetch").private().param("op", TypeExpr::Int).returns(t_mixed()).body(vec![
        s_assign("__elephc_value", call("elephc_php_ext_iter_fetch", vec![id(), e_var("op")])),
        s_assign("__elephc_call", call_of()),
        s_if(
            call("ptr_is_null", vec![e_var("__elephc_value")]),
            vec![
                // No error recorded: a fatal error the engine already reported.
                s_if(
                    strict_eq(call("elephc_php_ext_call_error_class", vec![e_var("__elephc_call")]), e_str("")),
                    vec![s_expr(call("exit", vec![e_int(255)]))],
                    vec![],
                    None,
                ),
                s_expr(e_method_call(e_this(), "fail", vec![])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                call("elephc_php_ext_value_check", vec![e_var("__elephc_call"), e_var("__elephc_value")]),
                BinOp::StrictNotEq,
                e_int(0),
            ),
            vec![s_expr(e_method_call(e_this(), "fail", vec![]))],
            vec![],
            None,
        ),
        s_return(call("__elephc_php_ext_value", vec![e_var("__elephc_call"), e_var("__elephc_value")])),
    ]);
    crate::synthetic_class::class(ITERATOR_CLASS)
        .final_()
        .implements("\\Iterator")
        .private_prop("id", TypeExpr::Int, Some(e_int(0)))
        .method(method("__construct").param("id", TypeExpr::Int).body(vec![crate::synthetic_class::s_prop_assign(
            e_this(),
            "id",
            e_var("id"),
        )]))
        .method(method("__destruct").body(vec![s_expr(call("elephc_php_ext_iter_free", vec![id()]))]))
        .method(method("rewind").returns(TypeExpr::Void).body(vec![s_expr(e_method_call(e_this(), "step", vec![e_int(0)]))]))
        .method(
            method("valid")
                .returns(TypeExpr::Bool)
                .body(vec![s_return(strict_eq(e_method_call(e_this(), "step", vec![e_int(1)]), e_int(1)))]),
        )
        .method(method("next").returns(TypeExpr::Void).body(vec![s_expr(e_method_call(e_this(), "step", vec![e_int(2)]))]))
        .method(method("current").returns(t_mixed()).body(vec![s_return(e_method_call(e_this(), "fetch", vec![e_int(3)]))]))
        .method(method("key").returns(t_mixed()).body(vec![s_return(e_method_call(e_this(), "fetch", vec![e_int(4)]))]))
        .method(step)
        .method(fetch)
        .method(fail)
        .build()
}

/// Whether instances of `class` are traversed with the extension's
/// `get_iterator` handler: it implements `IteratorAggregate`, or `Traversable`
/// without `Iterator`'s methods.
fn iterates_through_handler(class: &SurfaceClass) -> bool {
    let has = |name: &str| class.interfaces.iter().any(|implemented| implemented == name);
    !class.interface && (has("IteratorAggregate") || (has("Traversable") && !has("Iterator")))
}

/// `getIterator(): \Iterator`, iterating the engine object with the
/// extension's own iterator.
fn get_iterator_method(class: &SurfaceClass) -> crate::synthetic_class::MethodBuilder {
    let call_of = || call("elephc_php_ext_iter_call", vec![e_var("__elephc_id")]);
    method("getIterator").returns(t_class("\\Iterator")).body(vec![
        s_assign("__elephc_id", call("elephc_php_ext_iter_new", vec![e_method_call(e_this(), OBJECT_METHOD, vec![])])),
        s_if(
            strict_eq(e_var("__elephc_id"), e_int(0)),
            vec![s_throw(e_new(
                "\\Error",
                vec![e_str(&format!("Objects of hosted class {} cannot be iterated", class.name))],
            ))],
            vec![],
            None,
        ),
        // Starting the iteration threw: rethrow its error once it is freed.
        s_if(
            e_binop(e_var("__elephc_id"), BinOp::Lt, e_int(0)),
            vec![
                s_assign("__elephc_class", call("elephc_php_ext_call_error_class", vec![call_of()])),
                s_assign("__elephc_message", call("elephc_php_ext_call_error_message", vec![call_of()])),
                s_assign("__elephc_code", e_cast(CastType::Int, call("elephc_php_ext_call_error_code", vec![call_of()]))),
                s_expr(call("elephc_php_ext_iter_free", vec![e_var("__elephc_id")])),
                s_expr(call(
                    "__elephc_php_ext_throw",
                    vec![e_var("__elephc_class"), e_var("__elephc_message"), e_var("__elephc_code")],
                )),
            ],
            vec![],
            None,
        ),
        s_return(e_new(&format!("\\{ITERATOR_CLASS}"), vec![e_var("__elephc_id")])),
    ])
}

/// The table's static property naming the object the next wrapper
/// constructor adopts (0: none, construct normally).
const ADOPTING: &str = "adopting";

/// Builds the statement that assigns to the object table's "adopting" static property.
fn set_adopting(value: Expr) -> Stmt {
    crate::synthetic_class::s_static_prop_assign(&format!("\\{OBJECT_TABLE}"), ADOPTING, value)
}

/// The first statement of every wrapper constructor: adopt the object
/// `__elephc_php_ext_adopt` handed over, instead of creating one.
fn adoption_branch() -> Stmt {
    let adopting = || e_static_prop(&format!("\\{OBJECT_TABLE}"), ADOPTING);
    s_if(
        e_binop(adopting(), BinOp::StrictNotEq, e_int(0)),
        vec![object_id(), set_table_slot(adopting()), set_handle(adopting()), set_adopting(e_int(0)), s_return_void()],
        vec![],
        None,
    )
}

/// The constructor `new` runs for a wrapped class: its own, or the nearest
/// wrapped ancestor's, with the class declaring it.
fn constructor_of<'a>(
    class: &'a SurfaceClass,
    classes: &[(&'a InstalledExtension, &'a SurfaceClass)],
) -> Option<(&'a InstalledExtension, &'a SurfaceClass, &'a SurfaceFunction)> {
    let mut current = Some(class);
    let mut depth = 0;
    while let Some(candidate) = current {
        let owner = classes.iter().find(|(_, other)| other.name == candidate.name)?;
        if let Some(constructor) = candidate.methods.iter().find(|method| method.name.eq_ignore_ascii_case("__construct")) {
            return Some((owner.0, candidate, constructor));
        }
        depth += 1;
        current = candidate
            .parent
            .as_ref()
            .filter(|_| depth < 32)
            .and_then(|parent| classes.iter().find(|(_, other)| &other.name == parent).map(|(_, other)| *other));
    }
    None
}

/// Arguments the wrapper constructor accepts for its required parameters,
/// never read (the adoption branch returns first). `None` when one cannot be
/// made up (a by-reference or object parameter).
fn placeholder_arguments(
    class: &SurfaceClass,
    classes: &[(&InstalledExtension, &SurfaceClass)],
    wrapped: &BTreeSet<String>,
) -> Option<Vec<Expr>> {
    let Some((extension, owner, constructor)) = constructor_of(class, classes) else {
        return Some(Vec::new());
    };
    let Ok(planned) = plan(extension, constructor, wrapped, Some(&owner.name)) else {
        // Declared as an explaining variadic: no argument is required.
        return Some(Vec::new());
    };
    planned
        .fixed
        .iter()
        .take(constructor.required as usize)
        .map(|param| {
            if param.by_ref {
                return None;
            }
            match &param.ty {
                None => Some(e_null()),
                Some(TypeExpr::Int) => Some(e_int(0)),
                Some(TypeExpr::Float) => Some(e_float(0.0)),
                Some(TypeExpr::Str) => Some(e_str("")),
                Some(TypeExpr::Bool) => Some(e_bool(false)),
                Some(TypeExpr::Nullable(_)) => Some(e_null()),
                Some(ty) if *ty == t_mixed() => Some(e_null()),
                Some(ty) if *ty == t_array() => Some(e_array(vec![])),
                Some(_) => None,
            }
        })
        .collect()
}

/// `__ElephcPhpExtObjects::$objects[spl_object_id($this)]`, read as an int.
fn table_slot() -> Expr {
    e_null_coalesce(e_index(e_static_prop(&format!("\\{OBJECT_TABLE}"), "objects"), e_var("__elephc_id")), e_int(0))
}

/// Builds the statement that stores a value in the object table slot for the current wrapper
/// object's id.
fn set_table_slot(value: Expr) -> Stmt {
    s_static_prop_array_assign(&format!("\\{OBJECT_TABLE}"), "objects", e_var("__elephc_id"), value)
}

/// Builds the statement that records a handle in the wrapper's own property.
fn set_handle(value: Expr) -> Stmt {
    crate::synthetic_class::s_prop_assign(e_this(), HANDLE_PROPERTY, value)
}

/// `__clone()`: the copy starts with the source's handle in its property and
/// no table slot; it takes a clone of the engine object made by the class's
/// `clone_obj` handler, as PHP's `clone` does, or none while the source has
/// not created its object yet.
fn clone_method() -> crate::synthetic_class::MethodBuilder {
    let source = || e_var("__elephc_source");
    method("__clone").body(vec![
        s_assign("__elephc_source", crate::synthetic_class::e_this_prop(HANDLE_PROPERTY)),
        set_handle(e_int(0)),
        s_if(
            e_binop(source(), BinOp::StrictNotEq, e_int(0)),
            vec![
                s_assign("__elephc_object", call("__elephc_php_ext_clone", vec![source()])),
                object_id(),
                set_table_slot(e_var("__elephc_object")),
                set_handle(e_var("__elephc_object")),
            ],
            vec![],
            None,
        ),
    ])
}

/// Builds the statement that assigns the current wrapper object's `spl_object_id` to the local
/// `__elephc_id`.
fn object_id() -> Stmt {
    s_assign("__elephc_id", call("spl_object_id", vec![e_this()]))
}

/// The wrapper classes and interfaces of one extension, parents first.
fn wrapper_class_declarations(extension: &InstalledExtension, wrapped: &BTreeSet<String>, has_ini: bool) -> Vec<Stmt> {
    let classes: Vec<&SurfaceClass> =
        extension.surface.classes.iter().filter(|class| wrapped.contains(&class.name)).collect();
    let mut declared: BTreeSet<&str> = BTreeSet::new();
    let mut ordered = Vec::new();
    while ordered.len() < classes.len() {
        let before = ordered.len();
        for class in &classes {
            if declared.contains(class.name.as_str()) {
                continue;
            }
            let ready = class
                .parent
                .iter()
                .chain(class.interfaces.iter())
                .all(|name| declared.contains(name.as_str()) || !classes.iter().any(|other| &other.name == name));
            if ready {
                declared.insert(&class.name);
                ordered.push(*class);
            }
        }
        if ordered.len() == before {
            break;
        }
    }
    ordered
        .into_iter()
        .map(|class| {
            let all = &extension.surface.classes;
            wrap_in_namespace(&class.name, |local| {
                if class.interface {
                    wrapper_interface(class, local, wrapped)
                } else {
                    wrapper_class(extension, class, local, wrapped, all, has_ini)
                }
            })
        })
        .collect()
}

/// A hosted interface: its hosted parents, no methods (each implementing
/// wrapper declares its own).
fn wrapper_interface(class: &SurfaceClass, local: &str, wrapped: &BTreeSet<String>) -> Stmt {
    class
        .interfaces
        .iter()
        .filter(|name| wrapped.contains(*name))
        .fold(interface(local), |builder, parent| builder.extends(&format!("\\{parent}")))
        .build()
}

/// Engine interfaces Elephc declares too, with the methods (name, parameter
/// count, return type) a wrapper must have, as the extension's arginfo states
/// them, for the interface to be declared on it.
const ENGINE_INTERFACES: &[(&str, &[(&str, usize, Option<&str>)])] = &[
    ("Countable", &[("count", 0, Some("int"))]),
    (
        "ArrayAccess",
        &[
            ("offsetExists", 1, Some("bool")),
            ("offsetGet", 1, Some("mixed")),
            ("offsetSet", 2, Some("void")),
            ("offsetUnset", 1, Some("void")),
        ],
    ),
    ("JsonSerializable", &[("jsonSerialize", 0, Some("mixed"))]),
    (
        "Iterator",
        &[
            ("current", 0, Some("mixed")),
            ("key", 0, Some("mixed")),
            ("next", 0, Some("void")),
            ("rewind", 0, Some("void")),
            ("valid", 0, Some("bool")),
        ],
    ),
];

/// The engine interfaces `class` implements whose methods its wrapper
/// declares with exactly the interface's shape.
fn engine_interfaces(class: &SurfaceClass) -> Vec<&'static str> {
    ENGINE_INTERFACES
        .iter()
        .filter(|(name, _)| class.interfaces.iter().any(|implemented| implemented == name))
        .filter(|(_, methods)| {
            methods.iter().all(|(method_name, count, returns)| {
                class.methods.iter().any(|method| {
                    method.name.eq_ignore_ascii_case(method_name)
                        && method.params.len() == *count
                        && method.required as usize == *count
                        && method.params.iter().all(|param| {
                            !param.by_ref && !param.variadic && param.ty.as_deref().is_none_or(|ty| ty == "mixed")
                        })
                        && method.returns.as_deref().unwrap_or("mixed") == returns.unwrap_or("mixed")
                })
            })
        })
        .map(|(name, _)| *name)
        .collect()
}

/// Methods a wrapper never forwards: object lifecycle the wrapper handles
/// itself, and magic Elephc does not route to a hosted object yet.
const UNFORWARDED_METHODS: &[&str] = &[
    "__construct", "__destruct", "__clone", "__get", "__set", "__isset", "__unset", "__call", "__callstatic",
    "__serialize", "__unserialize", "__sleep", "__wakeup", "__set_state", "__debuginfo", "__invoke",
];

/// A hosted class: the wrapper holding its engine object, with one method
/// per public method the extension declares.
fn wrapper_class(
    extension: &InstalledExtension,
    class: &SurfaceClass,
    local: &str,
    wrapped: &BTreeSet<String>,
    all: &[SurfaceClass],
    has_ini: bool,
) -> Stmt {
    let parent = class.parent.as_ref().and_then(|name| all.iter().find(|other| &other.name == name));
    let inherited: BTreeSet<&str> = parent.map(|parent| parent.interfaces.iter().map(String::as_str).collect()).unwrap_or_default();
    let mut builder = crate::synthetic_class::class(local)
        .when(class.is_abstract, |builder| builder.abstract_())
        .when(class.is_final, |builder| builder.final_());
    builder = match &class.parent {
        Some(parent) => builder.extends(&format!("\\{parent}")),
        None => builder.implements(&format!("\\{OBJECT_INTERFACE}")),
    };
    for name in class.interfaces.iter().filter(|name| !inherited.contains(name.as_str())) {
        if wrapped.contains(name) {
            builder = builder.implements(&format!("\\{name}"));
        }
    }
    for name in engine_interfaces(class) {
        if !parent.is_some_and(|parent| engine_interfaces(parent).contains(&name)) {
            builder = builder.implements(&format!("\\{name}"));
        }
    }
    let iterable = iterates_through_handler(class);
    if iterable && !parent.is_some_and(iterates_through_handler) {
        builder = builder.implements("\\IteratorAggregate");
    }
    if iterable {
        builder = builder.method(get_iterator_method(class));
    }
    for constant in &class.constants {
        if let Some(value) = constant_expr(&constant.value) {
            builder = builder.constant(&constant.name, value);
        }
    }

    // __elephcObject(): the engine object, created on first use as `new`
    // does, so a subclass constructor that never calls its parent's still
    // gets one.
    let module = call(&extension.record.module_accessor(), vec![]);
    let mut create = Vec::new();
    if has_ini {
        create.push(s_expr(call("__elephc_php_ext_setup", vec![])));
    }
    create.push(s_assign("__elephc_object", call("elephc_php_ext_object_new", vec![module, e_str(&class.name)])));
    create.push(s_if(strict_eq(e_var("__elephc_object"), e_int(0)), vec![s_expr(call("exit", vec![e_int(255)]))], vec![], None));
    create.push(set_table_slot(e_var("__elephc_object")));
    create.push(set_handle(e_var("__elephc_object")));
    builder = builder.method(
        method(OBJECT_METHOD)
            .returns(TypeExpr::Int)
            .body(vec![
                object_id(),
                s_assign("__elephc_object", table_slot()),
                s_if(strict_eq(e_var("__elephc_object"), e_int(0)), create, vec![], None),
                s_return(e_var("__elephc_object")),
            ]),
    );
    // __elephcWrap(int $object): the class around an object the extension
    // made, built through the constructor's adoption branch.
    let siblings: Vec<(&InstalledExtension, &SurfaceClass)> = all.iter().map(|other| (extension, other)).collect();
    if !class.is_abstract {
        if let Some(arguments) = placeholder_arguments(class, &siblings, wrapped) {
            let qualified = format!("\\{}", class.name);
            builder = builder.method(
                method(WRAP_METHOD)
                    .static_()
                    .param("__elephc_object", TypeExpr::Int)
                    .returns(t_class(&qualified))
                    .body(vec![set_adopting(e_var("__elephc_object")), s_return(e_new(&qualified, arguments))]),
            );
        }
    }
    let own_constructor = class.methods.iter().any(|method| method.name.eq_ignore_ascii_case("__construct"));
    if parent.is_none() && !own_constructor {
        builder = builder.method(method("__construct").body(vec![adoption_branch()]));
    }
    if parent.is_none() {
        builder = builder
            .method(
                method(ADOPT_METHOD)
                    .param("__elephc_object", TypeExpr::Int)
                    .returns(TypeExpr::Void)
                    .body(vec![object_id(), set_table_slot(e_var("__elephc_object")), set_handle(e_var("__elephc_object"))]),
            )
            .method(method("__destruct").body(vec![
                object_id(),
                s_assign("__elephc_object", table_slot()),
                s_if(
                    e_binop(e_var("__elephc_object"), BinOp::StrictNotEq, e_int(0)),
                    vec![
                        set_table_slot(e_int(0)),
                        set_handle(e_int(0)),
                        s_expr(call("elephc_php_ext_object_release", vec![e_var("__elephc_object")])),
                    ],
                    vec![],
                    None,
                ),
            ]))
            .protected_prop(HANDLE_PROPERTY, TypeExpr::Int, Some(e_int(0)))
            .methods(property_methods())
            .method(clone_method())
            // What `var_dump()` and `print_r()` show is the engine object's
            // debug table, as in PHP, never the wrapper's own bookkeeping.
            .method(method("__debugInfo").body(vec![s_return(e_cast(
                CastType::Array,
                call(
                    "__elephc_php_ext_property",
                    vec![e_method_call(e_this(), OBJECT_METHOD, vec![]), e_str(""), e_int(4), e_null()],
                ),
            ))]));
    }

    for function in &class.methods {
        let lower = function.name.to_ascii_lowercase();
        // A non-public constructor is declared with its visibility: `new` stays
        // refused outside the class, as in PHP, and adoption still runs it.
        if (!function.is_public() && lower != "__construct") || function.is_abstract {
            continue;
        }
        if lower != "__construct" && UNFORWARDED_METHODS.contains(&lower.as_str()) {
            continue;
        }
        // Declared by the wrapper itself (`get_iterator_method`).
        if lower == "getiterator" && iterable {
            continue;
        }
        builder = builder.method(wrapper_method(extension, class, function, wrapped, has_ini));
    }
    builder.build()
}

/// `__get`, `__set`, `__isset` and `__unset`, answered by the engine object
/// (its declared properties, or the extension's property handlers).
fn property_methods() -> Vec<crate::synthetic_class::MethodBuilder> {
    let access = |op: i64, value: Expr| {
        call(
            "__elephc_php_ext_property",
            vec![e_method_call(e_this(), OBJECT_METHOD, vec![]), e_var("name"), e_int(op), value],
        )
    };
    vec![
        method("__get").param("name", TypeExpr::Str).returns(t_mixed()).body(vec![s_return(access(0, e_null()))]),
        method("__set")
            .param("name", TypeExpr::Str)
            .param("value", t_mixed())
            .returns(TypeExpr::Void)
            .body(vec![s_expr(access(1, e_var("value")))]),
        method("__isset")
            .param("name", TypeExpr::Str)
            .returns(TypeExpr::Bool)
            .body(vec![s_return(e_cast(CastType::Bool, access(2, e_null())))]),
        method("__unset").param("name", TypeExpr::Str).returns(TypeExpr::Void).body(vec![s_expr(access(3, e_null()))]),
    ]
}

/// One method of a wrapper class.
fn wrapper_method(
    extension: &InstalledExtension,
    class: &SurfaceClass,
    function: &SurfaceFunction,
    wrapped: &BTreeSet<String>,
    has_ini: bool,
) -> crate::synthetic_class::MethodBuilder {
    let display = format!("{}::{}", class.name, function.name);
    let is_constructor = function.name.eq_ignore_ascii_case("__construct");
    let callee = Callee::Method { class: &class.name, is_static: function.is_static };
    let builder = method(&function.name)
        .when(function.is_static, |builder| builder.static_())
        .when(function.visibility.as_deref() == Some("private"), |builder| builder.private())
        .when(function.visibility.as_deref() == Some("protected"), |builder| builder.protected());
    match plan_wrapper(extension, function, &display, callee, has_ini, wrapped) {
        Ok(planned) => {
            let builder = declare_params!(builder, planned.plan);
            let mut body = planned.body;
            if is_constructor {
                body.insert(0, adoption_branch());
            }
            let builder = if is_constructor { builder } else { builder.returns(planned.returns) };
            builder.body(body)
        }
        Err(reason) => {
            // The arginfo's parameters, untyped and optional, so any call PHP
            // accepts compiles and explains itself. (Not a variadic: the
            // backend's method ABI adapters cannot project an unread one.)
            let builder = function
                .params
                .iter()
                .filter(|param| !param.variadic)
                .fold(builder, |builder, param| builder.param_untyped_default(&param.name, e_null()));
            let builder = if is_constructor { builder } else { builder.returns(t_mixed()) };
            let mut body = vec![s_throw(e_new(
                "\\Error",
                vec![e_str(&format!("{display}() cannot be called from Elephc yet: {reason}"))],
            ))];
            if is_constructor {
                body.insert(0, adoption_branch());
            }
            builder.body(body)
        }
    }
}

/// Builds the namespaced constant declarations for the extension's constants, skipping those whose
/// value cannot be expressed as a PHP literal.
fn constant_declarations(extension: &InstalledExtension) -> Vec<Stmt> {
    extension
        .surface
        .constants
        .iter()
        .filter_map(|constant| {
            let value = constant_expr(&constant.value)?;
            Some(wrap_in_namespace(&constant.name, |local| s_const(local, value)))
        })
        .collect()
}

/// Converts a surface constant value to an AST literal expression, or None for an unsupported value
/// or an unparsable float.
fn constant_expr(value: &ConstantValue) -> Option<Expr> {
    match value {
        ConstantValue::Null => Some(e_null()),
        ConstantValue::Bool { value } => Some(e_bool(*value)),
        ConstantValue::Int { value } => Some(e_int(*value)),
        ConstantValue::Float { value } => value.parse::<f64>().ok().map(e_float),
        ConstantValue::String { value } => Some(e_str(value)),
        ConstantValue::Unsupported => None,
    }
}

/// Emits `build(local_name)` inside `namespace Ns { … }` when `qualified` is
/// namespaced, so a hosted `Ns\f` is declared where PHP code looks for it.
fn wrap_in_namespace(qualified: &str, build: impl FnOnce(&str) -> Stmt) -> Stmt {
    match qualified.rsplit_once('\\') {
        Some((namespace, local)) => s_namespace(namespace, vec![build(local)]),
        None => build(qualified),
    }
}

/// Maps an arginfo type to a declarable Elephc type. `None` means the type
/// needs a value Elephc cannot pass to an extension yet (a callable, an object
/// of a class no wrapper declares). `self_class` resolves `self` and `static`.
fn param_type(ty: Option<&str>, classes: &BTreeSet<String>, self_class: Option<&str>) -> Option<Option<TypeExpr>> {
    let Some(ty) = ty else { return Some(None) };
    let simple = |name: &str| -> Option<TypeExpr> {
        match name {
            "int" => Some(TypeExpr::Int),
            "float" => Some(TypeExpr::Float),
            "string" => Some(TypeExpr::Str),
            "bool" => Some(TypeExpr::Bool),
            "array" => Some(t_array()),
            "mixed" => Some(t_mixed()),
            // Passed as a hosted `Closure` the extension calls back through.
            // Declared mixed: inside a namespace the type would not resolve as
            // PHP's callable, and the extension's own argument parsing checks it.
            "callable" => Some(t_mixed()),
            "self" | "static" => self_class.map(|class| t_class(&format!("\\{class}"))),
            class if classes.contains(class) => Some(t_class(&format!("\\{class}"))),
            _ => None,
        }
    };
    if let Some(declared) = simple(ty) {
        return Some(Some(declared));
    }
    if let Some(inner) = ty.strip_prefix('?') {
        // `mixed` already admits null; `?mixed` is not a PHP type.
        return simple(inner).map(|declared| Some(if declared == t_mixed() { declared } else { t_nullable(declared) }));
    }
    let scalar_members = ["int", "float", "string", "bool", "array", "null", "false", "true", "mixed"];
    if ty.split('|').all(|member| scalar_members.contains(&member)) {
        // A scalar union: the extension's own argument parsing checks it.
        return Some(Some(t_mixed()));
    }
    None
}

/// A return type naming a wrapped class.
struct ObjectReturn {
    declared: TypeExpr,
    class: String,
    nullable: bool,
}

/// `C`, `?C`, `self`, `static`: a result Elephc declares with its class, so
/// it can be passed on where that class is expected.
fn object_return(ty: Option<&str>, classes: &BTreeSet<String>, self_class: Option<&str>) -> Option<ObjectReturn> {
    let ty = ty?;
    let (nullable, name) = match ty.strip_prefix('?') {
        Some(inner) => (true, inner),
        None => (false, ty),
    };
    let class = match name {
        "self" | "static" => self_class?.to_string(),
        other if classes.contains(other) => other.to_string(),
        _ => return None,
    };
    let named = t_class(&format!("\\{class}"));
    Some(ObjectReturn { declared: if nullable { t_nullable(named) } else { named }, class, nullable })
}

/// The Elephc return type and the cast applied to the walked result.
fn return_type(ty: Option<&str>) -> (TypeExpr, Option<CastType>) {
    match ty {
        Some("int") => (TypeExpr::Int, Some(CastType::Int)),
        Some("float") => (TypeExpr::Float, Some(CastType::Float)),
        Some("string") => (TypeExpr::Str, Some(CastType::String)),
        Some("bool") => (TypeExpr::Bool, Some(CastType::Bool)),
        // Not `array`: the checker types an `(array)` cast of a mixed value as
        // mixed, so the declaration would be refused. The extension's own
        // arginfo already guarantees the value is an array.
        Some("void") => (TypeExpr::Void, None),
        _ => (t_mixed(), None),
    }
}

/// A default Elephc can declare, from arginfo's source text.
fn default_expr(text: &str, extension: &InstalledExtension) -> Option<Expr> {
    let text = text.trim();
    match text {
        "null" | "NULL" => return Some(e_null()),
        "true" => return Some(e_bool(true)),
        "false" => return Some(e_bool(false)),
        "[]" => return Some(e_array(vec![])),
        _ => {}
    }
    if let Ok(value) = text.parse::<i64>() {
        return Some(e_int(value));
    }
    if text.contains(['.', 'e', 'E']) && !text.starts_with(['"', '\'']) {
        if let Ok(value) = text.parse::<f64>() {
            return Some(e_float(value));
        }
    }
    for quote in ['"', '\''] {
        if let Some(inner) = text.strip_prefix(quote).and_then(|rest| rest.strip_suffix(quote)) {
            if !inner.contains('\\') && !inner.contains('$') {
                return Some(e_str(inner));
            }
            return None;
        }
    }
    if let Some(constant) = extension.surface.constants.iter().find(|constant| constant.name == text) {
        return constant_expr(&constant.value);
    }
    if KNOWN_CONSTANTS.contains(&text) {
        return Some(e_const(text));
    }
    None
}

/// How one wrapper passes its parameters.
struct Plan {
    /// Declared parameters that are always passed, in order.
    fixed: Vec<PlannedParam>,
    tail: Option<Tail>,
}

/// Arguments passed only when the caller writes them.
enum Tail {
    /// The extension's own variadic parameter: any number, by value.
    Variadic(String),
    /// Optional parameters whose default only the C code knows, or that are
    /// by reference: each is declared untyped with the `OMITTED` sentinel as
    /// its default, and passed only if the caller gave it, by position or by
    /// name. Each keeps its own passing mode, and the checker refuses a
    /// duplicate, unknown or surplus argument as it does for any function.
    Optional { params: Vec<TailParam> },
}

/// One parameter of an optional tail.
struct TailParam {
    name: String,
    by_ref: bool,
    /// The arginfo default, passed when a later argument is given but this
    /// one is not; `None` when only the C code knows it, which PHP refuses.
    default: Option<Expr>,
}

/// The default of an optional-tail parameter: a value no caller writes, so
/// the wrapper tells an omitted argument from any given one.
const OMITTED: &str = "\u{1}elephc:omitted\u{1}";

struct PlannedParam {
    name: String,
    ty: Option<TypeExpr>,
    default: Option<Expr>,
    by_ref: bool,
}

/// The PHP name a wrapper function gives a parameter: the arginfo name unchanged, so named
/// arguments keep working.
fn local_name(param: &SurfaceParam) -> String {
    // Arginfo names are PHP identifiers already; prefixing would change named
    // arguments, so they are kept, and the wrapper's own locals are prefixed.
    param.name.clone()
}

/// The explanation that a parameter's type cannot yet be passed to an extension, naming the
/// parameter and its type.
fn unpassable(param: &SurfaceParam) -> String {
    format!(
        "parameter ${} takes {}, which Elephc cannot pass to an extension yet",
        param.name,
        param.ty.as_deref().unwrap_or("?")
    )
}

/// Plans a wrapper, or explains why the function cannot be called yet.
///
/// A by-reference parameter with a default is never declared with that
/// default: omitted, it would bind the wrapper's write-back to a variable that
/// only ever held null, which the backend does not carry back (and whose write
/// corrupts memory). It starts the optional tail instead.
fn plan(
    extension: &InstalledExtension,
    function: &SurfaceFunction,
    classes: &BTreeSet<String>,
    self_class: Option<&str>,
) -> Result<Plan, String> {
    let param_type = |ty: Option<&str>| param_type(ty, classes, self_class);
    let mut fixed = Vec::new();
    for (index, param) in function.params.iter().enumerate() {
        let ty = param_type(param.ty.as_deref()).ok_or_else(|| unpassable(param))?;
        if param.variadic {
            if param.by_ref {
                return Err(format!("parameter ${} is a by-reference variadic", param.name));
            }
            return Ok(Plan { fixed, tail: Some(Tail::Variadic(local_name(param))) });
        }
        if (index as u32) < function.required {
            fixed.push(PlannedParam { name: local_name(param), ty, default: None, by_ref: param.by_ref });
            continue;
        }
        let default = param.default.as_deref().and_then(|text| default_expr(text, extension));
        match default {
            Some(default) if !param.by_ref => {
                fixed.push(PlannedParam { name: local_name(param), ty, default: Some(default), by_ref: false });
            }
            _ => {
                let rest = &function.params[index..];
                if let Some(variadic) = rest.iter().find(|later| later.variadic) {
                    return Err(format!(
                        "parameter ${} follows optional parameters, which cannot be skipped positionally yet",
                        variadic.name
                    ));
                }
                for later in rest {
                    param_type(later.ty.as_deref()).ok_or_else(|| unpassable(later))?;
                }
                let params = rest
                    .iter()
                    .map(|later| TailParam {
                        name: local_name(later),
                        by_ref: later.by_ref,
                        default: later.default.as_deref().and_then(|text| default_expr(text, extension)),
                    })
                    .collect();
                return Ok(Plan { fixed, tail: Some(Tail::Optional { params }) });
            }
        }
    }
    Ok(Plan { fixed, tail: None })
}

/// What a wrapper calls.
#[derive(Clone, Copy)]
enum Callee<'a> {
    Function,
    /// A method of hosted class `class`, on the wrapper's own engine object
    /// unless static.
    Method { class: &'a str, is_static: bool },
}

/// A planned wrapper: its parameters, body and declared return type.
struct Wrapper {
    plan: Plan,
    body: Vec<Stmt>,
    returns: TypeExpr,
}

/// The wrapper for one hosted function.
fn wrapper(
    extension: &InstalledExtension,
    function: &SurfaceFunction,
    local: &str,
    has_ini: bool,
    classes: &BTreeSet<String>,
) -> Stmt {
    match plan_wrapper(extension, function, &function.name, Callee::Function, has_ini, classes) {
        Ok(planned) => declare_params!(crate::synthetic_class::function(local), planned.plan)
            .returns(planned.returns)
            .body(planned.body)
            .build(),
        Err(reason) => unsupported_wrapper(function, local, &reason),
    }
}

/// Plans a wrapper's parameters and builds its body, or explains why the
/// callee cannot be called yet. `display` is the name PHP's messages use
/// (`f`, `C::m`).
fn plan_wrapper(
    extension: &InstalledExtension,
    function: &SurfaceFunction,
    display: &str,
    callee: Callee<'_>,
    has_ini: bool,
    classes: &BTreeSet<String>,
) -> Result<Wrapper, String> {
    let self_class = match callee {
        Callee::Method { class, .. } => Some(class),
        Callee::Function => None,
    };
    let is_constructor = self_class.is_some() && function.name.eq_ignore_ascii_case("__construct");
    let object_return =
        if is_constructor { None } else { object_return(function.returns.as_deref(), classes, self_class) };
    let (declared_return, cast) = match &object_return {
        _ if is_constructor => (TypeExpr::Void, None),
        Some(object) => (object.declared.clone(), None),
        None => return_type(function.returns.as_deref()),
    };
    let plan = plan(extension, function, classes, self_class)?;

    let call_var = || e_var("__elephc_call");
    let fixed_count = plan.fixed.len() as i64;
    // Whether the caller gave optional-tail parameter `name`, by position or name.
    let given = |name: &str| e_binop(e_var(name), BinOp::StrictNotEq, e_str(OMITTED));
    let mut body = Vec::new();
    if has_ini {
        body.push(s_expr(call("__elephc_php_ext_setup", vec![])));
    }
    let on_object = matches!(callee, Callee::Method { is_static: false, .. });
    if on_object {
        body.push(s_assign("__elephc_this", e_method_call(e_this(), OBJECT_METHOD, vec![])));
    }
    let argc = match &plan.tail {
        Some(Tail::Variadic(name)) => e_binop(e_int(fixed_count), BinOp::Add, call("count", vec![e_var(name)])),
        // The extension sees arguments up to the last one given. The checks
        // run before the call exists, so a refusal has nothing to free.
        Some(Tail::Optional { params }) => {
            body.push(s_assign("__elephc_argc", e_int(fixed_count)));
            for (position, param) in params.iter().enumerate() {
                body.push(s_if(
                    given(&param.name),
                    vec![s_assign("__elephc_argc", e_int(fixed_count + position as i64 + 1))],
                    vec![],
                    None,
                ));
            }
            for (position, param) in params.iter().enumerate() {
                let number = fixed_count + position as i64 + 1;
                if param.default.is_none() {
                    body.push(s_if(
                        e_binop(
                            e_not(given(&param.name)),
                            BinOp::And,
                            e_binop(e_var("__elephc_argc"), BinOp::Gt, e_int(number)),
                        ),
                        vec![s_throw(e_new(
                            "\\ArgumentCountError",
                            vec![e_str(&format!(
                                "{display}(): Argument #{number} (${}) must be passed explicitly, because the default value is not known",
                                param.name
                            ))],
                        ))],
                        vec![],
                        None,
                    ));
                }
            }
            e_var("__elephc_argc")
        }
        None => e_int(fixed_count),
    };
    let module = call(&extension.record.module_accessor(), vec![]);
    let site = next_site();
    let object = || if on_object { e_var("__elephc_this") } else { e_int(0) };
    body.push(s_assign("__elephc_argc", argc));
    let argc = || e_var("__elephc_argc");
    let new_call = match callee {
        Callee::Function => call("elephc_php_ext_call_new_at", vec![module, e_str(&function.name), argc(), e_int(site)]),
        Callee::Method { class, .. } => call(
            "elephc_php_ext_method_new_at",
            vec![module, e_str(class), e_str(&function.name), object(), argc(), e_int(site)],
        ),
    };
    // After the first call the site knows its callee: no name crosses the boundary.
    body.push(s_assign("__elephc_call", call("elephc_php_ext_site_call", vec![e_int(site), object(), argc()])));
    body.push(s_if(call("ptr_is_null", vec![call_var()]), vec![s_assign("__elephc_call", new_call)], vec![], None));
    // The engine already reported why (a module that failed to start, a
    // function it does not export), as PHP would for a fatal.
    body.push(s_if(call("ptr_is_null", vec![call_var()]), vec![s_expr(call("exit", vec![e_int(255)]))], vec![], None));
    // Mode: 1 by reference, 2 a callable the extension may call back.
    let takes_callable = |index: i64| {
        function.params.get(index as usize).is_some_and(|param| {
            param.ty.as_deref().is_some_and(|ty| ty.trim_start_matches('?') == "callable")
        })
    };
    let pass = |index: i64, value: Expr, by_ref: bool| {
        let mode = i64::from(by_ref) | (i64::from(takes_callable(index)) << 1);
        s_expr(call("__elephc_php_ext_arg", vec![call_var(), e_int(index), value, e_int(mode)]))
    };
    for (index, param) in plan.fixed.iter().enumerate() {
        let index = index as i64;
        let direct = match (&param.ty, param.by_ref || takes_callable(index)) {
            (Some(TypeExpr::Int), false) => Some(("elephc_php_ext_call_arg_long", e_var(&param.name))),
            (Some(TypeExpr::Float), false) => Some(("elephc_php_ext_call_arg_double", e_var(&param.name))),
            (Some(TypeExpr::Bool), false) => Some((
                "elephc_php_ext_call_arg_bool",
                crate::synthetic_class::e_ternary(e_var(&param.name), e_int(1), e_int(0)),
            )),
            _ => None,
        };
        body.push(match direct {
            Some((setter, value)) => s_expr(call(setter, vec![call_var(), e_int(index), value])),
            None => pass(index, e_var(&param.name), param.by_ref),
        });
    }
    match &plan.tail {
        Some(Tail::Variadic(name)) => {
            body.push(s_assign("__elephc_index", e_int(fixed_count)));
            body.push(s_foreach(
                e_var(name),
                None,
                "__elephc_extra",
                vec![
                    s_expr(call(
                        "__elephc_php_ext_arg",
                        vec![call_var(), e_var("__elephc_index"), e_var("__elephc_extra"), e_int(0)],
                    )),
                    s_assign("__elephc_index", e_binop(e_var("__elephc_index"), BinOp::Add, e_int(1))),
                ],
            ));
        }
        // A skipped parameter before a given one gets its arginfo default.
        Some(Tail::Optional { params }) => {
            for (position, param) in params.iter().enumerate() {
                let index = fixed_count + position as i64;
                let skipped = match &param.default {
                    Some(default) => vec![s_if(
                        e_binop(e_var("__elephc_argc"), BinOp::Gt, e_int(index)),
                        vec![pass(index, default.clone(), param.by_ref)],
                        vec![],
                        None,
                    )],
                    None => vec![],
                };
                body.push(s_if(
                    given(&param.name),
                    vec![pass(index, e_var(&param.name), param.by_ref)],
                    vec![],
                    Some(skipped),
                ));
            }
        }
        None => {}
    }
    body.push(s_assign("__elephc_status", call("__elephc_php_ext_invoke", vec![call_var()])));
    // Written back even when the extension threw: PHP keeps what it wrote.
    let written_back = |index: i64| {
        call(
            "__elephc_php_ext_rebuild",
            vec![call_var(), call("elephc_php_ext_call_ref", vec![call_var(), e_int(index)])],
        )
    };
    for (index, param) in plan.fixed.iter().enumerate() {
        if param.by_ref {
            body.push(s_assign(&param.name, written_back(index as i64)));
        }
    }
    // Only into a variable the caller gave: an omitted by-reference
    // parameter is the wrapper's own local.
    if let Some(Tail::Optional { params }) = &plan.tail {
        for (position, param) in params.iter().enumerate() {
            if param.by_ref {
                body.push(s_if(
                    given(&param.name),
                    vec![s_assign(&param.name, written_back(fixed_count + position as i64))],
                    vec![],
                    None,
                ));
            }
        }
    }
    body.push(s_if(
        e_binop(e_var("__elephc_status"), BinOp::StrictNotEq, e_int(0)),
        vec![s_expr(call("__elephc_php_ext_raise", vec![call_var()]))],
        vec![],
        None,
    ));
    // A fluent method returns its own object: the same wrapper, as in PHP.
    let returns_own_class = object_return.as_ref().is_some_and(|object| Some(object.class.as_str()) == self_class);
    if on_object && (declared_return == t_mixed() || returns_own_class) {
        body.push(s_if(
            strict_eq(call("elephc_php_ext_call_returned", vec![call_var(), e_var("__elephc_this")]), e_int(1)),
            vec![s_expr(call("elephc_php_ext_call_free", vec![call_var()])), s_return(e_this())],
            vec![],
            None,
        ));
    }
    let direct = match declared_return {
        TypeExpr::Int if cast.is_some() => {
            Some((1, call("ptr_get", vec![call("elephc_php_ext_call_result", vec![call_var()])])))
        }
        TypeExpr::Float if cast.is_some() => Some((2, call("elephc_php_ext_call_result_double", vec![call_var()]))),
        TypeExpr::Bool if cast.is_some() => Some((
            3,
            strict_eq(call("elephc_php_ext_call_result_bool", vec![call_var()]), e_int(1)),
        )),
        _ => None,
    };
    if let Some((kind, value)) = direct {
        body.push(s_if(
            strict_eq(call("elephc_php_ext_call_result_is", vec![call_var(), e_int(kind)]), e_int(1)),
            vec![
                s_assign("__elephc_value", value),
                s_expr(call("elephc_php_ext_call_free", vec![call_var()])),
                s_return(e_var("__elephc_value")),
            ],
            vec![],
            None,
        ));
    }
    let result = call("__elephc_php_ext_result", vec![call_var()]);
    if let Some(object) = &object_return {
        // Narrowed by `instanceof`, so the declared class reaches the caller.
        let value = || e_var("__elephc_result");
        body.push(s_assign("__elephc_result", result));
        if object.nullable {
            body.push(s_if(strict_eq(value(), e_null()), vec![s_return(e_null())], vec![], None));
        }
        body.push(s_if(e_instance_of(value(), &format!("\\{}", object.class)), vec![s_return(value())], vec![], None));
        body.push(s_throw(e_new(
            "\\TypeError",
            vec![e_str(&format!(
                "{display}(): Return value must be of type {}{}",
                if object.nullable { "?" } else { "" },
                object.class
            ))],
        )));
    } else if declared_return == TypeExpr::Void {
        body.push(s_expr(result));
    } else {
        body.push(s_return(match cast {
            Some(cast) => e_cast(cast, result),
            None => result,
        }));
    }
    Ok(Wrapper { plan, body, returns: declared_return })
}

/// A declaration for a function whose parameters cannot be passed yet. It is
/// still declared, so calling it fails with the reason instead of an
/// "undefined function".
fn unsupported_wrapper(function: &SurfaceFunction, local: &str, reason: &str) -> Stmt {
    crate::synthetic_class::function(local)
        .variadic("__elephc_arguments", Some(t_mixed()))
        .returns(t_mixed())
        .body(vec![s_throw(e_new(
            "\\Error",
            vec![e_str(&format!("{}() cannot be called from Elephc yet: {reason}", function.name))],
        ))])
        .build()
}

/// Names the functions and methods whose wrappers only raise, for
/// `extension add` to report.
pub fn unsupported_functions(extension: &InstalledExtension) -> Vec<(String, String)> {
    let wrapped = wrapped_classes(std::slice::from_ref(extension));
    let functions = extension.surface.functions.iter().filter_map(|function| {
        plan(extension, function, &wrapped, None).err().map(|reason| (function.name.clone(), reason))
    });
    let methods = extension
        .surface
        .classes
        .iter()
        .filter(|class| wrapped.contains(&class.name) && !class.interface)
        .flat_map(|class| {
            class
                .methods
                .iter()
                .filter(|method| method.is_public() && !method.is_abstract)
                .filter_map(|method| {
                    plan(extension, method, &wrapped, Some(&class.name))
                        .err()
                        .map(|reason| (format!("{}::{}", class.name, method.name), reason))
                })
                .collect::<Vec<_>>()
        });
    functions.chain(methods).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;
    use crate::php_ext::build::BuildRecord;
    use crate::php_ext::surface::ExtensionSurface;
    use std::path::PathBuf;

    /// Test helper: builds an `InstalledExtension` named `demo` from a surface JSON string.
    fn installed(surface_json: &str) -> InstalledExtension {
        InstalledExtension {
            name: "demo".into(),
            artifact_dir: PathBuf::from("/x"),
            record: BuildRecord { extension: "demo".into(), archive: "lib/libelephc_ext_demo.a".into(), cxx: false, libraries: vec![] },
            surface: ExtensionSurface::from_json(surface_json).expect("valid surface"),
        }
    }

    const SURFACE: &str = r#"{"module":"demo","version":"1.0.0","functions":[
        {"name":"demo_decode","required":1,"returns":null,"params":[
            {"name":"json","type":"string","default":null},
            {"name":"assoc","type":"bool","default":null},
            {"name":"depth","type":"int","default":null}]},
        {"name":"demo_add","required":1,"returns":"int","params":[
            {"name":"a","type":"int","default":null},
            {"name":"b","type":"int","default":"DEMO_STEP"}]},
        {"name":"demo_inc","required":1,"returns":"int|false","params":[
            {"name":"key","type":"string","default":null},
            {"name":"success","type":null,"by_ref":true,"default":"null"}]},
        {"name":"demo_map","required":2,"returns":"array","params":[
            {"name":"callback","type":"object","default":null},
            {"name":"items","type":"array","default":null}]},
        {"name":"demo_sum","required":0,"returns":"int","params":[
            {"name":"values","type":"int","variadic":true,"default":null}]}],
        "classes":[
            {"name":"DemoException","parent":"RuntimeException","methods":[]},
            {"name":"DemoParseException","parent":"DemoException","methods":[]},
            {"name":"DemoThing","parent":null,"methods":[{"name":"go","required":0,"returns":null,"params":[]}]}],
        "constants":[{"name":"DEMO_STEP","value":{"type":"int","value":3}},
                     {"name":"DEMO_LIST","value":{"type":"array"}}],
        "ini":[]}"#;

    /// Test helper: finds the function declaration with the given name in a generated program,
    /// panicking when it is missing.
    fn function<'a>(program: &'a Program, name: &str) -> &'a StmtKind {
        &program
            .iter()
            .find(|stmt| matches!(&stmt.kind, StmtKind::FunctionDecl { name: n, .. } if n == name))
            .unwrap_or_else(|| panic!("{name} is declared"))
            .kind
    }

    /// Test helper: the hosted extensions of a project that installs the sample `demo` surface and
    /// sets no ini values.
    fn hosted() -> HostedExtensions {
        HostedExtensions { extensions: vec![installed(SURFACE)], ini: vec![] }
    }

    /// simdjson's shape: optional parameters with no stated default must be
    /// passed only when the caller passes them, so they become a variadic tail.
    #[test]
    fn unknown_defaults_become_a_variadic_tail() {
        let program = declarations(&hosted());
        let StmtKind::FunctionDecl { params, variadic, return_type, .. } = function(&program, "demo_decode") else { unreachable!() };
        let names: Vec<&str> = params.iter().map(|param| param.0.as_str()).collect();
        assert_eq!(names, ["json", "assoc", "depth"], "each parameter keeps its name");
        assert!(params[1].2.is_some() && params[2].2.is_some(), "optional, defaulting to the sentinel");
        assert!(variadic.is_none());
        assert_eq!(return_type.as_ref(), Some(&t_mixed()), "no declared return type means mixed");
    }

    /// A default naming the extension's own constant is inlined; a declared
    /// scalar return type is declared and cast.
    #[test]
    fn known_defaults_are_declared_and_always_passed() {
        let program = declarations(&hosted());
        let StmtKind::FunctionDecl { params, variadic, return_type, .. } = function(&program, "demo_add") else { unreachable!() };
        assert_eq!(params.len(), 2);
        assert!(params[1].2.is_some(), "DEMO_STEP inlined as the default");
        assert!(variadic.is_none());
        assert_eq!(return_type.as_ref(), Some(&TypeExpr::Int));
    }

    /// An optional by-reference parameter becomes a by-reference variadic tail:
    /// declared with its `null` default, an omitted argument would bind the
    /// write-back to a null-only variable, which the backend does not carry
    /// back and whose write corrupts memory.
    #[test]
    fn optional_by_reference_parameters_become_a_by_reference_tail() {
        let program = declarations(&hosted());
        let StmtKind::FunctionDecl { params, variadic, .. } = function(&program, "demo_inc") else {
            unreachable!()
        };
        assert_eq!(params.len(), 2, "$key and &$success");
        assert!(variadic.is_none());
        assert!(!params[0].3 && params[1].3, "only $success is by reference");
    }

    /// An object of the program's own classes cannot be handed to an extension
    /// yet; the function is still declared so a call explains itself.
    #[test]
    fn unpassable_parameters_produce_an_explaining_wrapper() {
        let extension = installed(SURFACE);
        let unsupported = unsupported_functions(&extension);
        assert_eq!(unsupported.len(), 1);
        assert_eq!(unsupported[0].0, "demo_map");
        assert!(unsupported[0].1.contains("object"));
        let program = declarations(&hosted());
        let StmtKind::FunctionDecl { variadic, .. } = function(&program, "demo_map") else { unreachable!() };
        assert_eq!(variadic.as_deref(), Some("__elephc_arguments"));
    }

    /// Verifies a variadic arginfo parameter becomes a variadic wrapper parameter with no fixed
    /// parameters.
    #[test]
    fn variadic_parameters_stay_variadic() {
        let program = declarations(&hosted());
        let StmtKind::FunctionDecl { params, variadic, .. } = function(&program, "demo_sum") else { unreachable!() };
        assert!(params.is_empty());
        assert_eq!(variadic.as_deref(), Some("values"));
    }

    /// Exception classes are declared parents-first, and rebuilt
    /// subclass-first so a parent's arm never catches its child.
    #[test]
    fn exception_classes_are_declared_and_rebuilt_in_order() {
        let program = declarations(&hosted());
        let classes: Vec<&str> = program
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::ClassDecl { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            classes,
            vec![OBJECT_TABLE, ITERATOR_CLASS, "DemoException", "DemoParseException", "DemoThing"],
            "exceptions plainly, DemoThing as a wrapper"
        );
        let throwables = declared_throwables(&hosted().extensions);
        let child = throwables.iter().position(|name| name == "DemoParseException").unwrap();
        let parent = throwables.iter().position(|name| name == "DemoException").unwrap();
        let base = throwables.iter().position(|name| name == "RuntimeException").unwrap();
        assert!(child < parent && parent < base);
    }

    /// Verifies scalar constants are declared in the generated program while constants of
    /// unsupported types are skipped.
    #[test]
    fn scalar_constants_are_declared_and_others_skipped() {
        let program = declarations(&hosted());
        let constants: Vec<&str> = program
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::ConstDecl { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(constants, vec!["DEMO_STEP"]);
    }

    /// A class with methods becomes a wrapper: the hidden interface, the
    /// object lifecycle, property routing, and one method per public method.
    #[test]
    fn classes_with_methods_become_wrappers() {
        let program = declarations(&hosted());
        let (implements, methods) = program
            .iter()
            .find_map(|stmt| match &stmt.kind {
                StmtKind::ClassDecl { name, implements, methods, .. } if name == "DemoThing" => {
                    Some((implements, methods))
                }
                _ => None,
            })
            .expect("DemoThing is declared");
        assert_eq!(implements.len(), 1, "the hidden interface only");
        let names: Vec<&str> = methods.iter().map(|method| method.name.as_str()).collect();
        for expected in [
            OBJECT_METHOD, "__construct", ADOPT_METHOD, "__destruct", "__get", "__set", "__isset", "__unset", "__clone",
            "__debugInfo", "go",
        ] {
            assert!(names.contains(&expected), "{expected} in {names:?}");
        }
        assert!(
            program.iter().any(|stmt| matches!(&stmt.kind, StmtKind::FunctionDecl { name, .. } if name == "__elephc_php_ext_adopt")),
            "results are adopted by class"
        );
    }

    /// `static`, `self` and wrapped class names declare the class, so a
    /// result can be passed on where the class is expected.
    #[test]
    fn object_returns_declare_their_class() {
        let wrapped: BTreeSet<String> = ["Ds\\Map".to_string(), "Ds\\Pair".to_string()].into();
        let pair = object_return(Some("Ds\\Pair"), &wrapped, Some("Ds\\Map")).expect("a wrapped class");
        assert_eq!(pair.class, "Ds\\Pair");
        assert!(!pair.nullable);
        let own = object_return(Some("?static"), &wrapped, Some("Ds\\Map")).expect("static is the class");
        assert_eq!((own.class.as_str(), own.nullable), ("Ds\\Map", true));
        assert!(object_return(Some("Traversable"), &wrapped, Some("Ds\\Map")).is_none(), "an unwrapped class stays mixed");
        assert!(object_return(Some("int"), &wrapped, None).is_none());
    }

    /// Verifies arginfo types map to Elephc types: nullable scalars, union types as mixed, wrapped
    /// classes and `static` as classes, a missing type as untyped, and callables or unwrapped
    /// classes as unpassable.
    #[test]
    fn maps_arginfo_types() {
        let none = BTreeSet::new();
        assert_eq!(param_type(Some("?string"), &none, None), Some(Some(t_nullable(TypeExpr::Str))));
        assert_eq!(param_type(Some("array|string"), &none, None), Some(Some(t_mixed())));
        assert_eq!(param_type(Some("callable"), &none, None), Some(Some(t_mixed())), "passed as a hosted Closure");
        assert_eq!(param_type(Some("?callable"), &none, None), Some(Some(t_mixed())), "never ?mixed");
        assert_eq!(param_type(Some("object"), &none, None), None);
        assert_eq!(param_type(Some("Foo\\Bar"), &none, None), None);
        assert_eq!(param_type(None, &none, None), Some(None));
        let wrapped: BTreeSet<String> = ["Foo\\Bar".to_string()].into();
        assert_eq!(param_type(Some("?Foo\\Bar"), &wrapped, None), Some(Some(t_nullable(t_class("\\Foo\\Bar")))));
        assert_eq!(param_type(Some("static"), &none, Some("Foo\\Bar")), Some(Some(t_class("\\Foo\\Bar"))));
    }

    /// Verifies arginfo default text converts to expressions for ints, negative numbers, floats,
    /// empty strings and known constants, and is left out for class constants and strings with
    /// escapes.
    #[test]
    fn reads_arginfo_defaults() {
        let extension = installed(SURFACE);
        assert!(default_expr("512", &extension).is_some());
        assert!(default_expr("-1", &extension).is_some());
        assert!(default_expr("1.5", &extension).is_some());
        assert!(default_expr("\"\"", &extension).is_some());
        assert!(default_expr("PHP_INT_MAX", &extension).is_some());
        assert!(default_expr("DEMO_STEP", &extension).is_some());
        assert!(default_expr("Foo::BAR", &extension).is_none(), "class constants are left to C");
        assert!(default_expr("\"a\\n\"", &extension).is_none(), "escapes are not re-encoded");
    }
}
