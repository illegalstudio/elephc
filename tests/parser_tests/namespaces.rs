//! Purpose:
//! Integration or regression tests for parser AST coverage of namespaces, including namespace semicolon and use group, namespace block with qualified names, and dunder namespace magic constant.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

/// Parses a namespace declaration with semicolon syntax and a use group import
/// that combines class, function, and const imports with aliases.
#[test]
fn test_parse_namespace_semicolon_and_use_group() {
    let stmts = parse_source(
        "<?php namespace App\\Core; use Lib\\Utils\\{Formatter, function render as draw, const ANSWER};",
    );
    assert_eq!(stmts.len(), 2);
    match &stmts[0].kind {
        StmtKind::NamespaceDecl { name } => {
            assert_eq!(name.as_ref().map(Name::as_str), Some("App\\Core"));
        }
        other => panic!("expected namespace decl, got {:?}", other),
    }
    match &stmts[1].kind {
        StmtKind::UseDecl { imports } => {
            assert_eq!(imports.len(), 3);
            assert_eq!(imports[0].kind, UseKind::Class);
            assert_eq!(imports[0].name.as_str(), "Lib\\Utils\\Formatter");
            assert_eq!(imports[0].alias, "Formatter");
            assert_eq!(imports[1].kind, UseKind::Function);
            assert_eq!(imports[1].name.as_str(), "Lib\\Utils\\render");
            assert_eq!(imports[1].alias, "draw");
            assert_eq!(imports[2].kind, UseKind::Const);
            assert_eq!(imports[2].name.as_str(), "Lib\\Utils\\ANSWER");
            assert_eq!(imports[2].alias, "ANSWER");
        }
        other => panic!("expected use decl, got {:?}", other),
    }
}

/// Parses lexer-tokenized predefined constants in every grouped-use suffix position.
#[test]
fn test_parse_grouped_use_const_tokenized_names() {
    let stmts = parse_source(
        "<?php namespace App; use const Vendor\\{PHP_INT_MAX as MAX, PHP_INT_MIN as MIN};",
    );
    match &stmts[1].kind {
        StmtKind::UseDecl { imports } => {
            assert_eq!(imports.len(), 2);
            assert_eq!(imports[0].name.as_str(), "Vendor\\PHP_INT_MAX");
            assert_eq!(imports[0].alias, "MAX");
            assert_eq!(imports[1].name.as_str(), "Vendor\\PHP_INT_MIN");
            assert_eq!(imports[1].alias, "MIN");
        }
        other => panic!("expected use decl, got {:?}", other),
    }
}

/// Parses a namespace block containing a class with extends, implements, trait use,
/// and a fully-qualified static method call inside a method body.
#[test]
fn test_parse_namespace_block_with_qualified_names() {
    let stmts = parse_source(
        "<?php namespace App\\Models { class User extends Base\\Record implements \\Contracts\\Jsonable { use Shared\\Loggable; public function make() { return Factory\\UserFactory::build(); } } }",
    );
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::NamespaceBlock { name, body } => {
            assert_eq!(name.as_ref().map(Name::as_str), Some("App\\Models"));
            assert_eq!(body.len(), 1);
            match &body[0].kind {
                StmtKind::ClassDecl {
                    extends,
                    implements,
                    trait_uses,
                    methods,
                    ..
                } => {
                    assert_eq!(extends.as_ref().map(Name::as_str), Some("Base\\Record"));
                    assert_eq!(implements.len(), 1);
                    assert!(implements[0].is_fully_qualified());
                    assert_eq!(implements[0].as_str(), "Contracts\\Jsonable");
                    assert_eq!(trait_uses[0].trait_names[0].as_str(), "Shared\\Loggable");
                    match &methods[0].body[0].kind {
                        StmtKind::Return(Some(expr)) => match &expr.kind {
                            ExprKind::StaticMethodCall {
                                receiver, method, ..
                            } => {
                                match receiver {
                                    StaticReceiver::Named(name) => {
                                        assert_eq!(name.as_str(), "Factory\\UserFactory");
                                    }
                                    other => panic!("expected named receiver, got {:?}", other),
                                }
                                assert_eq!(method, "build");
                            }
                            other => panic!("expected static method call, got {:?}", other),
                        },
                        other => panic!("expected return stmt, got {:?}", other),
                    }
                }
                other => panic!("expected class decl, got {:?}", other),
            }
        }
        other => panic!("expected namespace block, got {:?}", other),
    }
}

/// Parses the `__NAMESPACE__` magic constant in an echo statement and verifies
/// it is lowered to the internal `MagicConstant::Namespace` variant.
#[test]
fn test_parse_dunder_namespace_magic_constant() {
    let stmts = parse_source("<?php echo __NAMESPACE__;");
    assert_eq!(
        echoed_expr(&stmts),
        &ExprKind::MagicConstant(MagicConstant::Namespace)
    );
}

/// Verifies a fully qualified predefined constant's span starts at its leading `\` (#1307), so a
/// diagnostic on `\PHP_EOL` points at the whole name rather than past the separator.
#[test]
fn test_fully_qualified_predefined_constant_span_starts_at_the_backslash() {
    let stmts = parse_source("<?php echo \\PHP_EOL;");
    let StmtKind::Echo(expr) = &stmts[0].kind else {
        panic!("expected an echo statement, got {:?}", stmts[0].kind);
    };
    // `<?php echo ` is eleven columns, so the `\` is at column 12.
    assert_eq!((expr.span.line, expr.span.col), (1, 12));
}

/// Verifies a reserved word parses as a segment of a qualified namespace name (#826, #840),
/// while a lone keyword is still refused as a namespace name.
#[test]
fn test_reserved_word_namespace_segments_parse() {
    for source in [
        "<?php namespace Demo\\Namespace;",
        "<?php namespace Vendor\\Default\\Theme;",
        "<?php use Vendor\\Default\\Theme\\Example;",
        "<?php new \\Vendor\\List\\Item();",
    ] {
        let stmts = parse_source(source);
        assert!(!stmts.is_empty(), "{source}: expected a statement");
    }
    assert!(parse_fails("<?php namespace Namespace;"));
}

/// Verifies a leading `namespace\` is never read as a literal first segment spelled
/// `namespace`. It is PHP's relative-name prefix, so `new namespace\Foo()` inside `App` means
/// `App\Foo`; reading it as a segment bound it to `App\namespace\Foo`, a silent miscompile
/// through every `parse_name` caller (`new`, `extends`, `instanceof`, trait `use`, types,
/// `catch`). The parser may refuse the form or resolve it, but must never produce that segment.
/// A `namespace` segment after a separator is still an ordinary segment.
#[test]
fn test_relative_namespace_prefix_is_never_a_literal_segment() {
    for body in [
        "new namespace\\Foo();",
        "class B extends namespace\\Foo {}",
        "class C implements namespace\\I {}",
        "$ok = $x instanceof namespace\\Foo;",
        "class D { use namespace\\T; }",
        "function f(namespace\\Foo $x): namespace\\Foo { return $x; }",
        "try {} catch (namespace\\E $e) {}",
        "#[namespace\\Attr] function g() {}",
        "echo namespace\\Foo::class;",
    ] {
        let source = format!("<?php namespace App; {body}");
        let Ok(tokens) = tokenize(&source) else { continue };
        if let Ok(stmts) = parse(&tokens) {
            let dump = format!("{stmts:?}").to_ascii_lowercase();
            assert!(
                !dump.contains("\"namespace\""),
                "{body}: `namespace\\` became a literal name segment: {dump}"
            );
        }
    }
    let stmts = parse_source("<?php new \\Demo\\Namespace\\Subject();");
    let dump = format!("{stmts:?}");
    assert!(dump.contains("\"Namespace\""), "a trailing segment must stay a segment: {dump}");
}

/// Verifies `namespace\CONFIG;` at statement position is never read as the declaration
/// `namespace \CONFIG;`, which silently moved every later declaration of the file into a
/// namespace named `CONFIG` (#826 review). The parser may refuse it or read it as the relative
/// constant fetch it is in PHP, but the only namespace declared must stay `App`.
#[test]
fn test_relative_prefix_statement_is_never_a_namespace_declaration() {
    let tokens = tokenize("<?php namespace App; namespace\\CONFIG; class Widget {}").unwrap();
    if let Ok(stmts) = parse(&tokens) {
        let declared: Vec<String> = stmts
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::NamespaceDecl { name } => {
                    Some(name.as_ref().map(|name| name.as_canonical()).unwrap_or_default())
                }
                _ => None,
            })
            .collect();
        assert_eq!(declared, vec!["App".to_string()]);
    }
}

/// Verifies the reserved-word FIRST segment of a qualified name starts a name in statement,
/// expression, type, `implements`, `catch`, attribute and `use` positions (#826), and that the
/// word must touch the `\`: a keyword followed by a space and a fully qualified name is still
/// the keyword (`use function \Lib\f;`, `new \Foo()`).
#[test]
fn test_reserved_word_first_segment_starts_a_name_everywhere() {
    for source in [
        "<?php Default\\Palette::accent();",
        "<?php echo Default\\Palette::X;",
        "<?php Function\\run();",
        "<?php Default\\Palette::$hits = 1;",
        "<?php function f(Default\\Palette $p): ?Default\\Palette { return $p; }",
        "<?php class C implements Default\\Marker {}",
        "<?php try {} catch (Default\\Failure | List\\Failure $e) {}",
        "<?php #[Vendor\\Default\\Attr] #[Default\\Attr] function g() {}",
        "<?php $x = new Static\\Factory();",
        "<?php $ok = $x instanceof Static\\Factory;",
        "<?php class H { public Default\\Palette $p; }",
    ] {
        let stmts = parse_source(source);
        assert!(!stmts.is_empty(), "{source}: expected a statement");
    }

    let stmts = parse_source("<?php use Function\\Lib\\Tool, function \\Lib\\helper;");
    let StmtKind::UseDecl { imports } = &stmts[0].kind else {
        panic!("expected a use declaration, got {:?}", stmts[0].kind);
    };
    assert_eq!(imports[0].kind, UseKind::Class);
    assert_eq!(imports[0].name.parts, vec!["Function", "Lib", "Tool"]);
    assert_eq!(imports[1].kind, UseKind::Function);
    assert_eq!(imports[1].name.parts, vec!["Lib", "helper"]);

    let stmts = parse_source("<?php $o = new \\Foo();");
    let dump = format!("{stmts:?}");
    assert!(dump.contains("NewObject"), "`new \\Foo` must stay a `new`: {dump}");
    // A lone keyword is not a name, and a group-use prefix needs a glued segment too.
    assert!(parse_fails("<?php Default::accent();"));
    assert!(parse_fails("<?php use Default\\{Palette};"));
}
