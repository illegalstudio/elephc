//! Purpose:
//! Declarative eval registry entry for `glob`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch implements portable PHP glob flags without exposing host
//!   libc flag values to the eval surface.

eval_builtin! {
    contract: "glob",
    area: Filesystem,
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
        [pattern] => eval_glob_result(*pattern, None, values),
        [pattern, flags] => eval_glob_result(*pattern, Some(*flags), values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `glob($pattern, $flags = 0)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_glob(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [pattern] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            eval_glob_result(pattern, None, values)
        }
        [pattern, flags] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_glob_result(pattern, Some(flags), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Expands one local glob pattern into an indexed PHP string array using portable flags.
pub(in crate::interpreter) fn eval_glob_result(
    pattern: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let pattern = eval_path_string(pattern, values)?;
    let flags = flags
        .map(|value| eval_int_value(value, values))
        .transpose()?
        .unwrap_or(0);
    let matches = eval_glob_matches(&pattern, flags);
    let mut result = values.array_new(matches.len())?;
    for (index, path) in matches.iter().enumerate() {
        result = super::scandir::eval_array_set_indexed_bytes(result, index, path.as_bytes(), values)?;
    }
    Ok(result)
}

/// Collects matches for one local glob pattern while honoring PHP's portable flags.
pub(in crate::interpreter) fn eval_glob_matches(pattern: &str, flags: i64) -> Vec<String> {
    if pattern.is_empty() {
        return Vec::new();
    }
    let patterns = if flags & EVAL_GLOB_BRACE != 0 {
        eval_glob_expand_braces(pattern)
    } else {
        vec![pattern.to_string()]
    };
    let mut matches = Vec::new();
    for expanded in patterns {
        eval_glob_collect_pattern(&expanded, flags, &mut matches);
    }
    if flags & EVAL_GLOB_ONLYDIR != 0 {
        matches.retain(|path| std::path::Path::new(path.trim_end_matches('/')).is_dir());
    }
    if flags & EVAL_GLOB_NOSORT == 0 {
        matches.sort();
    }
    if flags & EVAL_GLOB_MARK != 0 {
        for path in &mut matches {
            if std::path::Path::new(path.as_str()).is_dir() && !path.ends_with('/') {
                path.push('/');
            }
        }
    }
    if matches.is_empty() && flags & EVAL_GLOB_NOCHECK != 0 {
        matches.push(pattern.to_string());
    }
    matches
}

/// Collects matches for one brace-expanded pattern into the shared result vector.
fn eval_glob_collect_pattern(pattern: &str, flags: i64, matches: &mut Vec<String>) {
    if !eval_glob_component_has_magic(pattern) {
        if std::path::Path::new(pattern).exists() {
            matches.push(pattern.to_string());
        }
        return;
    }
    let absolute = pattern.starts_with('/');
    let components: Vec<&str> = pattern
        .split('/')
        .filter(|component| !component.is_empty())
        .collect();
    let base = if absolute {
        std::path::PathBuf::from("/")
    } else {
        std::path::PathBuf::from(".")
    };
    let prefix = if absolute { "/" } else { "" };
    eval_glob_collect(&base, prefix, &components, flags, matches);
}

/// Recursively expands one glob path component at a time.
pub(in crate::interpreter) fn eval_glob_collect(
    base: &std::path::Path,
    prefix: &str,
    components: &[&str],
    flags: i64,
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
            eval_glob_collect(&next_base, &next_prefix, rest, flags, matches);
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
    if flags & EVAL_GLOB_NOSORT == 0 {
        names.sort();
    }
    let fnmatch_flags = EVAL_FNM_PERIOD
        | if flags & EVAL_GLOB_NOESCAPE != 0 {
            EVAL_FNM_NOESCAPE
        } else {
            0
        };
    for name in names {
        if !super::fnmatch::eval_fnmatch_bytes(
            component.as_bytes(),
            name.as_bytes(),
            fnmatch_flags,
        ) {
            continue;
        }
        let next_base = base.join(&name);
        if rest.is_empty() {
            matches.push(eval_glob_join_output(prefix, &name));
        } else if next_base.is_dir() {
            let next_prefix = eval_glob_join_output(prefix, &name);
            eval_glob_collect(&next_base, &next_prefix, rest, flags, matches);
        }
    }
}

/// Expands comma-separated brace alternatives recursively while preserving unmatched braces.
fn eval_glob_expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };
    let bytes = pattern.as_bytes();
    let mut depth = 0usize;
    let mut close = None;
    for (offset, byte) in bytes[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return vec![pattern.to_string()];
    };
    let body = &pattern[open + 1..close];
    let mut alternatives = Vec::new();
    let mut start = 0usize;
    let mut nested = 0usize;
    for (index, byte) in body.bytes().enumerate() {
        match byte {
            b'{' => nested += 1,
            b'}' => nested = nested.saturating_sub(1),
            b',' if nested == 0 => {
                alternatives.push(&body[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if alternatives.is_empty() {
        return vec![pattern.to_string()];
    }
    alternatives.push(&body[start..]);
    let mut expanded = Vec::new();
    for alternative in alternatives {
        let candidate = format!("{}{}{}", &pattern[..open], alternative, &pattern[close + 1..]);
        expanded.extend(eval_glob_expand_braces(&candidate));
    }
    expanded
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
