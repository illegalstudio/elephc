//! Purpose:
//! Declarative eval registry entry for `chown`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the ownership/group helper.

eval_builtin! {
    name: "chown",
    area: Filesystem,
    params: [filename, user],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use std::ffi::CString;
use crate::stream_wrappers;
use super::*;

/// Dispatches direct eval calls for the `chown` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_chown_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_chown_like("chown", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `chown` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_chown_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename, principal] => eval_chown_like_result("chown", *filename, *principal, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
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
