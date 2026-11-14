//! Purpose:
//! Verifies declaration expressions retain physical source policy when rewritten or imported.
//!
//! Called from:
//! - Focused source-profile and default metadata unit tests.
//!
//! Key details:
//! - Origins describe producers even when the current thread or consuming class has another policy.

use crate::ast_visit::{walk_program, AstVisitor};
use crate::parser::ast::{Expr, Program, TypeExpr};
use crate::source::{SourceMode, SourceProfile};

/// Checks every declared value's root without confusing synthesized helper children with producers.
struct Origins { expected: SourceProfile, count: usize }

impl AstVisitor for Origins {
    /// Observes typed parameter, property and constant defaults through the shared AST traversal.
    fn declared_value(&mut self, _hint: &TypeExpr, value: &Expr) {
        assert_eq!(value.source_profile, self.expected, "{value:?}");
        self.count += 1;
    }
}

/// Requires each pipeline snapshot to retain all six source declaration values.
fn assert_origins(program: &Program, expected: SourceProfile) {
    let mut visitor = Origins { expected, count: 0 };
    walk_program(program, &mut visitor);
    assert_eq!(visitor.count, 6);
}

/// Physical producer profiles survive frontend rebuilding and optimization outside the parse scope.
#[test]
fn expression_origins_retain_declaration_profiles_through_frontend_passes() {
    for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
        for strict_types in [false, true] {
            let source = format!("<?php declare(strict_types={});
class OriginHost {{ public const int VALUE = 1 + 1; public int $value = self::VALUE;
    public static int $shared = self::VALUE; public function method(int $value = self::VALUE) {{}} }}
function originFunction(int $value = OriginHost::VALUE) {{}}
$closure = fn(int $value = 1 + 1) => $value;", u8::from(strict_types));
            let expected = SourceProfile { mode, strict_types };
            let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(&source).unwrap(), mode).unwrap();
            assert_origins(&program, expected);
            program = crate::magic_constants::substitute_file_and_scope_constants(program, std::path::Path::new("origin.php"));
            assert_origins(&program, expected);
            program = crate::conditional::apply(program, &std::collections::HashSet::new());
            assert_origins(&program, expected);
            program = crate::name_resolver::resolve(program).unwrap();
            assert_origins(&program, expected);
            program = crate::optimize::fold_constants(program);
            assert_origins(&program, expected);
            let checked = crate::types::checker::check_staged_semantics(&program,
                crate::codegen_support::platform::Target::detect_host(), elephc_semantics::SemanticProfile::PERMISSIVE).unwrap();
            assert_eq!(checked.functions["originFunction"].defaults[0].as_ref().unwrap().source_profile, expected);
            assert_eq!(checked.classes["OriginHost"].constants["VALUE"].source_profile, expected);
        }
    }
}

/// An imported trusted trait default keeps its own producer policy rather than the using class's.
#[test]
fn expression_origins_preserve_trusted_trait_and_signature_default_producers() {
    let internal = "<?php declare(strict_types=1); trait OriginTrait { public function method(float $value = 1 + 1): float { return $value; } } function trustedOrigin(float $value = 1 + 1): float { return $value; }";
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(internal).unwrap(), SourceMode::Internal).unwrap();
    program.extend(crate::parser::parse(&crate::lexer::tokenize("<?php class OriginConsumer { use OriginTrait; }").unwrap()).unwrap());
    let program = crate::optimize::fold_constants(crate::name_resolver::resolve(program).unwrap());
    let checked = crate::types::checker::check_staged_semantics(&program,
        crate::codegen_support::platform::Target::detect_host(), elephc_semantics::SemanticProfile::PERMISSIVE).unwrap();
    let expected = SourceProfile { mode: SourceMode::Internal, strict_types: true };
    let method = &checked.classes["OriginConsumer"].methods["method"];
    for sig in [method, &checked.functions["trustedOrigin"]] {
        assert_eq!(sig.defaults[0].as_ref().unwrap().source_profile, expected);
        assert_eq!(sig.default_value(0).unwrap().source_profile, expected);
    }
}
