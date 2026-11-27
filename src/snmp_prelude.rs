//! Purpose:
//! Declares the first PHP `ext/snmp` slice and its `elephc_snmp` bridge externs.
//!
//! Called from:
//! - `crate::pipeline::compile_config` when the program names an `snmp*`
//!   function, `SNMP`, or the bridge is forced with `--with-snmp`.
//!
//! Key details:
//! - The bridge is a pure-Rust SNMP v1/v2c UDP client; v3/USM, MIB management,
//!   and the PHP 8.6 output flags are intentionally not declared yet.
//! - Wrapper loops live in PHP so the bridge exposes a small session/response
//!   ABI; strings are binary-safe through `ptr` plus `hex` encoding.

use crate::parser::ast::Program;

/// The `ext/snmp` PHP source and bridge externs.
const SNMP_PRELUDE_SRC: &str = include_str!("snmp_prelude/snmp.php");
/// The PHP 8.6 `Snmp\*` enums.
const SNMP_ENUM_SRC: &str = include_str!("snmp_prelude/enums.php");

/// Returns whether `usage` reaches the SNMP surface.
fn requested(usage: &crate::prelude_prune::usage::Usage) -> bool {
    usage.functions.iter().any(|name| name.starts_with("snmp"))
        || usage.classes.iter().any(|name| name == "snmp" || name == "snmpexception")
        || usage.constants.iter().any(|name| name.starts_with("SNMP_"))
        || usage.literals.iter().any(|literal| {
            let literal = literal.to_ascii_lowercase();
            literal.starts_with("snmp") || literal == "snmpexception"
        })
}

/// Prepends the SNMP prelude when the program reaches it or it is forced.
pub fn inject_if_used(
    mut program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !force {
        let usage = crate::prelude_prune::usage::collect(&program);
        if !requested(&usage) {
            return program;
        }
    }
    let tokens = crate::lexer::tokenize(SNMP_PRELUDE_SRC).expect("SNMP prelude must tokenize");
    let mut declarations =
        crate::parser::parse_internal(&tokens).expect("SNMP prelude must parse");
    let enum_tokens = crate::lexer::tokenize(SNMP_ENUM_SRC).expect("SNMP enums must tokenize");
    declarations.extend(
        crate::parser::parse_internal(&enum_tokens).expect("SNMP enums must parse"),
    );
    inventory.record_program("snmp", &declarations);
    declarations.append(&mut program);
    declarations
}
