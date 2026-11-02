//! Purpose:
//! Integration or regression tests for parser AST coverage of extensions, including packed class and typed buffer decl, buffer packed element field access, and ptr cast.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP snippets are parsed and assertions inspect AST shape, precedence, or expected parse failures.

use super::*;

/// Parses a `packed class` declaration followed by a `buffer<T>` typed assignment using `buffer_new<T>(len)`.
/// Verifies the AST shape of `PackedClassDecl` (name, two float fields) and `TypedAssign` with `BufferNew` expr.
#[test]
fn test_parse_packed_class_and_typed_buffer_decl() {
    let stmts = parse_source(
        "<?php packed class Vec2 { public float $x; public float $y; } buffer<Vec2> $points = buffer_new<Vec2>(4);",
    );
    assert_eq!(stmts.len(), 2);

    match &stmts[0].kind {
        StmtKind::PackedClassDecl { name, fields } => {
            assert_eq!(name, "Vec2");
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "x");
            assert_eq!(fields[0].type_expr, TypeExpr::Float);
            assert_eq!(fields[1].name, "y");
            assert_eq!(fields[1].type_expr, TypeExpr::Float);
        }
        other => panic!("expected packed class decl, got {:?}", other),
    }

    match &stmts[1].kind {
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => {
            assert_eq!(name, "points");
            assert_eq!(
                type_expr,
                &TypeExpr::Buffer(Box::new(TypeExpr::Named(Name::unqualified("Vec2"))))
            );
            match &value.kind {
                ExprKind::BufferNew { element_type, len } => {
                    assert_eq!(element_type, &TypeExpr::Named(Name::unqualified("Vec2")));
                    assert_eq!(len.kind, ExprKind::IntLiteral(4));
                }
                other => panic!("expected buffer_new, got {:?}", other),
            }
        }
        other => panic!("expected typed assign, got {:?}", other),
    }
}

/// Parses `$points[0]->x` (array access into buffer variable, then property access).
/// Verifies the AST shape: `Echo` → `PropertyAccess` with `ArrayAccess` on variable `points`.
#[test]
fn test_parse_buffer_packed_element_field_access() {
    let stmts = parse_source("<?php echo $points[0]->x;");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Echo(expr) => match &expr.kind {
            ExprKind::PropertyAccess { object, property } => {
                assert_eq!(property, "x");
                match &object.kind {
                    ExprKind::ArrayAccess { array, index } => {
                        assert_eq!(array.kind, ExprKind::Variable("points".into()));
                        assert_eq!(index.kind, ExprKind::IntLiteral(0));
                    }
                    other => panic!("expected packed buffer element access, got {:?}", other),
                }
            }
            other => panic!("expected property access, got {:?}", other),
        },
        other => panic!("expected echo, got {:?}", other),
    }
}

// --- Assignment ---

/// Parses `$q = ptr_cast<Point>($p)` as an assignment with `PtrCast` expr.
/// Verifies `PtrCast` captures the target type name and the inner variable expression.
#[test]
fn test_parse_ptr_cast() {
    let stmts = parse_source("<?php $q = ptr_cast<Point>($p);");
    match &stmts[0].kind {
        StmtKind::Assign { value, .. } => match &value.kind {
            ExprKind::PtrCast { target_type, expr } => {
                assert_eq!(target_type, "Point");
                assert!(matches!(expr.kind, ExprKind::Variable(_)));
            }
            _ => panic!("Expected PtrCast"),
        },
        _ => panic!("Expected Assign"),
    }
}

/// Parses a sequence of ptr-family builtin calls: `ptr_null`, `ptr`, `ptr_is_null`, `ptr_get`,
/// `ptr_set`, `ptr_offset`, `ptr_sizeof`, `ptr_read16`, `ptr_write16`, `ptr_read_string`, `ptr_write_string`.
/// Verifies each is parsed as a `FunctionCall` expr (not a specialized ptr variant).
#[test]
fn test_parse_ptr_builtins_as_function_calls() {
    let stmts = parse_source("<?php ptr_null(); ptr($x); ptr_is_null($p); ptr_get($p); ptr_set($p, 1); ptr_offset($p, 8); ptr_sizeof(\"int\"); ptr_read16($p); ptr_write16($p, 1); ptr_read_string($p, 4); ptr_write_string($p, \"hi\");");
    // All should parse as FunctionCall
    for stmt in &stmts {
        match &stmt.kind {
            StmtKind::ExprStmt(expr) => match &expr.kind {
                ExprKind::FunctionCall { .. } => {}
                _ => panic!("Expected FunctionCall, got {:?}", expr.kind),
            },
            _ => panic!("Expected ExprStmt"),
        }
    }
}

/// Parses `<?php extern function abs(int $n): int;` as an `ExternFunctionDecl`.
/// Verifies one int parameter, int return type, and no associated library.
#[test]
fn test_parse_extern_function() {
    let stmts = parse_source("<?php extern function abs(int $n): int;");
    match &stmts[0].kind {
        StmtKind::ExternFunctionDecl {
            name,
            params,
            return_type,
            library,
        } => {
            assert_eq!(name, "abs");
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "n");
            assert!(matches!(return_type, elephc::parser::ast::CType::Int));
            assert!(library.is_none());
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
}

/// Parses an `extern` block containing two function declarations (`init` and `cleanup`)
/// with library `"curl"`. Verifies each becomes an `ExternFunctionDecl` with the correct library.
#[test]
fn test_parse_extern_block() {
    let stmts = parse_source(
        r#"<?php extern "curl" { function init(): ptr; function cleanup(ptr $h): void; }"#,
    );
    assert_eq!(stmts.len(), 2);
    match &stmts[0].kind {
        StmtKind::ExternFunctionDecl { name, library, .. } => {
            assert_eq!(name, "init");
            assert_eq!(library.as_deref(), Some("curl"));
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
    match &stmts[1].kind {
        StmtKind::ExternFunctionDecl { name, library, .. } => {
            assert_eq!(name, "cleanup");
            assert_eq!(library.as_deref(), Some("curl"));
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
}

/// Parses extern functions, classes, globals, and an expanding library block inside braced
/// namespaces while preserving each namespace body's declaration list.
#[test]
fn test_parse_extern_declarations_inside_braced_namespaces() {
    let stmts = parse_source(
        r#"<?php
namespace App {
    extern function abs(i32 $value): i32;
    extern class Point { public int $x; }
    extern global int $errno;
    extern "c" { function labs(int $value): int; function llabs(int $value): int; }
}
namespace Other {
    extern function getpid(): i32;
}
"#,
    );
    assert_eq!(stmts.len(), 2);

    let StmtKind::NamespaceBlock { name, body } = &stmts[0].kind else {
        panic!("expected first namespace block, got {:?}", stmts[0].kind);
    };
    assert_eq!(name.as_ref().map(Name::as_str), Some("App"));
    assert_eq!(body.len(), 5);
    assert!(matches!(
        &body[0].kind,
        StmtKind::ExternFunctionDecl { name, library, .. }
            if name == "abs" && library.is_none()
    ));
    assert!(matches!(
        &body[1].kind,
        StmtKind::ExternClassDecl { name, .. } if name == "Point"
    ));
    assert!(matches!(
        &body[2].kind,
        StmtKind::ExternGlobalDecl { name, .. } if name == "errno"
    ));
    for stmt in &body[3..] {
        assert!(matches!(
            &stmt.kind,
            StmtKind::ExternFunctionDecl { library, .. }
                if library.as_deref() == Some("c")
        ));
    }

    let StmtKind::NamespaceBlock { name, body } = &stmts[1].kind else {
        panic!("expected second namespace block, got {:?}", stmts[1].kind);
    };
    assert_eq!(name.as_ref().map(Name::as_str), Some("Other"));
    assert_eq!(body.len(), 1);
    assert!(matches!(
        &body[0].kind,
        StmtKind::ExternFunctionDecl { name, .. } if name == "getpid"
    ));
}

/// Parses `extern class Point { public int $x; public float $y; }` as an `ExternClassDecl`.
/// Verifies name and field list (name, type-kind) are captured correctly.
#[test]
fn test_parse_extern_class() {
    let stmts = parse_source("<?php extern class Point { public int $x; public float $y; }");
    match &stmts[0].kind {
        StmtKind::ExternClassDecl { name, fields } => {
            assert_eq!(name, "Point");
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "x");
            assert_eq!(fields[1].name, "y");
        }
        _ => panic!("Expected ExternClassDecl"),
    }
}

/// Parses an extern class whose fields use explicit C widths, keeping each field's C type.
#[test]
fn test_parse_extern_class_with_c_widths() {
    use elephc::parser::ast::{CIntWidth, CType};
    let stmts = parse_source(
        "<?php extern class Event { public u32 $type; public u16 $mod; public bool $down; public f32 $x; }",
    );
    match &stmts[0].kind {
        StmtKind::ExternClassDecl { fields, .. } => {
            let types: Vec<&CType> = fields.iter().map(|field| &field.c_type).collect();
            assert_eq!(
                types,
                vec![
                    &CType::IntWidth(CIntWidth::U32),
                    &CType::IntWidth(CIntWidth::U16),
                    &CType::Bool,
                    &CType::F32,
                ]
            );
        }
        _ => panic!("Expected ExternClassDecl"),
    }
}

/// Parses `extern global int $errno;` as an `ExternGlobalDecl`.
/// Verifies the name and C type are captured correctly.
#[test]
fn test_parse_extern_global() {
    let stmts = parse_source("<?php extern global int $errno;");
    match &stmts[0].kind {
        StmtKind::ExternGlobalDecl { name, c_type } => {
            assert_eq!(name, "errno");
            assert!(matches!(c_type, elephc::parser::ast::CType::Int));
        }
        _ => panic!("Expected ExternGlobalDecl"),
    }
}

/// Parses every explicit C integer width in extern parameters and returns, with `i64` folding
/// to the plain 64-bit `int`.
#[test]
fn test_parse_extern_int_widths() {
    use elephc::parser::ast::{CIntWidth, CType};
    let stmts = parse_source(
        "<?php extern function widths(i8 $a, u8 $b, i16 $c, u16 $d, i32 $e, u32 $f, i64 $g, u64 $h): u32;",
    );
    match &stmts[0].kind {
        StmtKind::ExternFunctionDecl { params, return_type, .. } => {
            let widths: Vec<&CType> = params.iter().map(|param| &param.c_type).collect();
            assert_eq!(
                widths,
                vec![
                    &CType::IntWidth(CIntWidth::I8),
                    &CType::IntWidth(CIntWidth::U8),
                    &CType::IntWidth(CIntWidth::I16),
                    &CType::IntWidth(CIntWidth::U16),
                    &CType::IntWidth(CIntWidth::I32),
                    &CType::IntWidth(CIntWidth::U32),
                    &CType::Int,
                    &CType::IntWidth(CIntWidth::U64),
                ]
            );
            assert_eq!(return_type, &CType::IntWidth(CIntWidth::U32));
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
}

/// Parses the C `float` width: `f32` is its own C type while `f64` and `double` fold to `float`.
#[test]
fn test_parse_extern_float_widths() {
    use elephc::parser::ast::CType;
    let stmts = parse_source("<?php extern function fw(f32 $a, f64 $b, double $c): f32;");
    match &stmts[0].kind {
        StmtKind::ExternFunctionDecl { params, return_type, .. } => {
            let types: Vec<&CType> = params.iter().map(|param| &param.c_type).collect();
            assert_eq!(types, vec![&CType::F32, &CType::Float, &CType::Float]);
            assert_eq!(return_type, &CType::F32);
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
}

/// Parses `?string` on extern parameters and returns as `CType::NullableStr`, in a library
/// block as SDL3 declares it, while the neighbouring `ptr` and plain `string` stay unchanged.
#[test]
fn test_parse_extern_nullable_string_param_and_return() {
    use elephc::parser::ast::CType;
    let stmts = parse_source(
        r#"<?php extern "SDL3" { function SDL_CreateRenderer(ptr $window, ?string $name): ptr; function SDL_GetError(): ?string; function SDL_SetHint(string $name, ?string $value): bool; }"#,
    );
    assert_eq!(stmts.len(), 3);
    match &stmts[0].kind {
        StmtKind::ExternFunctionDecl { params, return_type, .. } => {
            let types: Vec<&CType> = params.iter().map(|param| &param.c_type).collect();
            assert_eq!(types, vec![&CType::Ptr, &CType::NullableStr]);
            assert_eq!(return_type, &CType::Ptr);
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
    match &stmts[1].kind {
        StmtKind::ExternFunctionDecl { params, return_type, .. } => {
            assert!(params.is_empty());
            assert_eq!(return_type, &CType::NullableStr);
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
    match &stmts[2].kind {
        StmtKind::ExternFunctionDecl { params, .. } => {
            let types: Vec<&CType> = params.iter().map(|param| &param.c_type).collect();
            assert_eq!(types, vec![&CType::Str, &CType::NullableStr]);
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
}

/// Rejects `?` before any extern type other than `string`, and a dangling `?`, because only a
/// `char*` gains a NULL mapping (a `ptr` already carries NULL as the address 0).
#[test]
fn test_parse_extern_nullable_rejects_non_string_types() {
    assert!(parse_fails("<?php extern function f(?int $x): int;"));
    assert!(parse_fails("<?php extern function f(?ptr $x): int;"));
    assert!(parse_fails("<?php extern function f(): ?ptr<Point>;"));
    assert!(parse_fails("<?php extern function f(? $x): int;"));
}

/// Parses `extern "m" function sin(float $x): float;` as an `ExternFunctionDecl` with library `"m"`.
/// Verifies name, library, param types, and return type are all captured correctly.
#[test]
fn test_parse_extern_lib_function() {
    let stmts = parse_source(r#"<?php extern "m" function sin(float $x): float;"#);
    match &stmts[0].kind {
        StmtKind::ExternFunctionDecl { name, library, .. } => {
            assert_eq!(name, "sin");
            assert_eq!(library.as_deref(), Some("m"));
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
}

/// Parses `extern function signal(int $sig, callable $handler): ptr;`.
/// Verifies the second parameter has `CType::Callable`.
#[test]
fn test_parse_extern_callable_param() {
    let stmts = parse_source(r#"<?php extern function signal(int $sig, callable $handler): ptr;"#);
    match &stmts[0].kind {
        StmtKind::ExternFunctionDecl { params, .. } => {
            assert_eq!(params.len(), 2);
            assert!(matches!(
                params[1].c_type,
                elephc::parser::ast::CType::Callable
            ));
        }
        _ => panic!("Expected ExternFunctionDecl"),
    }
}

/// Parses `array<int>` in parameter and return position.
/// Verifies both resolve to `TypeExpr::Array` carrying the element type.
#[test]
fn test_parse_array_type_argument() {
    let stmts = parse_source("<?php function f(array<int> $a): array<string> { return []; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl {
            params,
            return_type,
            ..
        } => {
            assert_eq!(
                params[0].1,
                Some(TypeExpr::Array(Box::new(TypeExpr::Int))),
                "parameter kept its element type"
            );
            assert_eq!(
                return_type.as_ref(),
                Some(&TypeExpr::Array(Box::new(TypeExpr::Str))),
                "return type kept its element type"
            );
        }
        other => panic!("expected function decl, got {:?}", other),
    }
}

/// Parses a bare `array` hint and asserts it stays `TypeExpr::Named("array")`.
///
/// `synthetic_class::t_array` and `parser_agreement` both depend on the bare keyword NOT
/// producing `TypeExpr::Array`: that variant is reserved for element-typed forms, and
/// picking the other shape yields an AST no parse could produce.
#[test]
fn test_parse_bare_array_stays_named() {
    let stmts = parse_source("<?php function f(array $a) { return $a; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert_eq!(
                params[0].1,
                Some(TypeExpr::Named(Name::unqualified("array"))),
                "bare array must not become TypeExpr::Array"
            );
        }
        other => panic!("expected function decl, got {:?}", other),
    }
}

/// Parses a union element type inside `array<...>`.
#[test]
fn test_parse_array_type_argument_union_element() {
    let stmts = parse_source("<?php function f(array<int|string> $a) { return $a; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert_eq!(
                params[0].1,
                Some(TypeExpr::Array(Box::new(TypeExpr::Union(vec![
                    TypeExpr::Int,
                    TypeExpr::Str,
                ])))),
            );
        }
        other => panic!("expected function decl, got {:?}", other),
    }
}

/// `array<>` lexes as the single `<>` token (PHP's `!=` alias), so the empty type argument
/// list has to be rejected explicitly rather than read as a bare `array`.
#[test]
fn test_parse_array_type_argument_rejects_empty_list() {
    assert!(parse_fails("<?php function f(array<> $a) { return $a; }"));
}

/// An unterminated type argument list is a parse error.
#[test]
fn test_parse_array_type_argument_rejects_missing_close() {
    assert!(parse_fails("<?php function f(array<int $a) { return $a; }"));
}

/// Two type arguments select the associative form, which has hash storage rather than a
/// packed element vector and is therefore a distinct `TypeExpr` variant.
#[test]
fn test_parse_assoc_array_type_arguments() {
    let stmts = parse_source("<?php function f(array<string, int> $m) { return $m; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert_eq!(
                params[0].1,
                Some(TypeExpr::AssocArray {
                    key: Box::new(TypeExpr::Str),
                    value: Box::new(TypeExpr::Int),
                }),
            );
        }
        other => panic!("expected function decl, got {:?}", other),
    }
}

/// The associative form nests: a class-typed value keeps its name for later resolution.
#[test]
fn test_parse_assoc_array_named_value() {
    let stmts = parse_source("<?php function f(array<int, Foo> $m) { return $m; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert_eq!(
                params[0].1,
                Some(TypeExpr::AssocArray {
                    key: Box::new(TypeExpr::Int),
                    value: Box::new(TypeExpr::Named(Name::unqualified("Foo"))),
                }),
            );
        }
        other => panic!("expected function decl, got {:?}", other),
    }
}

/// A trailing comma with no value type is a parse error, not a one-argument form.
#[test]
fn test_parse_assoc_array_rejects_missing_value_type() {
    assert!(parse_fails("<?php function f(array<string,> $m) { return $m; }"));
}

/// Three type arguments have no meaning; the parser stops after the value type and the
/// unexpected comma fails the `>` expectation.
#[test]
fn test_parse_array_type_arguments_reject_three() {
    assert!(parse_fails(
        "<?php function f(array<string, int, bool> $m) { return $m; }"
    ));
}

/// Parses a type parameter list on a function declaration.
#[test]
fn test_parse_function_type_params() {
    let stmts = parse_source("<?php function identity<T>(T $value): T { return $value; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl {
            type_params,
            params,
            return_type,
            ..
        } => {
            assert_eq!(
                type_params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
                vec!["T"]
            );
            assert_eq!(
                params[0].1,
                Some(TypeExpr::Named(Name::unqualified("T"))),
                "a type parameter stays a plain named type until substitution"
            );
            assert_eq!(
                return_type.as_ref(),
                Some(&TypeExpr::Named(Name::unqualified("T")))
            );
        }
        other => panic!("expected function decl, got {:?}", other),
    }
}

/// Several type parameters are comma-separated.
#[test]
fn test_parse_function_multiple_type_params() {
    let stmts = parse_source("<?php function pair<K, V>(K $k, V $v): array { return [$k, $v]; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { type_params, .. } => {
            assert_eq!(
                type_params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
                vec!["K", "V"]
            );
        }
        other => panic!("expected function decl, got {:?}", other),
    }
}

/// An ordinary function has no type parameters, and the `<` probe must not consume anything.
#[test]
fn test_parse_plain_function_has_no_type_params() {
    let stmts = parse_source("<?php function plain(int $x): int { return $x; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl {
            type_params,
            params,
            ..
        } => {
            assert!(type_params.is_empty());
            assert_eq!(params[0].1, Some(TypeExpr::Int));
        }
        other => panic!("expected function decl, got {:?}", other),
    }
}

/// `<>` lexes as the single `LessGreater` token, so the empty list is rejected explicitly.
#[test]
fn test_parse_function_type_params_reject_empty_list() {
    assert!(parse_fails("<?php function f<>($x) { return $x; }"));
}

/// A repeated type parameter name would make substitution ambiguous.
#[test]
fn test_parse_function_type_params_reject_duplicate() {
    assert!(parse_fails(
        "<?php function f<T, T>(T $x): T { return $x; }"
    ));
}

/// Parses an upper bound and a default, the RFC's `<T : Bound>` and `<K = Default>` spellings.
///
/// The `:` is unambiguous inside a type parameter list: a return type's `:` comes after the
/// parameter list's `)`, and an enum's backing `:` never appears inside `<...>`.
#[test]
fn test_parse_function_type_param_bound_and_default() {
    let stmts = parse_source(
        "<?php function f<T : Entity, K = string>(T $a, int $b): int { return $b; }",
    );
    match &stmts[0].kind {
        StmtKind::FunctionDecl { type_params, .. } => {
            assert_eq!(type_params.len(), 2);
            assert_eq!(type_params[0].name, "T");
            assert_eq!(
                type_params[0].bound,
                Some(TypeExpr::Named(Name::unqualified("Entity")))
            );
            assert_eq!(type_params[0].default, None);
            assert_eq!(type_params[1].name, "K");
            assert_eq!(type_params[1].bound, None);
            assert_eq!(type_params[1].default, Some(TypeExpr::Str));
        }
        other => panic!("expected function decl, got {:?}", other),
    }
}

/// A bound and a default may both appear, bound first.
#[test]
fn test_parse_function_type_param_bound_then_default() {
    let stmts = parse_source(
        "<?php function f<T : Entity = Entity>(int $b): int { return $b; }",
    );
    match &stmts[0].kind {
        StmtKind::FunctionDecl { type_params, .. } => {
            assert_eq!(
                type_params[0].bound,
                Some(TypeExpr::Named(Name::unqualified("Entity")))
            );
            assert_eq!(
                type_params[0].default,
                Some(TypeExpr::Named(Name::unqualified("Entity")))
            );
        }
        other => panic!("expected function decl, got {:?}", other),
    }
}


// --- Generic classes and interfaces -------------------------------------------------

/// A class declares type parameters the same way a function does.
#[test]
fn test_parse_class_type_params() {
    let stmts = parse_source("<?php class Box<T> { private T $v; }");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            generics,
            properties,
            ..
        } => {
            let generics = generics.as_ref().expect("class is generic");
            assert_eq!(
                generics
                    .type_params
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>(),
                vec!["T"]
            );
            assert_eq!(
                properties[0].type_expr,
                Some(TypeExpr::Named(Name::unqualified("T"))),
                "a type parameter stays a plain named type until substitution"
            );
        }
        other => panic!("expected class decl, got {:?}", other),
    }
}

/// An interface declares them too, with a bound.
#[test]
fn test_parse_interface_type_params_with_bound() {
    let stmts = parse_source("<?php interface Repository<T: Entity> { public function f(): T; }");
    match &stmts[0].kind {
        StmtKind::InterfaceDecl { generics, .. } => {
            let generics = generics.as_ref().expect("interface is generic");
            assert_eq!(generics.type_params.len(), 1);
            assert_eq!(generics.type_params[0].name, "T");
            assert_eq!(
                generics.type_params[0].bound,
                Some(TypeExpr::Named(Name::unqualified("Entity")))
            );
        }
        other => panic!("expected interface decl, got {:?}", other),
    }
}

/// `implements Repository<User>` keeps the arguments on the interface they were written on.
#[test]
fn test_parse_implements_with_type_arguments() {
    let stmts = parse_source("<?php class R implements Repository<User>, Countable {}");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            generics,
            implements,
            ..
        } => {
            let generics = generics.as_ref().expect("inheritance carries type arguments");
            assert!(
                generics.type_params.is_empty(),
                "the class itself declares none"
            );
            assert_eq!(
                implements.iter().map(|n| n.as_str()).collect::<Vec<_>>(),
                vec!["Repository", "Countable"]
            );
            assert_eq!(
                generics.interface_args,
                vec![
                    vec![TypeExpr::Named(Name::unqualified("User"))],
                    Vec::new()
                ],
                "alignment is what keeps the arguments on the right interface"
            );
        }
        other => panic!("expected class decl, got {:?}", other),
    }
}

/// `extends Box<int>` records the parent's type arguments.
#[test]
fn test_parse_extends_with_type_arguments() {
    let stmts = parse_source("<?php class Small extends Box<int> {}");
    match &stmts[0].kind {
        StmtKind::ClassDecl {
            generics, extends, ..
        } => {
            assert_eq!(extends.as_ref().map(|n| n.as_str()), Some("Box"));
            assert_eq!(
                generics.as_ref().expect("parent carries arguments").extends_args,
                vec![TypeExpr::Int]
            );
        }
        other => panic!("expected class decl, got {:?}", other),
    }
}

/// An ordinary class stays `generics: None`, which is the contract every pre-generics pass
/// relies on.
#[test]
fn test_parse_ordinary_class_has_no_generics() {
    let stmts = parse_source("<?php class Plain implements Countable { public int $x = 1; }");
    match &stmts[0].kind {
        StmtKind::ClassDecl { generics, .. } => assert!(generics.is_none()),
        other => panic!("expected class decl, got {:?}", other),
    }
}

/// A class type carrying type arguments parses to `GenericClass`, head and arguments apart.
#[test]
fn test_parse_generic_class_type_in_a_parameter() {
    let stmts = parse_source("<?php function f(Box<int> $b) { return $b; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert_eq!(
                params[0].1,
                Some(TypeExpr::GenericClass {
                    name: Name::unqualified("Box"),
                    args: vec![TypeExpr::Int],
                })
            );
        }
        other => panic!("expected function decl, got {:?}", other),
    }
}

/// `Box<Box<int>>` ends in one `>>` token, which the parser splits back into two closes.
#[test]
fn test_parse_nested_generic_class_type_splits_shift_token() {
    let stmts = parse_source("<?php function f(Box<Box<int>> $b) { return $b; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert_eq!(
                params[0].1,
                Some(TypeExpr::GenericClass {
                    name: Name::unqualified("Box"),
                    args: vec![TypeExpr::GenericClass {
                        name: Name::unqualified("Box"),
                        args: vec![TypeExpr::Int],
                    }],
                })
            );
        }
        other => panic!("expected function decl, got {:?}", other),
    }
}

/// The same split applies to the array form, which could not nest before.
#[test]
fn test_parse_nested_array_type_arguments_split_shift_token() {
    let stmts = parse_source("<?php function f(array<array<int>> $m) { return $m; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert_eq!(
                params[0].1,
                Some(TypeExpr::Array(Box::new(TypeExpr::Array(Box::new(
                    TypeExpr::Int
                )))))
            );
        }
        other => panic!("expected function decl, got {:?}", other),
    }
}

/// Three levels lex as `>>` then `>`, and the credit rule covers that too.
#[test]
fn test_parse_three_level_nesting_splits_both_tokens() {
    let stmts = parse_source("<?php function f(array<array<array<int>>> $m) { return $m; }");
    match &stmts[0].kind {
        StmtKind::FunctionDecl { params, .. } => {
            assert_eq!(
                params[0].1,
                Some(TypeExpr::Array(Box::new(TypeExpr::Array(Box::new(
                    TypeExpr::Array(Box::new(TypeExpr::Int))
                )))))
            );
        }
        other => panic!("expected function decl, got {:?}", other),
    }
}

/// `new Box<int>(...)` is its own node until instantiation rewrites it.
#[test]
fn test_parse_new_with_type_arguments() {
    let stmts = parse_source("<?php $b = new Box<int>(1);");
    let source = format!("{:?}", stmts);
    assert!(
        source.contains("NewGeneric"),
        "expected a NewGeneric node, got {source}"
    );
}

/// `Box<>` lexes as the single `<>` token, the same trap `array<>` springs.
#[test]
fn test_parse_empty_class_type_arguments_are_rejected() {
    assert!(parse_fails("<?php function f(Box<> $b) { return $b; }"));
}

/// Static access on a generic class type parses to a generic receiver.
#[test]
fn test_parse_static_access_on_a_generic_class_type() {
    let source = format!("{:?}", parse_source("<?php echo Box<int>::of(1);"));
    assert!(
        source.contains("Generic(GenericClass"),
        "expected a generic static receiver, got {source}"
    );
}

/// The recognition must not swallow an ordinary comparison chain, which is the same tokens
/// without the `::`.
#[test]
fn test_parse_comparison_chain_is_not_mistaken_for_type_arguments() {
    let stmts = parse_source("<?php $x = A < $b && $b > $c;");
    assert_eq!(stmts.len(), 1, "expected one statement");
}

/// An enum records inherited type arguments on its `implements` clause, so a generic interface
/// parses instead of being rejected. (`EnumDecl` gained a `generics` field with no type
/// parameters of its own: an enum cannot BE generic, it can only implement one at a type.)
#[test]
fn test_parse_enum_implementing_a_generic_interface() {
    let source = format!(
        "{:?}",
        parse_source("<?php enum Status implements Holder<int> { case Ready; }")
    );
    assert!(
        source.contains("type_params: [], extends_args: [], interface_args: [[Int]]"),
        "expected the implements clause to carry type arguments, got {source}"
    );
}

/// `instanceof Box<int>` parses to a generic target.
#[test]
fn test_parse_instanceof_a_generic_class_type() {
    let source = format!("{:?}", parse_source("<?php var_dump($x instanceof Box<int>);"));
    assert!(
        source.contains("Generic(GenericClass"),
        "expected a generic instanceof target, got {source}"
    );
}

/// The recognition declines whatever could still be a comparison operand, so a sequence this
/// parser accepts today keeps its old reading.
#[test]
fn test_parse_instanceof_followed_by_an_operand_stays_a_comparison() {
    let source = format!("{:?}", parse_source("<?php $r = $x instanceof Foo < BAR > $z;"));
    assert!(
        !source.contains("Generic(GenericClass"),
        "expected two comparisons, got {source}"
    );
}

/// Variance has two spellings, and the words need one token of lookahead.
#[test]
fn test_parses_word_variance_markers() {
    assert!(!parse_fails("<?php class Box<out T> { public function get(): T { return $this->v; } }"));
    assert!(!parse_fails("<?php class Sink<in T> { public function accept(T $v): void {} }"));
}

/// `out` is an ordinary PHP identifier, so a type parameter may still be CALLED `out`. What
/// separates the two is only whether another identifier follows.
#[test]
fn test_out_is_still_a_usable_type_parameter_name() {
    assert!(!parse_fails("<?php class Holder<out> { public function get(): out { return $this->v; } }"));
}

/// Preserves the pointer target through ordinary, buffer and extern type syntax.
#[test]
fn test_parse_nested_pointer_type_shapes() {
    let stmts = parse_source("<?php ptr<ptr<u8>> $p = ptr_null(); buffer<ptr<u8>> $b = buffer_new<ptr<u8>>(1); extern function f(ptr<ptr<u8>> $out): void;");
    match &stmts[0].kind {
        StmtKind::TypedAssign { type_expr, .. } => assert_eq!(type_expr, &TypeExpr::Ptr(Some(Name::unqualified("ptr<u8>")))),
        other => panic!("expected typed assignment, got {:?}", other),
    }
    match &stmts[1].kind {
        StmtKind::TypedAssign { type_expr, .. } => assert_eq!(type_expr, &TypeExpr::Buffer(Box::new(TypeExpr::Ptr(Some(Name::unqualified("u8")))))),
        other => panic!("expected typed assignment, got {:?}", other),
    }
    match &stmts[2].kind {
        StmtKind::ExternFunctionDecl { params, .. } => assert_eq!(params[0].c_type, elephc::parser::ast::CType::TypedPtr("ptr<u8>".into())),
        other => panic!("expected extern declaration, got {:?}", other),
    }
    parse_source("<?php function f(pointer<pointer<u8>> $p): void {} function g(ptr<pointer<u8>> $p): void {}");
}

/// Parses pointer elements when consecutive type argument closers form a shift token.
#[test]
fn test_buffer_pointer_type_closers() {
    parse_source("<?php function f(buffer<ptr<u8>> $b, ptr<ptr<u8>> $p): buffer<ptr<u8>> { return buffer_new<ptr<u8>>(1); }");
}

/// Parses pointer-to-pointer parameters in native declarations.
#[test]
fn test_extern_nested_pointer_type() {
    parse_source("<?php extern \"SDL3\" { function SDL_LoadWAV(string $path, ptr $spec, ptr<ptr<u8>> $data, ptr<u32> $length): bool; }");
}

/// Rejects an unmatched closing angle in a native pointer type.
#[test]
fn test_extern_nested_pointer_unbalanced_close() {
    assert!(parse_fails("<?php extern function f(ptr<ptr<u8>>> $p): void;"));
    assert!(parse_fails("<?php extern function f(ptr<ptr<u8> $p): void;"));
}
