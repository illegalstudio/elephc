//! Purpose:
//! Injects PHP-written implementations of standard-library functions that elephc has no
//! native builtin for: `preg_quote`, `strspn`, `strcspn`, `strrchr`, `strip_tags`,
//! `substr_compare`, `get_debug_type`, `http_build_query`, `version_compare`,
//! `random_bytes`, `pack` and `unpack`.
//!
//! Called from:
//! - `crate::pipeline::compile()`, AFTER `autoload::run` (so vendor/PSR-4 code is visible) and
//!   after name resolution; the injected declarations are name-resolved on the way in.
//!
//! Key details:
//! - These are declared PHP FUNCTIONS (like `zend_version`), not catalog builtins: written once
//!   in elephc-PHP, they compile through the ordinary pipeline on every supported target with
//!   no per-architecture assembly. `function_exists()` reports `true` for them.
//! - Pay-for-use and per function: a function is injected only when the program references it
//!   (call, first-class callable, or string literal for `function_exists`/callable strings)
//!   and does not declare its own. Detection is the shared exhaustive walk in
//!   `opcache_prelude::detect`.
//! - The PHP sources live in `src/stdlib_prelude/*.php` and are parsed (not built) on demand,
//!   the same pay-for-use tokenize+parse the curl prelude does; only referenced functions are
//!   ever parsed.
//! - `pack`/`unpack` cover the full integer/string/hex/float code set; unknown codes throw a
//!   `ValueError` naming the code.

use crate::parser::ast::Program;

/// One injectable function: the PHP name the program references and the source declaring it
/// (plus any `__elephc_*` helpers it needs).
struct StdlibFunction {
    name: &'static str,
    source: &'static str,
}

/// Every function this prelude can declare, in injection order.
const FUNCTIONS: &[StdlibFunction] = &[
    StdlibFunction { name: "preg_quote", source: include_str!("stdlib_prelude/preg_quote.php") },
    StdlibFunction { name: "strspn", source: include_str!("stdlib_prelude/strspn.php") },
    StdlibFunction { name: "strcspn", source: include_str!("stdlib_prelude/strcspn.php") },
    StdlibFunction { name: "strrchr", source: include_str!("stdlib_prelude/strrchr.php") },
    StdlibFunction { name: "strip_tags", source: include_str!("stdlib_prelude/strip_tags.php") },
    StdlibFunction { name: "substr_compare", source: include_str!("stdlib_prelude/substr_compare.php") },
    StdlibFunction { name: "get_debug_type", source: include_str!("stdlib_prelude/get_debug_type.php") },
    StdlibFunction { name: "http_build_query", source: include_str!("stdlib_prelude/http_build_query.php") },
    StdlibFunction { name: "version_compare", source: include_str!("stdlib_prelude/version_compare.php") },
    StdlibFunction { name: "random_bytes", source: include_str!("stdlib_prelude/random_bytes.php") },
    StdlibFunction { name: "pack", source: include_str!("stdlib_prelude/pack.php") },
    StdlibFunction { name: "unpack", source: include_str!("stdlib_prelude/unpack.php") },
];

/// Returns whether `name` is a function this prelude can declare.
///
/// Lets the `function_exists('name')` guard fold (see `optimize::fold::expr`) treat these like
/// builtins: once the prelude injects the declaration unconditionally, a vendor polyfill written as
/// `if (!function_exists('get_debug_type')) { function get_debug_type() {} }` folds its guard to
/// `false` and drops the polyfill instead of emitting a second definition of the symbol.
pub(crate) fn provides(name: &str) -> bool {
    FUNCTIONS.iter().any(|function| function.name.eq_ignore_ascii_case(name))
}

/// Prepends the declarations of the stdlib functions this program references.
///
/// Returns the program unchanged when nothing is referenced. Injection is hoisted function
/// declarations only, so prepending cannot change top-level execution order.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Result<Program, crate::errors::CompileError> {
    crate::compiler_stack::with_compiler_stack(|| inject_on_compiler_stack(program, inventory))
}

/// The body of [`inject_if_used`], run on the stack `with_compiler_stack` sized.
fn inject_on_compiler_stack(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Result<Program, crate::errors::CompileError> {
    let mut combined: Program = Vec::new();
    for function in FUNCTIONS {
        if !crate::opcache_prelude::detect::program_references(&program, function.name)
            || crate::opcache_prelude::detect::program_declares(&program, function.name)
        {
            continue;
        }
        let tokens = crate::lexer::tokenize(function.source)
            .unwrap_or_else(|e| panic!("stdlib prelude `{}` must tokenize: {e:?}", function.name));
        let declarations = crate::parser::parse_internal(&tokens)
            .unwrap_or_else(|e| panic!("stdlib prelude `{}` must parse: {e:?}", function.name));
        combined.extend(declarations);
    }
    if combined.is_empty() {
        return Ok(program);
    }
    // Past the pipeline's name-resolution pass: resolve the declarations before splicing.
    let mut combined = crate::name_resolver::resolve(combined)?;
    inventory.record_program("stdlib", &combined);
    combined.extend(program);
    Ok(combined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        crate::parser::parse(&tokens).expect("fixture must parse")
    }

    fn declares(program: &Program, expected: &str) -> bool {
        program.iter().any(|stmt| {
            matches!(&stmt.kind, StmtKind::FunctionDecl { name, .. } if name.eq_ignore_ascii_case(expected))
        })
    }

    fn inject(program: Program) -> Program {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        inject_if_used(program, &mut inventory).expect("stdlib prelude must inject")
    }

    /// Every embedded source tokenizes, parses, and declares the PHP name it is listed under.
    #[test]
    fn every_function_source_declares_its_name() {
        for function in FUNCTIONS {
            let tokens = crate::lexer::tokenize(function.source).expect("must tokenize");
            let declarations = crate::parser::parse_internal(&tokens).expect("must parse");
            assert!(declares(&declarations, function.name), "{} not declared", function.name);
        }
    }

    #[test]
    fn unrelated_program_gets_nothing() {
        let injected = inject(parse("<?php echo 1;"));
        assert!(!declares(&injected, "pack"));
    }

    #[test]
    fn only_referenced_functions_are_injected() {
        let injected = inject(parse("<?php echo strspn('abc', 'a');"));
        assert!(declares(&injected, "strspn"));
        assert!(!declares(&injected, "strcspn"));
    }

    #[test]
    fn user_declaration_is_not_clobbered() {
        let injected = inject(parse("<?php function strrchr($a, $b) { return 1; } echo strrchr('a', 'b');"));
        let count = injected
            .iter()
            .filter(|s| matches!(&s.kind, StmtKind::FunctionDecl { name, .. } if name == "strrchr"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn provides_is_case_insensitive() {
        assert!(provides("Get_Debug_Type"));
        assert!(!provides("strlen"));
    }

    #[test]
    fn string_literal_reference_injects() {
        let injected = inject(parse("<?php var_dump(function_exists('version_compare'));"));
        assert!(declares(&injected, "version_compare"));
    }
}
