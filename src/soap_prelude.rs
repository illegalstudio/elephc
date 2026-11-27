//! Purpose:
//! Declares the first PHP `ext/soap` client slice (`SoapClient`/`SoapFault`)
//! and its WSDL-less request/response implementation.
//!
//! Called from:
//! - `crate::pipeline::compile_config` when the program names a SOAP class or
//!   function.
//!
//! Key details:
//! - The client is WSDL-less: it requires `location` and `uri` options and
//!   posts a SOAP 1.1 envelope through the existing HTTP stream wrapper.
//! - The `keep_headers` option is validated and stored; WSDL imports, where
//!   PHP 8.6 applies header redaction, are not implemented yet.
//! - Response parsing is a small namespace-tolerant XML reader in PHP so the
//!   prelude does not force the XML/libxml2 prelude into every SOAP program.

use crate::parser::ast::Program;

/// The `ext/soap` PHP source.
const SOAP_PRELUDE_SRC: &str = include_str!("soap_prelude/soap.php");

/// Returns whether `usage` reaches the SOAP surface.
fn requested(usage: &crate::prelude_prune::usage::Usage) -> bool {
    usage.functions.iter().any(|name| {
        name == "is_soap_fault" || name == "use_soap_error_handler" || name.starts_with("soap")
    }) || usage.classes.iter().any(|name| {
        name == "soapclient"
            || name == "soapfault"
            || name == "soapheader"
            || name == "soapvar"
            || name == "soapparam"
    }) || usage.literals.iter().any(|literal| {
        let literal = literal.to_ascii_lowercase();
        literal == "soapclient" || literal == "soapfault"
    })
}

/// Prepends the SOAP prelude when the program reaches it.
pub fn inject_if_used(
    mut program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    let usage = crate::prelude_prune::usage::collect(&program);
    if !requested(&usage) {
        return program;
    }
    let tokens = crate::lexer::tokenize(SOAP_PRELUDE_SRC).expect("SOAP prelude must tokenize");
    let mut declarations =
        crate::parser::parse_internal(&tokens).expect("SOAP prelude must parse");
    inventory.record_program("soap", &declarations);
    declarations.append(&mut program);
    declarations
}
