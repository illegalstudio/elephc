use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

type BuiltinResult = Result<Option<PhpType>, CompileError>;

pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "var_dump" | "print_r" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Void))
        }
        "fopen" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "fopen() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "fclose" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "fclose() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "fread" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "fread() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "fwrite" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "fwrite() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "fgets" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "fgets() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Str))
        }
        "feof" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "feof() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "readline" => {
            if !args.is_empty() && args.len() > 1 {
                return Err(CompileError::new(span, "readline() takes 0 or 1 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "fseek" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(span, "fseek() takes 2 or 3 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "ftell" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "ftell() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Int))
        }
        "rewind" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "rewind() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "file_get_contents" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "file_get_contents() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Union(vec![PhpType::Str, PhpType::Bool])))
        }
        "file_put_contents" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "file_put_contents() takes exactly 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "file" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "file() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "file_exists" | "is_file" | "is_dir" | "is_readable" | "is_writable"
        | "is_writeable" | "is_executable" | "is_link" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "filesize" | "filemtime" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Int))
        }
        "fileatime" | "filectime" | "fileperms" | "fileowner" | "filegroup" | "fileinode" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![PhpType::Int, PhpType::Bool])))
        }
        "filetype" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "filetype() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![PhpType::Str, PhpType::Bool])))
        }
        "clearstatcache" => {
            // PHP signature is `clearstatcache(bool $clear_realpath_cache = false, string $filename = ""): void`.
            // elephc has no stat cache, so we accept up to 2 args and treat the
            // call as a no-op without inspecting them.
            if args.len() > 2 {
                return Err(CompileError::new(
                    span,
                    "clearstatcache() takes at most 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Void))
        }
        "stat" | "lstat" | "fstat" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::AssocArray {
                    key: Box::new(PhpType::Mixed),
                    value: Box::new(PhpType::Int),
                },
                PhpType::Bool,
            ])))
        }
        "copy" | "rename" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        "unlink" | "mkdir" | "rmdir" | "chdir" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "scandir" | "glob" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "getcwd" => {
            if !args.is_empty() {
                return Err(CompileError::new(span, "getcwd() takes no arguments"));
            }
            Ok(Some(PhpType::Str))
        }
        "tempnam" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "tempnam() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "sys_get_temp_dir" => {
            if !args.is_empty() {
                return Err(CompileError::new(
                    span,
                    "sys_get_temp_dir() takes no arguments",
                ));
            }
            Ok(Some(PhpType::Str))
        }
        "fgetcsv" => {
            if args.is_empty() || args.len() > 3 {
                return Err(CompileError::new(span, "fgetcsv() takes 1 to 3 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "fputcsv" => {
            if args.len() < 2 || args.len() > 4 {
                return Err(CompileError::new(span, "fputcsv() takes 2 to 4 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "chmod" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            let mode_ty = checker.infer_type(&args[1], env)?;
            if mode_ty != PhpType::Int {
                return Err(CompileError::new(
                    args[1].span,
                    "chmod() mode must be int",
                ));
            }
            Ok(Some(PhpType::Bool))
        }
        "chown" | "chgrp" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            let principal_ty = checker.infer_type(&args[1], env)?;
            if !matches!(principal_ty, PhpType::Int | PhpType::Str) {
                return Err(CompileError::new(
                    args[1].span,
                    &format!("{}() owner/group must be int or string", name),
                ));
            }
            Ok(Some(PhpType::Bool))
        }
        "umask" => {
            if args.len() > 1 {
                return Err(CompileError::new(span, "umask() takes 0 or 1 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "ftruncate" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "ftruncate() takes exactly 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        "fsync" | "fflush" | "fdatasync" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "touch" => {
            if args.is_empty() || args.len() > 3 {
                return Err(CompileError::new(span, "touch() takes 1, 2, or 3 arguments"));
            }
            checker.infer_type(&args[0], env)?;
            let mut timestamp_types = Vec::new();
            for arg in args.iter().skip(1) {
                let ty = checker.infer_type(arg, env)?;
                if !matches!(ty, PhpType::Int | PhpType::Void) {
                    return Err(CompileError::new(
                        arg.span,
                        "touch() timestamp arguments must be int or null",
                    ));
                }
                timestamp_types.push(ty);
            }
            if matches!(timestamp_types.first(), Some(PhpType::Void))
                && matches!(timestamp_types.get(1), Some(ty) if !matches!(ty, PhpType::Void))
            {
                return Err(CompileError::new(
                    span,
                    "touch() mtime cannot be null when atime is provided",
                ));
            }
            Ok(Some(PhpType::Bool))
        }
        "basename" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(span, "basename() takes 1 or 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "dirname" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(span, "dirname() takes 1 or 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            if matches!(args.get(1).map(|arg| &arg.kind), Some(ExprKind::IntLiteral(levels)) if *levels < 1)
            {
                return Err(CompileError::new(
                    span,
                    "dirname() levels must be greater than or equal to 1",
                ));
            }
            Ok(Some(PhpType::Str))
        }
        "fnmatch" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(span, "fnmatch() takes 2 or 3 arguments"));
            }
            for arg in &args[..2] {
                checker.infer_type(arg, env)?;
            }
            if let Some(flags) = args.get(2) {
                if !matches!(flags.kind, ExprKind::IntLiteral(0)) {
                    return Err(CompileError::new(
                        span,
                        "fnmatch() flags other than 0 are not supported yet",
                    ));
                }
            }
            Ok(Some(PhpType::Bool))
        }
        "realpath" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "realpath() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Union(vec![PhpType::Str, PhpType::Bool])))
        }
        "pathinfo" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(
                    span,
                    "pathinfo() takes 1 or 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            let flag = match args.get(1) {
                Some(flag) => Some(pathinfo_static_flag_value(flag).ok_or_else(|| {
                    CompileError::new(
                        span,
                        "pathinfo() flag must be a compile-time PATHINFO_* constant, bitmask, or integer literal",
                    )
                })?),
                None => None,
            };
            if flag.is_none() || flag == Some(15) {
                Ok(Some(PhpType::AssocArray {
                    key: Box::new(PhpType::Str),
                    value: Box::new(PhpType::Str),
                }))
            } else {
                Ok(Some(PhpType::Str))
            }
        }
        _ => Ok(None),
    }
}

fn pathinfo_static_flag_value(flag: &Expr) -> Option<i64> {
    match &flag.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => match name.as_str() {
            "PATHINFO_DIRNAME" => Some(1),
            "PATHINFO_BASENAME" => Some(2),
            "PATHINFO_EXTENSION" => Some(4),
            "PATHINFO_FILENAME" => Some(8),
            "PATHINFO_ALL" => Some(15),
            _ => None,
        },
        ExprKind::Negate(inner) => pathinfo_static_flag_value(inner).map(|value| -value),
        ExprKind::BinaryOp { left, op, right } => {
            let left = pathinfo_static_flag_value(left)?;
            let right = pathinfo_static_flag_value(right)?;
            match op {
                BinOp::BitAnd => Some(left & right),
                BinOp::BitOr => Some(left | right),
                BinOp::BitXor => Some(left ^ right),
                _ => None,
            }
        }
        _ => None,
    }
}
