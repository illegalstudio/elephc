//! Purpose:
//! Injects PHP's `ext/ffi` surface (`FFI`, `FFI\CData`, `FFI\CType`, and the two exception
//! classes) as AST declarations over the `elephc_ffi` bridge.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness before name resolution.
//!
//! Key details:
//! - The surface is built as AST, never parsed from PHP text in production. The PHP form in
//!   `fragments` exists only under `cfg(test)` as the transcription source and parse oracle.
//! - The bridge is linked by declaring `extern "elephc_ffi"` functions, so FFI-free programs
//!   never carry the parser, libffi, or the runtime.
//! - The generated classes route all C parsing, layout, allocation, field access, and dynamic
//!   calls through the bridge; only PHP-visible object behavior lives in the prelude.

use crate::parser::ast::Program;

mod build;
mod detect;
#[cfg(test)]
mod fragments;

/// Injects the FFI prelude when referenced or explicitly forced (`--with-ffi`).
pub fn inject_if_used(
    program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        if !force && !detect::program_uses_ffi(&program) {
            return program;
        }
        let mut declarations = build::ffi_declarations();
        promote_cdata_debug_span(&mut declarations);
        bake_api_enabled(&mut declarations, api_enabled());
        inventory.record_program("ffi", &declarations);
        declarations.extend(program);
        declarations
    })
}

/// Decides whether the generated `FFI` surface may run, mirroring php-src's `ffi.enable`.
///
/// php-src defaults the directive to `preload`: CLI code is always allowed, while a
/// non-CLI SAPI only allows preloaded scripts (which a compiled binary never is). An
/// explicit `--ini ffi.enable=...` override wins, as `-d` does for the extension.
fn api_enabled() -> bool {
    let web = crate::codegen_support::compile_is_web_sapi();
    match crate::codegen_support::ini_overrides()
        .into_iter()
        .rev()
        .find(|(key, _)| key.eq_ignore_ascii_case("ffi.enable"))
    {
        Some((_, value)) => parse_ffi_enable(&value, web),
        None => !web,
    }
}

/// Parses one `ffi.enable` value with php-src's tri-state semantics.
///
/// `preload` follows the SAPI rule, `true`/`yes`/`on` and any nonzero `atoi` result enable
/// the API, and everything else disables it. The comparison is ASCII-case-insensitive,
/// matching `zend_ini_parse_bool`.
fn parse_ffi_enable(value: &str, web: bool) -> bool {
    let value = value.trim();
    if value.eq_ignore_ascii_case("preload") {
        return !web;
    }
    if value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
    {
        return true;
    }
    // `zend_ini_parse_bool` falls back to C `atoi`: optional sign, then the leading
    // decimal digits, and zero when no digit is present.
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
        index += 1;
    }
    let start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if start == index {
        return false;
    }
    let magnitude = value[start..index].parse::<u128>().unwrap_or(u128::MAX);
    magnitude != 0
}

/// Bakes the compile-time `ffi.enable` verdict into the generated `FFI::$__elephc_enabled`
/// property so every guarded API entry point throws php-src's exact exception.
fn bake_api_enabled(program: &mut [crate::parser::ast::Stmt], enabled: bool) {
    use crate::parser::ast::{ClassProperty, Stmt, StmtKind};

    /// Replaces the generated default on the restriction latch when this is the FFI class.
    fn patch(declaration: &mut Stmt, enabled: bool) -> bool {
        let StmtKind::ClassDecl { name, properties, .. } = &mut declaration.kind else {
            return false;
        };
        if !name.eq_ignore_ascii_case("FFI") {
            return false;
        }
        patch_property(properties, enabled);
        true
    }

    /// Rewrites `FFI::$__elephc_enabled` to the compile-time verdict.
    fn patch_property(properties: &mut [ClassProperty], enabled: bool) {
        for property in properties {
            if property.name == "__elephc_enabled" {
                property.default = Some(crate::synthetic_class::e_bool(enabled));
            }
        }
    }

    for statement in program.iter_mut() {
        if patch(statement, enabled) {
            continue;
        }
        let StmtKind::NamespaceBlock { name: Some(name), body } = &mut statement.kind else {
            continue;
        };
        if !name.as_canonical().eq_ignore_ascii_case("FFI") {
            continue;
        }
        for declaration in body.iter_mut() {
            patch(declaration, enabled);
        }
    }
}

/// Marks the generated `FFI\CData` class as PHP-source-shaped so the runtime `var_dump`
/// walker installs its dynamic `__debugInfo()` entry. Compiler-internal classes deliberately
/// use only the foldable projection path; FFI's fields and union members are computed at
/// runtime, so this one class must use the dynamic path.
fn promote_cdata_debug_span(program: &mut [crate::parser::ast::Stmt]) {
    use crate::parser::ast::StmtKind;
    for stmt in program.iter_mut() {
        let StmtKind::NamespaceBlock { name: Some(name), body } = &mut stmt.kind else { continue };
        if !name.as_canonical().eq_ignore_ascii_case("FFI") {
            continue;
        }
        for declaration in body.iter_mut() {
            if matches!(&declaration.kind, StmtKind::ClassDecl { name, .. } if name.eq_ignore_ascii_case("CData")) {
                declaration.span = crate::span::Span::new(1, 1);
            }
        }
    }
}

/// Parses the PHP form of the prelude exactly as the compiler would at injection time;
/// the oracle's reference side.
#[cfg(test)]
pub(crate) fn parsed_prelude() -> Program {
    let tokens = crate::lexer::tokenize(fragments::SRC).expect("FFI prelude PHP must tokenize");
    crate::parser::parse_internal(&tokens).expect("FFI prelude PHP must parse")
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Parse-parity checks for the AST-built FFI surface against its PHP form.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - The production declarations must match the parsed PHP node by node; the test strips
    //!   source spans before comparing.

    use super::*;

    /// The PHP form parses, which is the transcription source's contract.
    #[test]
    fn php_form_parses() {
        let program = parsed_prelude();
        assert!(!program.is_empty());
    }

    /// The AST builders reconstruct the PHP form declaration by declaration.
    #[test]
    fn built_declarations_match_the_php() {
        let parsed = parsed_prelude();
        let built = build::ffi_declarations();
        assert_eq!(built.len(), parsed.len(), "declaration count differs");
        for (index, (built_stmt, parsed_stmt)) in built.iter().zip(parsed.iter()).enumerate() {
            assert_eq!(
                strip_spans(&format!("{built_stmt:?}")),
                strip_spans(&format!("{parsed_stmt:?}")),
                "declaration {index} differs from its PHP form"
            );
        }
    }

    /// `ffi.enable` follows php-src's tri-state parsing: `preload` is CLI-only, the
    /// boolean spellings enable the API, and `atoi`-style nonzero numerics enable it.
    #[test]
    fn ffi_enable_values_match_php_src() {
        for (value, web, enabled) in [
            ("1", true, true),
            ("0", true, false),
            ("On", false, true),
            ("off", true, false),
            ("yes", false, true),
            ("no", false, false),
            ("true", true, true),
            ("false", true, false),
            ("preload", false, true),
            ("preload", true, false),
            ("2", true, true),
            ("-1", true, true),
            ("", true, false),
            (" true ", true, true),
            ("1abc", true, true),
            ("abc", true, false),
        ] {
            assert_eq!(
                parse_ffi_enable(value, web),
                enabled,
                "ffi.enable={value:?} web={web}"
            );
        }
    }

    /// The pipeline's compile profile and `--ini` overrides decide the verdict: the
    /// default `preload` follows the SAPI, and an explicit value always wins.
    #[test]
    fn api_enabled_reads_the_compile_profile_and_override() {
        let previous_overrides = crate::codegen_support::ini_overrides();
        let previous_version = crate::codegen_support::compile_php_version();
        let previous_web = crate::codegen_support::compile_is_web_sapi();

        crate::codegen_support::set_compile_profile(crate::web_prelude::PhpVersion::Php85, false);
        crate::codegen_support::set_ini_overrides(Vec::new());
        assert!(api_enabled(), "CLI defaults to preload");

        crate::codegen_support::set_compile_profile(crate::web_prelude::PhpVersion::Php85, true);
        assert!(!api_enabled(), "web SAPI rejects preload");

        crate::codegen_support::set_ini_overrides(vec![(
            "ffi.enable".to_string(),
            "1".to_string(),
        )]);
        assert!(api_enabled(), "an explicit enable wins over the SAPI default");

        crate::codegen_support::set_ini_overrides(vec![(
            "ffi.enable".to_string(),
            "0".to_string(),
        )]);
        crate::codegen_support::set_compile_profile(crate::web_prelude::PhpVersion::Php85, false);
        assert!(!api_enabled(), "an explicit disable wins over the CLI default");

        crate::codegen_support::set_compile_profile(previous_version, previous_web);
        crate::codegen_support::set_ini_overrides(previous_overrides);
    }

    /// The injection-time verdict reaches the generated `FFI::$__elephc_enabled` slot that
    /// every guarded API method reads.
    #[test]
    fn api_enable_verdict_is_baked_into_the_class() {
        use crate::parser::ast::{ExprKind, StmtKind};

        let mut declarations = build::ffi_declarations();
        bake_api_enabled(&mut declarations, false);
        let mut found = false;
        let mut check = |declaration: &crate::parser::ast::Stmt| {
            let StmtKind::ClassDecl { name, properties, .. } = &declaration.kind else {
                return;
            };
            if !name.eq_ignore_ascii_case("FFI") {
                return;
            }
            for property in properties {
                if property.name != "__elephc_enabled" {
                    continue;
                }
                found = true;
                assert!(matches!(
                    property.default.as_ref().map(|default| &default.kind),
                    Some(ExprKind::BoolLiteral(false))
                ));
            }
        };
        for statement in &declarations {
            check(statement);
            if let StmtKind::NamespaceBlock { name: Some(name), body } = &statement.kind {
                if name.as_canonical().eq_ignore_ascii_case("FFI") {
                    for declaration in body {
                        check(declaration);
                    }
                }
            }
        }
        assert!(found, "FFI::$__elephc_enabled must exist");
    }

    /// Earlier injected preludes contain dynamic calls of their own; those must not opt a
    /// user program into the FFI surface, while a user dynamic call still does.
    #[test]
    fn internal_prelude_dynamic_calls_do_not_inject_ffi() {
        let parse = |source: &str| {
            let tokens = crate::lexer::tokenize(source).expect("tokenize");
            crate::parser::parse(&tokens).expect("parse")
        };

        let mut internal = parse("<?php $handler();");
        for statement in &mut internal {
            statement.source_mode = crate::source::SourceMode::Internal;
        }
        assert!(!detect::program_uses_ffi(&internal));

        // Enumeration and opaque eval do not opt into FFI; a computed probe or a dynamic
        // class construction does.
        let user = parse("<?php get_declared_classes();");
        assert!(!detect::program_uses_ffi(&user));

        // A dynamic function call can only name a free function, so it does not opt into
        // FFI; dynamic class construction and computed class probes do.
        assert!(!detect::program_uses_ffi(&parse("<?php $callback();")));
        assert!(!detect::program_uses_ffi(&parse("<?php eval('$x = 1;');")));
        assert!(detect::program_uses_ffi(&parse(
            r#"<?php eval('FFI::cdef("int abs(int);");');"#
        )));
        assert!(!detect::program_uses_ffi(&parse("<?php eval($code);")));
        assert!(!detect::program_uses_ffi(&parse("<?php class_exists($name);")));
        assert!(detect::program_uses_ffi(&parse("<?php class_exists('FFI');")));
        // A probe on an object cannot name the FFI surface either; without this, the
        // pervasive `get_parent_class($object)` / `method_exists($object, 'm')` shape
        // compiles the whole FFI prelude.
        assert!(!detect::program_uses_ffi(&parse("<?php get_parent_class($object);")));
        assert!(!detect::program_uses_ffi(&parse("<?php method_exists($object, $method);")));
        assert!(!detect::program_uses_ffi(&parse("<?php function_exists($name);")));
        assert!(!detect::program_uses_ffi(&parse("<?php is_callable($value);")));
        assert!(detect::program_uses_ffi(&parse("<?php $name = 'X'; new $name();")));
    }

    /// The prelude gives `FFI::new` the concrete `FFI\CData` result PHP declares.
    #[test]
    fn new_returns_cdata() {
        let mut program = build::ffi_declarations();
        let tokens = crate::lexer::tokenize("<?php $ffi = FFI::cdef('int abs(int);'); echo $ffi->abs(-3); $x = FFI::new('int'); var_dump($x); FFI::sizeof($x);").expect("tokenize");
        program.extend(crate::parser::parse_internal(&tokens).expect("parse"));
        let program = crate::name_resolver::resolve(program).expect("resolve");
        let program = crate::func_args::desugar(program).expect("func_args desugar");
        let checked = crate::types::checker::check_types(
            &program,
            crate::codegen_support::platform::Target::parse("macos-aarch64").expect("target"),
        )
        .expect("check");
        let class = checked.classes.get("FFI").expect("FFI");
        let signature = class.static_methods.get(&crate::names::php_symbol_key("new")).expect("new");
        assert!(matches!(signature.return_type, crate::types::PhpType::Object(_)), "{:?}", signature.return_type);
    }

    /// Removes span payloads so a built node and a parsed node compare on structure alone.
    fn strip_spans(rendered: &str) -> String {
        let mut cleaned = String::with_capacity(rendered.len());
        let mut rest = rendered;
        while let Some(at) = rest.find("Span {") {
            cleaned.push_str(&rest[..at]);
            let Some(close) = rest[at..].find('}') else {
                cleaned.push_str(&rest[at..]);
                return cleaned;
            };
            rest = &rest[at + close + 1..];
        }
        cleaned.push_str(rest);
        cleaned
    }
}
