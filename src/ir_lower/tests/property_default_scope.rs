//! Purpose:
//! Verifies lexical default binding and deferred trait errors across the supported target matrix.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Declaration-only tests use the full frontend, not executable reachability pruning.
//! - Every target must emit the same catchable error and valid default paths.

use crate::codegen::platform::Target;
use std::path::Path;

/// Valid and unbound trait defaults both pass through target-independent EIR.
#[test]
fn property_default_review_all_supported_targets() {
    let source = r#"<?php
trait Bound {
    public string $name = self::class;
    public static string $owner = parent::class;
    public array $names = [self::class, parent::class];
    public function __construct(public string $promoted = parent::class) {}
}

class Base {}
class Consumer extends Base { use Bound; }
trait Unbound { public string $name = parent::class; }
class UnboundConsumer { use Unbound; public static string $valid = 'ok'; }
trait Promoted { public function __construct(public string $name = parent::class) {} }
class PromotedConsumer { use Promoted; }
$consumer = new Consumer();
echo $consumer->name, Consumer::$owner, $consumer->promoted;
try { new UnboundConsumer(); } catch (Error $e) { echo $e->getMessage(); }
try { echo UnboundConsumer::$valid; } catch (Error $e) { echo $e->getMessage(); }
try { new PromotedConsumer(); } catch (Error $e) { echo $e->getMessage(); }
$explicit = new PromotedConsumer('explicit');
echo $explicit->name;
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// An initialization failure remains inside the active source-level handler.
#[test]
fn property_default_review_constructor_handler() {
    let module = super::lower_source(r#"<?php
trait Values { public string $bad = parent::class; }
class Consumer { use Values; public function __construct(string $value) {} }
function argument(): string { echo 'argument'; return 'value'; }
try { new Consumer(argument()); } catch (Error $e) { echo 'caught'; }
"#);
    let main = module.functions.iter().find(|function| function.name == "main").unwrap();
    assert!(main.instructions.iter().any(|inst| inst.op == crate::ir::Op::TryPushHandler));
    assert!(main.instructions.iter().any(|inst| inst.op == crate::ir::Op::ThrowException));
}

/// Production metadata-aware DCE retains constructor and static-write Error catches too.
#[test]
fn property_default_review_metadata_catch_reachability() {
    let source = r#"<?php
trait Values { public string $bad = parent::class; }
class Consumer {
    use Values;
    public static string $valid = 'ok';
    public function __construct(string $value = 'value') {}
}
try { new Consumer(); } catch (Error $e) { echo 'new'; }
try { Consumer::$valid = 'value'; } catch (Error $e) { echo 'write'; }
"#;
    let parsed = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let check = crate::types::check(&parsed).unwrap();
    let optimizer = crate::optimize::PostTypecheckOptimizer::new_with_type_metadata(
        &parsed, &check.functions, &check.classes, &check.interfaces,
    );
    let optimized = optimizer.eliminate_dead_code(parsed, check.local_binding_decision_spans());
    assert_eq!(optimized.iter().filter(|statement| {
        matches!(&statement.kind, crate::parser::ast::StmtKind::Try { catches, .. } if !catches.is_empty())
    }).count(), 2);
}
