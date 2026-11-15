//! Purpose:
//! Verifies checked settype invocation and valid conversion domains through native EIR.
//!
//! Called from:
//! - Selected-profile all-target and native semantic-profile regressions.
//!
//! Key details:
//! - Eval callable coverage is independent from compiler callable and reference evidence.

use crate::codegen_support::platform::Target;
use elephc_semantics::{SemanticProfile, StrictFeature};

const CALLABLES: &str = r#"<?php
mixed $value = '9223372036854775807.9'; $alias =& $value;
$cast = settype(...); $cast($value, 'integer'); echo $alias, '|';
$name = 'settype'; $name($value, 'double'); echo is_float($alias) ? 'F|' : 'X|';
$value = $argc > 0 ? 'junk' : 'also junk';
try { $cast($value, 'int'); echo 'X'; } catch (ValueError $error) { echo $alias, '|'; }
try { $name($value, 'float'); echo 'X'; } catch (ValueError $error) { echo $alias; }
$plain = $argc > 0 ? 7 : 8; $target = $argc > 0 ? 'NuLl' : 'null';
settype($plain, $target); echo is_null($plain) ? '|N' : '|X';
$value = 'copy me'; $cast($value, 'STRING'); echo is_string($alias) ? '|S' : '|X';
$name($value, 'boolean'); echo $alias ? '|T' : '|X';
$value = 'kept'; $target = $argc > 0 ? 'resource' : 'unset';
try { $cast($value, $target); echo 'X'; } catch (ValueError $error) { echo '|R', $alias; }
$target = $argc > 0 ? "int\0x" : 'unset';
try { $name($value, $target); echo 'X'; } catch (ValueError $error) { echo '|B', $alias; }
try { call_user_func('settype', $value, 'int'); echo 'X'; } catch (ValueError $error) { echo '|C', $alias; }
$value = '42'; call_user_func_array('settype', [$value, 'integer']); echo is_string($alias) ? '|V' : '|X';
$out = ['result' => false]; $out['result'] = $cast($value, 'string'); echo $out['result'] ? '|P' : '|X';
$value = '42'; call_user_func('settype', $value, 'integer'); echo is_string($alias) ? '|Q' : '|X';
function callbackValueCopy(mixed &$value): void { $value = 'mutated'; }
mixed $copy = 'original'; call_user_func('callbackValueCopy', $copy); echo '|', $copy;
$callback = $argc > 0 ? 'callbackValueCopy' : 'callbackValueCopy';
call_user_func_array($callback, [$copy]); echo '|', $copy;
$direct = callbackValueCopy(...); $direct($copy); echo '|', $copy;
$value = '42'; $pack = [$value, 'integer']; call_user_func_array('settype', $pack);
echo is_string($alias) && is_string($pack[0]) ? '|W' : '|X';
mixed $effect = 0;
class SettypeStringEffect { public function __toString(): string { global $effect; $effect = 9; return 'text'; } }
$object = new SettypeStringEffect(); $target = $argc > 0 ? 'string' : 'STRING';
settype($object, $target); echo '|', $effect;
array $referencePack = ['42', 'integer']; $elementAlias =& $referencePack[0];
call_user_func_array('settype', $referencePack); echo is_int($elementAlias) ? '|L' : '|X';
"#;

/// Prepared callable conversion graphs validate and emit for every supported target.
#[test]
fn strict_settype_callables_eir_all_targets() {
    let program = prepared(CALLABLES);
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_checked_cast_int") && asm.contains("__rt_checked_cast_float"), "{name}");
    }
}

/// Native callable invocations must retain checked conversions and publish to the original reference.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_settype_callables_native_values_failures_and_owners() {
    let program = prepared(CALLABLES);
    let target = Target::detect_host();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    for optimize in [false, true] {
        let _policy = crate::optimize::scoped_semantic_profile(profile);
        let mut program = program.clone();
        if optimize {
            program = crate::optimize::fold_constants_for_target(program, target);
            program = crate::optimize::propagate_constants(program, checked.mixed_storage_local_names(), checked.buffer_read_sites.clone());
            program = crate::optimize::prune_constant_control_flow(program, checked.local_binding_decision_spans());
            program = crate::optimize::normalize_control_flow(program, checked.local_binding_decision_spans());
            program = crate::optimize::eliminate_dead_code(program, checked.local_binding_decision_spans());
        }
        let mut module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        if optimize { crate::ir_passes::optimize_module(&mut module); }
        crate::ir::validate_module(&module).unwrap();
        let output = super::native_module::run(&module);
        let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {stdout} {stderr}");
        assert_eq!(stdout, "9223372036854775807|F|junk|junk|N|S|T|Rkept|Bkept|Ckept|V|P|Q|original|original|mutated|W|9|L", "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}

/// Explicit array-literal references retain their target-independent parser diagnostic.
#[test]
fn strict_settype_callables_array_literal_references_remain_diagnosed() {
    let error = crate::parser::parse(&crate::lexer::tokenize(
        "<?php mixed $value = '42'; call_user_func_array('settype', [&$value, 'integer']);").unwrap()).unwrap_err();
    assert!(error.message.contains("Reference elements in array literals"), "{error:?}");
}

/// Prepared bodies also compile in permissive catalogs while retaining ordinary numeric conversion.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_settype_callables_permissive_prepared_body_control() {
    let program = prepared("<?php mixed $value = 'junk'; $cast = settype(...); $cast($value, 'int'); echo $value;");
    let target = Target::detect_host();
    let checked = crate::types::checker::check_staged_semantics(&program, target, SemanticProfile::PERMISSIVE).unwrap();
    let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
    let output = super::native_module::run(&module);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "0");
    assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
}

/// Mirrors implicit prelude injection for prepared builtin bodies in the production frontend.
fn prepared(source: &str) -> crate::parser::ast::Program {
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    let program = crate::object_cast_prelude::inject_if_used(program, &mut inventory).unwrap();
    crate::name_resolver::resolve(program).unwrap()
}

/// Emits live property and array aliases with the actual selected-profile checker metadata.
fn verify_strict_settype_places_alias_contract(name: &str) {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/strict/settype_places.php"));
    let program = prepared(source); let target = Target::parse(name).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
    crate::ir::validate_module(&module).unwrap();
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    assert!(module.required_runtime_features.eval_bridge, "runtime-selected passing modes require metadata");
}

supported_target_cases!(strict_settype_places_alias_contract, verify_strict_settype_places_alias_contract);

/// Checked calls promote a standalone Mixed property without relying on an explicit reference alias.
#[test]
fn strict_settype_places_known_receiver_promotions() {
    for call in ["settype($cell->value, 'int');", "$cast = settype(...); $cast($cell->value, 'int');"] {
        let source = format!("<?php class StandaloneCastPlace {{ public mixed $value = '7.9'; }} $cell = new StandaloneCastPlace(); {call}");
        let program = prepared(&source); let target = Target::detect_host();
        let checked = crate::types::checker::check_staged_semantics(&program, target,
            SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
        let info = &checked.classes["StandaloneCastPlace"];
        let slot = info.visible_property_index("value").unwrap();
        assert!(info.property_slot_owns_reference(slot), "{call}: checked builtin references need live property storage");
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Erased ordinary/native reference contexts use the selected profile and every target ABI.
fn verify_strict_settype_erased_receiver_contexts(name: &str) {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/strict/settype_erased_receivers.php"));
    let (program, _) = super::strict_object_casts::prepare(source);
    let target = Target::parse(name).unwrap();
    let checked = crate::types::checker::check_staged_semantics(&program, target,
        SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
    let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
    crate::ir::validate_module(&module).unwrap();
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

supported_target_cases!(strict_settype_erased_receiver_contexts, verify_strict_settype_erased_receiver_contexts);

/// Known builtin references retain runtime names with selected checker metadata and no XML prelude.
fn verify_strict_settype_known_dynamic_places(name: &str) {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/strict/settype_known_dynamic_places.php"));
    let source = source.replace("public function __construct() { $alias =& $this->value; }", "");
    let program = prepared(&source); let target = Target::parse(name).unwrap();
    let checked = crate::types::checker::check_staged_semantics(&program, target,
        SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
    let info = &checked.classes["KnownDynamicConversionPlace"];
    let slot = info.visible_property_index("value").unwrap();
    assert!(info.property_slot_owns_reference(slot), "{name}: runtime references need canonical cells without prior aliases");
    let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
    crate::ir::validate_module(&module).unwrap();
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    assert!(!checked.classes.contains_key("SimpleXMLElement"), "ordinary references must not require native XML");
}

supported_target_cases!(strict_settype_known_dynamic_places, verify_strict_settype_known_dynamic_places);

/// Canonical unaliased property cells preserve ordinary reads, inheritance, conversion and cleanup.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_settype_unaliased_property_native_owners() {
    let program = prepared("<?php class UnaliasedConversionPlace { public mixed $value = '18.9'; } class InheritedConversionPlace extends UnaliasedConversionPlace {} $box = new InheritedConversionPlace(); settype($box->value, 'int'); echo $box->value; $box->value = ['kept']; try { settype($box->value, 'int'); echo 'X'; } catch (TypeError $error) { echo '|', $box->value[0]; }");
    let target = Target::detect_host();
    let checked = crate::types::checker::check_staged_semantics(&program, target,
        SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
    for optimize in [false, true] {
        let mut module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        if optimize { crate::ir_passes::optimize_module(&mut module); }
        let output = super::native_module::run(&module);
        let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {stdout} {stderr}");
        assert_eq!(stdout, "18|kept", "opt={optimize}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}

/// Checked scalar references keep declaration callbacks and canonical layouts on every target.
#[test]
fn strict_settype_typed_property_contracts_all_targets() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/strict/settype_typed_properties.php"));
    // The selected checker must establish scalar cells even without a preceding explicit alias.
    let source = source.lines().filter(|line| !line.starts_with("$numberAlias")
        && !line.starts_with("$amountAlias") && !line.starts_with("$enabledAlias")).collect::<Vec<_>>().join("\n");
    let program = prepared(&source);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target,
            SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        assert!(module.required_runtime_features.native_reference_validation, "{name}");
        crate::ir::validate_module(&module).unwrap();
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("__rt_reference_cell_promote_string"), "{name}");
        assert!(assembly.contains("__rt_reference_publish_native"), "{name}");
    }
}
