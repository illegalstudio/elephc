use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::Checker;

impl Checker {
    pub fn check_builtin(
        &mut self,
        name: &str,
        args: &[Expr],
        span: crate::span::Span,
        env: &TypeEnv,
    ) -> Result<Option<PhpType>, CompileError> {
        match name {
            "exit" | "die" => {
                if args.len() > 1 {
                    return Err(CompileError::new(span, "exit() takes 0 or 1 arguments"));
                }
                if let Some(arg) = args.first() {
                    let ty = self.infer_type(arg, env)?;
                    if ty != PhpType::Int {
                        return Err(CompileError::new(span, "exit() argument must be integer"));
                    }
                }
                Ok(Some(PhpType::Void))
            }
            "strlen" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "strlen() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                if ty != PhpType::Str {
                    return Err(CompileError::new(span, "strlen() argument must be string"));
                }
                Ok(Some(PhpType::Int))
            }
            "intval" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "intval() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Int))
            }
            "substr" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(CompileError::new(span, "substr() takes 2 or 3 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "strpos" | "strrpos" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 2 arguments", name)));
                }
                for arg in args { self.infer_type(arg, env)?; }
                // Returns Int (position) or Bool (false if not found)
                // We return Int for simplicity — false maps to -1 or similar
                Ok(Some(PhpType::Int))
            }
            "strstr" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "strstr() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "strtolower" | "strtoupper" | "ucfirst" | "lcfirst" | "ucwords"
            | "trim" | "ltrim" | "rtrim" | "strrev" | "str_repeat"
            | "str_replace" | "str_ireplace" | "chr"
            | "addslashes" | "stripslashes" | "nl2br" | "bin2hex" => {
                let expected = match name {
                    "str_repeat" => 2,
                    "str_replace" | "str_ireplace" => 3,
                    _ => 1,
                };
                if name == "chr" {
                    if args.len() != 1 {
                        return Err(CompileError::new(span, "chr() takes exactly 1 argument"));
                    }
                } else if args.len() != expected {
                    return Err(CompileError::new(
                        span, &format!("{}() takes exactly {} argument{}", name, expected, if expected > 1 { "s" } else { "" }),
                    ));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "hex2bin" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "hex2bin() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Str))
            }
            "substr_replace" => {
                if args.len() != 3 && args.len() != 4 {
                    return Err(CompileError::new(span, "substr_replace() takes 3 or 4 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "str_pad" => {
                if args.len() < 2 || args.len() > 4 {
                    return Err(CompileError::new(span, "str_pad() takes 2 to 4 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "str_split" => {
                if args.len() < 1 || args.len() > 2 {
                    return Err(CompileError::new(span, "str_split() takes 1 or 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
            }
            "wordwrap" => {
                if args.is_empty() || args.len() > 4 {
                    return Err(CompileError::new(span, "wordwrap() takes 1 to 4 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "ord" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "ord() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Int))
            }
            "strcmp" | "strcasecmp" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 2 arguments", name)));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Int))
            }
            "str_contains" | "str_starts_with" | "str_ends_with" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 2 arguments", name)));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Bool))
            }
            "explode" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "explode() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
            }
            "implode" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "implode() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "is_bool" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "is_bool() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "boolval" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "boolval() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "is_null" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "is_null() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "count" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "count() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                if !matches!(ty, PhpType::Array(_)) {
                    return Err(CompileError::new(span, "count() argument must be array"));
                }
                Ok(Some(PhpType::Int))
            }
            "array_pop" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "array_pop() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                match ty {
                    PhpType::Array(elem_ty) => Ok(Some(*elem_ty)),
                    _ => Err(CompileError::new(span, "array_pop() argument must be array")),
                }
            }
            "in_array" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "in_array() takes exactly 2 arguments"));
                }
                self.infer_type(&args[0], env)?;
                let arr_ty = self.infer_type(&args[1], env)?;
                if !matches!(arr_ty, PhpType::Array(_)) {
                    return Err(CompileError::new(span, "in_array() second argument must be array"));
                }
                Ok(Some(PhpType::Int))
            }
            "array_keys" | "array_values" => {
                if args.len() != 1 {
                    return Err(CompileError::new(
                        span, &format!("{}() takes exactly 1 argument", name),
                    ));
                }
                let ty = self.infer_type(&args[0], env)?;
                match (name, &ty) {
                    ("array_keys", PhpType::Array(_)) => {
                        Ok(Some(PhpType::Array(Box::new(PhpType::Int))))
                    }
                    ("array_values", PhpType::Array(elem_ty)) => {
                        Ok(Some(PhpType::Array(elem_ty.clone())))
                    }
                    _ => Err(CompileError::new(
                        span, &format!("{}() argument must be array", name),
                    )),
                }
            }
            "sort" | "rsort" => {
                if args.len() != 1 {
                    return Err(CompileError::new(
                        span, &format!("{}() takes exactly 1 argument", name),
                    ));
                }
                let ty = self.infer_type(&args[0], env)?;
                if !matches!(ty, PhpType::Array(_)) {
                    return Err(CompileError::new(
                        span, &format!("{}() argument must be array", name),
                    ));
                }
                Ok(Some(PhpType::Void))
            }
            "isset" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "isset() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Int))
            }
            "array_push" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "array_push() takes exactly 2 arguments"));
                }
                let arr_ty = self.infer_type(&args[0], env)?;
                let val_ty = self.infer_type(&args[1], env)?;
                if let PhpType::Array(elem_ty) = arr_ty {
                    if *elem_ty != val_ty {
                        return Err(CompileError::new(span, "array_push() type mismatch"));
                    }
                } else {
                    return Err(CompileError::new(span, "array_push() first argument must be array"));
                }
                Ok(Some(PhpType::Void))
            }
            "floatval" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "floatval() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Float))
            }
            "abs" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "abs() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                match ty {
                    PhpType::Float => Ok(Some(PhpType::Float)),
                    _ => Ok(Some(PhpType::Int)),
                }
            }
            "floor" | "ceil" | "round" | "sqrt" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 1 argument", name)));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Float))
            }
            "pow" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "pow() takes exactly 2 arguments"));
                }
                self.infer_type(&args[0], env)?;
                self.infer_type(&args[1], env)?;
                Ok(Some(PhpType::Float))
            }
            "min" | "max" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 2 arguments", name)));
                }
                let t0 = self.infer_type(&args[0], env)?;
                let t1 = self.infer_type(&args[1], env)?;
                if t0 == PhpType::Float || t1 == PhpType::Float {
                    Ok(Some(PhpType::Float))
                } else {
                    Ok(Some(PhpType::Int))
                }
            }
            "intdiv" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "intdiv() takes exactly 2 arguments"));
                }
                self.infer_type(&args[0], env)?;
                self.infer_type(&args[1], env)?;
                Ok(Some(PhpType::Int))
            }
            "is_float" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "is_float() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "is_int" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "is_int() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "is_string" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "is_string() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "is_numeric" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "is_numeric() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "fmod" | "fdiv" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 2 arguments", name)));
                }
                self.infer_type(&args[0], env)?;
                self.infer_type(&args[1], env)?;
                Ok(Some(PhpType::Float))
            }
            "rand" | "mt_rand" => {
                if args.len() != 0 && args.len() != 2 {
                    return Err(CompileError::new(span, &format!("{}() takes 0 or 2 arguments", name)));
                }
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                Ok(Some(PhpType::Int))
            }
            "random_int" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "random_int() takes exactly 2 arguments"));
                }
                self.infer_type(&args[0], env)?;
                self.infer_type(&args[1], env)?;
                Ok(Some(PhpType::Int))
            }
            "number_format" => {
                if args.is_empty() || args.len() > 4 {
                    return Err(CompileError::new(span, "number_format() takes 1 to 4 arguments"));
                }
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                Ok(Some(PhpType::Str))
            }
            "gettype" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "gettype() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Str))
            }
            "empty" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "empty() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "unset" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "unset() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Void))
            }
            "settype" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "settype() takes exactly 2 arguments"));
                }
                self.infer_type(&args[0], env)?;
                let ty = self.infer_type(&args[1], env)?;
                if ty != PhpType::Str {
                    return Err(CompileError::new(span, "settype() second argument must be a string"));
                }
                Ok(Some(PhpType::Bool))
            }
            "is_nan" | "is_finite" | "is_infinite" => {
                if args.len() != 1 {
                    return Err(CompileError::new(
                        span, &format!("{}() takes exactly 1 argument", name),
                    ));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "sprintf" => {
                if args.is_empty() {
                    return Err(CompileError::new(span, "sprintf() requires at least 1 argument"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "printf" => {
                if args.is_empty() {
                    return Err(CompileError::new(span, "printf() requires at least 1 argument"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Int))
            }
            "md5" | "sha1" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 1 argument", name)));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Str))
            }
            "htmlspecialchars" | "htmlentities" | "html_entity_decode"
            | "urlencode" | "urldecode" | "rawurlencode" | "rawurldecode"
            | "base64_encode" | "base64_decode" => {
                if args.len() != 1 {
                    return Err(CompileError::new(
                        span, &format!("{}() takes exactly 1 argument", name),
                    ));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Str))
            }
            "ctype_alpha" | "ctype_digit" | "ctype_alnum" | "ctype_space" => {
                if args.len() != 1 {
                    return Err(CompileError::new(
                        span, &format!("{}() takes exactly 1 argument", name),
                    ));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            _ => Ok(None),
        }
    }
}
