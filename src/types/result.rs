//! Purpose:
//! Defines the aggregate result returned by type checking to the pipeline.
//! Carries type environments, declarations, class metadata, warnings, FFI data, and required libraries forward.
//!
//! Called from:
//! - `crate::types::check()`
//! - `crate::pipeline::compile()`
//!
//! Key details:
//! - Fields are consumed by optimizer, codegen, and linker setup; keep additions explicit and phase-owned.

use std::collections::HashMap;

use crate::codegen::platform::{Platform, Target};
use crate::errors::{CompileError, CompileWarning};
use crate::parser::ast::Program;

use super::{
    checker, ClassInfo, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo,
    PackedClassInfo, PhpType, TypeEnv,
};

#[derive(Debug)]
pub struct CheckResult {
    pub global_env: TypeEnv,
    pub functions: HashMap<String, FunctionSig>,
    pub interfaces: HashMap<String, InterfaceInfo>,
    pub classes: HashMap<String, ClassInfo>,
    pub enums: HashMap<String, EnumInfo>,
    pub packed_classes: HashMap<String, PackedClassInfo>,
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    pub extern_classes: HashMap<String, ExternClassInfo>,
    pub extern_globals: HashMap<String, PhpType>,
    pub required_libraries: Vec<String>,
    pub warnings: Vec<CompileWarning>,
}

#[allow(dead_code)]
pub fn check(program: &Program) -> Result<CheckResult, CompileError> {
    checker::check_types(program, Platform::detect_host())
}

pub fn check_with_target(program: &Program, target: Target) -> Result<CheckResult, CompileError> {
    checker::check_types(program, target.platform)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{Arch, Target};

    fn parse_program(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("tokenize failed");
        crate::parser::parse(&tokens).expect("parse failed")
    }

    #[test]
    fn test_linux_crypto_builtin_linking_tracks_target_not_host() {
        let program = parse_program("<?php echo md5(\"abc\");");

        let linux = check_with_target(&program, Target::new(Platform::Linux, Arch::AArch64))
            .expect("linux type check failed");
        assert_eq!(linux.required_libraries, vec!["crypto"]);

        let mac = check_with_target(&program, Target::new(Platform::MacOS, Arch::AArch64))
            .expect("mac type check failed");
        assert!(mac.required_libraries.is_empty());
    }
}
