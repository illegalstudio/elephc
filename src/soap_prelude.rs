//! Purpose:
//! Declares PHP's `ext/soap` client and server surface (`SoapClient`,
//! `SoapServer`, `SoapFault`, header/value helpers) and its HTTP implementation.
//!
//! Called from:
//! - `crate::pipeline::compile_config` when the program names a SOAP class or
//!   function.
//!
//! Key details:
//! - The client supports WSDL and WSDL-less modes, SOAP 1.1/1.2 envelopes, and
//!   PHP 8.6's cross-origin `keep_headers` redaction during WSDL imports.
//! - `SoapServer` dispatches RPC-style requests to registered functions or
//!   object methods and serialises SOAP 1.1/1.2 responses and faults.
//! - Response parsing is a small namespace-tolerant XML reader in PHP so the
//!   prelude does not force the XML/libxml2 prelude into every SOAP program.

use crate::parser::ast::Program;

/// The `ext/soap` PHP source.
const SOAP_PRELUDE_SRC: &str = include_str!("soap_prelude/soap.php");
/// The PHP 8.6 `Soap\Url`/`Soap\Sdl` namespace classes.
const SOAP_CLASS_SRC: &str = include_str!("soap_prelude/classes.php");

/// Returns whether `usage` reaches the SOAP surface.
fn requested(usage: &crate::prelude_prune::usage::Usage) -> bool {
    usage.functions.iter().any(|name| {
        name == "is_soap_fault" || name == "use_soap_error_handler" || name.starts_with("soap")
    }) || usage.constants.iter().any(|name| {
        name.starts_with("SOAP_")
            || name.starts_with("XSD_")
            || name.starts_with("WSDL_CACHE_")
            || name == "UNKNOWN_TYPE"
            || name == "APACHE_MAP"
    }) || usage.classes.iter().any(|name| {
        name == "soapclient"
            || name == "soapfault"
            || name == "soapheader"
            || name == "soapvar"
            || name == "soapparam"
            || name == "soapserver"
            || name == "soap\\url"
            || name == "soap\\sdl"
    }) || usage.literals.iter().any(|literal| {
        let literal = literal.to_ascii_lowercase();
        literal == "soapclient"
            || literal == "soapfault"
            || literal == "soapserver"
            || literal == "soap\\url"
            || literal == "soap\\sdl"
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
    let class_tokens =
        crate::lexer::tokenize(SOAP_CLASS_SRC).expect("SOAP classes must tokenize");
    declarations.extend(
        crate::parser::parse_internal(&class_tokens).expect("SOAP classes must parse"),
    );
    inventory.record_program("soap", &declarations);
    declarations.append(&mut program);
    declarations
}
