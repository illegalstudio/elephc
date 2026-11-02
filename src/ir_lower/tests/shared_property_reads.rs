//! Purpose:
//! Pins shared runtime property dispatch across lexical scopes and supported target ABIs.
//!
//! Called from:
//! - AST-to-EIR emitter CI for all five supported targets.
//!
//! Key details:
//! - Repeated dynamic names share a helper while class-private scope and probe modes stay distinct.

/// Repeated global reads, class reads and probes call separately scoped copies of the same lowering.
fn verify_shared_property_read_scopes(target: &str) {
    let source = r#"<?php
class SharedPropertyScope {
    private mixed $secret = "kept";
    public function one(mixed $object, string $name): mixed { return $object->{$name}; }
    public function two(mixed $object, string $name): mixed { return $object->{$name}; }
}
class SharedPropertyEmpty { public string $value; }
function sharedGlobalReadOne(mixed $object, string $name): mixed { return $object->{$name}; }
function sharedGlobalReadTwo(mixed $object, string $name): mixed { return $object->{$name}; }
function sharedGlobalProbeOne(mixed $object, string $name): mixed { return $object->{$name} ?? null; }
function sharedGlobalProbeTwo(mixed $object, string $name): mixed { return $object->{$name} ?? null; }
$object = new SharedPropertyScope();
$object->one($object, "secret"); $object->two($object, "secret");
sharedGlobalReadOne($object, "secret"); sharedGlobalReadTwo($object, "secret");
sharedGlobalProbeOne($object, "secret"); sharedGlobalProbeTwo($object, "secret");
try { sharedGlobalReadOne(new SharedPropertyEmpty(), "value"); } catch (Error $error) {}
"#;
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    for marker in ["scope=global mode=read", "scope=global mode=probe", "scope=SharedPropertyScope mode=read"] {
        assert_eq!(assembly.matches(&format!("shared property read: {marker}")).count(), 1, "{target}: {marker}");
    }
    for name in ["sharedGlobalReadOne", "sharedGlobalReadTwo", "sharedGlobalProbeOne", "sharedGlobalProbeTwo"] {
        let symbol = crate::names::function_symbol(name);
        let body = assembly.split_once(&format!("{symbol}:\n")).unwrap().1.split(".globl").next().unwrap();
        assert!(body.contains("_eir_shared_property_read_"), "{target}: {name} must call the shared helper");
        assert!(!body.contains("mixed_dyn_prop_get_"), "{target}: {name} must not repeat the inline ladder");
    }
}

supported_target_cases!(shared_property_read_scopes, verify_shared_property_read_scopes);
