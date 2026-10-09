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

/// Deferred default presence, scope and reflection guards share one target-independent contract.
fn deferred_reflection_defaults_on_target(target_name: &str) {
    let source = r#"<?php
trait Invalid {
    public string $bad = parent::class;
    public function labeled(string $value = parent::class): string { return $value; }
}
class Consumer { use Invalid; public static array $names = [parent::A]; }
class Promoted { public function __construct(public int $value = parent::A) {} }
class Valid { public string $value = 'ok'; public function run($value = self::LABEL) {} const LABEL = 'value'; }
echo Consumer::class;
$bad = new ReflectionProperty(Consumer::class, 'bad');
echo $bad->hasDefaultValue();
try { $bad->getDefaultValue(); } catch (Error $error) { echo $error->getMessage(); }
$parameter = new ReflectionParameter([Promoted::class, '__construct'], 'value');
echo $parameter->isDefaultValueAvailable(), $parameter->getDefaultValueConstantName();
try { $parameter->getDefaultValue(); } catch (Error $error) { echo $error->getMessage(); }
$traitParameter = new ReflectionParameter([Invalid::class, 'labeled'], 'value');
echo $traitParameter->isDefaultValueAvailable();
try { $traitParameter->getDefaultValue(); } catch (Error $error) { echo $error->getMessage(); }
echo (new ReflectionClass(Valid::class))->getDefaultProperties()['value'];
$value = (new ReflectionClass(Valid::class))->newInstanceWithoutConstructor();
echo $value->value;
$source = $argv[1];
eval($source);
"#;
    let module = super::lower_source_at_for_target(source, Path::new("main.php"), Path::new("."),
        Target::parse(target_name).unwrap());
    let consumer = &module.class_infos["Consumer"];
    assert_eq!(consumer.deferred_property_default_error.as_deref(),
        Some("Cannot access \"parent\" when current class scope has no parent"),
        "{target_name}: initialization must prioritize the local constant error");
    let slot = consumer.visible_property_index("bad").unwrap();
    assert!(matches!(consumer.defaults[slot].as_ref().unwrap().kind,
        crate::parser::ast::ExprKind::Throw(_)), "{target_name}");
    let valid = &module.class_infos["Valid"];
    let trait_method = &module.declared_trait_methods["Invalid"]["labeled"];
    assert!(matches!(trait_method.signature.defaults[0].as_ref().unwrap().kind,
        crate::parser::ast::ExprKind::Throw(_)), "{target_name}: deferred trait default");
    assert!(!matches!(trait_method.source_defaults[0].as_ref().unwrap().kind,
        crate::parser::ast::ExprKind::Throw(_)), "{target_name}: retain trait source default");
    assert!(module.class_infos["ReflectionParameter"].visible_property_index("__default_error").is_some(),
        "{target_name}: retained parameter Error slot");
    let run = valid.method_decls.iter().find(|method| method.name == "run").unwrap();
    assert!(matches!(run.params[0].2.as_ref().unwrap().kind,
        crate::parser::ast::ExprKind::ScopedConstantAccess {
            receiver: crate::parser::ast::StaticReceiver::Self_, ..
        }), "{target_name}: preserve source-visible constant receiver");
    let main = module.functions.iter().find(|function| function.name == "main").unwrap();
    assert!(main.instructions.iter().any(|instruction| instruction.op == crate::ir::Op::Borrow),
        "{target_name}: metadata guards must borrow their receiver");
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target_name}: {error:?}"));
    assert!(assembly.contains("__elephc_eval_register_native_property_default_error"), "{target_name}");
}

/// Separates target cases so each keeps CI's normal per-test timeout budget.
macro_rules! deferred_reflection_target_case {
    ($name:ident, $target:literal) => {
        /// Verifies deferred default reflection metadata and assembly for one supported target.
        #[test]
        fn $name() { deferred_reflection_defaults_on_target($target); }
    };
}

deferred_reflection_target_case!(property_default_review_reflection_macos_aarch64, "macos-aarch64");
deferred_reflection_target_case!(property_default_review_reflection_ios_arm64, "ios-arm64");
deferred_reflection_target_case!(property_default_review_reflection_ios_sim_arm64, "ios-sim-arm64");
deferred_reflection_target_case!(property_default_review_reflection_linux_aarch64, "linux-aarch64");
deferred_reflection_target_case!(property_default_review_reflection_linux_x86_64, "linux-x86_64");

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
