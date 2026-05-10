//! Purpose:
//! Type-checks PHP IO builtin common helpers and signatures.
//! Validates arity, argument categories, resource handling, and return types before codegen sees calls.
//!
//! Called from:
//! - `crate::types::checker::builtins::io::check_builtin()`
//!
//! Key details:
//! - Return types and diagnostics must stay aligned with `crate::types::signatures` and builtin codegen emitters.

use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;

pub(super) type BuiltinResult = Result<Option<PhpType>, CompileError>;

pub(super) fn ensure_stream_resource(
    checker: &mut Checker,
    name: &str,
    arg: &Expr,
    env: &TypeEnv,
) -> Result<(), CompileError> {
    let actual = checker.infer_type(arg, env)?;
    let expected = PhpType::stream_resource();
    if stream_arg_accepts(checker, &expected, &actual) {
        Ok(())
    } else {
        Err(CompileError::new(
            arg.span,
            &format!("{}() expects resource, got {}", name, actual),
        ))
    }
}

fn stream_arg_accepts(checker: &Checker, expected: &PhpType, actual: &PhpType) -> bool {
    if checker.type_accepts(expected, actual) || matches!(actual, PhpType::Mixed) {
        return true;
    }
    match actual {
        PhpType::Union(members) => {
            let has_resource = members
                .iter()
                .any(|member| checker.type_accepts(expected, member));
            let only_resource_or_false = members
                .iter()
                .all(|member| checker.type_accepts(expected, member) || *member == PhpType::Bool);
            has_resource && only_resource_or_false
        }
        _ => false,
    }
}
