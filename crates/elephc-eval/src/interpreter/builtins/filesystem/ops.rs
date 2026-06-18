//! Purpose:
//! Directory, chmod, glob, tempnam, touch, umask, link, clearstatcache, and unlink builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem` re-exports.
//!
//! Key details:
//! - Helpers return PHP-compatible false/null/string/int cells via `RuntimeValueOps`.

use super::super::super::*;
use super::super::*;
use super::*;

/// Evaluates PHP `disk_free_space($directory)` or `disk_total_space($directory)`.
pub(in crate::interpreter) fn eval_builtin_disk_space(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    eval_disk_space_result(name, directory, values)
}

/// Reports available or total filesystem bytes as a PHP float, or 0.0 on failure.
pub(in crate::interpreter) fn eval_disk_space_result(
    name: &str,
    directory: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(directory)?;
    let Ok(path) = CString::new(bytes) else {
        return values.float(0.0);
    };
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::zeroed();
    let status = unsafe {
        // libc writes the statvfs fields for this NUL-terminated local path.
        libc::statvfs(path.as_ptr(), stats.as_mut_ptr())
    };
    if status != 0 {
        return values.float(0.0);
    }
    let stats = unsafe {
        // `statvfs` succeeded, so libc initialized the full stat buffer.
        stats.assume_init()
    };
    let block_size = if stats.f_frsize > 0 {
        stats.f_frsize
    } else {
        stats.f_bsize
    };
    let blocks = match name {
        "disk_free_space" => stats.f_bavail,
        "disk_total_space" => stats.f_blocks,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.float((block_size as f64) * (blocks as f64))
}

/// Evaluates a one-path filesystem operation that returns a PHP boolean.
pub(in crate::interpreter) fn eval_builtin_unary_path_bool(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_unary_path_bool_result(name, path, values)
}

/// Executes a one-path local filesystem operation and returns whether it succeeded.
pub(in crate::interpreter) fn eval_unary_path_bool_result(
    name: &str,
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(path, values)?;
    let ok = match name {
        "chdir" => std::env::set_current_dir(path).is_ok(),
        "mkdir" => std::fs::create_dir(path).is_ok(),
        "rmdir" => std::fs::remove_dir(path).is_ok(),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(ok)
}

/// Evaluates a two-path filesystem operation that returns a PHP boolean.
pub(in crate::interpreter) fn eval_builtin_binary_path_bool(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [from, to] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let from = eval_expr(from, context, scope, values)?;
    let to = eval_expr(to, context, scope, values)?;
    eval_binary_path_bool_result(name, from, to, values)
}

/// Executes a two-path local filesystem operation and returns whether it succeeded.
pub(in crate::interpreter) fn eval_binary_path_bool_result(
    name: &str,
    from: RuntimeCellHandle,
    to: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let from = eval_path_string(from, values)?;
    let to = eval_path_string(to, values)?;
    let ok = match name {
        "copy" => std::fs::copy(from, to).is_ok(),
        "link" => std::fs::hard_link(from, to).is_ok(),
        "rename" => std::fs::rename(from, to).is_ok(),
        "symlink" => std::os::unix::fs::symlink(from, to).is_ok(),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(ok)
}

/// Evaluates PHP `chmod($filename, $permissions)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_chmod(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename, permissions] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    let permissions = eval_expr(permissions, context, scope, values)?;
    eval_chmod_result(filename, permissions, values)
}

/// Changes one local file's mode and returns whether the operation succeeded.
pub(in crate::interpreter) fn eval_chmod_result(
    filename: RuntimeCellHandle,
    permissions: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let mode = eval_int_value(permissions, values)? as u32;
    let permissions = std::fs::Permissions::from_mode(mode);
    values.bool_value(std::fs::set_permissions(path, permissions).is_ok())
}

/// Evaluates PHP `scandir($directory)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_scandir(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    eval_scandir_result(directory, values)
}

/// Lists one local directory into an indexed string array, or an empty array on failure.
pub(in crate::interpreter) fn eval_scandir_result(
    directory: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(directory, values)?;
    let Ok(entries) = std::fs::read_dir(path) else {
        return values.array_new(0);
    };
    let mut names = vec![".".to_string(), "..".to_string()];
    for entry in entries {
        let entry = entry.map_err(|_| EvalStatus::RuntimeFatal)?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    let mut result = values.array_new(names.len())?;
    for (index, name) in names.iter().enumerate() {
        result = eval_array_set_indexed_bytes(result, index, name.as_bytes(), values)?;
    }
    Ok(result)
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
        result = eval_array_set_indexed_bytes(result, index, path.as_bytes(), values)?;
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

/// Writes one byte-string value into an indexed runtime array at a zero-based position.
pub(in crate::interpreter) fn eval_array_set_indexed_bytes(
    array: RuntimeCellHandle,
    index: usize,
    value: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    let value = values.string_bytes_value(value)?;
    values.array_set(array, key, value)
}

/// Evaluates PHP `tempnam($directory, $prefix)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_tempnam(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory, prefix] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    let prefix = eval_expr(prefix, context, scope, values)?;
    eval_tempnam_result(directory, prefix, values)
}

/// Creates a unique local temporary file and returns its path, or an empty string on failure.
pub(in crate::interpreter) fn eval_tempnam_result(
    directory: RuntimeCellHandle,
    prefix: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let directory = eval_path_string(directory, values)?;
    let prefix = values.string_bytes(prefix)?;
    let prefix = String::from_utf8_lossy(&prefix);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    for attempt in 0..1000_u32 {
        let candidate =
            std::path::Path::new(&directory).join(eval_tempnam_filename(&prefix, nonce, attempt));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return values.string(candidate.to_string_lossy().as_ref()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return values.string(""),
        }
    }
    values.string("")
}

/// Builds one deterministic tempnam candidate basename from prefix, process, and attempt data.
pub(in crate::interpreter) fn eval_tempnam_filename(
    prefix: &str,
    nonce: u128,
    attempt: u32,
) -> String {
    format!("{}{}_{:x}_{attempt}", prefix, std::process::id(), nonce)
}

/// Evaluates PHP `touch($filename, $mtime = null, $atime = null)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_touch(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [filename] => {
            let filename = eval_expr(filename, context, scope, values)?;
            eval_touch_result(filename, None, None, values)
        }
        [filename, mtime] => {
            let filename = eval_expr(filename, context, scope, values)?;
            let mtime = eval_expr(mtime, context, scope, values)?;
            eval_touch_result(filename, Some(mtime), None, values)
        }
        [filename, mtime, atime] => {
            let filename = eval_expr(filename, context, scope, values)?;
            let mtime = eval_expr(mtime, context, scope, values)?;
            let atime = eval_expr(atime, context, scope, values)?;
            eval_touch_result(filename, Some(mtime), Some(atime), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Creates or stamps one local file and returns whether the operation succeeded.
pub(in crate::interpreter) fn eval_touch_result(
    filename: RuntimeCellHandle,
    mtime: Option<RuntimeCellHandle>,
    atime: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let (mtime, atime) = eval_touch_times(mtime, atime, values)?;
    let file = match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)
    {
        Ok(file) => file,
        Err(_) => return values.bool_value(false),
    };
    let times = std::fs::FileTimes::new()
        .set_modified(mtime)
        .set_accessed(atime);
    values.bool_value(file.set_times(times).is_ok())
}

/// Resolves PHP touch timestamp defaults into concrete system times.
pub(in crate::interpreter) fn eval_touch_times(
    mtime: Option<RuntimeCellHandle>,
    atime: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(std::time::SystemTime, std::time::SystemTime), EvalStatus> {
    let now = std::time::SystemTime::now();
    let Some(mtime) = mtime else {
        return Ok((now, now));
    };
    if values.is_null(mtime)? {
        if let Some(atime) = atime {
            if !values.is_null(atime)? {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        return Ok((now, now));
    }
    let mtime = eval_system_time_from_unix(eval_int_value(mtime, values)?)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let Some(atime) = atime else {
        return Ok((mtime, mtime));
    };
    if values.is_null(atime)? {
        return Ok((mtime, mtime));
    }
    let atime = eval_system_time_from_unix(eval_int_value(atime, values)?)
        .ok_or(EvalStatus::RuntimeFatal)?;
    Ok((mtime, atime))
}

/// Converts a Unix timestamp in seconds into a `SystemTime`.
pub(in crate::interpreter) fn eval_system_time_from_unix(
    seconds: i64,
) -> Option<std::time::SystemTime> {
    if seconds >= 0 {
        std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(seconds as u64))
    } else {
        std::time::UNIX_EPOCH.checked_sub(std::time::Duration::from_secs(seconds.unsigned_abs()))
    }
}

/// Evaluates PHP `umask($mask = null)` over an optional eval expression.
pub(in crate::interpreter) fn eval_builtin_umask(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_umask_result(None, values),
        [mask] => {
            let mask = eval_expr(mask, context, scope, values)?;
            eval_umask_result(Some(mask), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Applies PHP `umask()` semantics and returns the previous mask.
pub(in crate::interpreter) fn eval_umask_result(
    mask: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let previous = match mask {
        Some(mask) => {
            let mask = eval_int_value(mask, values)? as u32;
            unsafe { umask(mask) }
        }
        None => unsafe {
            let current = umask(0);
            umask(current);
            current
        },
    };
    values.int(i64::from(previous))
}

/// Evaluates PHP `readlink($path)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_readlink(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_readlink_result(path, values)
}

/// Reads one symbolic-link target string, or returns PHP false on failure.
pub(in crate::interpreter) fn eval_readlink_result(
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(path, values)?;
    match std::fs::read_link(path) {
        Ok(target) => values.string(target.to_string_lossy().as_ref()),
        Err(_) => values.bool_value(false),
    }
}

/// Evaluates PHP `linkinfo($path)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_linkinfo(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let path = eval_expr(path, context, scope, values)?;
    eval_linkinfo_result(path, values)
}

/// Returns one symlink metadata device id, or PHP's `-1` failure sentinel.
pub(in crate::interpreter) fn eval_linkinfo_result(
    path: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(path, values)?;
    let dev = match std::fs::symlink_metadata(path) {
        Ok(metadata) => i64::try_from(metadata.dev()).map_err(|_| EvalStatus::RuntimeFatal)?,
        Err(_) => -1,
    };
    values.int(dev)
}

/// Evaluates `clearstatcache(...)` as an ordered no-op in eval.
pub(in crate::interpreter) fn eval_builtin_clearstatcache(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    for arg in args {
        eval_expr(arg, context, scope, values)?;
    }
    values.null()
}

/// Evaluates PHP `unlink($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_unlink(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_unlink_result(filename, values)
}

/// Deletes one local file and returns whether it succeeded.
pub(in crate::interpreter) fn eval_unlink_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    values.bool_value(std::fs::remove_file(path).is_ok())
}
