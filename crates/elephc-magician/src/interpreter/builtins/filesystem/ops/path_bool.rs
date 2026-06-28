//! Purpose:
//! Implements path operations that return PHP booleans, plus `chmod`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::ops` re-exports.
//!
//! Key details:
//! - Paths are coerced through the shared filesystem path helper before host
//!   filesystem operations are attempted.

use std::ffi::CString;

use super::super::super::super::*;
use super::super::super::*;
use super::super::*;
use crate::stream_wrappers;

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
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
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
    let Some(from) = stream_wrappers::local_filesystem_path(&from) else {
        return values.bool_value(false);
    };
    let Some(to) = stream_wrappers::local_filesystem_path(&to) else {
        return values.bool_value(false);
    };
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
    eval_chmod_result(filename, permissions, context, values)
}

/// Changes one local file's mode and returns whether the operation succeeded.
pub(in crate::interpreter) fn eval_chmod_result(
    filename: RuntimeCellHandle,
    permissions: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let mode = eval_int_value(permissions, values)? as u32;
    let metadata_value = values.int(i64::from(mode))?;
    if let Some(result) = eval_user_wrapper_stream_metadata_result(
        &path,
        EVAL_STREAM_META_ACCESS,
        metadata_value,
        context,
        values,
    )? {
        return Ok(result);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    let permissions = std::fs::Permissions::from_mode(mode);
    values.bool_value(std::fs::set_permissions(path, permissions).is_ok())
}

/// Evaluates PHP ownership/group path mutation builtins over eval expressions.
pub(in crate::interpreter) fn eval_builtin_chown_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename, principal] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    let principal = eval_expr(principal, context, scope, values)?;
    eval_chown_like_result(name, filename, principal, context, values)
}

/// Changes one local path owner or group and returns whether the operation succeeded.
pub(in crate::interpreter) fn eval_chown_like_result(
    name: &str,
    filename: RuntimeCellHandle,
    principal: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if matches!(name, "chown" | "chgrp") {
        let (option, metadata_value) =
            eval_chown_metadata_arg(name, principal, values)?;
        if let Some(result) =
            eval_user_wrapper_stream_metadata_result(&path, option, metadata_value, context, values)?
        {
            return Ok(result);
        }
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    let Some(path) = eval_c_string(&path) else {
        return values.bool_value(false);
    };
    let Some((uid, gid)) = eval_chown_principal_ids(name, principal, values)? else {
        return values.bool_value(false);
    };
    let status = unsafe {
        match name {
            "chown" | "chgrp" => libc::chown(path.as_ptr(), uid, gid),
            "lchown" | "lchgrp" => libc::lchown(path.as_ptr(), uid, gid),
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    };
    values.bool_value(status == 0)
}

/// Builds the wrapper metadata option and value for `chown()` or `chgrp()`.
fn eval_chown_metadata_arg(
    name: &str,
    principal: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(i64, RuntimeCellHandle), EvalStatus> {
    match (name, values.type_tag(principal)?) {
        ("chown", EVAL_TAG_INT) => {
            let principal = eval_int_value(principal, values)?;
            Ok((EVAL_STREAM_META_OWNER, values.int(principal)?))
        }
        ("chgrp", EVAL_TAG_INT) => {
            let principal = eval_int_value(principal, values)?;
            Ok((EVAL_STREAM_META_GROUP, values.int(principal)?))
        }
        ("chown", EVAL_TAG_STRING) => Ok((EVAL_STREAM_META_OWNER_NAME, principal)),
        ("chgrp", EVAL_TAG_STRING) => Ok((EVAL_STREAM_META_GROUP_NAME, principal)),
        ("chown" | "chgrp", _) => Err(EvalStatus::RuntimeFatal),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Resolves one PHP owner/group argument into libc uid/gid slots.
fn eval_chown_principal_ids(
    name: &str,
    principal: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(libc::uid_t, libc::gid_t)>, EvalStatus> {
    match (name, values.type_tag(principal)?) {
        ("chown" | "lchown", EVAL_TAG_INT) => {
            Ok(Some((
                eval_int_value(principal, values)? as libc::uid_t,
                !0 as libc::gid_t,
            )))
        }
        ("chgrp" | "lchgrp", EVAL_TAG_INT) => {
            Ok(Some((
                !0 as libc::uid_t,
                eval_int_value(principal, values)? as libc::gid_t,
            )))
        }
        ("chown" | "lchown", EVAL_TAG_STRING) => {
            Ok(eval_owner_name_id(principal, values)?.map(|uid| (uid, !0 as libc::gid_t)))
        }
        ("chgrp" | "lchgrp", EVAL_TAG_STRING) => {
            Ok(eval_group_name_id(principal, values)?.map(|gid| (!0 as libc::uid_t, gid)))
        }
        ("chown" | "chgrp" | "lchown" | "lchgrp", _) => Err(EvalStatus::RuntimeFatal),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Resolves a PHP user-name cell to a libc uid.
fn eval_owner_name_id(
    principal: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<libc::uid_t>, EvalStatus> {
    let name = values.string_bytes(principal)?;
    let Some(name) = eval_c_bytes(&name) else {
        return Ok(None);
    };
    let passwd = unsafe { libc::getpwnam(name.as_ptr()) };
    if passwd.is_null() {
        Ok(None)
    } else {
        Ok(Some(unsafe { (*passwd).pw_uid }))
    }
}

/// Resolves a PHP group-name cell to a libc gid.
fn eval_group_name_id(
    principal: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<libc::gid_t>, EvalStatus> {
    let name = values.string_bytes(principal)?;
    let Some(name) = eval_c_bytes(&name) else {
        return Ok(None);
    };
    let group = unsafe { libc::getgrnam(name.as_ptr()) };
    if group.is_null() {
        Ok(None)
    } else {
        Ok(Some(unsafe { (*group).gr_gid }))
    }
}

/// Converts a Rust path string into a C string, rejecting embedded NUL bytes.
fn eval_c_string(value: &str) -> Option<CString> {
    CString::new(value).ok()
}

/// Converts raw PHP bytes into a C string, rejecting embedded NUL bytes.
fn eval_c_bytes(value: &[u8]) -> Option<CString> {
    CString::new(value).ok()
}
