//! Purpose:
//! Regression tests for by-reference array parameter storage in synthesized declarations.
//!
//! Called from:
//! - The `driver::functions` unit test module.
//!
//! Key details:
//! - Synthetic declarations carry a dummy span, where bare `array` normally resolves to the
//!   concrete internal `Array(Mixed)` ABI. A by-reference parameter writes through the caller's
//!   cell, so it must keep the boxed `php_array()` contract instead of pinning one layout.

use crate::parser::ast::{Program, TypeExpr};
use crate::synthetic_class::{
    e_int, function, internal_declarations, s_array_assign, s_return_void, t_array,
};
use crate::types::PhpType;

/// Parses the caller half of a fixture that passes a packed array to a synthetic by-ref callee.
fn caller_program() -> Program {
    let tokens = crate::lexer::tokenize(
        "<?php\n$state = [1, 2, null];\ntake_ref($state);\necho $state[1];",
    )
    .expect("tokenize fixture");
    crate::parser::parse(&tokens).expect("parse fixture")
}

/// A synthesized `array &$state` parameter keeps the packed/associative union, so its caller
/// boxes the array instead of exposing a raw pointer the callee could misread as a hash.
#[test]
fn synthesized_by_ref_array_parameter_keeps_the_php_array_contract() {
    let mut program = caller_program();
    program.extend(internal_declarations(|| {
        vec![function("take_ref")
            .param_by_ref("state", Some(t_array()))
            .returns(TypeExpr::Void)
            .body(vec![
                s_array_assign("state", e_int(1), e_int(5)),
                s_return_void(),
            ])
            .build()]
    }));

    let checked = crate::types::check(&program).expect("synthetic by-ref array parameter is valid");
    assert_eq!(
        checked.functions["take_ref"].params[0].1,
        PhpType::php_array(),
        "synthesized by-reference array parameters must not pin packed storage"
    );
}

/// The same declaration built from real user source keeps its boxed by-reference storage even
/// though the callee body writes a non-zero index, which would otherwise select hash storage.
#[test]
fn user_by_ref_array_parameter_keeps_the_php_array_contract() {
    let tokens = crate::lexer::tokenize(
        "<?php\nfunction take_ref(array &$state): void { $state[1] = 5; }\n$state = [1, 2, null];\ntake_ref($state);\necho $state[1];",
    )
    .expect("tokenize fixture");
    let program = crate::parser::parse(&tokens).expect("parse fixture");

    let checked = crate::types::check(&program).expect("user by-ref array parameter is valid");
    assert_eq!(
        checked.functions["take_ref"].params[0].1,
        PhpType::php_array(),
        "a user `array &$x` hint keeps the packed/associative union"
    );
    assert_eq!(
        checked.global_env.get("state"),
        Some(&PhpType::php_array()),
        "the caller boxes the local it binds to the by-reference parameter"
    );
}
