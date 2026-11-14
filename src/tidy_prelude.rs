//! Purpose:
//! PHP's `ext/tidy` surface in elephc-PHP: the `tidy` and `tidyNode` classes,
//! the 24 `tidy_*` functions, and the `ob_tidyhandler` output callback, built
//! on the `elephc_tidy` bridge extern block.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`,
//!   after the xml prelude and before name resolution.
//!
//! Key details:
//! - WHY A PRELUDE AND NOT A NATIVE CLASS. Declaring the `extern "elephc_tidy"`
//!   block is what links `libelephc_tidy.a`, so the surface is bridge-gated: a
//!   program that never touches tidy declares neither class and links nothing.
//! - BUILT, NOT PARSED. The declarations are `synthetic_class` builders transcribed from
//!   the PHP form in `fragments`. That text stays under `cfg(test)` as the node-by-node
//!   parse-parity oracle; the compiler never tokenizes it.
//! - `tidy` and `tidyNode` carry class contracts in the shared catalog so
//!   `instanceof`, reflection, scalar casts, and `var_dump` agree with PHP.
//! - `ob_tidyhandler` is not in the PHP function catalog, but the detector treats
//!   a call or a string literal with that name as a tidy reference.

mod build;
mod detect;
#[cfg(test)]
mod fragments;

use crate::parser::ast::{Program, Stmt, TypeExpr};
use crate::synthetic_class::{e_bool, e_call, e_str, function, s_expr, s_if};

/// Builds the tidy surface declarations.
pub(crate) fn tidy_declarations() -> Program {
    build::tidy_declarations()
}

/// Parses the PHP form of the prelude exactly as the compiler used to; the oracle's
/// reference side.
#[cfg(test)]
pub(crate) fn parsed_prelude() -> Program {
    let tokens = crate::lexer::tokenize(fragments::SRC)
        .expect("tidy prelude PHP must tokenize");
    crate::parser::parse_internal(&tokens).expect("tidy prelude PHP must parse")
}

/// Whether `program` uses this prelude's surface; the same test `inject_if_used` applies.
///
/// Also run by `crate::autoload::prelude_probe` over the autoload closure, whose files
/// are only loaded after this prelude has been decided.
pub(crate) fn program_uses_surface(program: &[crate::parser::ast::Stmt]) -> bool {
    detect::program_uses_tidy(program)
}

/// Injects the tidy prelude when the program references the `ext/tidy` surface,
/// leaving every other program untouched.
///
/// `force` comes from `--with-tidy` (or the codegen harness); otherwise the decision
/// is `detect::program_uses_tidy`. The prelude carries only declarations, so
/// prepending it is order-independent — PHP hoists them.
pub fn inject_if_used(
    program: Program,
    force: bool,
    default_config: &str,
    open_basedir: &str,
    clean_output: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        inject_if_used_on_compiler_stack(
            program,
            force,
            default_config,
            open_basedir,
            clean_output,
            inventory,
        )
    })
}

/// Builds the compile-time INI constants the prelude reads.
///
/// `tidy.default_config` is `PHP_INI_SYSTEM` and `open_basedir` is startup state,
/// so neither can change at runtime; embedding them as literals keeps the prelude
/// independent of the CLI/web `ini_get` surface and its module split.
fn ini_declarations(default_config: &str, open_basedir: &str, clean_output: bool) -> Vec<Stmt> {
    vec![
        function("__elephc_tidy_ini_default_config")
            .returns(TypeExpr::Str)
            .returning(e_str(default_config))
            .build(),
        function("__elephc_tidy_ini_open_basedir")
            .returns(TypeExpr::Str)
            .returning(e_str(open_basedir))
            .build(),
        function("__elephc_tidy_ini_clean_output")
            .returns(TypeExpr::Bool)
            .returning(e_bool(clean_output))
            .build(),
    ]
}

/// Starts the automatic output buffer when `tidy.clean_output` is enabled.
fn clean_output_statement() -> Stmt {
    s_if(
        e_call("__elephc_tidy_ini_clean_output", vec![]),
        vec![s_expr(e_call("ob_start", vec![e_str("ob_tidyhandler")]))],
        vec![],
        None,
    )
}

/// The body of [`inject_if_used`], run on the stack `with_compiler_stack` sized.
fn inject_if_used_on_compiler_stack(
    program: Program,
    force: bool,
    default_config: &str,
    open_basedir: &str,
    clean_output: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !force && !detect::program_uses_tidy(&program) {
        return program;
    }
    let mut combined = tidy_declarations();
    combined.extend(ini_declarations(default_config, open_basedir, clean_output));
    if clean_output {
        combined.push(clean_output_statement());
    }
    inventory.record_program("tidy", &combined);
    combined.extend(program);
    combined
}

#[cfg(test)]
mod prefix_migration {
    //! Purpose:
    //! Development-only helper that rewrites the test-only PHP fragment with the `$_` prefix
    //! that keeps synthetic method locals from colliding with user globals.
    //!
    //! Called from:
    //! - `cargo test` with `ELEPHC_TIDY_PREFIX_OUT=<path>`; inert otherwise.
    //!
    //! Key details:
    //! - Parameter names are preserved because they are part of the public call contract;
    //!   `self::$prop` variables are skipped, and every other function/method-local variable
    //!   is prefixed once.
    //! - The rewritten text is fed back through `synthetic_class::transcribe` to regenerate
    //!   `src/tidy_prelude/build`; the oracle test then compares both forms.

    use std::collections::HashSet;
    use crate::lexer::Token;

    /// Rewrites the fragment when the output path variable is present.
    #[test]
    fn dump_prefixed_fragments_on_request() {
        let Ok(out) = std::env::var("ELEPHC_TIDY_PREFIX_OUT") else { return; };
        let source = super::fragments::SRC;
        let tokens = crate::lexer::tokenize(source).expect("fragment must tokenize");
        let mut line_starts = vec![0usize];
        for (offset, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(offset + 1);
            }
        }
        let start_offset = |span: crate::span::Span| {
            line_starts[span.line as usize - 1] + span.col as usize - 1
        };
        let end_offset = |span: crate::span::Span| {
            line_starts[span.end_line as usize - 1] + span.end_column() as usize - 1
        };
        let mut replacements: Vec<(usize, usize, String)> = Vec::new();
        let mut index = 0usize;
        while index < tokens.len() {
            if !matches!(&tokens[index].0, Token::Function) {
                index += 1;
                continue;
            }
            let mut open = index + 1;
            while open < tokens.len() && !matches!(&tokens[open].0, Token::LParen) {
                open += 1;
            }
            if open == tokens.len() {
                break;
            }
            let mut close = open + 1;
            let mut depth = 1usize;
            while close < tokens.len() && depth > 0 {
                match &tokens[close].0 {
                    Token::LParen => depth += 1,
                    Token::RParen => depth -= 1,
                    _ => {}
                }
                close += 1;
            }
            let mut params = HashSet::new();
            for (token, _) in &tokens[open + 1..close.saturating_sub(1)] {
                if let Token::Variable(name) = token {
                    params.insert(name.clone());
                }
            }
            let mut body_open = close;
            while body_open < tokens.len()
                && !matches!(&tokens[body_open].0, Token::LBrace | Token::Semicolon)
            {
                body_open += 1;
            }
            if body_open == tokens.len() || matches!(&tokens[body_open].0, Token::Semicolon) {
                index = close;
                continue;
            }
            let mut body_close = body_open + 1;
            let mut body_depth = 1usize;
            while body_close < tokens.len() && body_depth > 0 {
                match &tokens[body_close].0 {
                    Token::LBrace => body_depth += 1,
                    Token::RBrace => body_depth -= 1,
                    _ => {}
                }
                body_close += 1;
            }
            for position in body_open + 1..body_close.saturating_sub(1) {
                let Token::Variable(name) = &tokens[position].0 else { continue; };
                if params.contains(name) || name.starts_with('_') {
                    continue;
                }
                if position > 0 && matches!(&tokens[position - 1].0, Token::DoubleColon) {
                    continue;
                }
                let span = tokens[position].1.span;
                replacements.push((start_offset(span), end_offset(span), format!("$_{name}")));
            }
            index = body_close;
        }
        replacements.sort_by_key(|(start, _, _)| *start);
        let mut rewritten = String::with_capacity(source.len());
        let mut cursor = 0usize;
        for (start, end, replacement) in replacements {
            rewritten.push_str(&source[cursor..start]);
            rewritten.push_str(&replacement);
            cursor = end;
        }
        rewritten.push_str(&source[cursor..]);
        std::fs::write(out, rewritten).expect("must write prefixed fragment");
    }
}

#[cfg(test)]
mod oracle_tests {
    //! Purpose:
    //! Parse-parity oracle for the built tidy surface: `build::tidy_declarations` must
    //! equal the parse of the PHP form, declaration by declaration.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Spans are stripped because a built node has none and a parsed one does; everything
    //!   else — order, types, nesting, name qualification — must match.
    //! - `ELEPHC_TIDY_ORACLE_DUMP=<dir>` writes both renderings of the first divergence.

    use super::*;
    use crate::parser::ast::{Stmt, StmtKind};

    /// The built declarations are the parse of the PHP form, node for node.
    #[test]
    fn built_declarations_match_the_php() {
        let parsed = parsed_prelude();
        let built = build::tidy_declarations();
        assert_eq!(
            built.len(),
            parsed.len(),
            "declaration COUNT differs — built {} vs parsed {}",
            built.len(),
            parsed.len()
        );
        for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
            let decl = declaration_label(parsed_stmt);
            let left = strip_spans(&format!("{built_stmt:?}"));
            let right = strip_spans(&format!("{parsed_stmt:?}"));
            if left != right {
                if let Ok(dir) = std::env::var("ELEPHC_TIDY_ORACLE_DUMP") {
                    std::fs::write(format!("{dir}/built_{decl}.txt"), left.replace("}, ", "},\n"))
                        .expect("dump built");
                    std::fs::write(
                        format!("{dir}/parsed_{decl}.txt"),
                        right.replace("}, ", "},\n"),
                    )
                    .expect("dump parsed");
                }
                panic!("built declaration `{decl}` differs from its PHP form");
            }
        }
    }

    /// Names one declaration for the oracle's failure message.
    fn declaration_label(stmt: &Stmt) -> String {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, .. }
            | StmtKind::ClassDecl { name, .. }
            | StmtKind::ExternFunctionDecl { name, .. } => name.clone(),
            _ => "<other>".to_string(),
        }
    }

    /// Removes span payloads so a synthetic node and a parsed node compare on structure.
    fn strip_spans(rendered: &str) -> String {
        let mut cleaned = String::with_capacity(rendered.len());
        let mut rest = rendered;
        while let Some(at) = rest.find("Span {") {
            cleaned.push_str(&rest[..at]);
            cleaned.push_str("Span");
            let after = &rest[at..];
            let close = after.find('}').map(|end| end + 1).unwrap_or(after.len());
            rest = &after[close..];
        }
        cleaned.push_str(rest);
        cleaned
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Structural checks for the tidy prelude declarations.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - The source is parsed at test time; a syntax drift fails here before the
    //!   pipeline ever injects it.

    use super::*;
    use crate::parser::ast::StmtKind;

    /// The prelude declares the two classes, 24 functions and `ob_tidyhandler`.
    #[test]
    fn declares_the_full_surface() {
        let mut classes = Vec::new();
        let mut functions = Vec::new();
        for stmt in tidy_declarations() {
            match &stmt.kind {
                StmtKind::ClassDecl { name, .. } => classes.push(name.clone()),
                StmtKind::FunctionDecl { name, .. } => functions.push(name.clone()),
                StmtKind::ExternFunctionDecl { .. } => {}
                other => panic!("unexpected tidy prelude statement {other:?}"),
            }
        }
        classes.sort();
        assert_eq!(classes, vec!["tidy", "tidyNode"]);
        assert!(functions.contains(&"tidy_parse_string".to_string()));
        assert!(functions.contains(&"tidy_get_body".to_string()));
        assert!(functions.contains(&"ob_tidyhandler".to_string()));
        let public = functions
            .iter()
            .filter(|name| name.starts_with("tidy_"))
            .count();
        assert_eq!(public, 24);
    }

    /// Every bridge entry point the prelude calls is declared in its extern block.
    #[test]
    fn extern_block_covers_every_called_helper() {
        let mut externs = 0;
        for stmt in tidy_declarations() {
            if matches!(stmt.kind, StmtKind::ExternFunctionDecl { .. }) {
                externs += 1;
            }
        }
        assert!(externs >= 50, "unexpected extern count {externs}");
    }

    /// `--with-tidy` force-injects the surface for opaque dynamic eval source.
    #[test]
    fn forced_injection_keeps_the_whole_surface() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let injected = inject_if_used(
            Vec::new(),
            true,
            "",
            "",
            false,
            &mut inventory,
        );
        assert!(injected
            .iter()
            .any(|stmt| matches!(&stmt.kind, StmtKind::ClassDecl { name, .. } if name == "tidy")));
        assert!(injected.iter().any(|stmt| {
            matches!(&stmt.kind, StmtKind::FunctionDecl { name, .. } if name == "tidy_parse_string")
        }));
    }

    /// `tidyNode` is final and its constructor is private.
    #[test]
    fn tidy_node_is_final_with_a_private_constructor() {
        let class = tidy_declarations()
            .into_iter()
            .find(|stmt| matches!(&stmt.kind, StmtKind::ClassDecl { name, .. } if name == "tidyNode"))
            .expect("tidyNode must be declared");
        let StmtKind::ClassDecl {
            methods, is_final, ..
        } = &class.kind
        else {
            unreachable!("filtered above");
        };
        assert!(*is_final);
        let ctor = methods
            .iter()
            .find(|method| method.name == "__construct")
            .expect("tidyNode must declare a constructor");
        assert_eq!(ctor.visibility, crate::parser::ast::Visibility::Private);
    }
}
