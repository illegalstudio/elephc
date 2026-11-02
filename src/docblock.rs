//! Purpose:
//! Reads PHPStan/Psalm generic annotations out of `/** … */` doc comments and applies them to
//! the declaration that follows, so an annotated file compiles as generic without being
//! rewritten in elephc's native syntax.
//!
//! Called from:
//! - `crate::source::finalize_physical_program`, per physical file, after the strict audit.
//!
//! Key details:
//! - This is the portable surface, and it is FUNCTION-level only. `@template T`,
//!   `@param array<T> $a` and `@return T` above a free function mean what
//!   `function f<T>(array<T> $a): T` means and lower to the same monomorphic instantiations.
//! - A PHPDoc template must NEVER remove or rename a php symbol. The function therefore stays in
//!   the program, untouched, under its written name with its written types (the reading php
//!   itself has), so `function_exists`, string callables and reflection see it. The annotated
//!   reading is a hidden TWIN declaration (`<name>` + [`crate::generics::DOC_TWIN_SUFFIX`]) that
//!   the checker consults only at a direct call, to instantiate per call.
//! - METHOD-level, CLASS-level (`@template`, `@extends X<T>`, `@implements I<T>`, interfaces and
//!   traits too) and constructor-level templates are NOT adopted. A method is overridable and
//!   reachable by name, reflection, interface contracts and vtable slots, none of which can see
//!   an instantiation; a class template renamed `X` to `X<int>`, so `class_exists('X')`,
//!   `new $class` and `get_class()` diverged from php. Written syntax (`class Box<T>`,
//!   `function f<T>()`, `public function m<T>()`) still declares a template.
//! - The lexer discards comments, so the doc comments are recovered from the SOURCE TEXT and
//!   matched to declaration tokens by line and column. This runs per physical file before
//!   include resolution combines source files, so the original coordinates remain authoritative.
//! - Types inside an annotation go through the ordinary type grammar (`parse_type_expr`), so
//!   `array<string, Foo>` means one thing in the language and in a docblock, and gaining a
//!   type form gains it in both.

mod bindings;
pub mod erasure;
mod members;

use std::collections::HashMap;

use crate::parser::ast::{Program, Stmt, StmtKind, TypeExpr, TypeParam, Variance};

/// The generic annotations one doc comment carries.
#[derive(Debug, Default, Clone)]
struct DocBlock {
    /// `@template T`, `@template T of Foo`, `@template T = string`.
    type_params: Vec<TypeParam>,
    /// `@param <type> $name`, by parameter name without the `$`.
    params: HashMap<String, TypeExpr>,
    /// `@return <type>`.
    return_type: Option<TypeExpr>,
}

impl DocBlock {
    /// Returns whether this doc comment says anything this pass acts on.
    ///
    /// A `@param`/`@return` with no `@template` is left alone: it carries no type parameter, and
    /// treating it as a type declaration would silently turn every annotated PHP file in the
    /// world into one elephc type-checks differently.
    fn declares_generics(&self) -> bool {
        !self.type_params.is_empty()
    }

    /// Returns whether this doc comment carries nothing this pass could ever act on.
    ///
    /// Properties and ordinary methods can refer to a class template without declaring their
    /// own `@template`, so [`Self::declares_generics`] cannot filter out those annotations.
    fn is_empty(&self) -> bool {
        self.type_params.is_empty()
            && self.params.is_empty()
            && self.return_type.is_none()
    }
}

/// Applies generic doc-comment annotations in `source` to the declarations of `program`.
///
/// A function or method that already carries native type parameters is left untouched: written
/// syntax wins over an annotation, so a file can migrate one declaration at a time.
pub fn apply(program: Program, source: &str) -> Program {
    let blocks = collect(source);
    if blocks.is_empty() {
        return program;
    }
    program
        .into_iter()
        .flat_map(|stmt| apply_to_stmt(stmt, &blocks))
        .collect()
}

/// Applies the doc comment that ends just above `stmt`, if any.
///
/// A function with an adopted `@template` comes back as TWO statements: itself, untouched, and
/// its annotated twin (see [`doc_twin`]). Everything else comes back as itself.
fn apply_to_stmt(mut stmt: Stmt, blocks: &HashMap<(u32, u32), DocBlock>) -> Vec<Stmt> {
    match &mut stmt.kind {
        StmtKind::FunctionDecl { .. } => {
            let twin = blocks
                .get(&(stmt.span.line, stmt.span.col))
                .and_then(|block| doc_twin(&stmt, block));
            let mut out = vec![stmt];
            out.extend(twin);
            out
        }
        StmtKind::NamespaceBlock { body, .. } => {
            let nested = std::mem::take(body);
            *body = nested
                .into_iter()
                .flat_map(|inner| apply_to_stmt(inner, blocks))
                .collect();
            vec![stmt]
        }
        _ => vec![stmt],
    }
}

/// Builds the annotated twin of a function whose doc comment declares `@template`.
///
/// The twin is the template the checker instantiates at a direct call. The original declaration
/// is left exactly as written: it is the php-visible symbol. Written type parameters win over an
/// annotation, and a template no read annotation or written type mentions is not adopted.
fn doc_twin(original: &Stmt, block: &DocBlock) -> Option<Stmt> {
    let mut twin = original.clone();
    let StmtKind::FunctionDecl {
        name,
        type_params,
        params,
        variadic,
        variadic_type,
        return_type,
        ..
    } = &mut twin.kind
    else {
        return None;
    };
    if !block.declares_generics() || !type_params.is_empty() {
        return None;
    }
    *type_params = members::mentioned_templates(
        block,
        params,
        variadic.as_ref(),
        variadic_type.as_ref(),
        return_type.as_ref(),
    );
    if type_params.is_empty() {
        return None;
    }
    let in_scope: Vec<String> = type_params.iter().map(|param| param.name.clone()).collect();
    for (param_name, declared, _, _) in params.iter_mut() {
        if let Some(annotated) = block.params.get(param_name) {
            // The annotation REPLACES the written hint in the twin: `array $a` with
            // `@param array<T> $a` is the more precise of the two.
            members::apply_type(declared, annotated, &in_scope);
        }
    }
    if let Some(annotated) = variadic.as_ref().and_then(|v| block.params.get(v)) {
        members::apply_type(variadic_type, annotated, &in_scope);
    }
    if let Some(annotated) = &block.return_type {
        members::apply_stored_type(return_type, annotated, &in_scope);
    }
    name.push_str(crate::generics::DOC_TWIN_SUFFIX);
    Some(twin)
}

/// Extracts generic-bearing doc comments keyed by their declaration token positions.
fn collect(source: &str) -> HashMap<(u32, u32), DocBlock> {
    bindings::collect(source)
}

/// Parses the annotations of one doc comment, one opening tag per line.
///
/// A tall comment puts one tag on each line, so every `@template`, `@param`, and `@return` is
/// read. A short comment that packs several tags onto one line
/// (`/** @template T @param T $x @return T */`) reads only the tag that opens that line; later
/// `@tag`s on the same line are that tag's description and are not parsed.
fn parse_block(lines: &[&str]) -> DocBlock {
    let mut block = DocBlock::default();
    for line in lines {
        let stripped = strip_comment_markers(line);
        for segment in tag_segments(&stripped) {
            if let Some((rest, variance)) = strip_template_tag(segment) {
                if let Some(param) = parse_template(rest, variance) {
                    if !block.type_params.iter().any(|p| p.name == param.name) {
                        block.type_params.push(param);
                    }
                }
            } else if let Some(rest) = segment.strip_prefix("@param ") {
                if let Some((name, ty)) = parse_param(rest) {
                    // First read wins: a tag MENTIONED in a later description (`see @param U $w`)
                    // must not retype the parameter the earlier annotation bound.
                    block.params.entry(name).or_insert(ty);
                }
            } else if let Some(rest) = segment.strip_prefix("@return ") {
                // First read wins: a later `@return` inside a description (`see @return mixed`)
                // must not widen or replace the return type the earlier annotation bound.
                if block.return_type.is_none() {
                    block.return_type = parse_type(rest.trim());
                }
            }
        }
    }
    block
}

/// Returns the tag that opens one doc-comment line, as a single segment.
///
/// The whole trimmed line is that one segment. A later `@tag` on the same line stays inside the
/// opening tag's description and does not start another segment. A line that does not open with
/// a tag returns nothing. An unread opener such as `@deprecated` still counts: it opens the line,
/// so the rest of the line is not scanned for a tag this pass reads.
fn tag_segments(line: &str) -> Vec<&str> {
    const TAGS: [&str; 6] = [
        "@template-covariant ",
        "@template-contravariant ",
        "@template ",
        "@param ",
        "@return ",
        "@var ",
    ];
    // One segment per line: the tag that OPENS the line. A `@tag` that follows an annotation's
    // description is description — `@param T $v see @return mixed` must not retype the return,
    // and phpDocumentor itself treats the rest of the line as the opening tag's content. The
    // opening tag may be one this pass does not read (`@deprecated`): it still opens the line.
    let leading_tag = line.starts_with('@')
        && line[1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    let recognized_at_zero = TAGS.iter().any(|tag| line.starts_with(tag));
    if !leading_tag && !recognized_at_zero {
        return Vec::new();
    }
    vec![line.trim_end()]
}

/// Strips one doc-comment line down to the annotation it carries.
///
/// A doc comment has two shapes and a member almost always uses the short one: the opener, the
/// annotation and the closer on a single line (`/** @var T */`). Trimming only the continuation
/// `*` reads the tall shape and silently ignores the short one, so both markers come off here,
/// and the leading `*` after them, in that order.
fn strip_comment_markers(line: &str) -> String {
    elephc_generics::strip_comment_markers(line)
}

/// Splits a `@template` tag from its body, reading the variance the tag name carries.
///
/// PHPStan spells variance in the TAG (`@template-covariant T`) where native syntax spells it on
/// the parameter (`+T`). Both reach the same `TypeParam`. The hyphenated forms are tested first
/// only for clarity; they cannot collide with `@template ` because that one requires the space.
fn strip_template_tag(trimmed: &str) -> Option<(&str, Variance)> {
    elephc_generics::strip_template_tag(trimmed).map(|(rest, variance)| (rest, match variance {
        1 => Variance::Covariant, -1 => Variance::Contravariant, _ => Variance::Invariant,
    }))
}

/// Parses `T`, `T of Foo`, `T = string`, or `T of Foo = Foo`.
///
/// `of` is PHPStan's spelling of the bound that native syntax writes `T : Foo`. Both reach the
/// same `TypeParam`, because they are the same idea in two surfaces.
fn parse_template(rest: &str, variance: Variance) -> Option<TypeParam> {
    let mut rest = rest.trim();
    let name_end = rest
        .find(char::is_whitespace)
        .unwrap_or(rest.len());
    let name = rest[..name_end].trim().to_string();
    if name.is_empty() || !is_identifier(&name) {
        return None;
    }
    rest = rest[name_end..].trim();
    let mut bound = None;
    if let Some(after) = rest.strip_prefix("of ") {
        let after = after.trim();
        // The bound ends at a default `=` or at the NEXT TAG on the same line (one segment per
        // line carries the line's description after its tags).
        let bound_end = after
            .find(['=', '@'])
            .unwrap_or(after.len());
        // A bound this reader cannot parse (Symfony's `@template B of self::*_REFERENCE`) is not
        // "no bound": dropping it would read `B` wider than it was written. The template is not
        // adopted at all, and php — which never reads it — sees the same declaration.
        bound = Some(parse_type(after[..bound_end].trim())?);
        rest = after[bound_end..].trim();
    }
    let default = match rest.strip_prefix('=') {
        Some(value) => Some(parse_type(value.trim())?),
        None => None,
    };
    Some(TypeParam {
        name,
        bound,
        default,
        variance,
        doc_annotated: true,
    })
}

/// Parses `<type> $name`, returning the parameter name without its `$`.
fn parse_param(rest: &str) -> Option<(String, TypeExpr)> {
    let rest = rest.trim();
    let dollar = rest.find('$')?;
    // `@param T ...$xs` annotates the VARIADIC, whose type is `T` — the `...` is the parameter's
    // spelling, not part of the type, and leaving it in makes the whole annotation unparseable.
    let ty = parse_type(rest[..dollar].trim().trim_end_matches("...").trim())?;
    let name: String = rest[dollar + 1..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        return None;
    }
    Some((name, ty))
}

/// Parses one annotation type through the ordinary type grammar.
///
/// Reusing the language's own parser is what keeps the two surfaces from drifting: a docblock
/// cannot spell a type the language cannot, and a new type form is gained in both at once. An
/// annotation the grammar rejects — a PHPStan form elephc has no type for, such as
/// `non-empty-list<T>` — simply yields `None` and is ignored, rather than failing the compile
/// of a file that is valid PHP.
fn parse_type(text: &str) -> Option<TypeExpr> {
    if text.is_empty() {
        return None;
    }
    let source = format!("<?php {};", text);
    let tokens = crate::lexer::tokenize(&source).ok()?;
    let mut pos = 1usize;
    let span = tokens.get(pos).map(|(_, meta)| meta.span)?;
    let parsed = crate::parser::stmt::parse_type_expr(&tokens, &mut pos, span).ok()?;
    // The whole annotation must be consumed, or `array<T>|null` would silently become
    // `array<T>` and the annotation would mean less than it says.
    match tokens.get(pos).map(|(token, _)| token) {
        Some(crate::lexer::Token::Semicolon) => Some(parsed),
        _ => None,
    }
}

/// Returns whether `name` is a plain identifier, which a type parameter name must be.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Name;

    /// Parses `source` and applies its doc comments, the way `finalize_physical_program` does.
    fn program_of(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("tokenizes");
        let program = crate::parser::parse(&tokens).expect("parses");
        apply(program, source)
    }

    /// The hidden annotated twin of the function named `name` (see [`doc_twin`]).
    fn twin<'a>(program: &'a Program, name: &str) -> &'a Stmt {
        let twin_name = format!("{name}{}", crate::generics::DOC_TWIN_SUFFIX);
        program
            .iter()
            .flat_map(|stmt| match &stmt.kind {
                StmtKind::NamespaceBlock { body, .. } => body.iter().collect::<Vec<_>>(),
                _ => vec![stmt],
            })
            .find(|stmt| matches!(&stmt.kind, StmtKind::FunctionDecl { name, .. } if *name == twin_name))
            .expect("the function has an annotated twin")
    }

    /// Collects the sole generic docblock in a source fixture.
    fn block_of(source: &str) -> DocBlock {
        let blocks = collect(source);
        assert_eq!(blocks.len(), 1, "expected exactly one generic doc comment");
        blocks.into_values().next().expect("one block")
    }

    /// Reads a template and the parameter and return annotations that refer to it.
    #[test]
    fn reads_a_template_param_and_return() {
        let block = block_of(
            "<?php\n/**\n * @template T\n * @param array<T> $a\n * @return T\n */\nfunction f(array $a) {}\n",
        );
        assert_eq!(block.type_params.len(), 1);
        assert_eq!(block.type_params[0].name, "T");
        assert_eq!(
            block.params.get("a"),
            Some(&TypeExpr::Array(Box::new(TypeExpr::Named(
                Name::unqualified("T")
            ))))
        );
        assert_eq!(
            block.return_type,
            Some(TypeExpr::Named(Name::unqualified("T")))
        );
    }

    /// A tag MENTIONED in a description must not REPLACE an annotation already read: a
    /// `@param T $v see @return mixed` line kept `@return T` (first read wins) and the compile
    /// still refused a returned string.
    #[test]
    fn a_described_tag_does_not_replace_the_annotation_before_it() {
        let block = block_of(
            "<?php\n/**\n * @template T of int\n * @param T $v see @return mixed\n * @return T\n */\nfunction f($v): int { return 1; }\n",
        );
        assert_eq!(block.type_params.len(), 1);
        assert_eq!(block.params.get("v"), Some(&TypeExpr::Named(Name::unqualified("T"))));
        assert_eq!(block.return_type, Some(TypeExpr::Named(Name::unqualified("T"))));
    }

    /// A tag MENTIONED inside an annotation's own description stays prose: the description may
    /// carry sentence punctuation, and the split only crosses tag territory without it.
    #[test]
    fn a_tag_mentioned_in_a_description_stays_prose() {
        let block = block_of(
            "<?php\n/**\n * Sums a number.\n * @param int $v the value; the shape mirrors @template T in the docs.\n */\nfunction f2(int $v): int { return $v + 1; }\n",
        );
        assert_eq!(block.type_params.len(), 0, "a mentioned tag must not bind");
        assert!(block.params.contains_key("v"), "the real annotation still reads");
    }

    /// A one-line block reads its OPENING tag only: a later `@tag` on the same line is the
    /// opening tag's description (phpDocumentor semantics), so the `@return` here is not read.
    #[test]
    fn one_line_form_reads_its_opening_tag() {
        let block = block_of(
            "<?php\n/** @param string ...$args @return int */\nfunction f(...$args): int { return 1; }\n",
        );
        assert!(block.params.contains_key("args"), "the opening @param still reads");
        assert_eq!(block.return_type, None, "a same-line @tag is description, not a segment");
    }

    /// Prose that MENTIONS a tag is not an annotation: only a line that begins with one may
    /// contribute tags. A description like "Demonstrates @template T syntax." used to split into
    /// an `@template` segment and inject a stray `T` into the block.
    #[test]
    fn prose_mentioning_a_tag_is_not_an_annotation() {
        let block = block_of(
            "<?php\n/**\n * Demonstrates @template T syntax.\n * @param int $v\n */\nfunction f($v) {}\n",
        );
        assert_eq!(block.type_params.len(), 0, "prose must not become a template");
    }

    /// A line opening with an UNREAD annotation (`@deprecated`) contributes nothing: the
    /// compact one-line form parses its opening tag only, and an unread opening tag reads
    /// nothing at all. Write the tags on separate lines.
    #[test]
    fn an_unread_leading_annotation_contributes_nothing() {
        let blocks = collect(
            "<?php\n/** @deprecated @template T of int @param T $v @return T */\nfunction f($v) {}\n",
        );
        assert!(blocks.is_empty(), "an unread opening tag reads nothing");
    }

    /// A one-line block reads its OPENING tag only — here `@template T of int` (with its
    /// bound); the `@param`/`@return` after it are the opening tag's description. Declare the
    /// tags on separate lines to bind them all.
    #[test]
    fn one_line_form_reads_only_its_opening_tag() {
        let block = block_of(
            "<?php\n/** @template T of int @param T $v @return T */\nfunction f($v) { return $v; }\n",
        );
        assert_eq!(block.type_params.len(), 1);
        assert_eq!(block.type_params[0].name, "T");
        assert_eq!(block.type_params[0].bound, Some(TypeExpr::Int));
        assert!(block.params.is_empty(), "same-line tags are description");
        assert_eq!(block.return_type, None);
    }

    /// `of` is PHPStan's bound spelling; it reaches the same `TypeParam` as native `T : Foo`.
    #[test]
    fn reads_an_of_bound_and_a_default() {
        let block = block_of(
            "<?php\n/**\n * @template T of Entity\n * @template K = string\n */\nfunction f($a) {}\n",
        );
        assert_eq!(
            block.type_params[0].bound,
            Some(TypeExpr::Named(Name::unqualified("Entity")))
        );
        assert_eq!(block.type_params[1].default, Some(TypeExpr::Str));
    }

    /// A blank line between the comment and the declaration is idiomatic and must not break the
    /// association.
    #[test]
    fn skips_blank_lines_before_the_declaration() {
        let blocks = collect("<?php\n/**\n * @template T\n */\n\n\nfunction f($a) {}\n");
        assert!(blocks.contains_key(&(7, 1)), "got keys {:?}", blocks.keys());
    }

    /// A doc comment with no `@template` carries no type parameter, so this pass leaves the
    /// declaration alone — otherwise every annotated PHP file in the world would start
    /// type-checking differently.
    ///
    /// The block is still COLLECTED, because a class member's doc comment never carries a
    /// `@template` of its own and would be thrown away with it; what refuses it is the apply
    /// site.
    #[test]
    fn ignores_a_doc_comment_without_a_template() {
        let program =
            program_of("<?php\n/**\n * @param int $a\n * @return int\n */\nfunction f($a) {}\n");
        let StmtKind::FunctionDecl {
            type_params,
            params,
            return_type,
            ..
        } = &program[0].kind
        else {
            panic!("expected a function declaration");
        };
        assert!(type_params.is_empty());
        assert_eq!(params[0].1, None);
        assert_eq!(*return_type, None);
    }

    /// An annotation the language has no type for is ignored rather than failing the compile.
    #[test]
    fn ignores_an_unparseable_annotation_type() {
        let block = block_of(
            "<?php\n/**\n * @template T\n * @param non-empty-list<T> $a\n */\nfunction f($a) {}\n",
        );
        assert!(block.params.is_empty());
    }

    /// A trailing union member must not be silently dropped.
    #[test]
    fn requires_the_whole_annotation_to_parse() {
        assert_eq!(parse_type("int|string"), parse_type("int|string"));
        assert!(parse_type("int|").is_none());
    }

    /// Applies generic function annotations to the TWIN, and leaves the function as written.
    #[test]
    fn applies_the_annotation_to_the_declaration() {
        let source =
            "<?php\n/**\n * @template T\n * @param array<T> $a\n * @return T\n */\nfunction f(array $a) { return $a[0]; }\n";
        let program = program_of(source);
        match &program[0].kind {
            StmtKind::FunctionDecl { name, type_params, return_type, .. } => {
                assert_eq!(name, "f", "the php symbol keeps its written name");
                assert!(type_params.is_empty(), "and stays an ordinary function");
                assert_eq!(*return_type, None);
            }
            other => panic!("expected function decl, got {:?}", other),
        }
        match &twin(&program, "f").kind {
            StmtKind::FunctionDecl {
                type_params,
                params,
                return_type,
                ..
            } => {
                assert_eq!(type_params.len(), 1);
                assert_eq!(type_params[0].name, "T");
                assert_eq!(
                    params[0].1,
                    Some(TypeExpr::Array(Box::new(TypeExpr::Named(
                        Name::unqualified("T")
                    ))))
                );
                assert_eq!(
                    return_type.as_ref(),
                    Some(&TypeExpr::Named(Name::unqualified("T")))
                );
            }
            other => panic!("expected function decl, got {:?}", other),
        }
    }

    /// Written syntax wins over an annotation, so a file can migrate one function at a time.
    #[test]
    fn native_type_params_win_over_an_annotation() {
        let source =
            "<?php\n/**\n * @template T\n * @return T\n */\nfunction f<U>(U $a): U { return $a; }\n";
        let tokens = crate::lexer::tokenize(source).expect("tokenizes");
        let program = crate::parser::parse(&tokens).expect("parses");
        let program = apply(program, source);
        match &program[0].kind {
            StmtKind::FunctionDecl { type_params, .. } => {
                assert_eq!(type_params.len(), 1);
                assert_eq!(type_params[0].name, "U");
            }
            other => panic!("expected function decl, got {:?}", other),
        }
    }

    /// A member's doc comment is the SHORT shape — opener, annotation and closer on one line —
    /// and trimming only the continuation `*` reads the tall shape and silently ignores it.
    #[test]
    fn reads_a_single_line_doc_comment() {
        let block = block_of("<?php\n/** @template T */\nfunction f($a) {}\n");
        assert_eq!(block.type_params.len(), 1);
        assert_eq!(block.type_params[0].name, "T");
    }

    /// Code after the closing marker belongs to the same docblock, without losing its tags.
    #[test]
    fn binds_a_declaration_on_the_docblock_closing_line() {
        for source in [
            "<?php\n/** @template T */ function id(T $v): T { return $v; }\n",
            "<?php\n/**\n * @template T\n */ function id(T $v): T { return $v; }\n",
        ] {
            let program = program_of(source);
            let StmtKind::FunctionDecl { type_params, .. } = &twin(&program, "id").kind else {
                panic!("expected a function");
            };
            assert_eq!(type_params.len(), 1);
            assert_eq!(type_params[0].name, "T");
        }
    }

    /// A class-level `@template` is not adopted, so it gives neither the class nor a same-line
    /// method a template declaration.
    #[test]
    fn does_not_copy_a_class_template_to_a_same_line_method() {
        let program = program_of("<?php\n/** @template T */\nclass Box { public function __construct(public T $value) {} public function id(T $v): T { return $v; } }\n");
        let StmtKind::ClassDecl { generics, methods, .. } = &program[0].kind else {
            panic!("expected a class");
        };
        assert!(generics.is_none());
        assert!(methods.iter().all(|method| method.type_params.is_empty()));
    }

    /// A method's own `@template` is NOT adopted, wherever its docblock sits: a method is
    /// overridable and reachable by name, so no instantiation may stand in for it.
    #[test]
    fn method_templates_are_not_adopted() {
        let program = program_of("<?php\n/* café */ class C { /** @template T */ #[Marker(\"]\")] public function id(T $v): T { return $v; } /** @template U */ public function other(U $v): U { return $v; } }\n");
        let StmtKind::ClassDecl { generics, methods, .. } = &program[0].kind else {
            panic!("expected a class");
        };
        assert!(generics.is_none());
        assert!(methods[0].type_params.is_empty());
        assert!(methods[1].type_params.is_empty());
        assert_eq!(methods[0].name, "id");
    }

    /// Apparent annotations inside strings or line comments never become declaration metadata.
    #[test]
    fn ignores_docblock_markers_inside_strings_and_line_comments() {
        let program = program_of("<?php\n$x = '/** @template T */';\n// /** @template U */\nclass C {}\n");
        let StmtKind::ClassDecl { generics, .. } = &program[1].kind else {
            panic!("expected a class");
        };
        assert!(generics.is_none());
    }

    /// Enum methods are not adopted either.
    #[test]
    fn enum_method_templates_are_not_adopted() {
        let program = program_of("<?php\nenum Id {\n case A;\n /**\n  * @template T of int\n  * @param T $value\n  * @return T\n  */\n public function id($value) { return $value; }\n}\n");
        let StmtKind::EnumDecl { methods, .. } = &program[0].kind else {
            panic!("expected an enum");
        };
        assert!(methods[0].type_params.is_empty());
        assert_eq!(methods[0].params[0].1, None);
    }

    /// `@template` on a class is NOT adopted: php knows one class under its written name, so the
    /// class stays non-generic and its members' annotations naming the class parameter are not
    /// read.
    #[test]
    fn a_class_does_not_adopt_its_template_or_member_annotations() {
        let program = program_of(concat!(
            "<?php\n",
            "/** @template T */\n",
            "class Box {\n",
            "    /** @var T */\n",
            "    private $value;\n",
            "    /** @param T $value */\n",
            "    public function __construct($value) { $this->value = $value; }\n",
            "    /** @return T */\n",
            "    public function get() { return $this->value; }\n",
            "}\n",
        ));
        let StmtKind::ClassDecl {
            generics,
            properties,
            methods,
            ..
        } = &program[0].kind
        else {
            panic!("expected a class declaration");
        };
        assert!(generics.is_none(), "a class-level @template makes no template");
        assert_eq!(properties[0].type_expr, None);
        assert_eq!(methods[0].params[0].1, None);
        assert_eq!(methods[1].return_type, None);
        assert!(methods.iter().all(|method| method.type_params.is_empty()));
    }

    /// `@template` on the class must not promote every annotation in the body into a type
    /// declaration the compiler enforces: only one that MENTIONS a type parameter is honoured.
    #[test]
    fn a_member_annotation_naming_no_type_parameter_is_left_alone() {
        let program = program_of(concat!(
            "<?php\n",
            "/** @template T */\n",
            "class Box {\n",
            "    /** @param int $n */\n",
            "    public function set($n) {}\n",
            "}\n",
        ));
        let StmtKind::ClassDecl { methods, .. } = &program[0].kind else {
            panic!("expected a class declaration");
        };
        assert_eq!(methods[0].params[0].1, None);
    }

    /// A constructor's `@template` is not adopted, so neither the parameter nor the promoted
    /// property it declares is retyped and the class stays a plain class.
    #[test]
    fn a_constructor_template_adopts_nothing() {
        let program = program_of(concat!(
            "<?php\n",
            "class Box {\n",
            "    /**\n",
            "     * @template T\n",
            "     * @param T $value\n",
            "     */\n",
            "    public function __construct(private $value) {}\n",
            "}\n",
        ));
        let StmtKind::ClassDecl {
            generics,
            properties,
            methods,
            ..
        } = &program[0].kind
        else {
            panic!("expected a class declaration");
        };
        assert!(generics.is_none());
        assert!(methods[0].type_params.is_empty());
        assert_eq!(methods[0].params[0].1, None);
        assert!(properties[0].is_promoted);
        assert_eq!(properties[0].type_expr, None);
    }

    /// Class-level `@implements` / `@template` are not adopted: the class keeps exactly what was
    /// written, with no `GenericDecl`.
    #[test]
    fn implements_annotations_are_not_adopted() {
        let program = program_of(concat!(
            "<?php\n",
            "/**\n",
            " * @template T\n",
            " * @implements Reader<T>\n",
            " */\n",
            "class Box implements Countable, Reader {}\n",
        ));
        let StmtKind::ClassDecl { generics, .. } = &program[0].kind else {
            panic!("expected a class declaration");
        };
        assert!(generics.is_none());
    }

    /// `@extends Base<int>` is not adopted: the class stays a plain class under its written name.
    #[test]
    fn extends_annotation_is_not_adopted() {
        let program = program_of(concat!(
            "<?php\n",
            "/** @extends Holder<int> */\n",
            "class IntHolder extends Holder {}\n",
        ));
        let StmtKind::ClassDecl { generics, .. } = &program[0].kind else {
            panic!("expected a class declaration");
        };
        assert!(generics.is_none());
    }

    /// An interface's `@template` / `@extends` are not adopted either, nor are its members'
    /// annotations naming the interface parameter.
    #[test]
    fn an_interface_does_not_adopt_its_template_or_inherited_arguments() {
        let program = program_of(concat!(
            "<?php\n",
            "/**\n",
            " * @template T\n",
            " * @extends Reader<T>\n",
            " */\n",
            "interface Stream extends Reader {\n",
            "    /** @return T */\n",
            "    public function next();\n",
            "}\n",
        ));
        let StmtKind::InterfaceDecl {
            generics, methods, ..
        } = &program[0].kind
        else {
            panic!("expected an interface declaration");
        };
        assert!(generics.is_none());
        assert_eq!(methods[0].return_type, None);
    }

    /// Written syntax wins over an annotation, so a file can migrate one class at a time — and
    /// the test is whether the parser built a `GenericDecl` at all.
    #[test]
    fn native_class_type_params_win_over_an_annotation() {
        let program = program_of(concat!(
            "<?php\n",
            "/** @template T */\n",
            "class Box<U> {\n",
            "    /** @var T */\n",
            "    private $value;\n",
            "}\n",
        ));
        let StmtKind::ClassDecl {
            generics,
            properties,
            ..
        } = &program[0].kind
        else {
            panic!("expected a class declaration");
        };
        let generics = generics.as_ref().expect("written type parameters");
        assert_eq!(generics.type_params[0].name, "U");
        // `T` is not in scope, so the member annotation names nothing and is left alone.
        assert_eq!(properties[0].type_expr, None);
    }

    /// Declarations inside a `namespace { }` block are nested statements, and a doc comment above
    /// one is still the doc comment of that declaration.
    #[test]
    fn applies_inside_a_namespace_block() {
        let program = program_of(concat!(
            "<?php\n",
            "namespace App {\n",
            "    /** @template T */\n",
            "    function id(T $v): T { return $v; }\n",
            "}\n",
        ));
        let StmtKind::FunctionDecl { type_params, .. } = &twin(&program, "id").kind else {
            panic!("expected a function declaration");
        };
        assert_eq!(type_params[0].name, "T");
    }

    /// `@param T ...$xs` annotates the variadic, whose type is `T`: the `...` is the parameter's
    /// spelling and leaving it in makes the whole annotation unparseable.
    #[test]
    fn reads_a_variadic_annotation() {
        let program = program_of(concat!(
            "<?php\n",
            "/**\n",
            " * @template T\n",
            " * @param T ...$xs\n",
            " */\n",
            "function f(...$xs) {}\n",
        ));
        let StmtKind::FunctionDecl { variadic_type, .. } = &twin(&program, "f").kind else {
            panic!("expected a function declaration");
        };
        assert_eq!(*variadic_type, Some(TypeExpr::Named(Name::unqualified("T"))));
    }

    /// Tagless files retain generic annotations across attributes with nonstructural brackets.
    #[test]
    fn applies_generic_docblocks_after_attributes_in_lfc_sources() {
        let source = "/** @template T */\n#[Marker(\n    ']'\n)]\nfunction id(T $v): T { return $v; }";
        let tokens = crate::lexer::tokenize_with_mode(source, crate::source::SourceMode::Lfc)
            .expect("tokenizes");
        let program = apply(crate::parser::parse(&tokens).expect("parses"), source);
        let StmtKind::FunctionDecl { type_params, .. } = &twin(&program, "id").kind else {
            panic!("expected a function declaration");
        };
        assert_eq!(type_params[0].name, "T");
    }

    /// A method's own template is not adopted, with or without a class template: annotations
    /// stay what php reads, and the class stays non-generic.
    #[test]
    fn a_methods_own_template_is_not_adopted() {
        let program = program_of(concat!(
            "<?php\nclass C {\n",
            "/**\n * @template T of Entity\n * @template U = string\n",
            " * @param T $value\n * @param U ...$labels\n * @return int\n */\n",
            "public function id($value, ...$labels) { return 1; }\n}\n",
        ));
        let StmtKind::ClassDecl { generics, methods, .. } = &program[0].kind else {
            panic!("expected a class declaration");
        };
        assert!(generics.is_none());
        let method = &methods[0];
        assert!(method.type_params.is_empty());
        assert_eq!(method.params[0].1, None);
        assert_eq!(method.variadic_type, None);
        assert_eq!(method.return_type, None);
    }

    /// A method `@template` only an unread tag names (Symfony's `setParent()` and its
    /// `@psalm-this-out static<TNewParent>`) is not adopted: it would bind nothing at a call.
    #[test]
    fn a_method_template_no_read_annotation_mentions_is_not_adopted() {
        let program = program_of(concat!(
            "<?php\nclass C {\n",
            "/**\n * @template TNewParent of object\n * @psalm-this-out static<TNewParent>\n",
            " * @return $this\n */\n",
            "public function setParent(object $parent): static { return $this; }\n}\n",
        ));
        let StmtKind::ClassDecl { methods, .. } = &program[0].kind else {
            panic!("expected a class declaration");
        };
        assert!(methods[0].type_params.is_empty(), "an unmentioned template binds nothing");
    }

    /// A template whose bound this reader cannot parse is not adopted, and neither is the
    /// `@param` naming it: Symfony's `ContainerInterface::get` declares
    /// `@template B of self::*_REFERENCE` over a native `int $invalidBehavior`.
    #[test]
    fn an_unparseable_template_bound_adopts_nothing() {
        let program = program_of(concat!(
            "<?php\ninterface C {\n",
            "/**\n * @template O of object\n * @template B of self::*_REFERENCE\n",
            " * @param class-string<O> $id\n * @param B $mode\n */\n",
            "public function get(string $id, int $mode = 1): ?object;\n}\n",
        ));
        let StmtKind::InterfaceDecl { methods, .. } = &program[0].kind else {
            panic!("expected an interface declaration");
        };
        let method = &methods[0];
        // `O` survives the bound check but only `class-string<O>` names it, which this reader
        // does not parse, so nothing read mentions it and the method stays non-generic.
        assert!(method.type_params.is_empty());
        assert_eq!(method.params[1].1, Some(TypeExpr::Int));
    }

    /// Native method declarations retain their written types even when PHPDoc disagrees.
    #[test]
    fn native_method_templates_win_over_docblock_templates() {
        let program = program_of(concat!(
            "<?php\nclass C {\n",
            "/**\n * @template T\n * @param string $value\n * @return string\n */\n",
            "public function id<U>(U $value): U { return $value; }\n}\n",
        ));
        let StmtKind::ClassDecl { methods, .. } = &program[0].kind else {
            panic!("expected a class declaration");
        };
        let u = TypeExpr::Named(Name::unqualified("U"));
        assert_eq!(methods[0].type_params[0].name, "U");
        assert_eq!(methods[0].params[0].1, Some(u.clone()));
        assert_eq!(methods[0].return_type, Some(u));
    }

    /// No method annotation is adopted, so no template name reaches a property or a sibling.
    #[test]
    fn method_template_names_do_not_leak_to_other_members() {
        let program = program_of(concat!(
            "<?php\nclass C {\n",
            "/** @var U */\nprivate $other;\n",
            "/**\n * @template U\n * @param int $left\n * @param U $right\n * @return U\n */\n",
            "public function choose($left, $right) { return $right; }\n",
            "/**\n * @param U $value\n * @return U\n */\n",
            "public function unrelated($value) { return $value; }\n}\n",
        ));
        let StmtKind::ClassDecl { properties, methods, .. } = &program[0].kind else {
            panic!("expected a class declaration");
        };
        assert_eq!(properties[0].type_expr, None);
        for method in methods {
            assert!(method.type_params.is_empty());
            assert!(method.params.iter().all(|param| param.1.is_none()));
            assert_eq!(method.return_type, None);
        }
    }

}
