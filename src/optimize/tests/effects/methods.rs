//! Purpose:
//! Regression tests for optimizer effects methods behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

/// Parses a compact PHP program for effect-summary regression tests.
fn parse_program(source: &str) -> Program {
    let tokens = crate::lexer::tokenize(source).expect("tokenize failed");
    crate::parser::parse(&tokens).expect("parse failed")
}

/// Tests that a static method whose body contains only a call to a pure builtin
/// (strlen) is classified as PURE in static_method_effects.
#[test]
fn test_program_static_method_effects_recognize_pure_static_methods() {
    let program = vec![Stmt::new(
        StmtKind::ClassDecl {
            name: "Util".to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties: Vec::new(),
            methods: vec![ClassMethod {
                name: "len3".to_string(),
                visibility: Visibility::Public,
                is_static: true,
                is_abstract: false,
                is_final: false,
                has_body: true,
                params: Vec::new(),
                param_attributes: Vec::new(),
                variadic: None,
                variadic_by_ref: false,
                variadic_type: None,
                return_type: None,
                by_ref_return: false,
                body: vec![Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::FunctionCall {
                            name: Name::from("strlen"),
                            args: vec![Expr::string_lit("abc")],
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                )],
                span: Span::dummy(),
                attributes: Vec::new(),
            }],
        constants: Vec::new(),
        },
        Span::dummy(),
    )];

    let (_, static_method_effects, _) = compute_program_callable_effects(&program);

    assert_eq!(
        static_method_effects.get("Util::len3"),
        Some(&Effect::PURE)
    );
}

/// Tests that a static method calling another static method via `self::` receiver
/// is correctly resolved and classified as PURE, provided the called method is pure.
#[test]
fn test_program_static_method_effects_resolve_self_receiver() {
    let program = vec![Stmt::new(
        StmtKind::ClassDecl {
            name: "Util".to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties: Vec::new(),
            methods: vec![
                ClassMethod {
                    name: "len3".to_string(),
                    visibility: Visibility::Public,
                    is_static: true,
                    is_abstract: false,
                    is_final: false,
                    has_body: true,
                    params: Vec::new(),
                    param_attributes: Vec::new(),
                    variadic: None,
                    variadic_by_ref: false,
                    variadic_type: None,
                    return_type: None,
                    by_ref_return: false,
                    body: vec![Stmt::new(
                        StmtKind::Return(Some(Expr::new(
                            ExprKind::FunctionCall {
                                name: Name::from("strlen"),
                                args: vec![Expr::string_lit("abc")],
                            },
                            Span::dummy(),
                        ))),
                        Span::dummy(),
                    )],
                    span: Span::dummy(),
                    attributes: Vec::new(),
                },
                ClassMethod {
                    name: "relay".to_string(),
                    visibility: Visibility::Public,
                    is_static: true,
                    is_abstract: false,
                    is_final: false,
                    has_body: true,
                    params: Vec::new(),
                    param_attributes: Vec::new(),
                    variadic: None,
                    variadic_by_ref: false,
                    variadic_type: None,
                    return_type: None,
                    by_ref_return: false,
                    body: vec![Stmt::new(
                        StmtKind::Return(Some(Expr::new(
                            ExprKind::StaticMethodCall {
                                receiver: StaticReceiver::Self_,
                                method: "len3".to_string(),
                                args: Vec::new(),
                            },
                            Span::dummy(),
                        ))),
                        Span::dummy(),
                    )],
                    span: Span::dummy(),
                    attributes: Vec::new(),
                },
            ],
        constants: Vec::new(),
        },
        Span::dummy(),
    )];

    let (_, static_method_effects, _) = compute_program_callable_effects(&program);

    assert_eq!(
        static_method_effects.get("Util::relay"),
        Some(&Effect::PURE)
    );
}

/// Tests that a static method in a child class calling a parent static method via
/// `parent::` receiver is correctly resolved and classified as PURE, provided the
/// called method is pure.
#[test]
fn test_program_static_method_effects_resolve_parent_receiver() {
    let program = vec![
        Stmt::new(
            StmtKind::ClassDecl {
                name: "Base".to_string(),
                extends: None,
                implements: Vec::new(),
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                trait_uses: Vec::new(),
                properties: Vec::new(),
                methods: vec![ClassMethod {
                    name: "len3".to_string(),
                    visibility: Visibility::Public,
                    is_static: true,
                    is_abstract: false,
                    is_final: false,
                    has_body: true,
                    params: Vec::new(),
                    param_attributes: Vec::new(),
                    variadic: None,
                    variadic_by_ref: false,
                    variadic_type: None,
                    return_type: None,
                    by_ref_return: false,
                    body: vec![Stmt::new(
                        StmtKind::Return(Some(Expr::new(
                            ExprKind::FunctionCall {
                                name: Name::from("strlen"),
                                args: vec![Expr::string_lit("abc")],
                            },
                            Span::dummy(),
                        ))),
                        Span::dummy(),
                    )],
                    span: Span::dummy(),
                    attributes: Vec::new(),
                }],
            constants: Vec::new(),
            },
            Span::dummy(),
        ),
        Stmt::new(
            StmtKind::ClassDecl {
                name: "Child".to_string(),
                extends: Some(Name::from("Base")),
                implements: Vec::new(),
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                trait_uses: Vec::new(),
                properties: Vec::new(),
                methods: vec![ClassMethod {
                    name: "relay".to_string(),
                    visibility: Visibility::Public,
                    is_static: true,
                    is_abstract: false,
                    is_final: false,
                    has_body: true,
                    params: Vec::new(),
                    param_attributes: Vec::new(),
                    variadic: None,
                    variadic_by_ref: false,
                    variadic_type: None,
                    return_type: None,
                    by_ref_return: false,
                    body: vec![Stmt::new(
                        StmtKind::Return(Some(Expr::new(
                            ExprKind::StaticMethodCall {
                                receiver: StaticReceiver::Parent,
                                method: "len3".to_string(),
                                args: Vec::new(),
                            },
                            Span::dummy(),
                        ))),
                        Span::dummy(),
                    )],
                    span: Span::dummy(),
                    attributes: Vec::new(),
                }],
            constants: Vec::new(),
            },
            Span::dummy(),
        ),
    ];

    let (_, static_method_effects, _) = compute_program_callable_effects(&program);

    assert_eq!(
        static_method_effects.get("Child::relay"),
        Some(&Effect::PURE)
    );
}

/// Tests that a private instance method whose body only calls pure `strlen` is summarized pure.
#[test]
fn test_program_private_instance_method_effects_recognize_private_methods() {
    let program = vec![Stmt::new(
        StmtKind::ClassDecl {
            name: "Util".to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties: Vec::new(),
            methods: vec![ClassMethod {
                name: "len3".to_string(),
                visibility: Visibility::Private,
                is_static: false,
                is_abstract: false,
                is_final: false,
                has_body: true,
                params: Vec::new(),
                param_attributes: Vec::new(),
                variadic: None,
                variadic_by_ref: false,
                variadic_type: None,
                return_type: None,
                by_ref_return: false,
                body: vec![Stmt::new(
                    StmtKind::Return(Some(Expr::new(
                        ExprKind::FunctionCall {
                            name: Name::from("strlen"),
                            args: vec![Expr::string_lit("abc")],
                        },
                        Span::dummy(),
                    ))),
                    Span::dummy(),
                )],
                span: Span::dummy(),
                attributes: Vec::new(),
            }],
        constants: Vec::new(),
        },
        Span::dummy(),
    )];

    let (_, _, instance_method_effects) = compute_program_callable_effects(&program);

    assert_eq!(
        instance_method_effects.get("Util::len3"),
        Some(&Effect::PURE)
    );
}

/// Verifies public `$this` calls keep the exact body plus declared-return boundary.
#[test]
fn test_program_instance_method_effects_resolve_public_this_dispatch() {
    let program = parse_program(
        r#"<?php
final class Util {
    public function len3(): int { return strlen("abc"); }
    public function relay(): int { return $this->len3(); }
}
"#,
    );

    let (_, _, instance_method_effects) = compute_program_callable_effects(&program);

    assert_eq!(
        instance_method_effects.get("Util::relay"),
        Some(&Effect::PURE.with_may_throw())
    );
}

/// Verifies virtual `$this` dispatch unions effects from every concrete override.
#[test]
fn test_program_instance_method_effects_union_virtual_overrides() {
    let program = parse_program(
        r#"<?php
class Base {
    public function value(): int { return 1; }
    public function relay(): int { return $this->value(); }
}
final class Child extends Base {
    public function value(): int { echo "called"; return 2; }
}
"#,
    );

    let (_, _, instance_method_effects) = compute_program_callable_effects(&program);
    let relay = instance_method_effects
        .get("Base::relay")
        .expect("missing Base::relay summary");

    assert!(relay.has_side_effects);
    assert!(relay.may_throw);
}

/// Verifies `eval()` prevents closed-world subclass assumptions for virtual dispatch.
#[test]
fn test_program_instance_method_effects_keep_eval_virtual_dispatch_conservative() {
    let program = parse_program(
        r#"<?php
class EvalBase {
    public function value(): int { return 1; }
    public function relay(): int { return $this->value(); }
}
eval($source);
"#,
    );

    let (_, _, instance_method_effects) = compute_program_callable_effects(&program);
    let relay = instance_method_effects
        .get("EvalBase::relay")
        .expect("missing EvalBase::relay summary");

    assert!(relay.has_side_effects);
    assert!(relay.may_throw);
}

/// Verifies direct property reads distinguish untyped slots, typed slots, and magic getters.
#[test]
fn test_program_instance_property_effects_refine_declared_and_magic_reads() {
    let program = parse_program(
        r#"<?php
final class Box {
    public $safe;
    public int $risky;
    public function safeRead() { return $this->safe; }
    public function riskyRead() { return $this->risky; }
    public function missingRead() { return $this->missing; }
}
final class MagicBox {
    public function __get(string $name) { echo $name; return 1; }
    public function read() { return $this->missing; }
}
final class HookBox {
    public int $computed { get { echo "hook"; return 1; } }
    public function read() { return $this->computed; }
}
final class LockedBox {
    private function secret() { return 1; }
}
final class Intruder {
    public function read() { return (new LockedBox())->secret(); }
}
final class Vault {
    private $hidden = 1;
}
final class PropertyIntruder {
    public function read() { return (new Vault())->hidden; }
}
trait TypedPropertyTrait {
    public int $traitRisk;
}
final class TraitBox {
    use TypedPropertyTrait;
    public function read() { return $this->traitRisk; }
}
"#,
    );

    let (_, _, instance_method_effects) = compute_program_callable_effects(&program);

    assert_eq!(
        instance_method_effects.get("Box::saferead"),
        Some(&Effect::PURE)
    );
    assert!(
        instance_method_effects
            .get("Box::riskyread")
            .is_some_and(|effect| effect.may_throw)
    );
    assert!(
        instance_method_effects
            .get("Box::missingread")
            .is_some_and(|effect| effect.has_side_effects && !effect.may_throw)
    );
    assert!(
        instance_method_effects
            .get("MagicBox::read")
            .is_some_and(|effect| effect.has_side_effects)
    );
    assert!(
        instance_method_effects
            .get("HookBox::read")
            .is_some_and(|effect| effect.has_side_effects && effect.may_throw)
    );
    assert!(
        instance_method_effects
            .get("Intruder::read")
            .is_some_and(|effect| effect.may_throw)
    );
    assert!(
        instance_method_effects
            .get("PropertyIntruder::read")
            .is_some_and(|effect| effect.may_throw)
    );
    assert!(
        instance_method_effects
            .get("TraitBox::read")
            .is_some_and(|effect| effect.may_throw)
    );
}
