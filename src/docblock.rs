//! Purpose:
//! Reads PHPStan/Psalm generic annotations out of `/** … */` doc comments and applies them to
//! the declaration that follows, so an annotated file compiles as generic without being
//! rewritten in elephc's native syntax.
//!
//! Called from:
//! - `crate::source::finalize_physical_program`, per physical file, before the strict audit.
//!
//! Key details:
//! - This is the portable surface. `@template T`, `@param array<T> $a` and `@return T` mean
//!   exactly what `function f<T>(array<T> $a): T` means, and lower to the same monomorphic
//!   instantiations — the difference is that the annotated file still parses on php-src and
//!   still passes `--strict-php`.
//! - A class says the same four things: `@template` its parameters, `@var`/`@param`/`@return`
//!   its members' types, and `@extends`/`@implements` the instantiation it inherits. Inside one,
//!   only an annotation MENTIONING a type parameter is honoured — a member's doc comment carries
//!   no `@template` of its own, so without that test annotating a class would re-type its whole
//!   body.
//! - The lexer discards comments, so the doc comments are recovered from the SOURCE TEXT and
//!   matched to declarations by line. That is how every PHPDoc consumer works, and it is why
//!   this runs per physical file, while line numbers still mean what the file says: include
//!   resolution splices without rebasing them.
//! - Types inside an annotation go through the ordinary type grammar (`parse_type_expr`), so
//!   `array<string, Foo>` means one thing in the language and in a docblock, and gaining a
//!   type form gains it in both.

use std::collections::HashMap;

use crate::names::Name;
use crate::parser::ast::{
    ClassMethod, ClassProperty, GenericDecl, Program, Stmt, StmtKind, TypeExpr, TypeParam,
    Variance,
};

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
    /// A member's doc comment carries no `@template` — the class above it declared the type
    /// parameters — so [`Self::declares_generics`] cannot be the filter that decides which
    /// blocks are worth keeping. What decides whether a member annotation is HONOURED is
    /// whether its type mentions one of those parameters; see [`apply_member_type`].
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
/// A declaration that already carries native type parameters is left untouched: written syntax
/// wins over an annotation, so a file can migrate one function at a time.
pub fn apply(program: Program, source: &str) -> Program {
    let blocks = collect(source);
    if blocks.is_empty() {
        return program;
    }
    program
        .into_iter()
        .map(|stmt| apply_to_stmt(stmt, &blocks))
        .collect()
}

/// Applies the doc comment that ends just above `stmt`, if any.
fn apply_to_stmt(mut stmt: Stmt, blocks: &HashMap<usize, DocBlock>) -> Stmt {
    let block = blocks.get(&(stmt.span.line as usize));
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
            apply_to_members(&type_params, properties, methods, blocks);
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
            apply_to_members(&type_params, properties, methods, blocks);
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

/// Applies the doc comments of a generic declaration's own members.
///
/// `type_params` are the CLASS's, because a member's doc comment carries no `@template` of its
/// own. That is also why only an annotation MENTIONING one of them is honoured: `@param int $n`
/// inside a generic class is the same ordinary PHPStan annotation it is anywhere else, and
/// `@template` on the class must not silently promote every annotation in the body into a type
/// declaration the compiler enforces.
fn apply_to_members(
    type_params: &[String],
    properties: &mut [ClassProperty],
    methods: &mut [ClassMethod],
    blocks: &HashMap<usize, DocBlock>,
) {
    if type_params.is_empty() {
        return;
    }
    for property in properties.iter_mut() {
        let Some(block) = blocks.get(&(property.span.line as usize)) else {
            continue;
        };
        if let Some(annotated) = &block.var_type {
            apply_member_type(&mut property.type_expr, annotated, type_params);
        }
    }
    let mut promoted: Vec<(String, TypeExpr)> = Vec::new();
    for method in methods.iter_mut() {
        let Some(block) = blocks.get(&(method.span.line as usize)) else {
            continue;
        };
        let is_constructor = method.name.eq_ignore_ascii_case("__construct");
        for (name, declared, _, _) in method.params.iter_mut() {
            let Some(annotated) = block.params.get(name) else {
                continue;
            };
            if apply_member_type(declared, annotated, type_params) && is_constructor {
                promoted.push((name.clone(), annotated.clone()));
            }
        }
        if let Some(annotated) = method
            .variadic
            .as_ref()
            .and_then(|name| block.params.get(name))
        {
            apply_member_type(&mut method.variadic_type, annotated, type_params);
        }
        if let Some(annotated) = &block.return_type {
            apply_member_type(&mut method.return_type, annotated, type_params);
        }
    }
    // A promoted constructor parameter is ALSO a property, which the parser built from the same
    // written type. Retyping one and not the other leaves the class disagreeing with itself:
    // `new Box(5)` would infer `T = int` from the parameter and store into a `mixed` field.
    for property in properties.iter_mut().filter(|property| property.is_promoted) {
        if let Some((_, annotated)) = promoted.iter().find(|(name, _)| *name == property.name) {
            property.type_expr = Some(annotated.clone());
        }
    }
}

/// Replaces one member's declared type with its annotation, and reports whether it did.
///
/// The `mentions_type_param` test is the whole gate on member annotations; see
/// [`apply_to_members`].
fn apply_member_type(
    declared: &mut Option<TypeExpr>,
    annotated: &TypeExpr,
    type_params: &[String],
) -> bool {
    if !annotated.mentions_type_param(type_params) {
        return false;
    }
    *declared = Some(annotated.clone());
    true
}

/// Extracts every generic-bearing doc comment from `source`, keyed by the line of the first
/// code line after it.
///
/// Keying by the FOLLOWING line is what associates a block with its declaration, and it is why
/// blank lines between the two are skipped: `/** … */\n\nfunction f()` is idiomatic.
///
/// ATTRIBUTES are skipped for the same reason. `/** @template T */ #[Marker] class Box {}` files
/// the block against the attribute's line, while `apply_to_stmt` looks the declaration's line up —
/// the block is silently lost and the class is treated as non-generic. `#[` is unambiguous in PHP
/// 8 (a bare `#` is a line comment) and a group may span lines, so the walk follows its bracket
/// depth rather than assuming one line per attribute.
fn collect(source: &str) -> HashMap<usize, DocBlock> {
    let mut blocks: HashMap<usize, DocBlock> = HashMap::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut index = 0usize;
    while index < lines.len() {
        if !lines[index].trim_start().starts_with("/**") {
            index += 1;
            continue;
        }
        let start = index;
        while index < lines.len() && !lines[index].contains("*/") {
            index += 1;
        }
        let end = index.min(lines.len().saturating_sub(1));
        index += 1;
        let mut target = index;
        loop {
            while target < lines.len() && lines[target].trim().is_empty() {
                target += 1;
            }
            if target >= lines.len() || !lines[target].trim_start().starts_with("#[") {
                break;
            }
            let mut depth = 0i32;
            while target < lines.len() {
                for ch in lines[target].chars() {
                    match ch {
                        '[' => depth += 1,
                        ']' => depth -= 1,
                        _ => {}
                    }
                }
                target += 1;
                if depth <= 0 {
                    break;
                }
            }
        }
        if target >= lines.len() {
            continue;
        }
        let block = parse_block(&lines[start..=end]);
        if !block.is_empty() {
            // `Span` lines are 1-based.
            blocks.insert(target + 1, block);
        }
    }
    blocks
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
        apply(program, source)
    }

    fn block_of(source: &str) -> DocBlock {
        let blocks = collect(source);
        assert_eq!(blocks.len(), 1, "expected exactly one generic doc comment");
        blocks.into_values().next().expect("one block")
    }

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
        let blocks = collect("<?php\n/**\n * @template T\n */\n\n\nfunction f($a) {}\n");
        assert!(blocks.contains_key(&7), "got keys {:?}", blocks.keys());
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

    #[test]
    fn applies_the_annotation_to_the_declaration() {
        let source =
            "<?php\n/**\n * @template T\n * @param array<T> $a\n * @return T\n */\nfunction f(array $a) { return $a[0]; }\n";
        let tokens = crate::lexer::tokenize(source).expect("tokenizes");
        let program = crate::parser::parse(&tokens).expect("parses");
        let program = apply(program, source);
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

}
