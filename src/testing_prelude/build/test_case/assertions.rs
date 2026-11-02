//! Purpose:
//! The assertion methods of `Elephc\Testing\TestCase`: the PHPUnit-named subset over
//! `mixed` operands plus the elephc-only typed variants (`assertSameInt`, `assertSameFloat`,
//! `assertSameString`, `assertSameBool`) that avoid boxing on the hot path.
//!
//! Called from:
//! - `crate::testing_prelude::build::test_case::decl_test_case`.
//!
//! Key details:
//! - TRANSCRIBED from the PHP form in `crate::testing_prelude::fragments`; method ORDER is
//!   the PHP source order because the oracle zips the two method lists.
//! - Failure text follows PHPUnit's wording (`Failed asserting that 2 is identical to 1.`)
//!   wherever elephc can reproduce it; values are rendered by `TestCase::__elephc_export`.
//! - `assertInstanceOf` is deliberately absent: `is_a()` with a RUNTIME class name is the
//!   unfinished half of issue #1113 and answers `false`, so the assertion would be a silent
//!   wrong answer. Suites write `assertTrue($x instanceof Foo)` until that lands.

use crate::parser::ast::{BinOp, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_method_call, e_not, e_null, e_str, e_ternary, e_this, e_var,
    method, s_assign, s_expr, s_if, s_return_void, t_array, t_mixed, t_union, MethodBuilder,
};

/// Every assertion method, in PHP source order.
pub(super) fn assertion_methods() -> Vec<MethodBuilder> {
    vec![
        method_assert_true(),
        method_assert_false(),
        method_assert_same(),
        method_assert_not_same(),
        method_assert_equals(),
        method_assert_not_equals(),
        method_assert_null(),
        method_assert_not_null(),
        method_assert_empty(),
        method_assert_not_empty(),
        method_assert_count(),
        method_assert_contains(),
        method_assert_equals_with_delta(),
        method_assert_greater_than(),
        method_assert_greater_than_or_equal(),
        method_assert_less_than(),
        method_assert_less_than_or_equal(),
        method_assert_string_contains_string(),
        method_assert_string_starts_with(),
        method_assert_string_ends_with(),
        method_assert_same_int(),
        method_assert_same_float(),
        method_assert_same_string(),
        method_assert_same_bool(),
    ]
}

/// `TestCase::assertTrue()`, transcribed from the PHP form.
fn method_assert_true() -> MethodBuilder {
    method("assertTrue")
        .protected()
        .param("condition", TypeExpr::Bool)
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_var("condition"),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_str("Failed asserting that false is true.")])),
        ])
}

/// `TestCase::assertFalse()`, transcribed from the PHP form.
fn method_assert_false() -> MethodBuilder {
    method("assertFalse")
        .protected()
        .param("condition", TypeExpr::Bool)
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_not(e_var("condition")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_str("Failed asserting that true is false.")])),
        ])
}

/// `TestCase::assertSame()`, transcribed from the PHP form.
fn method_assert_same() -> MethodBuilder {
    method("assertSame")
        .protected()
        .param("expected", t_mixed())
        .param("actual", t_mixed())
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("expected"), BinOp::StrictEq, e_var("actual")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_method_call(e_this(), "__elephc_export", vec![e_var("actual")])), BinOp::Concat, e_str(" is identical to ")), BinOp::Concat, e_method_call(e_this(), "__elephc_export", vec![e_var("expected")])), BinOp::Concat, e_str("."))])),
        ])
}

/// `TestCase::assertNotSame()`, transcribed from the PHP form.
fn method_assert_not_same() -> MethodBuilder {
    method("assertNotSame")
        .protected()
        .param("expected", t_mixed())
        .param("actual", t_mixed())
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("expected"), BinOp::StrictNotEq, e_var("actual")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_method_call(e_this(), "__elephc_export", vec![e_var("actual")])), BinOp::Concat, e_str(" is not identical to ")), BinOp::Concat, e_method_call(e_this(), "__elephc_export", vec![e_var("expected")])), BinOp::Concat, e_str("."))])),
        ])
}

/// `TestCase::assertEquals()`, transcribed from the PHP form.
fn method_assert_equals() -> MethodBuilder {
    method("assertEquals")
        .protected()
        .param("expected", t_mixed())
        .param("actual", t_mixed())
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("expected"), BinOp::Eq, e_var("actual")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_method_call(e_this(), "__elephc_export", vec![e_var("actual")])), BinOp::Concat, e_str(" matches expected ")), BinOp::Concat, e_method_call(e_this(), "__elephc_export", vec![e_var("expected")])), BinOp::Concat, e_str("."))])),
        ])
}

/// `TestCase::assertNotEquals()`, transcribed from the PHP form.
fn method_assert_not_equals() -> MethodBuilder {
    method("assertNotEquals")
        .protected()
        .param("expected", t_mixed())
        .param("actual", t_mixed())
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("expected"), BinOp::NotEq, e_var("actual")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_method_call(e_this(), "__elephc_export", vec![e_var("actual")])), BinOp::Concat, e_str(" is not equal to ")), BinOp::Concat, e_method_call(e_this(), "__elephc_export", vec![e_var("expected")])), BinOp::Concat, e_str("."))])),
        ])
}

/// `TestCase::assertNull()`, transcribed from the PHP form.
fn method_assert_null() -> MethodBuilder {
    method("assertNull")
        .protected()
        .param("actual", t_mixed())
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("actual"), BinOp::StrictEq, e_null()),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_method_call(e_this(), "__elephc_export", vec![e_var("actual")])), BinOp::Concat, e_str(" is null."))])),
        ])
}

/// `TestCase::assertNotNull()`, transcribed from the PHP form.
fn method_assert_not_null() -> MethodBuilder {
    method("assertNotNull")
        .protected()
        .param("actual", t_mixed())
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("actual"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_str("Failed asserting that null is not null.")])),
        ])
}

/// `TestCase::assertEmpty()`, transcribed from the PHP form.
fn method_assert_empty() -> MethodBuilder {
    method("assertEmpty")
        .protected()
        .param("actual", t_mixed())
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_call("empty", vec![e_var("actual")]),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_method_call(e_this(), "__elephc_export", vec![e_var("actual")])), BinOp::Concat, e_str(" is empty."))])),
        ])
}

/// `TestCase::assertNotEmpty()`, transcribed from the PHP form.
fn method_assert_not_empty() -> MethodBuilder {
    method("assertNotEmpty")
        .protected()
        .param("actual", t_mixed())
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_not(e_call("empty", vec![e_var("actual")])),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_method_call(e_this(), "__elephc_export", vec![e_var("actual")])), BinOp::Concat, e_str(" is not empty."))])),
        ])
}

/// `TestCase::assertCount()`, transcribed from the PHP form.
fn method_assert_count() -> MethodBuilder {
    method("assertCount")
        .protected()
        .param("expected", TypeExpr::Int)
        .param("haystack", t_array())
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("actual", e_call("count", vec![e_var("haystack")])),
            s_if(
                e_binop(e_var("expected"), BinOp::StrictEq, e_var("actual")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that actual size "), BinOp::Concat, e_var("actual")), BinOp::Concat, e_str(" matches expected size ")), BinOp::Concat, e_var("expected")), BinOp::Concat, e_str("."))])),
        ])
}

/// `TestCase::assertContains()`, transcribed from the PHP form.
fn method_assert_contains() -> MethodBuilder {
    method("assertContains")
        .protected()
        .param("needle", t_mixed())
        .param("haystack", t_array())
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_call("in_array", vec![e_var("needle"), e_var("haystack"), e_bool(true)]),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_str("Failed asserting that an array contains "), BinOp::Concat, e_method_call(e_this(), "__elephc_export", vec![e_var("needle")])), BinOp::Concat, e_str("."))])),
        ])
}

/// `TestCase::assertEqualsWithDelta()`, transcribed from the PHP form.
fn method_assert_equals_with_delta() -> MethodBuilder {
    method("assertEqualsWithDelta")
        .protected()
        .param("expected", TypeExpr::Float)
        .param("actual", TypeExpr::Float)
        .param("delta", TypeExpr::Float)
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_call("abs", vec![e_binop(e_var("expected"), BinOp::Sub, e_var("actual"))]), BinOp::LtEq, e_var("delta")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_var("actual")), BinOp::Concat, e_str(" matches expected ")), BinOp::Concat, e_var("expected")), BinOp::Concat, e_str(" within delta ")), BinOp::Concat, e_var("delta")), BinOp::Concat, e_str("."))])),
        ])
}

/// `TestCase::assertGreaterThan()`, transcribed from the PHP form.
fn method_assert_greater_than() -> MethodBuilder {
    method("assertGreaterThan")
        .protected()
        .param("expected", t_union(vec![TypeExpr::Int, TypeExpr::Float]))
        .param("actual", t_union(vec![TypeExpr::Int, TypeExpr::Float]))
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("actual"), BinOp::Gt, e_var("expected")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_var("actual")), BinOp::Concat, e_str(" is greater than ")), BinOp::Concat, e_var("expected")), BinOp::Concat, e_str("."))])),
        ])
}

/// `TestCase::assertGreaterThanOrEqual()`, transcribed from the PHP form.
fn method_assert_greater_than_or_equal() -> MethodBuilder {
    method("assertGreaterThanOrEqual")
        .protected()
        .param("expected", t_union(vec![TypeExpr::Int, TypeExpr::Float]))
        .param("actual", t_union(vec![TypeExpr::Int, TypeExpr::Float]))
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("actual"), BinOp::GtEq, e_var("expected")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_var("actual")), BinOp::Concat, e_str(" is equal to or greater than ")), BinOp::Concat, e_var("expected")), BinOp::Concat, e_str("."))])),
        ])
}

/// `TestCase::assertLessThan()`, transcribed from the PHP form.
fn method_assert_less_than() -> MethodBuilder {
    method("assertLessThan")
        .protected()
        .param("expected", t_union(vec![TypeExpr::Int, TypeExpr::Float]))
        .param("actual", t_union(vec![TypeExpr::Int, TypeExpr::Float]))
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("actual"), BinOp::Lt, e_var("expected")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_var("actual")), BinOp::Concat, e_str(" is less than ")), BinOp::Concat, e_var("expected")), BinOp::Concat, e_str("."))])),
        ])
}

/// `TestCase::assertLessThanOrEqual()`, transcribed from the PHP form.
fn method_assert_less_than_or_equal() -> MethodBuilder {
    method("assertLessThanOrEqual")
        .protected()
        .param("expected", t_union(vec![TypeExpr::Int, TypeExpr::Float]))
        .param("actual", t_union(vec![TypeExpr::Int, TypeExpr::Float]))
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("actual"), BinOp::LtEq, e_var("expected")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_var("actual")), BinOp::Concat, e_str(" is equal to or less than ")), BinOp::Concat, e_var("expected")), BinOp::Concat, e_str("."))])),
        ])
}

/// `TestCase::assertStringContainsString()`, transcribed from the PHP form.
fn method_assert_string_contains_string() -> MethodBuilder {
    method("assertStringContainsString")
        .protected()
        .param("needle", TypeExpr::Str)
        .param("haystack", TypeExpr::Str)
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_call("str_contains", vec![e_var("haystack"), e_var("needle")]),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that '"), BinOp::Concat, e_var("haystack")), BinOp::Concat, e_str("' contains \"")), BinOp::Concat, e_var("needle")), BinOp::Concat, e_str("\"."))])),
        ])
}

/// `TestCase::assertStringStartsWith()`, transcribed from the PHP form.
fn method_assert_string_starts_with() -> MethodBuilder {
    method("assertStringStartsWith")
        .protected()
        .param("prefix", TypeExpr::Str)
        .param("string", TypeExpr::Str)
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_call("str_starts_with", vec![e_var("string"), e_var("prefix")]),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that '"), BinOp::Concat, e_var("string")), BinOp::Concat, e_str("' starts with \"")), BinOp::Concat, e_var("prefix")), BinOp::Concat, e_str("\"."))])),
        ])
}

/// `TestCase::assertStringEndsWith()`, transcribed from the PHP form.
fn method_assert_string_ends_with() -> MethodBuilder {
    method("assertStringEndsWith")
        .protected()
        .param("suffix", TypeExpr::Str)
        .param("string", TypeExpr::Str)
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_call("str_ends_with", vec![e_var("string"), e_var("suffix")]),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that '"), BinOp::Concat, e_var("string")), BinOp::Concat, e_str("' ends with \"")), BinOp::Concat, e_var("suffix")), BinOp::Concat, e_str("\"."))])),
        ])
}

/// `TestCase::assertSameInt()`, transcribed from the PHP form.
fn method_assert_same_int() -> MethodBuilder {
    method("assertSameInt")
        .protected()
        .param("expected", TypeExpr::Int)
        .param("actual", TypeExpr::Int)
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("expected"), BinOp::StrictEq, e_var("actual")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_var("actual")), BinOp::Concat, e_str(" is identical to ")), BinOp::Concat, e_var("expected")), BinOp::Concat, e_str("."))])),
        ])
}

/// `TestCase::assertSameFloat()`, transcribed from the PHP form.
fn method_assert_same_float() -> MethodBuilder {
    method("assertSameFloat")
        .protected()
        .param("expected", TypeExpr::Float)
        .param("actual", TypeExpr::Float)
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("expected"), BinOp::StrictEq, e_var("actual")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_var("actual")), BinOp::Concat, e_str(" is identical to ")), BinOp::Concat, e_var("expected")), BinOp::Concat, e_str("."))])),
        ])
}

/// `TestCase::assertSameString()`, transcribed from the PHP form.
fn method_assert_same_string() -> MethodBuilder {
    method("assertSameString")
        .protected()
        .param("expected", TypeExpr::Str)
        .param("actual", TypeExpr::Str)
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("expected"), BinOp::StrictEq, e_var("actual")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that '"), BinOp::Concat, e_var("actual")), BinOp::Concat, e_str("' is identical to '")), BinOp::Concat, e_var("expected")), BinOp::Concat, e_str("'."))])),
        ])
}

/// `TestCase::assertSameBool()`, transcribed from the PHP form.
fn method_assert_same_bool() -> MethodBuilder {
    method("assertSameBool")
        .protected()
        .param("expected", TypeExpr::Bool)
        .param("actual", TypeExpr::Bool)
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("expected"), BinOp::StrictEq, e_var("actual")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_var("message"), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that "), BinOp::Concat, e_ternary(e_var("actual"), e_str("true"), e_str("false"))), BinOp::Concat, e_str(" is identical to ")), BinOp::Concat, e_ternary(e_var("expected"), e_str("true"), e_str("false"))), BinOp::Concat, e_str("."))])),
        ])
}
