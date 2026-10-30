//! Purpose:
//! Injects PHP's `ext/gmp` surface — the final `GMP` class, its PHP 8.5 coercion and
//! operator rules, and the 51 procedural wrappers — as synthetic AST declarations over
//! the `elephc_gmp` bridge externs.
//!
//! Called from:
//! - `crate::pipeline::compile()` after the core/core-class preludes and before name
//!   resolution.
//! - Codegen fixtures and the PHPT corpus through the same pipeline.
//!
//! Key details:
//! - Injection is pay-for-use: a program that never names a GMP function or class does
//!   not declare `GMP`, does not link `elephc_gmp`, and does not require the managed
//!   `gmp` package. `--with-gmp` forces the whole surface for opaque dynamic use.
//! - Everything here is built with `crate::synthetic_class` nodes and the parser AST;
//!   no PHP source text is parsed at compile time.

mod class;
mod detect;
mod dsl;
mod functions;
mod validators;

use crate::parser::ast::{CType, Program, Stmt};
#[cfg(test)]
use crate::parser::ast::{StmtKind, TypeExpr};
use crate::synthetic_class::{e_int, e_str, extern_fn, s_const};

/// Injects the GMP surface when the program references it or `--with-gmp` forces it.
pub fn inject_if_used(
    mut program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !force && !detect::program_uses_gmp(&program) {
        return program;
    }
    let mut declarations = declarations();
    inventory.record_program("gmp", &declarations);
    declarations.append(&mut program);
    declarations
}

/// Builds every declaration the injected GMP surface contributes.
fn declarations() -> Vec<Stmt> {
    let mut program = extern_declarations();
    program.extend(constant_declarations());
    program.push(class::gmp_class());
    program.extend(validators::declarations());
    program.extend(functions::declarations());
    program
}

/// The fifteen raw bridge entry points.
fn extern_declarations() -> Vec<Stmt> {
    vec![
        extern_fn("elephc_gmp_binary", "elephc_gmp")
            .param("op", CType::Int)
            .param("left", CType::Str)
            .param("right", CType::Str)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_gmp_unary", "elephc_gmp")
            .param("op", CType::Int)
            .param("value", CType::Str)
            .param("argument", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_gmp_ternary", "elephc_gmp")
            .param("op", CType::Int)
            .param("first", CType::Str)
            .param("second", CType::Str)
            .param("third", CType::Str)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_gmp_bit", "elephc_gmp")
            .param("value", CType::Str)
            .param("index", CType::Int)
            // C `bool` is ABI-UB for values other than 0/1; the bridge takes a C int.
            .param("set", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_gmp_parse", "elephc_gmp")
            .param("value", CType::Str)
            .param("base", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_gmp_strval", "elephc_gmp")
            .param("value", CType::Str)
            .param("base", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_gmp_intval", "elephc_gmp")
            .param("value", CType::Str)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_gmp_double", "elephc_gmp")
            .param("value", CType::Str)
            .returns(CType::Float)
            .build(),
        extern_fn("elephc_gmp_fits_slong", "elephc_gmp")
            .param("value", CType::Str)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_gmp_import", "elephc_gmp")
            .param("hex", CType::Str)
            .param("word_size", CType::Int)
            .param("order", CType::Int)
            .param("endian", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_gmp_export", "elephc_gmp")
            .param("value", CType::Str)
            .param("word_size", CType::Int)
            .param("order", CType::Int)
            .param("endian", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_gmp_random_seed", "elephc_gmp")
            .param("seed", CType::Str)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_gmp_random_bits", "elephc_gmp")
            .param("bits", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_gmp_random_range", "elephc_gmp")
            .param("minimum", CType::Str)
            .param("maximum", CType::Str)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_gmp_version", "elephc_gmp")
            .returns(CType::Str)
            .build(),
    ]
}

/// PHP-visible GMP constants, including the pinned library version.
fn constant_declarations() -> Vec<Stmt> {
    vec![
        s_const("GMP_ROUND_ZERO", e_int(0)),
        s_const("GMP_ROUND_PLUSINF", e_int(1)),
        s_const("GMP_ROUND_MINUSINF", e_int(2)),
        s_const("GMP_MSW_FIRST", e_int(1)),
        s_const("GMP_LSW_FIRST", e_int(2)),
        s_const("GMP_LITTLE_ENDIAN", e_int(4)),
        s_const("GMP_BIG_ENDIAN", e_int(8)),
        s_const("GMP_NATIVE_ENDIAN", e_int(16)),
        s_const("GMP_VERSION", e_str(elephc_builtin_contract::GMP_VERSION)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses one fixture and asks the real detector.
    fn uses(source: &str) -> bool {
        let tokens = crate::lexer::tokenize(source).expect("fixture tokenizes");
        let program = crate::parser::parse(&tokens).expect("fixture parses");
        detect::program_uses_gmp(&program)
    }

    /// Verifies the detector recognizes calls and class references in every spelling.
    #[test]
    fn detector_matches_calls_and_the_class() {
        assert!(uses("<?php echo gmp_strval(gmp_init(42));"));
        assert!(uses("<?php echo \\gmp_abs(-1);"));
        assert!(uses("<?php echo \\App\\gmp_abs(-1);"));
        assert!(uses("<?php $n = new GMP('42');"));
        assert!(uses("<?php $n = new \\GMP('42');"));
        assert!(uses("<?php function f(GMP $n): void {}"));
        assert!(uses("<?php var_dump($x instanceof GMP);"));
    }

    /// Verifies unrelated names do not pull the prelude or the bridge in.
    #[test]
    fn detector_rejects_unrelated_programs() {
        assert!(!uses("<?php echo gmdate('c');"));
        assert!(!uses("<?php function gmp_helper(): int { return 1; } echo gmp_helper();"));
        assert!(!uses("<?php $g = new GmpValue();"));
        assert!(!uses("<?php echo 1 + 2;"));
    }

    /// Verifies the built surface declares the whole public helper set without parsing PHP.
    #[test]
    fn synthetic_surface_declares_every_function() {
        let program = declarations();
        let mut names = std::collections::HashSet::new();
        for statement in &program {
            if let StmtKind::FunctionDecl { name, .. } = &statement.kind {
                names.insert(name.as_str());
            }
        }
        for name in elephc_builtin_contract::GMP_FUNCTIONS {
            let helper = format!("__elephc_gmp_{}", name.trim_start_matches("gmp_"));
            assert!(names.contains(helper.as_str()), "prelude must declare {helper}");
        }
        assert!(names.contains("__elephc_gmp_value"));
        assert!(names.contains("__elephc_gmp_operand"));
    }

    /// Verifies the class is final, hides the legacy Serializable surface, and carries the
    /// dynamic-property attribute.
    #[test]
    fn class_has_the_php_82_surface() {
        let class = class::gmp_class();
        let StmtKind::ClassDecl {
            name,
            is_final,
            implements,
            methods,
            properties,
            ..
        } = &class.kind
        else {
            panic!("GMP surface must declare a class");
        };
        assert_eq!(name, "GMP");
        assert!(is_final);
        assert!(
            !implements.iter().any(|interface| interface.as_str() == "Serializable"),
            "PHP 8.5 GMP does not implement Serializable"
        );
        assert!(properties.iter().any(|property| property.name == "__elephc_num"
            && property.type_expr == Some(TypeExpr::Str)));
        for required in [
            "__construct",
            "__toString",
            "__debugInfo",
            "__serialize",
            "__unserialize",
            "__elephc_legacy_unserialize",
            "serialize",
            "unserialize",
            "__elephc_add",
            "__elephc_sub",
            "__elephc_mul",
            "__elephc_div",
            "__elephc_mod",
            "__elephc_pow",
            "__elephc_bitand",
            "__elephc_bitor",
            "__elephc_bitxor",
            "__elephc_shl",
            "__elephc_shr",
            "__elephc_compare",
            "__elephc_bool",
            "__elephc_to_int",
            "__elephc_to_float",
            "__elephc_to_number",
        ] {
            assert!(
                methods.iter().any(|method| method.name == required),
                "missing GMP::${required}",
            );
        }
        assert!(class.attributes.iter().any(|group| group.attributes.iter().any(|attribute| {
            attribute.name.as_str().eq_ignore_ascii_case("AllowDynamicProperties")
        })));
    }

    /// Verifies every extern is declared against the elephc_gmp library.
    #[test]
    fn externs_bind_the_gmp_bridge() {
        let externs = extern_declarations();
        assert_eq!(externs.len(), 15);
        for statement in externs {
            let StmtKind::ExternFunctionDecl { library, .. } = statement.kind else {
                panic!("expected an extern declaration");
            };
            assert_eq!(library.as_deref(), Some("elephc_gmp"));
        }
    }
}
