use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
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
                } else if name == "trim" || name == "ltrim" || name == "rtrim" {
                    if args.is_empty() || args.len() > 2 {
                        return Err(CompileError::new(
                            span, &format!("{}() takes 1 or 2 arguments", name),
                        ));
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
                if args.is_empty() || args.len() > 2 {
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
                if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                    return Err(CompileError::new(span, "count() argument must be array"));
                }
                Ok(Some(PhpType::Int))
            }
            "buffer_len" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "buffer_len() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                if !matches!(ty, PhpType::Buffer(_)) {
                    return Err(CompileError::new(span, "buffer_len() argument must be buffer<T>"));
                }
                Ok(Some(PhpType::Int))
            }
            "buffer_free" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "buffer_free() takes exactly 1 argument"));
                }
                // Restrict to plain local variables — ref params, globals, and statics
                // have aliased storage that buffer_free cannot safely nullify.
                match &args[0].kind {
                    ExprKind::Variable(name) => {
                        if self.current_class.is_some() && name == "this" {
                            return Err(CompileError::new(span, "buffer_free() cannot free $this"));
                        }
                        if self.active_ref_params.contains(name)
                            || self.active_globals.contains(name)
                            || self.active_statics.contains(name)
                        {
                            return Err(CompileError::new(
                                span,
                                "buffer_free() argument must be a local variable",
                            ));
                        }
                    }
                    _ => {
                        let ty = self.infer_type(&args[0], env)?;
                        if !matches!(ty, PhpType::Buffer(_)) {
                            return Err(CompileError::new(span, "buffer_free() argument must be buffer<T>"));
                        }
                        return Err(CompileError::new(
                            span,
                            "buffer_free() argument must be a local variable",
                        ));
                    }
                }
                let ty = self.infer_type(&args[0], env)?;
                if !matches!(ty, PhpType::Buffer(_)) {
                    return Err(CompileError::new(span, "buffer_free() argument must be buffer<T>"));
                }
                Ok(Some(PhpType::Void))
            }
            "array_pop" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "array_pop() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                match ty {
                    PhpType::Array(elem_ty) => Ok(Some(*elem_ty)),
                    PhpType::AssocArray { value, .. } => Ok(Some(*value)),
                    _ => Err(CompileError::new(span, "array_pop() argument must be array")),
                }
            }
            "in_array" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "in_array() takes exactly 2 arguments"));
                }
                self.infer_type(&args[0], env)?;
                let arr_ty = self.infer_type(&args[1], env)?;
                if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
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
                    ("array_keys", PhpType::AssocArray { key, .. }) => {
                        Ok(Some(PhpType::Array(key.clone())))
                    }
                    ("array_values", PhpType::Array(elem_ty)) => {
                        Ok(Some(PhpType::Array(elem_ty.clone())))
                    }
                    ("array_values", PhpType::AssocArray { value, .. }) => {
                        Ok(Some(PhpType::Array(value.clone())))
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
                if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
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
            "floor" | "ceil" | "sqrt"
            | "sin" | "cos" | "tan" | "asin" | "acos" | "atan"
            | "sinh" | "cosh" | "tanh"
            | "log2" | "log10" | "exp"
            | "deg2rad" | "rad2deg" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 1 argument", name)));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Float))
            }
            "log" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(CompileError::new(span, "log() takes 1 or 2 arguments"));
                }
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                Ok(Some(PhpType::Float))
            }
            "atan2" | "hypot" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 2 arguments", name)));
                }
                self.infer_type(&args[0], env)?;
                self.infer_type(&args[1], env)?;
                Ok(Some(PhpType::Float))
            }
            "pi" => {
                if !args.is_empty() {
                    return Err(CompileError::new(span, "pi() takes no arguments"));
                }
                Ok(Some(PhpType::Float))
            }
            "round" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(CompileError::new(span, "round() takes 1 or 2 arguments"));
                }
                self.infer_type(&args[0], env)?;
                if args.len() == 2 { self.infer_type(&args[1], env)?; }
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
                if args.len() < 2 {
                    return Err(CompileError::new(span, &format!("{}() requires at least 2 arguments", name)));
                }
                let mut has_float = false;
                for arg in args {
                    let t = self.infer_type(arg, env)?;
                    if t == PhpType::Float { has_float = true; }
                }
                if has_float {
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
                if !args.is_empty() && args.len() != 2 {
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
            "hash" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "hash() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "sscanf" => {
                if args.len() < 2 {
                    return Err(CompileError::new(span, "sscanf() takes at least 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
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
            // -- I/O and debugging --
            "var_dump" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "var_dump() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Void))
            }
            "print_r" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "print_r() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Void))
            }
            // -- File I/O --
            "fopen" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "fopen() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Int))
            }
            "fclose" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "fclose() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "fread" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "fread() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "fwrite" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "fwrite() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Int))
            }
            "fgets" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "fgets() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Str))
            }
            "feof" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "feof() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "readline" => {
                if !args.is_empty() && args.len() > 1 {
                    return Err(CompileError::new(span, "readline() takes 0 or 1 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "fseek" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(CompileError::new(span, "fseek() takes 2 or 3 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Int))
            }
            "ftell" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "ftell() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Int))
            }
            "rewind" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "rewind() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "file_get_contents" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "file_get_contents() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Str))
            }
            "file_put_contents" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "file_put_contents() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Int))
            }
            "file" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "file() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
            }
            "file_exists" | "is_file" | "is_dir" | "is_readable" | "is_writable" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 1 argument", name)));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "filesize" | "filemtime" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 1 argument", name)));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Int))
            }
            "copy" | "rename" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 2 arguments", name)));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Bool))
            }
            "unlink" | "mkdir" | "rmdir" | "chdir" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 1 argument", name)));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "scandir" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "scandir() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
            }
            "glob" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "glob() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
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
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "sys_get_temp_dir" => {
                if !args.is_empty() {
                    return Err(CompileError::new(span, "sys_get_temp_dir() takes no arguments"));
                }
                Ok(Some(PhpType::Str))
            }
            "fgetcsv" => {
                if args.is_empty() || args.len() > 3 {
                    return Err(CompileError::new(span, "fgetcsv() takes 1 to 3 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
            }
            "fputcsv" => {
                if args.len() < 2 || args.len() > 4 {
                    return Err(CompileError::new(span, "fputcsv() takes 2 to 4 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Int))
            }
            // -- v0.6 array functions --

            // 1-arg array functions returning same array type
            "array_reverse" | "array_unique" => {
                if args.len() != 1 {
                    return Err(CompileError::new(
                        span, &format!("{}() takes exactly 1 argument", name),
                    ));
                }
                let ty = self.infer_type(&args[0], env)?;
                if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                    return Err(CompileError::new(
                        span, &format!("{}() argument must be array", name),
                    ));
                }
                Ok(Some(ty))
            }
            "array_flip" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "array_flip() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                match ty {
                    PhpType::Array(elem_ty) => Ok(Some(PhpType::AssocArray {
                        key: elem_ty,
                        value: Box::new(PhpType::Int),
                    })),
                    PhpType::AssocArray { key, value } => Ok(Some(PhpType::AssocArray {
                        key: value,
                        value: key,
                    })),
                    _ => Err(CompileError::new(span, "array_flip() argument must be array")),
                }
            }
            "array_shift" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "array_shift() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                match ty {
                    PhpType::Array(elem_ty) => Ok(Some(*elem_ty)),
                    PhpType::AssocArray { value, .. } => Ok(Some(*value)),
                    _ => Err(CompileError::new(span, "array_shift() argument must be array")),
                }
            }

            // 1-arg array functions returning scalar
            "array_sum" | "array_product" => {
                if args.len() != 1 {
                    return Err(CompileError::new(
                        span, &format!("{}() takes exactly 1 argument", name),
                    ));
                }
                let ty = self.infer_type(&args[0], env)?;
                match ty {
                    PhpType::Array(ref elem_ty) if **elem_ty == PhpType::Float => {
                        Ok(Some(PhpType::Float))
                    }
                    PhpType::Array(_) => Ok(Some(PhpType::Int)),
                    PhpType::AssocArray { ref value, .. } if **value == PhpType::Float => {
                        Ok(Some(PhpType::Float))
                    }
                    PhpType::AssocArray { .. } => Ok(Some(PhpType::Int)),
                    _ => Err(CompileError::new(
                        span, &format!("{}() argument must be array", name),
                    )),
                }
            }
            "array_rand" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "array_rand() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                    return Err(CompileError::new(span, "array_rand() argument must be array"));
                }
                Ok(Some(PhpType::Int))
            }

            // 1-arg void (modify in place)
            "shuffle" | "natsort" | "natcasesort"
            | "asort" | "arsort" | "ksort" | "krsort" => {
                if args.len() != 1 {
                    return Err(CompileError::new(
                        span, &format!("{}() takes exactly 1 argument", name),
                    ));
                }
                let ty = self.infer_type(&args[0], env)?;
                if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                    return Err(CompileError::new(
                        span, &format!("{}() argument must be array", name),
                    ));
                }
                Ok(Some(PhpType::Void))
            }

            // 2-arg: array_key_exists($key, $arr) → Bool
            "array_key_exists" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "array_key_exists() takes exactly 2 arguments"));
                }
                self.infer_type(&args[0], env)?;
                let arr_ty = self.infer_type(&args[1], env)?;
                if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                    return Err(CompileError::new(span, "array_key_exists() second argument must be array"));
                }
                Ok(Some(PhpType::Bool))
            }
            // 2-arg: array_search($needle, $arr) → Int
            "array_search" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "array_search() takes exactly 2 arguments"));
                }
                self.infer_type(&args[0], env)?;
                let arr_ty = self.infer_type(&args[1], env)?;
                if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                    return Err(CompileError::new(span, "array_search() second argument must be array"));
                }
                Ok(Some(PhpType::Int))
            }
            // 2-arg: array_merge($arr1, $arr2) → same array type
            "array_merge" | "array_diff" | "array_intersect"
            | "array_diff_key" | "array_intersect_key" => {
                if args.len() != 2 {
                    return Err(CompileError::new(
                        span, &format!("{}() takes exactly 2 arguments", name),
                    ));
                }
                let ty1 = self.infer_type(&args[0], env)?;
                self.infer_type(&args[1], env)?;
                if !matches!(ty1, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                    return Err(CompileError::new(
                        span, &format!("{}() first argument must be array", name),
                    ));
                }
                Ok(Some(ty1))
            }
            // 2-arg: array_unshift($arr, $val) → Int (new count)
            "array_unshift" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "array_unshift() takes exactly 2 arguments"));
                }
                let arr_ty = self.infer_type(&args[0], env)?;
                self.infer_type(&args[1], env)?;
                if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                    return Err(CompileError::new(span, "array_unshift() first argument must be array"));
                }
                Ok(Some(PhpType::Int))
            }
            // 2-arg: array_combine($keys, $values) → AssocArray
            "array_combine" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "array_combine() takes exactly 2 arguments"));
                }
                let keys_ty = self.infer_type(&args[0], env)?;
                let vals_ty = self.infer_type(&args[1], env)?;
                let key_elem = match keys_ty {
                    PhpType::Array(elem) => *elem,
                    _ => return Err(CompileError::new(span, "array_combine() first argument must be array")),
                };
                let val_elem = match vals_ty {
                    PhpType::Array(elem) => *elem,
                    _ => return Err(CompileError::new(span, "array_combine() second argument must be array")),
                };
                Ok(Some(PhpType::AssocArray {
                    key: Box::new(key_elem),
                    value: Box::new(val_elem),
                }))
            }
            // 2-arg: array_fill_keys($keys, $val) → AssocArray
            "array_fill_keys" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "array_fill_keys() takes exactly 2 arguments"));
                }
                let keys_ty = self.infer_type(&args[0], env)?;
                let val_ty = self.infer_type(&args[1], env)?;
                let key_elem = match keys_ty {
                    PhpType::Array(elem) => *elem,
                    _ => return Err(CompileError::new(span, "array_fill_keys() first argument must be array")),
                };
                Ok(Some(PhpType::AssocArray {
                    key: Box::new(key_elem),
                    value: Box::new(val_ty),
                }))
            }

            // 3-arg: array_pad($arr, $size, $val) → same array type
            "array_pad" => {
                if args.len() != 3 {
                    return Err(CompileError::new(span, "array_pad() takes exactly 3 arguments"));
                }
                let ty = self.infer_type(&args[0], env)?;
                self.infer_type(&args[1], env)?;
                self.infer_type(&args[2], env)?;
                if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                    return Err(CompileError::new(span, "array_pad() first argument must be array"));
                }
                Ok(Some(ty))
            }
            // 3-arg: array_fill($start, $count, $val) → Array
            "array_fill" => {
                if args.len() != 3 {
                    return Err(CompileError::new(span, "array_fill() takes exactly 3 arguments"));
                }
                self.infer_type(&args[0], env)?;
                self.infer_type(&args[1], env)?;
                let val_ty = self.infer_type(&args[2], env)?;
                Ok(Some(PhpType::Array(Box::new(val_ty))))
            }

            // 2-3 arg: array_slice, array_splice → same array type
            "array_slice" | "array_splice" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(CompileError::new(
                        span, &format!("{}() takes 2 or 3 arguments", name),
                    ));
                }
                let ty = self.infer_type(&args[0], env)?;
                for arg in &args[1..] { self.infer_type(arg, env)?; }
                if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                    return Err(CompileError::new(
                        span, &format!("{}() first argument must be array", name),
                    ));
                }
                Ok(Some(ty))
            }
            // 2-arg: array_chunk($arr, $size) → Array of arrays
            "array_chunk" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "array_chunk() takes exactly 2 arguments"));
                }
                let ty = self.infer_type(&args[0], env)?;
                self.infer_type(&args[1], env)?;
                match ty {
                    PhpType::Array(elem_ty) => {
                        Ok(Some(PhpType::Array(Box::new(PhpType::Array(elem_ty)))))
                    }
                    PhpType::AssocArray { .. } => {
                        Err(CompileError::new(span, "array_chunk() argument must be indexed array"))
                    }
                    _ => Err(CompileError::new(span, "array_chunk() first argument must be array")),
                }
            }

            // 2-arg: array_column($arr, $column_key) → Array of column values
            "array_column" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "array_column() takes exactly 2 arguments"));
                }
                let ty = self.infer_type(&args[0], env)?;
                self.infer_type(&args[1], env)?;
                match ty {
                    PhpType::Array(inner) => {
                        match *inner {
                            PhpType::AssocArray { value, .. } => {
                                Ok(Some(PhpType::Array(value)))
                            }
                            _ => Err(CompileError::new(span, "array_column() requires an array of associative arrays")),
                        }
                    }
                    _ => Err(CompileError::new(span, "array_column() first argument must be array")),
                }
            }

            // 2-arg: range($start, $end) → Array(Int)
            "range" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "range() takes exactly 2 arguments"));
                }
                self.infer_type(&args[0], env)?;
                self.infer_type(&args[1], env)?;
                Ok(Some(PhpType::Array(Box::new(PhpType::Int))))
            }

            // -- callback-based array functions --

            "array_map" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "array_map() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                let arr_ty = self.infer_type(&args[1], env)?;
                // Resolve callback function so codegen emits it with correct param types
                if let ExprKind::StringLiteral(cb_name) = &args[0].kind {
                    if let PhpType::Array(ref elem_ty) = arr_ty {
                        // Create a dummy arg matching the actual element type
                        let dummy_arg = match elem_ty.as_ref() {
                            PhpType::Str => Expr::new(ExprKind::StringLiteral(String::new()), span),
                            PhpType::Float => Expr::new(ExprKind::FloatLiteral(0.0), span),
                            PhpType::Bool => Expr::new(ExprKind::BoolLiteral(false), span),
                            _ => Expr::new(ExprKind::IntLiteral(0), span),
                        };
                        let dummy_args = vec![dummy_arg];
                        let _ = self.check_function_call(cb_name, &dummy_args, span, env);
                    }
                }
                match arr_ty {
                    PhpType::Array(elem_ty) => Ok(Some(PhpType::Array(elem_ty))),
                    _ => Err(CompileError::new(span, "array_map() second argument must be array")),
                }
            }
            "array_filter" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "array_filter() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                let arr_ty = self.infer_type(&args[0], env)?;
                // Resolve callback function so codegen emits it
                if let ExprKind::StringLiteral(cb_name) = &args[1].kind {
                    let dummy_args = vec![Expr::new(
                        ExprKind::IntLiteral(0), span,
                    )];
                    let _ = self.check_function_call(cb_name, &dummy_args, span, env);
                }
                match arr_ty {
                    PhpType::Array(elem_ty) => Ok(Some(PhpType::Array(elem_ty))),
                    _ => Err(CompileError::new(span, "array_filter() first argument must be array")),
                }
            }
            "array_reduce" => {
                if args.len() != 3 {
                    return Err(CompileError::new(span, "array_reduce() takes exactly 3 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                // Resolve callback function so codegen emits it
                if let ExprKind::StringLiteral(cb_name) = &args[1].kind {
                    let dummy_args = vec![
                        Expr::new(ExprKind::IntLiteral(0), span),
                        Expr::new(ExprKind::IntLiteral(0), span),
                    ];
                    let _ = self.check_function_call(cb_name, &dummy_args, span, env);
                }
                Ok(Some(PhpType::Int))
            }
            "array_walk" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "array_walk() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                // Resolve callback function so codegen emits it
                if let ExprKind::StringLiteral(cb_name) = &args[1].kind {
                    let dummy_args = vec![Expr::new(
                        ExprKind::IntLiteral(0), span,
                    )];
                    let _ = self.check_function_call(cb_name, &dummy_args, span, env);
                }
                Ok(Some(PhpType::Void))
            }
            "usort" | "uksort" | "uasort" => {
                if args.len() != 2 {
                    return Err(CompileError::new(
                        span, &format!("{}() takes exactly 2 arguments", name),
                    ));
                }
                for arg in args { self.infer_type(arg, env)?; }
                // Resolve callback function so codegen emits it
                if let ExprKind::StringLiteral(cb_name) = &args[1].kind {
                    let dummy_args = vec![
                        Expr::new(ExprKind::IntLiteral(0), span),
                        Expr::new(ExprKind::IntLiteral(0), span),
                    ];
                    let _ = self.check_function_call(cb_name, &dummy_args, span, env);
                }
                Ok(Some(PhpType::Void))
            }
            // -- System functions --
            "time" => {
                if !args.is_empty() {
                    return Err(CompileError::new(span, "time() takes no arguments"));
                }
                Ok(Some(PhpType::Int))
            }
            "microtime" => {
                if args.len() > 1 {
                    return Err(CompileError::new(span, "microtime() takes 0 or 1 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Float))
            }
            "sleep" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "sleep() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Int))
            }
            "usleep" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "usleep() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Void))
            }
            "getenv" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "getenv() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Str))
            }
            "putenv" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "putenv() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Bool))
            }
            "php_uname" => {
                if args.len() > 1 {
                    return Err(CompileError::new(span, "php_uname() takes 0 or 1 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "phpversion" => {
                if !args.is_empty() {
                    return Err(CompileError::new(span, "phpversion() takes no arguments"));
                }
                Ok(Some(PhpType::Str))
            }
            "exec" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "exec() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Str))
            }
            "shell_exec" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "shell_exec() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Str))
            }
            "system" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "system() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Str))
            }
            "passthru" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "passthru() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Void))
            }
            "define" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "define() takes exactly 2 arguments"));
                }
                let name_str = match &args[0].kind {
                    ExprKind::StringLiteral(s) => s.clone(),
                    _ => return Err(CompileError::new(span, "define() first argument must be a string literal")),
                };
                let ty = self.infer_type(&args[1], env)?;
                self.constants.insert(name_str, ty);
                Ok(Some(PhpType::Void))
            }
            "call_user_func_array" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "call_user_func_array() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                // Resolve callback function so codegen emits it
                if let ExprKind::StringLiteral(cb_name) = &args[0].kind {
                    // Extract actual args from the array literal to get correct types
                    if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                        let ret_ty = self.check_function_call(cb_name, elems, span, env)?;
                        return Ok(Some(ret_ty));
                    }
                    // Fallback: use dummy args
                    if let Some(decl) = self.fn_decls.get(cb_name.as_str()).cloned() {
                        let dummy_args: Vec<Expr> = decl.params.iter()
                            .map(|_| Expr::new(ExprKind::IntLiteral(0), span))
                            .collect();
                        let ret_ty = self.check_function_call(cb_name, &dummy_args, span, env)?;
                        return Ok(Some(ret_ty));
                    }
                }
                Ok(Some(PhpType::Int))
            }
            "call_user_func" => {
                if args.is_empty() {
                    return Err(CompileError::new(span, "call_user_func() takes at least 1 argument"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                // Resolve callback function so codegen emits it
                if let ExprKind::StringLiteral(cb_name) = &args[0].kind {
                    let cb_args = args[1..].to_vec();
                    let ret_ty = self.check_function_call(cb_name, &cb_args, span, env)?;
                    return Ok(Some(ret_ty));
                }
                Ok(Some(PhpType::Int))
            }
            "function_exists" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "function_exists() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                // If the function name is a string literal and exists in fn_decls,
                // resolve it so codegen can see it in ctx.functions
                if let ExprKind::StringLiteral(cb_name) = &args[0].kind {
                    if self.fn_decls.contains_key(cb_name.as_str())
                        && !self.functions.contains_key(cb_name.as_str())
                    {
                        // Create dummy args matching the function's parameter count
                        if let Some(decl) = self.fn_decls.get(cb_name.as_str()).cloned() {
                            let dummy_args: Vec<Expr> = decl.params.iter()
                                .map(|_| Expr::new(ExprKind::IntLiteral(0), span))
                                .collect();
                            let _ = self.check_function_call(cb_name, &dummy_args, span, env);
                        }
                    }
                }
                Ok(Some(PhpType::Bool))
            }

            // -- Date/time functions --
            "date" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(CompileError::new(span, "date() takes 1 or 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "mktime" => {
                if args.len() != 6 {
                    return Err(CompileError::new(span, "mktime() takes exactly 6 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Int))
            }
            "strtotime" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "strtotime() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Int))
            }
            // -- JSON functions --
            "json_encode" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "json_encode() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Str))
            }
            "json_decode" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "json_decode() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Str))
            }
            "json_last_error" => {
                if !args.is_empty() {
                    return Err(CompileError::new(span, "json_last_error() takes no arguments"));
                }
                Ok(Some(PhpType::Int))
            }
            // -- Regex functions --
            "preg_match" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "preg_match() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Int))
            }
            "preg_match_all" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "preg_match_all() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Int))
            }
            "preg_replace" => {
                if args.len() != 3 {
                    return Err(CompileError::new(span, "preg_replace() takes exactly 3 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Str))
            }
            "preg_split" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "preg_split() takes exactly 2 arguments"));
                }
                for arg in args { self.infer_type(arg, env)?; }
                Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
            }

            // --- Pointer builtins ---
            "ptr" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "ptr() takes exactly 1 argument"));
                }
                match &args[0].kind {
                    ExprKind::Variable(_) => {
                        self.infer_type(&args[0], env)?;
                    }
                    _ => {
                        return Err(CompileError::new(
                            span,
                            "ptr() argument must be a variable",
                        ));
                    }
                }
                Ok(Some(PhpType::Pointer(None)))
            }
            "ptr_null" => {
                if !args.is_empty() {
                    return Err(CompileError::new(span, "ptr_null() takes 0 arguments"));
                }
                Ok(Some(PhpType::Pointer(None)))
            }
            "ptr_is_null" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "ptr_is_null() takes exactly 1 argument"));
                }
                let ptr_ty = self.infer_type(&args[0], env)?;
                self.ensure_pointer_type(&ptr_ty, span, "ptr_is_null()")?;
                Ok(Some(PhpType::Bool))
            }
            "ptr_offset" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "ptr_offset() takes exactly 2 arguments"));
                }
                let ptr_ty = self.infer_type(&args[0], env)?;
                self.ensure_pointer_type(&ptr_ty, span, "ptr_offset()")?;
                let offset_ty = self.infer_type(&args[1], env)?;
                if offset_ty != PhpType::Int {
                    return Err(CompileError::new(
                        span,
                        "ptr_offset() second argument must be integer",
                    ));
                }
                Ok(Some(ptr_ty))
            }
            "ptr_get" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "ptr_get() takes exactly 1 argument"));
                }
                let ptr_ty = self.infer_type(&args[0], env)?;
                self.ensure_pointer_type(&ptr_ty, span, "ptr_get()")?;
                Ok(Some(PhpType::Int))
            }
            "ptr_read8" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "ptr_read8() takes exactly 1 argument"));
                }
                let ptr_ty = self.infer_type(&args[0], env)?;
                self.ensure_pointer_type(&ptr_ty, span, "ptr_read8()")?;
                Ok(Some(PhpType::Int))
            }
            "ptr_read32" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "ptr_read32() takes exactly 1 argument"));
                }
                let ptr_ty = self.infer_type(&args[0], env)?;
                self.ensure_pointer_type(&ptr_ty, span, "ptr_read32()")?;
                Ok(Some(PhpType::Int))
            }
            "ptr_set" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "ptr_set() takes exactly 2 arguments"));
                }
                let ptr_ty = self.infer_type(&args[0], env)?;
                self.ensure_pointer_type(&ptr_ty, span, "ptr_set()")?;
                let value_ty = self.infer_type(&args[1], env)?;
                self.ensure_word_pointer_value(&value_ty, span)?;
                Ok(Some(PhpType::Void))
            }
            "ptr_write8" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "ptr_write8() takes exactly 2 arguments"));
                }
                let ptr_ty = self.infer_type(&args[0], env)?;
                self.ensure_pointer_type(&ptr_ty, span, "ptr_write8()")?;
                let value_ty = self.infer_type(&args[1], env)?;
                if value_ty != PhpType::Int {
                    return Err(CompileError::new(span, "ptr_write8() value must be int"));
                }
                Ok(Some(PhpType::Void))
            }
            "ptr_write32" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "ptr_write32() takes exactly 2 arguments"));
                }
                let ptr_ty = self.infer_type(&args[0], env)?;
                self.ensure_pointer_type(&ptr_ty, span, "ptr_write32()")?;
                let value_ty = self.infer_type(&args[1], env)?;
                if value_ty != PhpType::Int {
                    return Err(CompileError::new(span, "ptr_write32() value must be int"));
                }
                Ok(Some(PhpType::Void))
            }
            "ptr_sizeof" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "ptr_sizeof() takes exactly 1 argument"));
                }
                match &args[0].kind {
                    ExprKind::StringLiteral(type_name) => {
                        if self.normalize_pointer_target_type(type_name).is_none() {
                            return Err(CompileError::new(
                                span,
                                &format!("Unknown type for ptr_sizeof(): {}", type_name),
                            ));
                        }
                    }
                    _ => {
                        return Err(CompileError::new(
                            span,
                            "ptr_sizeof() argument must be a string literal",
                        ));
                    }
                }
                Ok(Some(PhpType::Int))
            }

            _ => Ok(None),
        }
    }
}
