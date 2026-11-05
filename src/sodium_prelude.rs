//! Purpose:
//! Injects PHP's `ext/sodium` surface as synthetic AST declarations over the
//! `elephc_sodium` bridge and the managed `libsodium` native package.
//!
//! Called from:
//! - `crate::pipeline::compile()` after include resolution and before name resolution.
//! - The codegen test harness through the same pipeline.
//!
//! Key details:
//! - Injection is pay-for-use: a program that never names a `sodium_*` function,
//!   `password_*` function, `SODIUM_*`/`PASSWORD_*` constant or `SodiumException`
//!   declares nothing, links no bridge and requires no native package. `--with-sodium`
//!   forces the whole surface for opaque dynamic use.
//! - Everything here is built with `crate::synthetic_class` nodes and the parser AST;
//!   no PHP source text is parsed at compile time.
//! - The bridge crosses the C boundary as hexadecimal text because Elephc's `char*`
//!   extern ABI truncates embedded NUL bytes.

mod constants;
mod detect;
mod dsl;
mod mutators;
mod functions;
mod password;
mod spec;
mod validators;

use crate::parser::ast::{CType, Program, Stmt};
use crate::synthetic_class::{class, extern_fn};

/// Injects the sodium surface when the program references it or `--with-sodium` forces it.
pub fn inject_if_used(
    mut program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !force && !detect::program_uses_sodium(&program) {
        return program;
    }
    let mut declarations = declarations();
    inventory.record_program("sodium", &declarations);
    declarations.append(&mut program);
    declarations
}

/// Builds every declaration the injected sodium surface contributes.
pub(crate) fn declarations() -> Vec<Stmt> {
    let mut program = extern_declarations();
    program.extend(constants::declarations());
    program.push(class("SodiumException").extends("Exception").build());
    program.extend(validators::declarations());
    program.extend(functions::declarations());
    program.extend(functions::framing_helpers());
    program.extend(mutators::declarations());
    program.extend(password::declarations());
    program
}

/// The bridge symbols the prelude calls.
fn extern_declarations() -> Vec<Stmt> {
    vec![
        extern_fn("elephc_sodium_call", "elephc_sodium")
            .param("op", CType::Str)
            .param("a0", CType::Str)
            .param("a1", CType::Str)
            .param("a2", CType::Str)
            .param("a3", CType::Str)
            .param("i0", CType::Int)
            .param("i1", CType::Int)
            .param("i2", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_sodium_pwhash", "elephc_sodium")
            .param("password", CType::Str)
            .param("salt", CType::Str)
            .param("length", CType::Int)
            .param("opslimit", CType::Int)
            .param("memlimit", CType::Int)
            .param("algorithm", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_sodium_pwhash_str_alg", "elephc_sodium")
            .param("password", CType::Str)
            .param("opslimit", CType::Int)
            .param("memlimit", CType::Int)
            .param("algorithm", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("_rt_str_zero", "elephc_sodium")
            .param("string", CType::Str)
            .returns(CType::Void)
            .build(),
        extern_fn("elephc_sodium_version", "elephc_sodium")
            .returns(CType::Str)
            .build(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// Verifies the synthetic surface declares the exception and every helper.
    #[test]
    fn declarations_cover_the_whole_surface() {
        let program = declarations();
        let mut classes = Vec::new();
        let mut functions = std::collections::HashSet::new();
        for statement in &program {
            match &statement.kind {
                StmtKind::ClassDecl { name, .. } => classes.push(name.as_str()),
                StmtKind::FunctionDecl { name, .. } => {
                    functions.insert(name.as_str());
                }
                _ => {}
            }
        }
        assert!(classes.contains(&"SodiumException"));
        for name in elephc_builtin_contract::SODIUM_FUNCTIONS {
            let helper = format!("__elephc_sodium_{}", name.trim_start_matches("sodium_"));
            assert!(functions.contains(helper.as_str()), "missing {helper}");
        }
    }

    /// Verifies the detector recognizes calls and the exception class.
    #[test]
    fn detector_matches_calls_and_the_class() {
        assert!(uses("<?php echo sodium_bin2hex('x');"));
        assert!(uses("<?php echo \\sodium_bin2hex('x');"));
        assert!(uses("<?php echo \\App\\sodium_bin2hex('x');"));
        assert!(uses("<?php try {} catch (SodiumException $e) {}"));
        assert!(uses("<?php $x = new \\SodiumException('x');"));
    }

    /// Verifies unrelated names do not pull the prelude or the bridge in.
    #[test]
    fn detector_rejects_unrelated_programs() {
        assert!(!uses("<?php echo sodium_chloride();"));
        assert!(!uses("<?php function sodium_helper(): int { return 1; } echo sodium_helper();"));
        assert!(!uses("<?php echo 1 + 2;"));
    }

    /// Parses one fixture and asks the real detector.
    fn uses(source: &str) -> bool {
        let tokens = crate::lexer::tokenize(source).expect("fixture tokenizes");
        let program = crate::parser::parse(&tokens).expect("fixture parses");
        detect::program_uses_sodium(&program)
    }
}
