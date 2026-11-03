//! Purpose:
//! Injects PHP's `ext/zip` surface: the `ZipArchive` class and the deprecated procedural
//! `zip_*` functions, implemented in PHP over the `elephc_zip` bridge's extern block.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`, before
//!   name resolution.
//!
//! Key details:
//! - Built as AST: `build.rs` is the `synthetic_class` transcription of the PHP surface.
//!   The provenance PHP text lives under `tests/php_src/zip/prelude.php` and is never parsed
//!   by the compiler. The bridge is what makes the surface optional.
//! - Every wrapper binds the handle to a local before calling, matching the hash/curl
//!   prelude rule for `mixed` properties.
//! - PHP-visible binary data crosses the bridge as hex because extern string parameters are
//!   NUL-terminated C strings.
//! - The archive engine is pure Rust, so the surface needs no managed native package.
//!   `addPattern()` calls the bridge's pure-Rust matcher instead of linking PCRE2, keeping
//!   ordinary zip programs free of the regex package.
//! - `zip://` is a PHP user-space wrapper over the bridge, and `getStream*()` uses a
//!   companion memory wrapper so CRC failures and archive-close invalidation surface the
//!   way php-src's native zip stream reports them.

mod build;
mod detect;

use crate::parser::ast::Program;

/// Builds the non-web `ini_set` compatibility function.
///
/// The web prelude owns the complete session/OPcache `ini_set()` surface. Outside `--web`,
/// this declaration provides the one directive zip itself observes, `open_basedir`, and
/// reports other directives as unsupported (`false`) instead of leaving the name undefined.
fn ini_set_declaration() -> crate::parser::ast::Stmt {
    use crate::parser::ast::{BinOp, CastType, TypeExpr};
    use crate::synthetic_class::{
        e_binop, e_bool, e_cast, e_str, e_var, function, s_assign, s_global, s_if,
        s_return, t_mixed,
    };
    function("ini_set")
        .param("option", TypeExpr::Str)
        .param("value", t_mixed())
        .returns(TypeExpr::Union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_global("__elephc_zip_open_basedir"),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("open_basedir")),
                vec![
                    s_assign(
                        "_previous",
                        e_cast(CastType::String, e_var("__elephc_zip_open_basedir")),
                    ),
                    s_assign(
                        "__elephc_zip_open_basedir",
                        e_cast(CastType::String, e_var("value")),
                    ),
                    s_return(e_var("_previous")),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(false)),
        ])
        .build()
}

/// Builds the `ext/zip` surface from AST builders.
pub(crate) fn zip_declarations() -> Program {
    let mut program = crate::synthetic_class::with_synthetic_spans(build::zip_declarations);
    // The class is a normal PHP declaration even though the procedural wrappers stay internal:
    // `print_r` and `var_dump` only consult `__debugInfo()` for non-internal classes.
    for stmt in &mut program {
        if matches!(&stmt.kind, crate::parser::ast::StmtKind::ClassDecl { name, .. } if name == "ZipArchive") {
            stmt.source_mode = crate::source::SourceMode::Php;
            // `is_internal` keys off the class span, not only the source mode. Give the
            // user-visible class a real-source position so `var_dump` uses `__debugInfo()`.
            stmt.span = crate::span::Span::new(1, 1);
        }
    }
    program
}

/// Returns whether the program already declares `ini_set` (the CLI OPcache prelude owns it there).
fn declares_ini_set(program: &Program) -> bool {
    program.iter().any(|stmt| matches!(
        &stmt.kind,
        crate::parser::ast::StmtKind::FunctionDecl { name, .. } if name.eq_ignore_ascii_case("ini_set")
    ))
}

/// Whether `program` uses the zip surface.
pub(crate) fn program_uses_surface(program: &[crate::parser::ast::Stmt]) -> bool {
    detect::program_uses_zip(program)
}

/// Injects the zip prelude when the program references the surface or `force` is set.
pub fn inject_if_used(
    program: Program,
    force: bool,
    web: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        if !force && !detect::program_uses_zip(&program) {
            return program;
        }
        let mut declarations = zip_declarations();
        if !web && !declares_ini_set(&program) {
            declarations.push(crate::synthetic_class::with_synthetic_spans(ini_set_declaration));
        }
        inventory.record_program("zip", &declarations);
        declarations.extend(program);
        declarations
    })
}
