//! Purpose:
//! Reads the builtin registry's write-only by-reference parameters, so a variable passed to one
//! is treated as a DEFINITION of that variable rather than a use of it.
//!
//! Called from:
//! - `crate::types::checker::builtins::check_builtin` for argument validation and inference.
//! - `crate::types::checker::inference::expr::effects` for function-call argument effects.
//!
//! Key details:
//! - The written type comes from `ParamSpec::writes`. Because the checker binds the variable to
//!   that type, the slot codegen later writes into always has the matching representation — the
//!   two cannot drift, since neither side names the type itself.
//! - Only parameters declared `ref(T)` appear here. A plain `ref` parameter is in-out: the
//!   builtin reads it too, so it keeps the ordinary `Undefined variable` diagnostic.

use crate::builtins::convert::type_spec_to_php;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Returns each argument this call only writes, as `(argument index, variable, written type)`.
///
/// PHP binds an undeclared variable to a by-reference parameter by auto-vivifying it to `null`
/// — `stream_socket_client($url, $errno, $errstr)` is the manual's own idiom and never declares
/// `$errno`. Each argument returned here is therefore a definition site for its variable.
///
/// Resolves an argument to its parameter by NAME when the call names it and by position
/// otherwise, because the assignment-effects pass runs before argument normalization:
/// `stream_socket_client($url, error_message: $s)` puts `$s` at index 1, which is
/// `$error_code`'s position, and binding it there would give it `int` where the builtin writes
/// a string.
///
/// This is the single answer both consumers use — the passes that must not READ these arguments
/// and the pass that BINDS them — so the positions skipped and the positions defined cannot
/// disagree.
pub(crate) fn write_only_variable_args(builtin_name: &str, args: &[Expr]) -> Vec<WriteOnlyArg> {
    let Some(def) = crate::builtins::registry::lookup(builtin_name) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        let (target, param) = match &arg.kind {
            ExprKind::NamedArg { name, value } => (
                value.as_ref(),
                def.spec.params.iter().find(|param| param.name == name),
            ),
            _ => (arg, def.spec.params.get(index)),
        };
        // Past the fixed parameters the argument belongs to the variadic TAIL, which carries
        // its own write-only declaration: `sscanf($s, '%d %s', $n, $w)` fills both variables and
        // neither has to exist beforehand.
        let written = match param {
            Some(param) => param.writes,
            None if index >= def.spec.params.len() => def.spec.variadic_writes,
            None => None,
        };
        let Some(written) = written else {
            continue;
        };
        let ExprKind::Variable(variable) = &target.kind else {
            continue;
        };
        found.push(WriteOnlyArg {
            index,
            variable: variable.clone(),
            written: type_spec_to_php(&written),
            span: target.span,
        });
    }
    found
}

/// Rejects a write-only argument the builtin has nowhere to write back into.
///
/// A by-reference parameter needs storage: `stream_socket_client($url, 0, $s)` gives it a
/// literal, which PHP also refuses. The rule is derived from the `ref(T)` declaration rather
/// than restated per builtin, because it used to be hand-copied into nine `check` hooks across
/// six files — and every copy shared a defect, rejecting the very `null` that argument
/// normalization had just materialized for an omitted optional parameter.
///
/// `null` is therefore accepted: it is both the default this compiler fills in and what PHP
/// itself allows a caller to pass to discard the output.
pub(crate) fn check_write_only_args(
    builtin_name: &str,
    args: &[Expr],
) -> Result<(), crate::errors::CompileError> {
    let Some(def) = crate::builtins::registry::lookup(builtin_name) else {
        return Ok(());
    };
    for (index, arg) in args.iter().enumerate() {
        let (target, param) = match &arg.kind {
            ExprKind::NamedArg { name, value } => (
                value.as_ref(),
                def.spec.params.iter().find(|param| param.name == name),
            ),
            _ => (arg, def.spec.params.get(index)),
        };
        let write_only_name = match param {
            Some(param) if param.writes.is_some() => Some(param.name),
            // A variadic write-only tail names itself the way php's manual does.
            None if index >= def.spec.params.len() && def.spec.variadic_writes.is_some() => {
                def.spec.variadic
            }
            _ => None,
        };
        let Some(param_name) = write_only_name else {
            continue;
        };
        if matches!(target.kind, ExprKind::Variable(_) | ExprKind::Null) {
            continue;
        }
        return Err(crate::errors::CompileError::new(
            target.span,
            &format!(
                "{}() parameter ${} must be passed a variable",
                builtin_name.trim_start_matches('\\'),
                param_name
            ),
        ));
    }
    Ok(())
}

/// One argument a builtin only writes, resolved to the variable it defines.
pub(crate) struct WriteOnlyArg {
    /// Position in the call's argument list, which is what an inference loop skips.
    pub index: usize,
    /// The variable the call defines.
    pub variable: String,
    /// The type it holds once the call returns.
    pub written: PhpType,
    /// Span of the variable, so a reassignment diagnostic points at the argument.
    pub span: crate::span::Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies an in-out by-ref parameter is NOT reported: `stream_select()` reads its arrays as
    /// well as writing them, so those arguments stay ordinary uses and keep their diagnostic.
    #[test]
    fn in_out_by_ref_params_are_not_write_only() {
        let args = [variable("read"), variable("write"), variable("except")];
        assert!(write_only_variable_args("stream_select", &args).is_empty());
        assert!(write_only_variable_args("array_splice", &args).is_empty());
    }

    /// Verifies a builtin with no by-reference parameter reports nothing.
    #[test]
    fn value_only_builtins_report_no_write_only_params() {
        let args = [variable("subject")];
        assert!(write_only_variable_args("strlen", &args).is_empty());
        assert!(write_only_variable_args("no_such_builtin", &args).is_empty());
    }

    /// Builds a bare `$name` argument expression.
    fn variable(name: &str) -> Expr {
        Expr::new(ExprKind::Variable(name.to_string()), crate::span::Span::dummy())
    }

    /// Builds a `name: $value` argument expression.
    fn named(name: &str, value: &str) -> Expr {
        Expr::new(
            ExprKind::NamedArg {
                name: name.to_string(),
                value: Box::new(variable(value)),
            },
            crate::span::Span::dummy(),
        )
    }

    /// Verifies positional arguments resolve to the parameter sharing their index.
    #[test]
    fn positional_out_params_resolve_by_index() {
        let args = [variable("url"), variable("errno"), variable("errstr")];
        let found = write_only_variable_args("stream_socket_client", &args);
        let resolved: Vec<_> = found
            .iter()
            .map(|out| (out.index, out.variable.as_str(), out.written.clone()))
            .collect();
        assert_eq!(
            resolved,
            vec![(1, "errno", PhpType::Int), (2, "errstr", PhpType::Str)]
        );
    }

    /// Verifies a NAMED argument resolves to the parameter it names, not to the parameter at its
    /// index. `error_message:` sits at index 1 here, which is `$error_code`'s position: binding
    /// by index would give the variable `int` where the builtin writes a string.
    #[test]
    fn named_out_params_resolve_by_name_not_by_index() {
        let args = [variable("url"), named("error_message", "errstr")];
        let found = write_only_variable_args("stream_socket_client", &args);
        let resolved: Vec<_> = found
            .iter()
            .map(|out| (out.index, out.variable.as_str(), out.written.clone()))
            .collect();
        assert_eq!(resolved, vec![(1, "errstr", PhpType::Str)]);
    }

    /// Verifies an argument with no storage to write back into is left alone.
    #[test]
    fn non_variable_out_param_arguments_are_ignored() {
        let args = [
            variable("url"),
            Expr::new(ExprKind::IntLiteral(0), crate::span::Span::dummy()),
        ];
        assert!(write_only_variable_args("stream_socket_client", &args).is_empty());
    }
}
