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
use crate::span::Span;

use super::{
    checker, AttrArgEntry, ClassInfo, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig,
    InterfaceInfo, PackedClassInfo, PhpType, ReturnAliasSummaries, TypeEnv,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Describes a statically-known access violation that PHP raises as a catchable
/// `Error` at runtime instead of a compile-time rejection.
pub struct ThrowAccessInfo {
    /// Source span of the offending call/assignment, used as the lookup key.
    pub span: Span,
    /// The kind of violation, selecting the PHP error message template.
    pub kind: ThrowAccessKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Categorizes statically-decided access violations lowered to runtime `Error` throws.
pub enum ThrowAccessKind {
    /// A private/protected method call from an inaccessible scope.
    PrivateMethod {
        /// Visibility label (`private` or `protected`).
        visibility: String,
        /// Class holding the method (e.g. `C`).
        class_name: String,
        /// Method name without the trailing parentheses (e.g. `secret`).
        method: String,
    },
    /// A write to a readonly property outside its declaring constructor.
    ReadonlyProperty {
        /// Class holding the property (e.g. `Box`).
        class_name: String,
        /// Property name without the leading `$` (e.g. `x`).
        property: String,
    },
}

#[derive(Debug)]
/// Aggregate result of type checking, carrying type environments, declarations,
/// class metadata, warnings, FFI data, and required libraries forward to optimizer,
/// codegen, and linker setup.
pub struct CheckResult {
    pub global_env: TypeEnv,
    pub functions: HashMap<String, FunctionSig>,
    pub function_attribute_names: HashMap<String, Vec<String>>,
    pub function_attribute_args: HashMap<String, Vec<Option<Vec<AttrArgEntry>>>>,
    pub callable_param_sigs: HashMap<(String, String), FunctionSig>,
    /// Proven return-to-parameter storage aliases for source-declared callables.
    pub(crate) return_alias_summaries: ReturnAliasSummaries,
    #[allow(dead_code)]
    pub callable_return_sigs: HashMap<String, FunctionSig>,
    #[allow(dead_code)]
    pub callable_array_return_sigs: HashMap<String, FunctionSig>,
    pub interfaces: HashMap<String, InterfaceInfo>,
    pub classes: HashMap<String, ClassInfo>,
    pub enums: HashMap<String, EnumInfo>,
    pub packed_classes: HashMap<String, PackedClassInfo>,
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    pub extern_classes: HashMap<String, ExternClassInfo>,
    pub extern_globals: HashMap<String, PhpType>,
    pub required_libraries: Vec<String>,
    pub warnings: Vec<CompileWarning>,
    /// Statically-decided access violations lowered to runtime `Error` throws,
    /// keyed by the source span of the offending call/assignment.
    pub throw_access_sites: HashMap<Span, ThrowAccessInfo>,
    /// Authoritative checker result types for builtin calls, keyed by call span.
    pub builtin_call_types: HashMap<Span, PhpType>,
}

/// Runs type checking using the host platform (auto-detected from the build environment).
/// Returns `Ok(CheckResult)` on success, or `Err(CompileError)` if type checking fails.
/// The `CheckResult` carries the resolved type environment, function signatures, class
/// metadata, warnings, and any required native libraries for linking.
#[allow(dead_code)]
pub fn check(program: &Program) -> Result<CheckResult, CompileError> {
    checker::check_types(program, Platform::detect_host())
}

/// Runs type checking targeting a specific platform (e.g., Linux instead of the host macOS).
/// Returns `Ok(CheckResult)` on success, or `Err(CompileError)` if type checking fails.
/// The target affects FFI metadata, required library resolution, and platform-specific type behavior.
pub fn check_with_target(program: &Program, target: Target) -> Result<CheckResult, CompileError> {
    checker::check_types(program, target.platform)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{Arch, Target};

    /// Parses PHP source text into an AST for use in tests.
    fn parse_program(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("tokenize failed");
        crate::parser::parse(&tokens).expect("parse failed")
    }

    /// Verifies that hashing builtins (here `md5`) require the pure-Rust
    /// `elephc_crypto` bridge on EVERY target after the Phase 2 migration off
    /// CommonCrypto/libcrypto — not the old linux-only system `crypto` library
    /// (which previously left macOS with no required library).
    #[test]
    fn test_hash_builtin_requires_elephc_crypto_on_all_targets() {
        let program = parse_program("<?php echo md5(\"abc\");");

        let linux = check_with_target(&program, Target::new(Platform::Linux, Arch::AArch64))
            .expect("linux type check failed");
        assert_eq!(linux.required_libraries, vec!["elephc_crypto"]);

        let mac = check_with_target(&program, Target::new(Platform::MacOS, Arch::AArch64))
            .expect("mac type check failed");
        assert_eq!(mac.required_libraries, vec!["elephc_crypto"]);
    }

    /// Verifies Windows rejects link-ownership builtins that php-src omits
    /// when `HAVE_LCHOWN` is unavailable.
    #[test]
    fn windows_rejects_unavailable_lchown_builtins() {
        let target = Target::new(Platform::Windows, Arch::X86_64);
        for name in ["lchown", "lchgrp"] {
            let program = parse_program(&format!("<?php {name}(\"link.txt\", -1);"));
            let error = check_with_target(&program, target)
                .expect_err("Windows must not expose lchown-family builtins");
            assert!(
                error.message.contains(&format!("Undefined function: {name}")),
                "unexpected {name} diagnostic: {}",
                error.message
            );
        }
    }

    /// Verifies Windows callable and Reflection surfaces cannot recover
    /// link-ownership builtins after direct builtin lookup rejects them.
    #[test]
    fn windows_rejects_unavailable_lchown_callable_surfaces() {
        let target = Target::new(Platform::Windows, Arch::X86_64);
        for (source, expected) in [
            (
                r#"<?php call_user_func_array("lchown", ["link.txt", -1]);"#,
                "Undefined function: lchown",
            ),
            (
                r#"<?php $callback = lchgrp(...);"#,
                "Undefined function for first-class callable: lchgrp",
            ),
            (
                r#"<?php $reflection = new ReflectionFunction("lchown");"#,
                "ReflectionFunction::__construct(): Function lchown() does not exist",
            ),
            (
                r#"<?php
class WindowsLinkOwnerIterator implements Iterator {
    private int $index = 0;
    public function current(): mixed { return $this->index; }
    public function key(): mixed { return $this->index; }
    public function next(): void { $this->index = $this->index + 1; }
    public function rewind(): void { $this->index = 0; }
    public function valid(): bool { return $this->index < 1; }
}
iterator_apply(
    new WindowsLinkOwnerIterator(),
    "lchgrp",
    ["link.txt", -1],
);
"#,
                "Undefined function: lchgrp",
            ),
        ] {
            let program = parse_program(source);
            let error = check_with_target(&program, target)
                .expect_err("Windows callable surface must omit lchown-family builtins");
            assert!(
                error.message.contains(expected),
                "unexpected Windows callable diagnostic: {}",
                error.message
            );
        }
    }

    /// Verifies Windows may declare user functions whose names are only
    /// reserved by php-src on platforms that provide `HAVE_LCHOWN`.
    #[test]
    fn windows_allows_user_lchown_function_declarations() {
        let target = Target::new(Platform::Windows, Arch::X86_64);
        for name in ["lchown", "lchgrp"] {
            let program = parse_program(&format!(
                "<?php function {name}(string $path, int $principal): bool {{ return true; }} \
                 echo {name}(\"link.txt\", -1);"
            ));
            check_with_target(&program, target)
                .expect("Windows must allow user declarations for unavailable builtins");
        }
    }

    /// Verifies unavailable Windows builtin names still specialize untyped user callables.
    #[test]
    fn windows_specializes_untyped_user_lchown_callable_surfaces() {
        let target = Target::new(Platform::Windows, Arch::X86_64);
        for source in [
            r#"<?php
function lchown($value) { return $value; }
$callback = lchown(...);
echo $callback("ok");
"#,
            r#"<?php
function lchgrp($current, $key, $iterator) { return $current > 0; }
$callback = lchgrp(...);
$filter = new CallbackFilterIterator(new ArrayIterator([1]), $callback);
foreach ($filter as $value) { echo $value; }
"#,
        ] {
            let program = parse_program(source);
            check_with_target(&program, target)
                .expect("Windows must specialize user functions named after unavailable builtins");
        }
    }

    /// Verifies enum class metadata preserves flattened trait relation data for runtime reflection.
    #[test]
    fn test_enum_class_info_preserves_trait_metadata() {
        let program = parse_program(
            r#"<?php
trait EnumMetaTrait {
    public function original() {}
}
enum EnumMetaTarget {
    use EnumMetaTrait {
        original as aliasOriginal;
    }
    case Ready;
}
"#,
        );

        let result = check(&program).expect("type check failed");
        let enum_class = result
            .classes
            .get("EnumMetaTarget")
            .expect("missing enum class metadata");

        assert_eq!(enum_class.used_traits, vec!["EnumMetaTrait"]);
        assert_eq!(
            enum_class.trait_aliases,
            vec![(
                "aliasOriginal".to_string(),
                "EnumMetaTrait::original".to_string()
            )]
        );
    }
}
