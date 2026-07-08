//! Purpose:
//! Declarative eval registry entry for `glob`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the local glob helper.

eval_builtin! {
    name: "glob",
    area: Filesystem,
    params: [pattern],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `glob` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_glob_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_glob(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `glob` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_glob_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [pattern] => eval_glob_result(*pattern, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `glob($pattern)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_glob(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pattern = eval_expr(pattern, context, scope, values)?;
    eval_glob_result(pattern, values)
}

/// Expands one local glob pattern into a sorted indexed PHP string array.
pub(in crate::interpreter) fn eval_glob_result(
    pattern: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let pattern = eval_path_string(pattern, values)?;
    let matches = eval_glob_matches(&pattern);
    let mut result = values.array_new(matches.len())?;
    for (index, path) in matches.iter().enumerate() {
        result = super::scandir::eval_array_set_indexed_bytes(result, index, path.as_bytes(), values)?;
    }
    Ok(result)
}

/// Collects sorted matches for one local glob pattern.
pub(in crate::interpreter) fn eval_glob_matches(pattern: &str) -> Vec<String> {
    if pattern.is_empty() {
        return Vec::new();
    }
    if !eval_glob_component_has_magic(pattern) {
        return std::path::Path::new(pattern)
            .exists()
            .then(|| pattern.to_string())
            .into_iter()
            .collect();
    }
    let absolute = pattern.starts_with('/');
    let components: Vec<&str> = pattern
        .split('/')
        .filter(|component| !component.is_empty())
        .collect();
    let mut matches = Vec::new();
    let base = if absolute {
        std::path::PathBuf::from("/")
    } else {
        std::path::PathBuf::from(".")
    };
    let prefix = if absolute { "/" } else { "" };
    eval_glob_collect(&base, prefix, &components, &mut matches);
    matches.sort();
    matches
}

/// Recursively expands one glob path component at a time.
pub(in crate::interpreter) fn eval_glob_collect(
    base: &std::path::Path,
    prefix: &str,
    components: &[&str],
    matches: &mut Vec<String>,
) {
    let Some((component, rest)) = components.split_first() else {
        if base.exists() && !prefix.is_empty() {
            matches.push(prefix.to_string());
        }
        return;
    };
    if !eval_glob_component_has_magic(component) {
        let next_base = base.join(component);
        if rest.is_empty() {
            if next_base.exists() {
                matches.push(eval_glob_join_output(prefix, component));
            }
        } else if next_base.is_dir() {
            let next_prefix = eval_glob_join_output(prefix, component);
            eval_glob_collect(&next_base, &next_prefix, rest, matches);
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    for name in names {
        if !eval_fnmatch_bytes(component.as_bytes(), name.as_bytes(), EVAL_FNM_PERIOD) {
            continue;
        }
        let next_base = base.join(&name);
        if rest.is_empty() {
            matches.push(eval_glob_join_output(prefix, &name));
        } else if next_base.is_dir() {
            let next_prefix = eval_glob_join_output(prefix, &name);
            eval_glob_collect(&next_base, &next_prefix, rest, matches);
        }
    }
}

/// Joins a display path prefix and component while preserving absolute-root output.
pub(in crate::interpreter) fn eval_glob_join_output(prefix: &str, component: &str) -> String {
    if prefix.is_empty() {
        component.to_string()
    } else if prefix == "/" {
        format!("/{component}")
    } else {
        format!("{prefix}/{component}")
    }
}

/// Returns whether a glob component contains wildcard syntax.
pub(in crate::interpreter) fn eval_glob_component_has_magic(component: &str) -> bool {
    component
        .as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'*' | b'?' | b'['))
}
