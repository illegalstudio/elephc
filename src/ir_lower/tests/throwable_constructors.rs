//! Purpose:
//! Verifies inherited builtin Throwable constructors have callable, layout-aware EIR bodies.
//!
//! Called from:
//! - The AST-to-EIR regression tests on every supported target.
//!
//! Key details:
//! - Ordinary subclasses add storage while Error and Exception retain their shared method owners.

use super::*;
use crate::ir::{Effects, Immediate, IrHeapKind, IrType, RuntimeCallTarget};

/// Every missed dynamic class branch retains a boxed runtime argument container on every target.
#[test]
fn dynamic_constructor_fallbacks_keep_runtime_argument_containers_on_all_targets() {
    let source = r#"<?php
class FallbackOpt { public function __construct($a = 1, $b = 2) {} }
class FallbackExact { public function __construct(public int $x) {} }
function fallbackNamed(string $name): mixed { return new $name(b: 5); }
function fallbackSpread(string $name, array $values): mixed { return new $name(...$values); }
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        for function in ["fallbackNamed", "fallbackSpread"] {
            let function = module.functions.iter().find(|candidate| candidate.name == function).unwrap();
            let fallback = function.instructions.iter().find(|inst| inst.op == crate::ir::Op::DynamicObjectNewMixed).unwrap();
            assert_eq!(fallback.immediate, Some(Immediate::Bool(true)), "{name}: {}", function.name);
            assert_eq!(fallback.operands.len(), 2, "{name}: {}", function.name);
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// Runtime constructor binding reaches valid fixed and lexical calls on every supported target.
#[test]
fn hidden_constructor_unpack_streams_lower_on_all_targets() {
    let source = r#"<?php
class StreamTarget extends ErrorException {
    public static function make(array $values): static { return new static(...$values); }
    public function reset(array $values): void { parent::__construct(...$values); }
}
$values = [9 => 'fresh', 'filename' => 'named.php'];
$error = new ErrorException(...$values);
$error->__construct(...$values);
$child = StreamTarget::make($values);
$child->reset($values);
function invokeStreamConstructor(callable $callback, array $values): void { $callback(...$values); }
$constructor = $error->__construct(...);
invokeStreamConstructor($constructor, $values);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.contains(&crate::names::method_symbol("ErrorException", "__construct")), "{name}");
    }
}

/// A lexical parent call alone keeps its builtin constructor dependency on every target.
#[test]
fn parent_static_calls_retain_builtin_throwable_constructors_on_all_targets() {
    let source = r#"<?php
class ParentDependency extends Exception {
    public function reset(): void { parent::__construct('fresh'); }
}
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        assert!(module.class_methods.iter().any(|method| method.name == "Exception::__construct"), "{name}");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let symbol = crate::names::method_symbol("Exception", "__construct");
        assert!(assembly.contains(&format!("{symbol}:")), "{name}: missing referenced parent symbol");
    }
}

/// Fixed and dynamic ordinary Throwable allocation retain the new-expression line on every target.
#[test]
fn ordinary_throwable_creation_lines_are_stamped_on_all_targets() {
    let source = "<?php\nfunction fixedLocation(): ErrorException {\nreturn new ErrorException('fixed');\n}\nfunction dynamicLocation(string $class): mixed {\nreturn new $class('dynamic');\n}\nclass StaticLocation extends ErrorException {\npublic static function make(): static {\nreturn new static('factory');\n}\n}\necho fixedLocation()->getLine(), dynamicLocation(ErrorException::class)->getLine(), StaticLocation::make()->getLine();";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = lower_source_at_for_target(source, Path::new("main.php"), Path::new("."), target);
        let offset = crate::codegen_support::throwable_layout::ordinary_creation_line_offset(
            "ErrorException", &module.class_infos,
        ).unwrap();
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        for (function, line) in [("fixedLocation", 3), ("dynamicLocation", 6), ("StaticLocation::make", 10)] {
            let start = format!("@fn name={function} symbol=");
            let end = format!("@endfn name={function}");
            let body = assembly.split_once(&start).unwrap().1.split_once(&end).unwrap().0;
            let stamp = match target.arch {
                crate::codegen::platform::Arch::AArch64 => format!("mov x9, #{line}\n    str x9, [x0, #{offset}]"),
                crate::codegen::platform::Arch::X86_64 => format!("mov QWORD PTR [rax + {offset}], {line}"),
            };
            assert!(body.contains(&stamp), "{name}: {function} lacks construction line {line}\n{body}");
        }
    }
}

/// Parent calls release their ABI-created nullable argument after return on every supported target.
#[test]
fn lexical_parent_call_retires_default_argument_box_on_all_targets() {
    let source = r#"<?php
class DefaultArgumentOwner {
    public function accept(?Exception $previous = null): void {}
}
class DerivedArgumentOwner extends DefaultArgumentOwner {
    public function exercise(): void { parent::accept(); }
}
$owner = new DerivedArgumentOwner();
$owner->exercise();
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = lower_source_at_for_target(source, Path::new("main.php"), Path::new("."), target);
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let symbol = crate::names::method_symbol("DefaultArgumentOwner", "accept");
        let call = if target.arch == crate::codegen::platform::Arch::AArch64 { "bl" } else { "call" };
        let (_, tail) = assembly.split_once(&format!("{call} {symbol}")).unwrap();
        let until_return = tail.lines().take_while(|line| line.trim() != "ret").collect::<Vec<_>>().join("\n");
        assert!(until_return.contains("__rt_decref_mixed"), "{name}: {until_return}");
    }
}

/// Every eval bridge owns a nullable previous result before handing it back across the native ABI.
fn verify_compact_throwable_eval_previous_getter(target: &str) {
    let source = r#"<?php
function throwEvalGetterException(): void { throw new RuntimeException("native"); }
$source = 'try { throwEvalGetterException(); } catch (RuntimeException $error) {
    echo $error->getPrevious() === null ? "end" : "previous";
}' . ' // ' . $argc;
eval($source);
"#;
    let name = target;
        let module = lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let start = assembly.find("__elephc_eval_builtin_throwable_getprevious:").unwrap();
        let body = &assembly[start..];
        let lookup = body.find("__rt_throwable_previous").unwrap();
        let object_box = body.find("__rt_mixed_from_value").unwrap();
        let null = body.find("__elephc_eval_builtin_throwable_previous_null:").unwrap();
        assert!(lookup < object_box && object_box < null, "{name}");
        assert!(body[null..].contains("__rt_mixed_from_value"), "{name}");
}

/// Schedules the expensive eval bridge fixture once per target under the unchanged CI timeout.
macro_rules! compact_previous_target_test {
    ($name:ident, $target:literal) => {
        /// Checks both owned previous-result arms for this supported target.
        #[test]
        fn $name() { verify_compact_throwable_eval_previous_getter($target); }
    };
}

compact_previous_target_test!(compact_throwable_eval_previous_getter_macos, "macos-aarch64");
compact_previous_target_test!(compact_throwable_eval_previous_getter_ios, "ios-arm64");
compact_previous_target_test!(compact_throwable_eval_previous_getter_ios_sim, "ios-sim-arm64");
compact_previous_target_test!(compact_throwable_eval_previous_getter_linux_arm, "linux-aarch64");
compact_previous_target_test!(compact_throwable_eval_previous_getter_linux_x86, "linux-x86_64");

/// Both constructor roots remain emitted and use normalized boxed previous operands on all targets.
#[test]
fn inherited_throwable_constructors_have_layout_aware_bodies() {
    let source = r#"<?php
class ExtendedException extends Exception { public int $marker = 42; }
class ExtendedError extends Error { public string $marker = "kept"; }
$error = new ExtendedException("outer", 1, new ExtendedError("inner"));
$error->__construct("again", 2, $error->getPrevious());
echo $error->getMessage();
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap());
        for owner in ["Error", "Exception"] {
            let constructor = module.class_methods.iter()
                .find(|function| function.name == format!("{owner}::__construct"))
                .unwrap_or_else(|| panic!("{name}: missing {owner} constructor"));
            let initialize = constructor.instructions.iter().find(|inst| {
                inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::ThrowableInitialize))
            }).expect("constructor must initialize the receiver's concrete layout");
            assert_eq!(initialize.operands.len(), 4, "{name}");
            let expected = [IrType::Heap(IrHeapKind::Object), IrType::Str, IrType::I64,
                IrType::Heap(IrHeapKind::Mixed)];
            for (operand, expected) in initialize.operands.iter().zip(expected) {
                assert_eq!(constructor.value(*operand).unwrap().ir_type, expected, "{name}: {owner}");
            }
            let signature = &module.class_infos[owner].methods["__construct"];
            assert_eq!(signature.params[0].1, crate::types::PhpType::Str, "{name}: {owner}");
            assert_eq!(signature.params[1].1, crate::types::PhpType::Int, "{name}: {owner}");
            assert!(signature.declared_params.iter().take(3).all(|declared| *declared), "{name}: {owner}");
            assert!(initialize.effects.contains(Effects::MAY_THROW | Effects::REFCOUNT_OP), "{name}");
        }
    }
}

/// An untyped subclass constructor cannot specialize a typed ancestor's nullable previous parameter.
#[test]
fn subclass_constructor_inference_preserves_ancestor_parameter_contracts() {
    let source = r#"<?php
class InferredPreviousException extends Exception {
    public function __construct($message, $code, $previous) {
        $this->message = $message;
        $this->code = $code;
        $this->previous = $previous;
    }
}
$previous = new Error("inner");
$child = new InferredPreviousException("child", 1, $previous);
$parent = new Exception("parent", 2, null);
$parent->__construct("again", 3, $child->getPrevious());
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = lower_source_at_for_target(source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap());
        let signature = &module.class_infos["Exception"].methods["__construct"];
        assert_eq!(signature.params[2].1.codegen_repr(), crate::types::PhpType::Mixed, "{name}");
        assert!(signature.declared_params[2], "{name}");
    }
}
