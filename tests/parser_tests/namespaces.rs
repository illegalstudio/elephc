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

/// Returns the fully qualified class names of every `new X()` statement in `stmts`, in order,
/// descending into braced namespace blocks. Each must be fully qualified.
fn new_object_names(stmts: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::NamespaceBlock { body, .. } => names.extend(new_object_names(body)),
            StmtKind::ExprStmt(Expr {
                kind: ExprKind::NewObject { class_name, .. },
                ..
            }) => {
                assert!(class_name.is_fully_qualified(), "{class_name:?} is not fully qualified");
                names.push(class_name.as_canonical());
            }
            _ => {}
        }
    }
    names
}

/// Verifies a relative name `namespace\Foo` resolves at parse time to the fully qualified name
/// in the namespace being parsed (#825): the unbraced form, each braced block, the global
/// namespace, and the global code the resolver sees after a braced block, whose `}` restores
/// the namespace before it.
#[test]
fn test_parse_relative_names_resolve_in_the_current_namespace() {
    let stmts = parse_source("<?php namespace App\\Core; new namespace\\Foo();");
    assert_eq!(new_object_names(&stmts), vec!["App\\Core\\Foo"]);

    let stmts = parse_source(
        "<?php namespace A { new namespace\\Foo(); } namespace B\\C { new namespace\\Sub\\Foo(); } \
         namespace { new namespace\\Foo(); }",
    );
    assert_eq!(new_object_names(&stmts), vec!["A\\Foo", "B\\C\\Sub\\Foo", "Foo"]);

    let stmts = parse_source("<?php new namespace\\Foo();");
    assert_eq!(new_object_names(&stmts), vec!["Foo"]);

    let stmts = parse_source("<?php namespace A { new namespace\\Foo(); } new namespace\\Foo();");
    assert_eq!(new_object_names(&stmts), vec!["A\\Foo", "Foo"]);
}

/// Verifies the word `namespace` used as an enum case, a method, a property or a class
/// constant never changes the namespace relative names resolve against (#825). A token scan
/// read `case namespace;` as the declaration `namespace;` and moved later names to the global
/// namespace.
#[test]
fn test_parse_relative_names_ignore_namespace_used_as_a_member_name() {
    let stmts = parse_source(
        "<?php namespace App; \
         enum Mode { case namespace; case other; } \
         class K { const namespace = 1; public $namespace; public function namespace() {} } \
         $o->namespace(); $o->namespace; K::namespace; Mode::namespace; \
         new namespace\\Foo();",
    );
    assert_eq!(new_object_names(&stmts), vec!["App\\Foo"]);
}

/// Verifies a relative name resolves in every name position: `extends`, `implements`, trait
/// `use`, parameter, return, property and intersection types, `catch`, `instanceof`,
/// attributes, calls, constants and `::class`. None may keep a segment spelled `namespace`.
#[test]
fn test_parse_relative_names_in_every_name_position() {
    let stmts = parse_source(
        "<?php namespace App; \
         #[namespace\\Tag] \
         class B extends namespace\\Base implements namespace\\Shape { \
             use namespace\\Greets; \
             public namespace\\Box $box; \
             public function f(namespace\\Box&namespace\\Shape $x): ?namespace\\Box { return null; } \
         } \
         try {} catch (namespace\\Failure $e) {} \
         $ok = $x instanceof namespace\\Box; \
         echo namespace\\helper(), namespace\\LIMIT, namespace\\Box::class;",
    );
    let dump = format!("{stmts:?}");
    assert!(!dump.to_ascii_lowercase().contains("\"namespace\""), "{dump}");
    for class in ["Tag", "Base", "Shape", "Greets", "Box", "Failure", "helper", "LIMIT"] {
        assert!(
            dump.contains(&format!("parts: [\"App\", \"{class}\"]")),
            "`namespace\\{class}` did not resolve to `App\\{class}`: {dump}"
        );
    }
}
