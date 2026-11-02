//! Purpose:
//! Builds `is_callable()`'s `$syntax_only` and `&$callable_name` forms as prelude declarations.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends these declarations when the
//!   program references `is_callable`.
//!
//! Key details:
//! - The backend's `is_callable` is a one-argument predicate: it answers whether the value can be
//!   CALLED. PHP's builtin takes three — `is_callable(mixed $value, bool $syntax_only = false,
//!   string &$callable_name = null)` — and the second argument changes the question to whether the
//!   value has callable SHAPE, which is a different answer for every name that does not resolve.
//!   `symfony/http-kernel`'s `ServiceValueResolver` asks exactly that (`is_callable($controller,
//!   true)`) before splitting `[$class, $method]` into `Class::method`, and the two-argument call
//!   stopped the whole build with "is_callable() takes exactly 1 argument".
//! - The declarations are BUILT, never parsed: elephc does not parse PHP source outside tests.
//! - Every rule below is transcribed from PHP 8.5.10, verified case by case rather than reasoned
//!   about, because `$syntax_only` is not simply "skip the existence check":
//!     * a STRING is shape-callable whatever it names, including `''`;
//!     * an ARRAY is shape-callable only with exactly the keys 0 and 1, element 1 a string and
//!       element 0 an object or a string — `[$o, 2]`, `[$o]`, `[$o, 'm', 'x']` and
//!       `['x' => $o, 'y' => 'm']` are all false;
//!     * an OBJECT still has to be invokable. `$syntax_only` does NOT relax that, so a plain
//!       object is false under both forms;
//!     * `$callable_name` is written even when the answer is false, and for a non-conforming
//!       array it is the literal `Array`.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_cast, e_concat, e_concat_all, e_index, e_instance_of, e_int,
    e_method_call, e_new_fq, e_not, e_null, e_str, e_ternary, e_var, function,
    php_semantics_declarations, s_assign, s_if, s_return, t_nullable,
};

/// Reserved helper backing `is_callable()` calls that pass `$syntax_only` or `$callable_name`.
pub(crate) const IS_CALLABLE_EXT_NAME: &str = "__elephc_is_callable_ext";

/// Returns the extended `is_callable()` declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![is_callable_ext()])
}

/// Builds `__elephc_is_callable_ext($value, $syntax_only = false, &$callable_name = null): bool`.
fn is_callable_ext() -> Stmt {
    function(IS_CALLABLE_EXT_NAME)
        .param_untyped("value")
        .param_untyped_default("syntax_only", e_bool(false))
        .param_by_ref_default("callable_name", Some(t_nullable(TypeExpr::Str)), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            string_arm(),
            closure_arm(),
            array_arm(),
            object_arm(),
            // Every remaining value is a scalar, null or a resource. PHP still writes the name --
            // `42` for the int, `1` for true, `''` for null -- and answers false.
            s_assign("callable_name", e_cast(CastType::String, e_var("value"))),
            s_return(e_bool(false)),
        ])
        .build()
}

/// `if (is_string($value)) { $callable_name = $value; return $syntax_only ? true : is_callable($value); }`
///
/// A string names a function or a `Class::method`, and shape alone cannot tell whether either
/// exists, so `$syntax_only` accepts every string — `''` included, verified.
fn string_arm() -> Stmt {
    s_if(
        e_call("is_string", vec![e_var("value")]),
        vec![
            s_assign("callable_name", e_var("value")),
            s_return(e_ternary(
                e_var("syntax_only"),
                e_bool(true),
                e_call("is_callable", vec![e_var("value")]),
            )),
        ],
        Vec::new(),
        None,
    )
}

/// `if ($value instanceof Closure) { $callable_name = (new ReflectionFunction($value))->getName(); return true; }`
///
/// A Closure is always callable and its reported name is the one reflection gives, which is where
/// PHP 8.4's `{closure:file:line}` spelling comes from. Taking it from reflection rather than
/// building a name here keeps the two answers from drifting apart.
fn closure_arm() -> Stmt {
    s_if(
        e_instance_of(e_var("value"), "Closure"),
        vec![
            s_assign(
                "callable_name",
                e_method_call(
                    e_new_fq("ReflectionFunction", vec![e_var("value")]),
                    "getName",
                    Vec::new(),
                ),
            ),
            s_return(e_bool(true)),
        ],
        Vec::new(),
        None,
    )
}

/// The `[$objectOrClass, 'method']` form, and the `Array` name every other array shape reports.
fn array_arm() -> Stmt {
    let conforms = e_binop(
        e_binop(
            e_binop(
                e_binop(
                    e_call("count", vec![e_var("value")]),
                    BinOp::StrictEq,
                    e_int(2),
                ),
                BinOp::And,
                e_call("array_key_exists", vec![e_int(0), e_var("value")]),
            ),
            BinOp::And,
            e_call("array_key_exists", vec![e_int(1), e_var("value")]),
        ),
        BinOp::And,
        e_binop(
            e_call("is_string", vec![e_index(e_var("value"), e_int(1))]),
            BinOp::And,
            e_binop(
                e_call("is_object", vec![e_index(e_var("value"), e_int(0))]),
                BinOp::Or,
                e_call("is_string", vec![e_index(e_var("value"), e_int(0))]),
            ),
        ),
    );
    s_if(
        e_call("is_array", vec![e_var("value")]),
        vec![
            s_if(
                e_not(conforms),
                vec![
                    s_assign("callable_name", e_str("Array")),
                    s_return(e_bool(false)),
                ],
                Vec::new(),
                None,
            ),
            s_assign(
                "target",
                e_ternary(
                    e_call("is_object", vec![e_index(e_var("value"), e_int(0))]),
                    e_call("get_class", vec![e_index(e_var("value"), e_int(0))]),
                    e_index(e_var("value"), e_int(0)),
                ),
            ),
            s_assign(
                "callable_name",
                e_concat_all(vec![
                    e_var("target"),
                    e_str("::"),
                    e_index(e_var("value"), e_int(1)),
                ]),
            ),
            s_return(e_ternary(
                e_var("syntax_only"),
                e_bool(true),
                e_call("is_callable", vec![e_var("value")]),
            )),
        ],
        Vec::new(),
        None,
    )
}

/// `if (is_object($value)) { $callable_name = get_class($value) . '::__invoke'; return is_callable($value); }`
///
/// `$syntax_only` is deliberately not consulted: PHP answers false for a non-invokable object
/// under both forms, while still reporting the `__invoke` name it looked for.
fn object_arm() -> Stmt {
    s_if(
        e_call("is_object", vec![e_var("value")]),
        vec![
            s_assign(
                "callable_name",
                e_concat(
                    e_call("get_class", vec![e_var("value")]),
                    e_str("::__invoke"),
                ),
            ),
            s_return(e_call("is_callable", vec![e_var("value")])),
        ],
        Vec::new(),
        None,
    )
}
