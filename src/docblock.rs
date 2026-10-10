//! Purpose:
//! Reads PHPStan/Psalm generic annotations out of `/** … */` doc comments and applies them to
//! the declaration that follows, so an annotated file compiles as generic without being
//! rewritten in elephc's native syntax.
//!
//! Called from:
//! - `crate::source::finalize_physical_program`, per physical file, after the strict audit.
//!
//! Key details:
//! - This is the portable surface. `@template T`, `@param array<T> $a` and `@return T` mean
//!   exactly what `function f<T>(array<T> $a): T` means, and lower to the same monomorphic
//!   instantiations — the difference is that the annotated file still parses on php-src and
//!   still passes `--strict-php`.
//! - A class says the same four things: `@template` its parameters, `@var`/`@param`/`@return`
//!   its members' types, and `@extends`/`@implements` the instantiation it inherits. Inside one,
//!   an ordinary member annotation must mention a class type parameter to be honoured.
//! - A method may declare its own `@template` parameters even in a non-generic class or trait.
//!   Its annotations then form a generic declaration, just as on a free function. Method-local
//!   type parameters do not enter the scope of other members, and native method syntax wins.
//! - The lexer discards comments, so the doc comments are recovered from the SOURCE TEXT and
//!   matched to declaration tokens by line and column. This runs per physical file before
//!   include resolution combines source files, so the original coordinates remain authoritative.
//! - Types inside an annotation go through the ordinary type grammar (`parse_type_expr`), so
//!   `array<string, Foo>` means one thing in the language and in a docblock, and gaining a
//!   type form gains it in both.

mod bindings;
mod members;

use std::collections::HashMap;

use crate::names::Name;
use crate::parser::ast::{
    GenericDecl, Program, Stmt, StmtKind, TypeExpr, TypeParam,
    Variance,
};
use crate::source::SourceMode;

/// The generic annotations one doc comment carries.
#[derive(Debug, Default, Clone)]
struct DocBlock {
    /// `@template T`, `@template T of Foo`, `@template T = string`.
    type_params: Vec<TypeParam>,
    /// `@param <type> $name`, by parameter name without the `$`.
    params: HashMap<String, TypeExpr>,
    /// `@return <type>`.
    return_type: Option<TypeExpr>,
    /// `@var <type>`, the annotation a property carries. Only a class member reads it.
    var_type: Option<TypeExpr>,
    /// `@extends Base<T>` — an inherited name and the type arguments written on it. A class has
    /// at most one parent, but an interface may extend several, so this is a list.
    extends: Vec<(Name, Vec<TypeExpr>)>,
    /// `@implements Iface<T>`, matched to the declaration's `implements` list by name.
    implements: Vec<(Name, Vec<TypeExpr>)>,
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

    /// Returns whether this doc comment says anything about a declaration's generic half.
    ///
    /// `@extends`/`@implements` count on their own: `class UserList implements IteratorAggregate`
    /// annotated `@implements IteratorAggregate<User>` declares no type parameter of its own and
    /// is still a class that instantiates someone else's template.
    fn declares_class_generics(&self) -> bool {
        self.declares_generics() || !self.extends.is_empty() || !self.implements.is_empty()
    }

    /// Returns whether this doc comment carries nothing this pass could ever act on.
    ///
    /// Properties and ordinary methods can refer to a class template without declaring their
    /// own `@template`, so [`Self::declares_generics`] cannot filter out those annotations.
    fn is_empty(&self) -> bool {
        self.type_params.is_empty()
            && self.params.is_empty()
            && self.return_type.is_none()
            && self.var_type.is_none()
            && self.extends.is_empty()
            && self.implements.is_empty()
    }
}

/// Applies generic doc-comment annotations in `source` to the declarations of `program`.
///
/// A function or method that already carries native type parameters is left untouched: written
/// syntax wins over an annotation, so a file can migrate one declaration at a time.
///
/// `mode` is the physical file's own [`SourceMode`]. The doc comments are recovered by
/// re-tokenizing `source`, so that tokenization must use the mode the parser did: PHP mode now
/// accepts a tagless file as pure inline HTML, so the collector cannot infer the mode from
/// whether `tokenize` succeeds.
pub fn apply(program: Program, source: &str, mode: SourceMode) -> Program {
    let blocks = collect(source, mode);
    if blocks.is_empty() {
        return program;
    }
    program
        .into_iter()
        .map(|stmt| apply_to_stmt(stmt, &blocks))
        .collect()
}

/// Applies the doc comment that ends just above `stmt`, if any.
fn apply_to_stmt(mut stmt: Stmt, blocks: &HashMap<(u32, u32), DocBlock>) -> Stmt {
    let block = blocks.get(&(stmt.span.line, stmt.span.col));
    match &mut stmt.kind {
        StmtKind::FunctionDecl {
            type_params,
            params,
            variadic,
            variadic_type,
            return_type,
            ..
        } => {
            let Some(block) = block else { return stmt };
            if !block.declares_generics() || !type_params.is_empty() {
                return stmt;
            }
            *type_params = block.type_params.clone();
            for (name, declared, _, _) in params.iter_mut() {
                if let Some(annotated) = block.params.get(name) {
                    // The annotation REPLACES the written hint, which is the point: `array $a` with
                    // `@param array<T> $a` is precisely the shape PHPStan-annotated code has, and
                    // the annotation is the more precise of the two.
                    *declared = Some(annotated.clone());
                }
            }
            if let Some(annotated) = variadic.as_ref().and_then(|name| block.params.get(name)) {
                *variadic_type = Some(annotated.clone());
            }
            if let Some(annotated) = &block.return_type {
                *return_type = Some(annotated.clone());
            }
        }
        StmtKind::ClassDecl {
            generics,
            extends,
            implements,
            properties,
            methods,
            ..
        } => {
            let extends_args = adopt_inherited(block, extends.as_ref());
            let type_params = adopt_generics(block, generics, extends_args, implements);
            members::apply(&type_params, properties, methods, blocks);
        }
        StmtKind::InterfaceDecl {
            generics,
            extends,
            properties,
            methods,
            ..
        } => {
            // An interface has no single parent, so everything it inherits is an interface and
            // everything annotated lands in `interface_args` — which is why `GenericDecl` says
            // `extends_args` is always empty for one.
            let type_params = adopt_generics(block, generics, Vec::new(), extends);
            members::apply(&type_params, properties, methods, blocks);
        }
        StmtKind::TraitDecl {
            generics, properties, methods, ..
        } => {
            let type_params = adopt_generics(block, generics, Vec::new(), &[]);
            members::apply(&type_params, properties, methods, blocks);
        }
        // Enums carry no class templates, but their methods may declare their own templates.
        StmtKind::EnumDecl { methods, .. } => {
            members::apply(&[], &mut [], methods, blocks);
        }
        StmtKind::NamespaceBlock { body, .. } => {
            let nested = std::mem::take(body);
            *body = nested
                .into_iter()
                .map(|inner| apply_to_stmt(inner, blocks))
                .collect();
        }
        _ => {}
    }
    stmt
}

/// Gives a declaration its generic half from `block`, and returns the type parameters now in
/// scope for its members.
///
/// Written syntax wins over an annotation, so a file can migrate one declaration at a time — and
/// the test is whether the parser built a `GenericDecl` at all, not whether it holds type
/// parameters: `class Repo implements Repository<User>` already said what it inherits natively.
fn adopt_generics(
    block: Option<&DocBlock>,
    generics: &mut Option<Box<GenericDecl>>,
    extends_args: Vec<TypeExpr>,
    inherited_interfaces: &[Name],
) -> Vec<String> {
    if let Some(written) = generics.as_ref() {
        return written
            .type_params
            .iter()
            .map(|param| param.name.clone())
            .collect();
    }
    let Some(block) = block.filter(|block| block.declares_class_generics()) else {
        return Vec::new();
    };
    let names = block
        .type_params
        .iter()
        .map(|param| param.name.clone())
        .collect();
    let interface_args = align_inherited(inherited_interfaces, block);
    *generics = GenericDecl::new(block.type_params.clone(), extends_args, interface_args);
    names
}

/// Reads the type arguments annotated on a class's single parent.
fn adopt_inherited(block: Option<&DocBlock>, parent: Option<&Name>) -> Vec<TypeExpr> {
    block
        .zip(parent)
        .and_then(|(block, parent)| lookup_inherited(parent, block))
        .cloned()
        .unwrap_or_default()
}

/// Aligns annotated type arguments with a written inheritance list, index by index.
///
/// The alignment is the parser's own invariant: `parse_name_list` returns one entry per written
/// name, empty where that name was bare, so `class Box<T> implements Countable` carries `[[]]`.
/// Building the same shape here is what keeps an annotated declaration and the native one it
/// stands for the SAME AST, which the printer round-trip compares directly.
fn align_inherited(written: &[Name], block: &DocBlock) -> Vec<Vec<TypeExpr>> {
    written
        .iter()
        .map(|name| lookup_inherited(name, block).cloned().unwrap_or_default())
        .collect()
}

/// Finds the type arguments annotated for one inherited name, matching on the last segment.
///
/// PHPStan writes `@extends Collection<T>` above `extends \App\Collection`: the annotation uses
/// whatever spelling is in scope, and this pass runs before name resolution, so neither side is
/// canonical yet. The basename is the only part the two are guaranteed to share.
///
/// Both annotations are searched whichever kind of declaration asked. `@extends` is PHPStan's
/// spelling for an interface extending a generic interface, and elephc stores that in the same
/// `interface_args` as a class's `implements`; accepting either spelling costs nothing and
/// refusing one would reject an annotation that says exactly what it means.
fn lookup_inherited<'a>(written: &Name, block: &'a DocBlock) -> Option<&'a Vec<TypeExpr>> {
    let written = written.last_segment()?;
    block
        .extends
        .iter()
        .chain(block.implements.iter())
        .find(|(name, _)| {
            name.last_segment()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(written))
        })
        .map(|(_, args)| args)
}

/// Extracts generic-bearing doc comments keyed by their declaration token positions.
fn collect(source: &str, mode: SourceMode) -> HashMap<(u32, u32), DocBlock> {
    bindings::collect(source, mode)
}

/// Parses the annotations of one doc comment's lines.
fn parse_block(lines: &[&str]) -> DocBlock {
    let mut block = DocBlock::default();
    for line in lines {
        let trimmed = strip_comment_markers(line);
        let trimmed = trimmed.as_str();
        if let Some((rest, variance)) = strip_template_tag(trimmed) {
            if let Some(param) = parse_template(rest, variance) {
                if !block.type_params.iter().any(|p| p.name == param.name) {
                    block.type_params.push(param);
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("@param ") {
            if let Some((name, ty)) = parse_param(rest) {
                block.params.insert(name, ty);
            }
        } else if let Some(rest) = trimmed.strip_prefix("@return ") {
            block.return_type = parse_type(rest.trim());
        } else if let Some(rest) = trimmed.strip_prefix("@var ") {
            block.var_type = parse_var(rest);
        } else if let Some(rest) = trimmed.strip_prefix("@extends ") {
            block.extends.extend(parse_inherited(rest));
        } else if let Some(rest) = trimmed.strip_prefix("@implements ") {
            block.implements.extend(parse_inherited(rest));
        }
    }
    block
}

/// Strips one doc-comment line down to the annotation it carries.
///
/// A doc comment has two shapes and a member almost always uses the short one: the opener, the
/// annotation and the closer on a single line (`/** @var T */`). Trimming only the continuation
/// `*` reads the tall shape and silently ignores the short one, so both markers come off here,
/// and the leading `*` after them, in that order.
fn strip_comment_markers(line: &str) -> String {
    line.trim()
        .trim_start_matches("/**")
        .trim_end_matches("*/")
        .trim()
        .trim_start_matches('*')
        .trim()
        .to_string()
}

/// Splits a `@template` tag from its body, reading the variance the tag name carries.
///
/// PHPStan spells variance in the TAG (`@template-covariant T`) where native syntax spells it on
/// the parameter (`+T`). Both reach the same `TypeParam`. The hyphenated forms are tested first
/// only for clarity; they cannot collide with `@template ` because that one requires the space.
fn strip_template_tag(trimmed: &str) -> Option<(&str, Variance)> {
    for (tag, variance) in [
        ("@template-covariant ", Variance::Covariant),
        ("@template-contravariant ", Variance::Contravariant),
        ("@template ", Variance::Invariant),
    ] {
        if let Some(rest) = trimmed.strip_prefix(tag) {
            return Some((rest, variance));
        }
    }
    None
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
        let bound_end = after.find('=').unwrap_or(after.len());
        bound = parse_type(after[..bound_end].trim());
        rest = after[bound_end..].trim();
    }
    let default = rest
        .strip_prefix('=')
        .and_then(|value| parse_type(value.trim()));
    Some(TypeParam {
        name,
        bound,
        default,
        variance,
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

/// Parses `@var <type>`, with or without the trailing variable a PHPStan inline `@var` names.
fn parse_var(rest: &str) -> Option<TypeExpr> {
    let rest = rest.trim();
    let text = match rest.find('$') {
        Some(dollar) => &rest[..dollar],
        None => rest,
    };
    parse_type(text.trim())
}

/// Parses `@extends Base<T>` / `@implements Iface<T>` into the inherited name and its arguments.
///
/// The annotation goes through the ordinary type grammar, so `Base<T>` arrives as the very
/// `TypeExpr::GenericClass` the native `extends Base<T>` would have produced. An annotation with
/// no type arguments yields `None`: it names an inheritance the declaration already states, and
/// carries nothing this pass could add.
fn parse_inherited(rest: &str) -> Option<(Name, Vec<TypeExpr>)> {
    match parse_type(rest.trim())? {
        TypeExpr::GenericClass { name, args } => Some((name, args)),
        _ => None,
    }
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
        apply(program, source, SourceMode::Php)
    }

    /// Collects the sole generic docblock in a source fixture.
    fn block_of(source: &str) -> DocBlock {
        let blocks = collect(source, SourceMode::Php);
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
        let blocks = collect("<?php\n/**\n * @template T\n */\n\n\nfunction f($a) {}\n", SourceMode::Php);
        assert!(blocks.contains_key(&(7, 1)), "got keys {:?}", blocks.keys());
    }

    /// A doc comment stays bound to the declaration directly below it when inline HTML or a
    /// `?>`/`<?php` tag sits above the comment. The tag bytes are consumed without a token, so
    /// the gap scan must step over them the way PHP's own doc-comment binding does.
    #[test]
    fn binds_through_inline_html_and_tags() {
        for source in [
            "<div>\n<?php\n/** @template T */\nfunction f() {}\n",
            "<?php echo 1; ?>\n<?php\n/** @template T */\nfunction f() {}\n",
            "<?php echo 1; ?>\n<div>\n<?php\n/** @template T */\nfunction f() {}\n",
            "<?=\n1;\n?>\n<div>\n<?php\n/** @template T */\nfunction f() {}\n",
            "<?PHP\n/** @template T */\nfunction f() {}\n",
        ] {
            let program = program_of(source);
            let type_params = program
                .iter()
                .find_map(|stmt| match &stmt.kind {
                    StmtKind::FunctionDecl { type_params, .. } => Some(type_params),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("expected a function declaration for {source:?}"));
            assert_eq!(type_params.len(), 1, "source: {source:?}");
            assert_eq!(type_params[0].name, "T", "source: {source:?}");
        }
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

    /// Applies generic function annotations as concrete AST type declarations.
    #[test]
    fn applies_the_annotation_to_the_declaration() {
        let source =
            "<?php\n/**\n * @template T\n * @param array<T> $a\n * @return T\n */\nfunction f(array $a) { return $a[0]; }\n";
        let tokens = crate::lexer::tokenize(source).expect("tokenizes");
        let program = crate::parser::parse(&tokens).expect("parses");
        let program = apply(program, source, SourceMode::Php);
        match &program[0].kind {
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
        let program = apply(program, source, SourceMode::Php);
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
            "<?php\n/** @template T */ class Box {}\n",
            "<?php\n/**\n * @template T\n */ class Box {}\n",
        ] {
            let program = program_of(source);
            let StmtKind::ClassDecl { generics, .. } = &program[0].kind else {
                panic!("expected a class");
            };
            assert_eq!(generics.as_ref().expect("annotated class").type_params[0].name, "T");
        }
    }

    /// Sharing a class line does not give a method the class's template declaration.
    #[test]
    fn does_not_copy_a_class_template_to_a_same_line_method() {
        let program = program_of("<?php\n/** @template T */\nclass Box { public function __construct(public T $value) {} public function id(T $v): T { return $v; } }\n");
        let StmtKind::ClassDecl { generics, methods, .. } = &program[0].kind else {
            panic!("expected a class");
        };
        assert!(generics.is_some());
        assert!(methods.iter().all(|method| method.type_params.is_empty()));
    }

    /// Each inline method gets its own block, even after Unicode comments or with attributes.
    #[test]
    fn binds_inline_members_by_column() {
        let program = program_of("<?php\n/* café */ class C { /** @template T */ #[Marker(\"]\")] public function id(T $v): T { return $v; } /** @template U */ public function other(U $v): U { return $v; } }\n");
        let StmtKind::ClassDecl { generics, methods, .. } = &program[0].kind else {
            panic!("expected a class");
        };
        assert!(generics.is_none());
        assert_eq!(methods[0].type_params[0].name, "T");
        assert_eq!(methods[1].type_params[0].name, "U");
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

    /// Enum methods adopt their own template parameters and annotated types.
    #[test]
    fn adopts_enum_method_templates() {
        let program = program_of("<?php\nenum Id {\n case A;\n /**\n  * @template T of int\n  * @param T $value\n  * @return T\n  */\n public function id($value) { return $value; }\n}\n");
        let StmtKind::EnumDecl { methods, .. } = &program[0].kind else {
            panic!("expected an enum");
        };
        assert_eq!(methods[0].type_params.len(), 1);
        assert_eq!(methods[0].type_params[0].bound, Some(TypeExpr::Int));
        assert_eq!(methods[0].params[0].1, Some(TypeExpr::Named(Name::unqualified("T"))));
    }

    /// `@template` on a class makes it a template, and its members' annotations name the type
    /// parameter it declared.
    #[test]
    fn a_class_adopts_its_template_and_member_annotations() {
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
        let generics = generics.as_ref().expect("class became a template");
        assert_eq!(generics.type_params.len(), 1);
        assert_eq!(generics.type_params[0].name, "T");
        let t = TypeExpr::Named(Name::unqualified("T"));
        assert_eq!(properties[0].type_expr, Some(t.clone()));
        assert_eq!(methods[0].params[0].1, Some(t.clone()));
        assert_eq!(methods[1].return_type, Some(t));
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

    /// A promoted constructor parameter is also a property, built by the parser from the same
    /// written type; retyping one and not the other leaves the class disagreeing with itself.
    #[test]
    fn a_promoted_parameter_retypes_its_property_too() {
        let program = program_of(concat!(
            "<?php\n",
            "/** @template T */\n",
            "class Box {\n",
            "    /** @param T $value */\n",
            "    public function __construct(private $value) {}\n",
            "}\n",
        ));
        let StmtKind::ClassDecl {
            properties, methods, ..
        } = &program[0].kind
        else {
            panic!("expected a class declaration");
        };
        let t = Some(TypeExpr::Named(Name::unqualified("T")));
        assert_eq!(methods[0].params[0].1, t);
        assert!(properties[0].is_promoted);
        assert_eq!(properties[0].type_expr, t);
    }

    /// `@implements` aligns with the written list index by index, the way `parse_name_list` does,
    /// so an annotated declaration and the native one it stands for are the same AST.
    #[test]
    fn implements_annotations_align_with_the_written_list() {
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
        let generics = generics.as_ref().expect("class became a template");
        assert_eq!(
            generics.interface_args,
            vec![
                Vec::new(),
                vec![TypeExpr::Named(Name::unqualified("T"))],
            ]
        );
    }

    /// `@extends Base<int>` is what a class with no type parameters of its own uses to name the
    /// instantiation of someone else's template.
    #[test]
    fn extends_annotation_fills_the_parent_type_arguments() {
        let program = program_of(concat!(
            "<?php\n",
            "/** @extends Holder<int> */\n",
            "class IntHolder extends Holder {}\n",
        ));
        let StmtKind::ClassDecl { generics, .. } = &program[0].kind else {
            panic!("expected a class declaration");
        };
        let generics = generics.as_ref().expect("class names what it inherits");
        assert!(generics.type_params.is_empty());
        assert_eq!(generics.extends_args, vec![TypeExpr::Int]);
    }

    /// An interface has no single parent, so everything it inherits lands in `interface_args`.
    #[test]
    fn an_interface_adopts_its_template_and_inherited_arguments() {
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
        let generics = generics.as_ref().expect("interface became a template");
        assert_eq!(generics.type_params.len(), 1);
        assert!(generics.extends_args.is_empty());
        assert_eq!(
            generics.interface_args,
            vec![vec![TypeExpr::Named(Name::unqualified("T"))]]
        );
        assert_eq!(
            methods[0].return_type,
            Some(TypeExpr::Named(Name::unqualified("T")))
        );
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
            "    class Box {}\n",
            "}\n",
        ));
        let StmtKind::NamespaceBlock { body, .. } = &program[0].kind else {
            panic!("expected a namespace block");
        };
        let StmtKind::ClassDecl { generics, .. } = &body[0].kind else {
            panic!("expected a class declaration");
        };
        assert_eq!(
            generics
                .as_ref()
                .expect("class became a template")
                .type_params[0]
                .name,
            "T"
        );
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
        let StmtKind::FunctionDecl { variadic_type, .. } = &program[0].kind else {
            panic!("expected a function declaration");
        };
        assert_eq!(*variadic_type, Some(TypeExpr::Named(Name::unqualified("T"))));
    }

    /// Tagless files retain generic annotations across attributes with nonstructural brackets.
    #[test]
    fn applies_generic_docblocks_after_attributes_in_lfc_sources() {
        let source = "/** @template T */\n#[Marker(\n    ']'\n)]\nclass Box {}";
        let tokens = crate::lexer::tokenize_with_mode(source, crate::source::SourceMode::Lfc)
            .expect("tokenizes");
        let program = apply(
            crate::parser::parse(&tokens).expect("parses"),
            source,
            SourceMode::Lfc,
        );
        let StmtKind::ClassDecl { generics, .. } = &program[0].kind else {
            panic!("expected a class declaration");
        };
        assert_eq!(
            generics.as_ref().expect("class became a template").type_params[0].name,
            "T"
        );
    }

    /// A method's template carries bounds, defaults, fixed parameters and a variadic element type.
    #[test]
    fn adopts_a_methods_own_template_without_a_class_template() {
        let program = program_of(concat!(
            "<?php\nclass C {\n",
            "/**\n * @template T of Entity\n * @template U = string\n",
            " * @param T $value\n * @param U ...$labels\n * @return int\n */\n",
            "public function id($value, ...$labels) { return 1; }\n}\n",
        ));
        let StmtKind::ClassDecl { generics, methods, .. } = &program[0].kind else {
            panic!("expected a class declaration");
        };
        assert!(generics.is_none(), "a method template must not make the class generic");
        let method = &methods[0];
        assert_eq!(method.type_params.len(), 2);
        assert_eq!(method.type_params[0].name, "T");
        assert_eq!(method.type_params[0].bound, Some(TypeExpr::Named(Name::unqualified("Entity"))));
        assert_eq!(method.type_params[1].default, Some(TypeExpr::Str));
        assert_eq!(method.params[0].1, Some(TypeExpr::Named(Name::unqualified("T"))));
        assert_eq!(method.variadic_type, Some(TypeExpr::Named(Name::unqualified("U"))));
        assert_eq!(method.return_type, Some(TypeExpr::Int));
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

    /// A method template does not introduce its names into properties or sibling methods.
    #[test]
    fn method_template_names_do_not_leak_to_other_members() {
        let program = program_of(concat!(
            "<?php\n/** @template T */\nclass C {\n",
            "/** @var U */\nprivate $other;\n",
            "/**\n * @template U\n * @param T $left\n * @param U $right\n * @return U\n */\n",
            "public function choose($left, $right) { return $right; }\n",
            "/**\n * @param U $value\n * @return U\n */\n",
            "public function unrelated($value) { return $value; }\n}\n",
        ));
        let StmtKind::ClassDecl { properties, methods, .. } = &program[0].kind else {
            panic!("expected a class declaration");
        };
        assert_eq!(properties[0].type_expr, None);
        assert_eq!(methods[0].type_params[0].name, "U");
        assert_eq!(methods[0].params[0].1, Some(TypeExpr::Named(Name::unqualified("T"))));
        assert_eq!(methods[0].params[1].1, Some(TypeExpr::Named(Name::unqualified("U"))));
        assert!(methods[1].type_params.is_empty());
        assert_eq!(methods[1].params[0].1, None);
        assert_eq!(methods[1].return_type, None);
    }

}
