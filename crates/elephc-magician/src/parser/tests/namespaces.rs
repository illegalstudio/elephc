//! Purpose:
//! Parser tests for namespaces and import resolution inside eval fragments.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases assert function, constant, class, and grouped imports.

use super::support::*;

/// Verifies dynamic function declarations preserve name, parameters, and body.
#[test]
fn parse_fragment_accepts_function_declaration_source() {
    let program =
        parse_fragment(br#"function dyn($x) { return $x + 1; }"#).expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::FunctionDecl {
            name: "dyn".to_string(),
            source_location: Some(EvalSourceLocation::single_line(1)),
            attributes: Vec::new(),
            params: vec!["x".to_string()],
            parameter_attributes: vec![Vec::new()],
            parameter_types: vec![None],
            parameter_defaults: vec![None],
            parameter_is_by_ref: vec![false],
            parameter_is_variadic: vec![false],
            return_type: None,
            body: vec![EvalStmt::Return(Some(EvalExpr::Binary {
                op: EvalBinOp::Add,
                left: Box::new(EvalExpr::LoadVar("x".to_string())),
                right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
            }))],
        }]
    );
}
/// Verifies semicolon namespace declarations qualify functions and unqualified calls.
#[test]
fn parse_fragment_accepts_semicolon_namespace_source() {
    let program = parse_fragment(
        br#"namespace Eval\Ns;
function dyn() { return __NAMESPACE__; }
return dyn();"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::FunctionDecl {
                name: "Eval\\Ns\\dyn".to_string(),
                source_location: Some(EvalSourceLocation::single_line(2)),
                attributes: Vec::new(),
                params: Vec::new(),
                parameter_attributes: Vec::new(),
                parameter_types: Vec::new(),
                parameter_defaults: Vec::new(),
                parameter_is_by_ref: Vec::new(),
                parameter_is_variadic: Vec::new(),
                return_type: None,
                body: vec![EvalStmt::Return(Some(EvalExpr::Const(EvalConst::String(
                    "Eval\\Ns".to_string()
                ))))],
            },
            EvalStmt::Return(Some(EvalExpr::NamespacedCall {
                name: "eval\\ns\\dyn".to_string(),
                fallback_name: "dyn".to_string(),
                args: Vec::new(),
            })),
        ]
    );
}
/// Verifies braced namespace declarations restore the previous namespace afterward.
#[test]
fn parse_fragment_accepts_braced_namespace_source() {
    let program = parse_fragment(
        br#"namespace Eval\Block {
    class Box {}
    return new Box();
}
return Box;"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::ClassDecl(EvalClass::new("Eval\\Block\\Box", Vec::new(), Vec::new())),
            EvalStmt::Return(Some(EvalExpr::NewObject {
                class_name: "Eval\\Block\\Box".to_string(),
                args: Vec::new(),
            })),
            EvalStmt::Return(Some(EvalExpr::ConstFetch("Box".to_string()))),
        ]
    );
}
/// Verifies namespace import declarations resolve functions, constants, and class aliases.
#[test]
fn parse_fragment_accepts_namespace_use_imports() {
    let program = parse_fragment(
        br#"namespace Eval\UseNs;
use function Lib\strlen as Alias;
use const Lib\VALUE as LocalValue;
use Lib\Box as BoxAlias;
return Alias(LocalValue, new BoxAlias\Inner());"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Call {
            name: "lib\\strlen".to_string(),
            args: vec![
                EvalCallArg::positional(EvalExpr::ConstFetch("Lib\\VALUE".to_string())),
                EvalCallArg::positional(EvalExpr::NewObject {
                    class_name: "Lib\\Box\\Inner".to_string(),
                    args: Vec::new(),
                }),
            ],
        }))]
    );
}
/// Verifies namespace class imports apply to eval class relation clauses.
#[test]
fn parse_fragment_accepts_imported_class_relations() {
    let program = parse_fragment(
        br#"namespace Eval\UseNs;
use Lib\Base as BaseAlias;
use Lib\Contracts\Iface;
class Child extends BaseAlias implements Iface, \Shared\Root {}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(EvalClass::with_relations(
            "Eval\\UseNs\\Child",
            Some("Lib\\Base".to_string()),
            vec![
                "Lib\\Contracts\\Iface".to_string(),
                "Shared\\Root".to_string()
            ],
            Vec::new(),
            Vec::new(),
        ))]
    );
}
/// Verifies namespace imports apply to eval interface parent clauses.
#[test]
fn parse_fragment_accepts_imported_interface_parents() {
    let program = parse_fragment(
        br#"namespace Eval\UseNs;
use Lib\Contracts\BaseIface;
interface LocalIface extends BaseIface, \Shared\RootIface {
    function read();
}"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::InterfaceDecl(EvalInterface::new(
            "Eval\\UseNs\\LocalIface",
            vec![
                "Lib\\Contracts\\BaseIface".to_string(),
                "Shared\\RootIface".to_string()
            ],
            vec![EvalInterfaceMethod::new("read", Vec::new())],
        ))]
    );
}
/// Verifies grouped namespace imports resolve functions, constants, and class aliases.
#[test]
fn parse_fragment_accepts_grouped_namespace_use_imports() {
    let program = parse_fragment(
        br#"namespace Eval\UseNs;
use Lib\{Box as BoxAlias, Sub\Thing, function imported_func as Alias};
use const Lib\{VALUE as LocalValue, OTHER};
return Alias(LocalValue, OTHER, new BoxAlias\Inner(), new Thing());"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::Return(Some(EvalExpr::Call {
            name: "lib\\imported_func".to_string(),
            args: vec![
                EvalCallArg::positional(EvalExpr::ConstFetch("Lib\\VALUE".to_string())),
                EvalCallArg::positional(EvalExpr::ConstFetch("Lib\\OTHER".to_string())),
                EvalCallArg::positional(EvalExpr::NewObject {
                    class_name: "Lib\\Box\\Inner".to_string(),
                    args: Vec::new(),
                }),
                EvalCallArg::positional(EvalExpr::NewObject {
                    class_name: "Lib\\Sub\\Thing".to_string(),
                    args: Vec::new(),
                }),
            ],
        }))]
    );
}
/// Verifies typed grouped namespace imports reject mixed per-entry kinds.
#[test]
fn parse_fragment_rejects_mixed_kind_typed_grouped_use_imports() {
    assert_eq!(
        parse_fragment(br#"use function Lib\{target, const VALUE};"#),
        Err(EvalParseError::UnexpectedToken)
    );
}
/// Verifies namespace blocks restore imports when control returns to the outer namespace.
#[test]
fn parse_fragment_restores_use_imports_after_namespace_block() {
    let program = parse_fragment(
        br#"namespace Eval\Outer;
use function Lib\outer_func;
namespace Eval\Block {
    use function Lib\inner_func as alias;
    return alias();
}
return outer_func();"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[
            EvalStmt::Return(Some(EvalExpr::Call {
                name: "lib\\inner_func".to_string(),
                args: Vec::new(),
            })),
            EvalStmt::Return(Some(EvalExpr::Call {
                name: "lib\\outer_func".to_string(),
                args: Vec::new(),
            })),
        ]
    );
}
/// Verifies imported aliases remain visible while parsing eval-declared function bodies.
#[test]
fn parse_fragment_applies_use_imports_inside_function_body() {
    let program = parse_fragment(
        br#"namespace Eval\UseNs;
use function Lib\target as alias;
function dyn() { return alias(); }"#,
    )
    .expect("fragment should parse");
    assert_eq!(
        program.statements(),
        &[EvalStmt::FunctionDecl {
            name: "Eval\\UseNs\\dyn".to_string(),
            source_location: Some(EvalSourceLocation::single_line(3)),
            attributes: Vec::new(),
            params: Vec::new(),
            parameter_attributes: Vec::new(),
            parameter_types: Vec::new(),
            parameter_defaults: Vec::new(),
            parameter_is_by_ref: Vec::new(),
            parameter_is_variadic: Vec::new(),
            return_type: None,
            body: vec![EvalStmt::Return(Some(EvalExpr::Call {
                name: "lib\\target".to_string(),
                args: Vec::new(),
            }))],
        }]
    );
}
/// Verifies import declarations are rejected inside eval-declared function bodies.
#[test]
fn parse_fragment_rejects_use_import_inside_function_body() {
    assert_eq!(
        parse_fragment(br#"function dyn() { use function Lib\target; }"#),
        Err(EvalParseError::UnsupportedConstruct)
    );
}
