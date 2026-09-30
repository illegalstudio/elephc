//! Purpose:
//! Declarative eval registry entry for `glob`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the local glob helper.

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
        [pattern] => eval_glob_result(*pattern, 0, values),
        [pattern, flags] => {
            let flags = eval_int_value(*flags, values)?;
            eval_glob_result(*pattern, flags, values)
        }
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
    let (pattern, flags) = match args {
        [pattern] => (pattern, 0),
        [pattern, flags] => {
            let flags = eval_expr(flags, context, scope, values)?;
            (pattern, eval_int_value(flags, values)?)
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let pattern = eval_expr(pattern, context, scope, values)?;
    eval_glob_result(pattern, flags, values)
}

/// Expands one local glob pattern into an indexed PHP string array, honouring `$flags`.
///
/// Answers php's `false` — after php's warning — for any bit outside `GLOB_AVAILABLE_FLAGS`, which
/// is what php does with glibc's own `GLOB_BRACE` value (1024) or with `-1`.
pub(in crate::interpreter) fn eval_glob_result(
    pattern: RuntimeCellHandle,
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    use elephc_builtin_contract::glob_flags::GLOB_AVAILABLE_FLAGS;

    if flags & !GLOB_AVAILABLE_FLAGS != 0 {
        values.warning(concat!(
            "Warning: glob(): At least one of the passed flags is invalid or not supported ",
            "on this platform\n"
        ))?;
        return values.bool_value(false);
    }
    let pattern = eval_path_string(pattern, values)?;
    let matches = eval_glob_matches_with_flags(&pattern, flags);
    let mut result = values.array_new(matches.len())?;
    for (index, path) in matches.iter().enumerate() {
        result = super::scandir::eval_array_set_indexed_bytes(result, index, path.as_bytes(), values)?;
    }
    Ok(result)
}

/// Collects the matches for one local glob pattern under php's `$flags`.
///
/// `GLOB_BRACE` expands first and the rest applies to each expansion SEPARATELY: measured on
/// `php -n` 8.5.6, `glob("d/{c*,a*}", GLOB_BRACE)` answers `["c.log", "a.txt"]`, so the sort is
/// per expansion and the expansions keep their written order. Duplicates survive too —
/// `{*.txt,*}` lists `a.txt` twice — so the concatenation must not deduplicate.
pub(in crate::interpreter) fn eval_glob_matches_with_flags(pattern: &str, flags: i64) -> Vec<String> {
    use elephc_builtin_contract::glob_flags::{
        GLOB_BRACE, GLOB_MARK, GLOB_NOCHECK, GLOB_NOSORT, GLOB_ONLYDIR,
    };

    let patterns = if flags & GLOB_BRACE != 0 {
        eval_glob_expand_braces(pattern)
    } else {
        vec![pattern.to_string()]
    };

    let mut collected = Vec::new();
    for expanded in patterns {
        let mut matches = eval_glob_matches_one(&expanded, flags);
        if flags & GLOB_NOSORT == 0 {
            matches.sort();
        }
        // php answers the pattern itself when this expansion matched nothing, and it does so
        // BEFORE GLOB_ONLYDIR looks at the result — which is why NOCHECK|ONLYDIR with no match
        // answers the empty array rather than the pattern.
        if matches.is_empty() && flags & GLOB_NOCHECK != 0 {
            matches.push(expanded.clone());
        }
        if flags & GLOB_MARK != 0 {
            for path in matches.iter_mut() {
                if !path.ends_with('/') && std::path::Path::new(path.as_str()).is_dir() {
                    path.push('/');
                }
            }
        }
        if flags & GLOB_ONLYDIR != 0 {
            // `is_dir()` follows symlinks, and so does php: a link to a directory is kept.
            matches.retain(|path| std::path::Path::new(path.as_str()).is_dir());
        }
        collected.extend(matches);
    }
    collected
}

/// Expands csh-style `{a,b}` alternatives, including nested ones, left to right.
///
/// An unmatched `{` makes php match nothing at all rather than treating the brace as a literal —
/// measured with a file actually named `a{b` present, which `glob("a{b", GLOB_BRACE)` does not
/// find. Returning no pattern at all reproduces that.
pub(in crate::interpreter) fn eval_glob_expand_braces(pattern: &str) -> Vec<String> {
    let bytes = pattern.as_bytes();
    let Some(open) = bytes.iter().position(|byte| *byte == b'{') else {
        return vec![pattern.to_string()];
    };
    // Find the `}` that closes THIS `{`, so nested alternatives stay with their own group.
    let mut depth = 0usize;
    let mut close = None;
    for (index, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(index);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return Vec::new();
    };

    // Split the group on the commas that belong to it, not to a nested group.
    let mut alternatives = Vec::new();
    let mut start = open + 1;
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate().take(close).skip(open + 1) {
        match byte {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b',' if depth == 0 => {
                alternatives.push(&pattern[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    alternatives.push(&pattern[start..close]);

    let head = &pattern[..open];
    let tail = &pattern[close + 1..];
    let mut expanded = Vec::new();
    for alternative in alternatives {
        expanded.extend(eval_glob_expand_braces(&format!("{head}{alternative}{tail}")));
    }
    expanded
}

/// Collects the matches for one brace-free pattern, in the order the filesystem gave them.
pub(in crate::interpreter) fn eval_glob_matches_one(pattern: &str, flags: i64) -> Vec<String> {
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
    eval_glob_collect(&base, prefix, &components, &mut matches, flags);
    matches
}

/// Recursively expands one glob path component at a time.
pub(in crate::interpreter) fn eval_glob_collect(
    base: &std::path::Path,
    prefix: &str,
    components: &[&str],
    matches: &mut Vec<String>,
    flags: i64,
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
            eval_glob_collect(&next_base, &next_prefix, rest, matches, flags);
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
        // php's GLOB_NOESCAPE means a backslash quotes nothing, which is exactly fnmatch's own
        // FNM_NOESCAPE — the one flag that translates straight across.
        let mut match_flags = EVAL_FNM_PERIOD;
        if flags & elephc_builtin_contract::glob_flags::GLOB_NOESCAPE != 0 {
            match_flags |= EVAL_FNM_NOESCAPE;
        }
        if !super::fnmatch::eval_fnmatch_bytes(
            component.as_bytes(),
            name.as_bytes(),
            match_flags,
        ) {
            continue;
        }
        let next_base = base.join(&name);
        if rest.is_empty() {
            matches.push(eval_glob_join_output(prefix, &name));
        } else if next_base.is_dir() {
            let next_prefix = eval_glob_join_output(prefix, &name);
            eval_glob_collect(&next_base, &next_prefix, rest, matches, flags);
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
