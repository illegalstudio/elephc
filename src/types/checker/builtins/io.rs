use crate::errors::CompileError;
use crate::parser::ast::Expr;
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
        "file_exists" | "is_file" | "is_dir" | "is_readable" | "is_writable" => {
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
        _ => Ok(None),
    }
}
