//! Purpose:
//! Injects PHP's filter extension surface as ordinary synthetic declarations: the public
//! functions, `Filter\*` exceptions, and the `elephc_filter` bridge externs.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness before name resolution.
//!
//! Key details:
//! - Pay-for-use: the prelude is injected only when the program names a filter function,
//!   constant, exception class, dynamic call or introspection probe.
//! - No PHP source is parsed outside tests; every declaration is built with the shared AST
//!   builders and lowered through the ordinary EIR backend on every supported target.
//! - Constants themselves are registered through the shared builtin contract catalog.

use crate::optimize::reachability::PreludeInventory;
use crate::parser::ast::Program;
use crate::synthetic_class::internal_declarations;

mod apply;
mod externs;
mod input;
mod options;
mod public;
pub(crate) mod request;
mod surface;

/// Public filter functions, matched case-insensitively by the usage walk.
pub(crate) const FUNCTIONS: &[&str] = &[
    "filter_var",
    "filter_var_array",
    "filter_input",
    "filter_input_array",
    "filter_has_var",
    "filter_id",
    "filter_list",
];

/// Public constants owned by ext/filter, matched case-sensitively by the usage walk.
pub(crate) const CONSTANTS: &[&str] = &[
    "INPUT_POST",
    "INPUT_GET",
    "INPUT_COOKIE",
    "INPUT_ENV",
    "INPUT_SERVER",
    "FILTER_CALLBACK",
    "FILTER_DEFAULT",
    "FILTER_UNSAFE_RAW",
    "FILTER_FLAG_NONE",
    "FILTER_REQUIRE_SCALAR",
    "FILTER_REQUIRE_ARRAY",
    "FILTER_FORCE_ARRAY",
    "FILTER_NULL_ON_FAILURE",
    "FILTER_THROW_ON_FAILURE",
    "FILTER_VALIDATE_INT",
    "FILTER_VALIDATE_BOOL",
    "FILTER_VALIDATE_BOOLEAN",
    "FILTER_VALIDATE_FLOAT",
    "FILTER_VALIDATE_REGEXP",
    "FILTER_VALIDATE_DOMAIN",
    "FILTER_VALIDATE_URL",
    "FILTER_VALIDATE_EMAIL",
    "FILTER_VALIDATE_IP",
    "FILTER_VALIDATE_MAC",
    "FILTER_SANITIZE_STRING",
    "FILTER_SANITIZE_STRIPPED",
    "FILTER_SANITIZE_ENCODED",
    "FILTER_SANITIZE_SPECIAL_CHARS",
    "FILTER_SANITIZE_FULL_SPECIAL_CHARS",
    "FILTER_SANITIZE_EMAIL",
    "FILTER_SANITIZE_URL",
    "FILTER_SANITIZE_NUMBER_INT",
    "FILTER_SANITIZE_NUMBER_FLOAT",
    "FILTER_SANITIZE_ADD_SLASHES",
    "FILTER_FLAG_ALLOW_OCTAL",
    "FILTER_FLAG_ALLOW_HEX",
    "FILTER_FLAG_STRIP_LOW",
    "FILTER_FLAG_STRIP_HIGH",
    "FILTER_FLAG_STRIP_BACKTICK",
    "FILTER_FLAG_ENCODE_LOW",
    "FILTER_FLAG_ENCODE_HIGH",
    "FILTER_FLAG_ENCODE_AMP",
    "FILTER_FLAG_NO_ENCODE_QUOTES",
    "FILTER_FLAG_EMPTY_STRING_NULL",
    "FILTER_FLAG_ALLOW_FRACTION",
    "FILTER_FLAG_ALLOW_THOUSAND",
    "FILTER_FLAG_ALLOW_SCIENTIFIC",
    "FILTER_FLAG_PATH_REQUIRED",
    "FILTER_FLAG_QUERY_REQUIRED",
    "FILTER_FLAG_IPV4",
    "FILTER_FLAG_IPV6",
    "FILTER_FLAG_NO_RES_RANGE",
    "FILTER_FLAG_NO_PRIV_RANGE",
    "FILTER_FLAG_GLOBAL_RANGE",
    "FILTER_FLAG_HOSTNAME",
    "FILTER_FLAG_EMAIL_UNICODE",
];

/// Returns the complete filter declaration set, before name resolution.
pub(crate) fn filter_declarations(config: request::FilterDefault, initialize: bool) -> Program {
    internal_declarations(|| {
        let mut declarations = externs::declarations();
        declarations.extend(options::declarations());
        declarations.extend(apply::declarations());
        declarations.extend(public::declarations());
        declarations.extend(surface::declarations());
        declarations.extend(input::declarations());
        declarations.extend(request::declarations(&config));
        if initialize {
            declarations.push(crate::synthetic_class::s_expr(crate::synthetic_class::e_call(
                "__elephc_filter_initialize_request",
                vec![],
            )));
        }
        declarations
    })
}

/// Returns whether the program names the filter surface, directly or through introspection.
pub(crate) fn program_uses_surface(program: &[crate::parser::ast::Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    usage.introspects
        || usage.dynamic_function_call
        || FUNCTIONS
            .iter()
            .any(|name| usage.references(name) || usage.literals.contains(*name))
        || CONSTANTS.iter().any(|name| usage.constants.contains(*name))
        || usage.classes.contains("filter\\filterexception")
        || usage.classes.contains("filter\\filterfailedexception")
}

/// Prepends the filter surface when the program can reach it.
///
/// The declarations are function/class/extern nodes only, so prepending cannot reorder the
/// user program's top-level statements.
#[allow(dead_code)]
pub fn inject_if_used(
    program: Program,
    inventory: &mut PreludeInventory,
) -> Program {
    inject_if_used_with_config(program, request::FilterDefault::default(), false, inventory)
}

/// Prepends the filter surface with the program's `--ini filter.default` configuration.
pub fn inject_if_used_with_ini(
    program: Program,
    ini_overrides: &[(String, String)],
    inventory: &mut PreludeInventory,
) -> Program {
    inject_if_used_with_ini_and_initializer(program, ini_overrides, true, inventory)
}

/// Prepends the filter surface with an explicit choice about the request-state initializer.
///
/// The production pipeline bakes and seeds request state at startup. Test harnesses that
/// populate superglobals with their own setup statements pass `false`: an initializer running
/// before those statements would snapshot empty raw input and hide it from `filter_input()`.
pub fn inject_if_used_with_ini_and_initializer(
    program: Program,
    ini_overrides: &[(String, String)],
    initialize: bool,
    inventory: &mut PreludeInventory,
) -> Program {
    inject_if_used_with_config(
        program,
        request::config_from_overrides(ini_overrides),
        initialize,
        inventory,
    )
}

/// Shared injection body with an explicit filter.default configuration.
fn inject_if_used_with_config(
    program: Program,
    config: request::FilterDefault,
    initialize: bool,
    inventory: &mut PreludeInventory,
) -> Program {
    let used = program_uses_surface(&program)
        || config.name != request::FilterDefault::default().name;
    if !used {
        return program;
    }
    let mut declarations = filter_declarations(config, initialize);
    inventory.record_program("filter", &declarations);
    declarations.extend(program);
    declarations
}
