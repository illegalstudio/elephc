//! Purpose:
//! Builds `Elephc\Testing\TestCase`: the lifecycle hooks, the runner-facing entry points,
//! the expected-exception bookkeeping, `markTestSkipped` / `markTestIncomplete`, the private
//! failure and value-rendering helpers, and (in `assertions`) every assertion of the
//! supported subset, one builder helper per method.
//!
//! Called from:
//! - `crate::testing_prelude::build::testing_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from the PHP form in `crate::testing_prelude::fragments` with
//!   `synthetic_class::transcribe`; `testing_prelude::oracle_tests` compares the built class
//!   against that parse node by node, so a change here must keep the PHP form in step.
//! - Method ORDER is the PHP source order: the oracle zips the two method lists.
//! - `tearDown()` always runs after a successful `setUp()`. When the test failed or errored,
//!   an exception from `tearDown()` is discarded so it cannot replace that outcome (a
//!   failure turned into a SKIP would pass the run). After a skip or an incomplete, and
//!   after a pass, an exception from `tearDown()` IS the test's outcome, so a broken
//!   cleanup can never hide behind a skip.
//!   The failure is rethrown from the `catch` rather than kept in a null-initialised local:
//!   rethrowing such a local crashes on the success path (journal task 9b79d5e5).
//! - `__elephc_invoke` rethrows the runtime's own outcomes (failure, skipped, incomplete)
//!   and turns any other throwable into a pass or a failure of `expectException()`.
//!   Matching is the exact class or its direct parent only: `is_a()` with a runtime class
//!   name is the unfinished half of issue #1113.
//! - Values in failure text go through the private `__elephc_export`, not `var_export()`:
//!   the var_export prelude is detected before this one is injected, so a call to it from
//!   here would not be seen.

use crate::parser::ast::{BinOp, CastType, Stmt, TypeExpr};
use crate::synthetic_class::{
    class, e_binop, e_bool, e_call, e_cast, e_closure_call, e_instance_of, e_int, e_method_call,
    e_new, e_not, e_null, e_str, e_ternary, e_this, e_this_prop, e_var, method, s_assign,
    s_expr, s_if, s_prop_assign, s_return, s_return_void, s_throw, s_try, t_class, t_mixed,
    ClassBuilder, MethodBuilder,
};

mod assertions;

/// Builds the abstract `TestCase` class with its methods in PHP source order.
pub(super) fn decl_test_case() -> Stmt {
    let mut methods: Vec<MethodBuilder> = vec![
        method_set_up(),
        method_tear_down(),
        method_elephc_run_test(),
        method_elephc_tear_down_after_failure(),
        method_elephc_invoke(),
        method_elephc_check_expected_exception(),
        method_expect_exception(),
        method_expect_exception_message(),
        method_mark_test_skipped(),
        method_mark_test_incomplete(),
        method_elephc_assertion_count(),
        method_fail(),
        method_elephc_pass(),
        method_elephc_fail(),
        method_elephc_export(),
    ];
    methods.extend(assertions::assertion_methods());
    methods
        .into_iter()
        .fold(
            class("TestCase")
            .abstract_()
            .private_prop("__elephc_assertions", TypeExpr::Int, Some(e_int(0)))
            .private_prop("__elephc_expected_exception", TypeExpr::Str, Some(e_str("")))
            .private_prop("__elephc_expected_message", TypeExpr::Str, Some(e_str(""))),
            ClassBuilder::method,
        )
        .build()
}

/// `TestCase::setUp()`, transcribed from the PHP form.
fn method_set_up() -> MethodBuilder {
    method("setUp")
        .protected()
        .returns(TypeExpr::Void)
}

/// `TestCase::tearDown()`, transcribed from the PHP form.
fn method_tear_down() -> MethodBuilder {
    method("tearDown")
        .protected()
        .returns(TypeExpr::Void)
}

/// `TestCase::__elephc_run_test()`, transcribed from the PHP form.
fn method_elephc_run_test() -> MethodBuilder {
    method("__elephc_run_test")
        .param("test", t_class("callable"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "__elephc_assertions", e_int(0)),
            s_prop_assign(e_this(), "__elephc_expected_exception", e_str("")),
            s_prop_assign(e_this(), "__elephc_expected_message", e_str("")),
            s_expr(e_method_call(e_this(), "setUp", vec![])),
            s_try(vec![
                s_expr(e_method_call(e_this(), "__elephc_invoke", vec![e_var("test")])),
            ], vec![
                (vec!["\\Throwable"], Some("thrown"), vec![
                    s_if(
                        e_binop(e_instance_of(e_var("thrown"), "SkippedTestError"), BinOp::Or, e_instance_of(e_var("thrown"), "IncompleteTestError")),
                        vec![
                            s_expr(e_method_call(e_this(), "tearDown", vec![])),
                            s_throw(e_var("thrown")),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "__elephc_tear_down_after_failure", vec![])),
                    s_throw(e_var("thrown")),
                ]),
            ], None),
            s_expr(e_method_call(e_this(), "tearDown", vec![])),
        ])
}

/// `TestCase::__elephc_tear_down_after_failure()`, transcribed from the PHP form.
fn method_elephc_tear_down_after_failure() -> MethodBuilder {
    method("__elephc_tear_down_after_failure")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_try(vec![
                s_expr(e_method_call(e_this(), "tearDown", vec![])),
            ], vec![
                (vec!["\\Throwable"], None, vec![
                    s_return_void(),
                ]),
            ], None),
        ])
}

/// `TestCase::__elephc_invoke()`, transcribed from the PHP form.
fn method_elephc_invoke() -> MethodBuilder {
    method("__elephc_invoke")
        .private()
        .param("test", t_class("callable"))
        .keep_unread_params()
        .returns(TypeExpr::Void)
        .body(vec![
            s_try(vec![
                s_expr(e_closure_call("test", vec![])),
            ], vec![
                (vec!["\\Throwable"], Some("thrown"), vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_instance_of(e_var("thrown"), "AssertionFailedError"), BinOp::Or, e_instance_of(e_var("thrown"), "SkippedTestError")), BinOp::Or, e_instance_of(e_var("thrown"), "IncompleteTestError")), BinOp::Or, e_binop(e_this_prop("__elephc_expected_exception"), BinOp::StrictEq, e_str(""))),
                        vec![
                            s_throw(e_var("thrown")),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "__elephc_check_expected_exception", vec![e_var("thrown")])),
                    s_return_void(),
                ]),
            ], None),
            s_if(
                e_binop(e_this_prop("__elephc_expected_exception"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_str(""), e_binop(e_binop(e_str("Failed asserting that exception of type \""), BinOp::Concat, e_this_prop("__elephc_expected_exception")), BinOp::Concat, e_str("\" is thrown."))])),
                ],
                vec![],
                None,
            ),
        ])
}

/// `TestCase::__elephc_check_expected_exception()`, transcribed from the PHP form.
fn method_elephc_check_expected_exception() -> MethodBuilder {
    method("__elephc_check_expected_exception")
        .private()
        .param("thrown", t_class("\\Throwable"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("expected", e_this_prop("__elephc_expected_exception")),
            s_assign("actual", e_call("get_class", vec![e_var("thrown")])),
            s_assign("parent", e_call("get_parent_class", vec![e_var("thrown")])),
            s_assign("matches", e_binop(e_call("strcasecmp", vec![e_var("actual"), e_var("expected")]), BinOp::StrictEq, e_int(0))),
            s_if(
                e_binop(e_not(e_var("matches")), BinOp::And, e_binop(e_var("parent"), BinOp::StrictNotEq, e_bool(false))),
                vec![
                    s_assign("matches", e_binop(e_call("strcasecmp", vec![e_var("parent"), e_var("expected")]), BinOp::StrictEq, e_int(0))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_var("matches")),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_str(""), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that exception of type \""), BinOp::Concat, e_var("actual")), BinOp::Concat, e_str("\" matches expected exception \"")), BinOp::Concat, e_var("expected")), BinOp::Concat, e_str("\"."))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
            s_if(
                e_binop(e_this_prop("__elephc_expected_message"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("str_contains", vec![e_method_call(e_var("thrown"), "getMessage", vec![]), e_this_prop("__elephc_expected_message")])),
                vec![
                    s_expr(e_method_call(e_this(), "__elephc_fail", vec![e_str(""), e_binop(e_binop(e_binop(e_binop(e_str("Failed asserting that exception message '"), BinOp::Concat, e_method_call(e_var("thrown"), "getMessage", vec![])), BinOp::Concat, e_str("' contains '")), BinOp::Concat, e_this_prop("__elephc_expected_message")), BinOp::Concat, e_str("'."))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "__elephc_pass", vec![])),
        ])
}

/// `TestCase::expectException()`, transcribed from the PHP form.
fn method_expect_exception() -> MethodBuilder {
    method("expectException")
        .protected()
        .param("exception", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "__elephc_expected_exception", e_call("ltrim", vec![e_var("exception"), e_str("\\")])),
        ])
}

/// `TestCase::expectExceptionMessage()`, transcribed from the PHP form.
fn method_expect_exception_message() -> MethodBuilder {
    method("expectExceptionMessage")
        .protected()
        .param("message", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "__elephc_expected_message", e_var("message")),
        ])
}

/// `TestCase::markTestSkipped()`, transcribed from the PHP form.
fn method_mark_test_skipped() -> MethodBuilder {
    method("markTestSkipped")
        .protected()
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_throw(e_new("SkippedTestError", vec![e_var("message")])),
        ])
}

/// `TestCase::markTestIncomplete()`, transcribed from the PHP form.
fn method_mark_test_incomplete() -> MethodBuilder {
    method("markTestIncomplete")
        .protected()
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_throw(e_new("IncompleteTestError", vec![e_var("message")])),
        ])
}

/// `TestCase::__elephc_assertion_count()`, transcribed from the PHP form.
fn method_elephc_assertion_count() -> MethodBuilder {
    method("__elephc_assertion_count")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_this_prop("__elephc_assertions")),
        ])
}

/// `TestCase::fail()`, transcribed from the PHP form.
fn method_fail() -> MethodBuilder {
    method("fail")
        .protected()
        .param_default("message", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_throw(e_new("AssertionFailedError", vec![e_var("message")])),
        ])
}

/// `TestCase::__elephc_pass()`, transcribed from the PHP form.
fn method_elephc_pass() -> MethodBuilder {
    method("__elephc_pass")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "__elephc_assertions", e_binop(e_this_prop("__elephc_assertions"), BinOp::Add, e_int(1))),
        ])
}

/// `TestCase::__elephc_fail()`, transcribed from the PHP form.
fn method_elephc_fail() -> MethodBuilder {
    method("__elephc_fail")
        .private()
        .param("message", TypeExpr::Str)
        .param("description", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "__elephc_assertions", e_binop(e_this_prop("__elephc_assertions"), BinOp::Add, e_int(1))),
            s_if(
                e_binop(e_var("message"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_throw(e_new("AssertionFailedError", vec![e_binop(e_binop(e_var("message"), BinOp::Concat, e_str("\n")), BinOp::Concat, e_var("description"))])),
                ],
                vec![],
                None,
            ),
            s_throw(e_new("AssertionFailedError", vec![e_var("description")])),
        ])
}

/// `TestCase::__elephc_export()`, transcribed from the PHP form.
fn method_elephc_export() -> MethodBuilder {
    method("__elephc_export")
        .private()
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_str("null")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_bool", vec![e_var("value")]),
                vec![
                    s_return(e_ternary(e_var("value"), e_str("true"), e_str("false"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("is_int", vec![e_var("value")]), BinOp::Or, e_call("is_float", vec![e_var("value")])),
                vec![
                    s_return(e_cast(CastType::String, e_var("value"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_string", vec![e_var("value")]),
                vec![
                    s_return(e_binop(e_binop(e_str("'"), BinOp::Concat, e_var("value")), BinOp::Concat, e_str("'"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_array", vec![e_var("value")]),
                vec![
                    s_return(e_binop(e_binop(e_str("Array ("), BinOp::Concat, e_call("count", vec![e_var("value")])), BinOp::Concat, e_str(")"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_object", vec![e_var("value")]),
                vec![
                    s_return(e_binop(e_call("get_class", vec![e_var("value")]), BinOp::Concat, e_str(" Object"))),
                ],
                vec![],
                None,
            ),
            s_return(e_call("gettype", vec![e_var("value")])),
        ])
}
