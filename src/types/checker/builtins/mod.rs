mod arrays;
mod callables;
mod catalog;
mod io;
mod numeric;
mod pointers;
mod strings;
mod system;

use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::Checker;

pub(crate) use catalog::is_supported_builtin_function;

impl Checker {
    fn require_linux_builtin_library(&mut self, library: &str) {
        if self.target_platform == crate::codegen::platform::Platform::Linux
            && !self.required_libraries.iter().any(|lib| lib == library)
        {
            self.required_libraries.push(library.to_string());
        }
    }

    pub fn check_builtin(
        &mut self,
        name: &str,
        args: &[Expr],
        span: crate::span::Span,
        env: &TypeEnv,
    ) -> Result<Option<PhpType>, CompileError> {
        if let Some(result) = strings::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        if let Some(result) = numeric::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        if let Some(result) = arrays::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        if let Some(result) = callables::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        if let Some(result) = io::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        if let Some(result) = system::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        if let Some(result) = pointers::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        Ok(None)
    }
}
