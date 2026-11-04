//! Purpose:
//! Keeps reflector construction independent of live static values across every target.
//!
//! Called from:
//! - The shared EIR lowering and emitter CI tests.
//!
//! Key details:
//! - Generic accessors materialize current maps through EIR.
//! - Construction metadata contains no retaining load of the subject's static storage.

use crate::ir::Op;

/// Construction leaves live values untouched, while a generic accessor has an initialization probe.
fn verify_reflection_static_metadata_lifetimes(target: &str) {
    let source = r#"<?php
class ReflectionStaticLifetimeSubject { public static mixed $value = null; }
enum ReflectionStaticLifetimeEnum: string {
    case First = 'first';
    public function label(): string { return $this->value; }
}
function constructStaticLifetimeEnum(): ReflectionEnum {
    return new ReflectionEnum(ReflectionStaticLifetimeEnum::class);
}
function constructStaticLifetimeReflector(): ReflectionClass {
    return new ReflectionClass(ReflectionStaticLifetimeSubject::class);
}
function readStaticLifetimeReflector(ReflectionClass $reflection): mixed {
    return $reflection->getStaticProperties();
}
echo count(readStaticLifetimeReflector(constructStaticLifetimeReflector()));
"#;
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    crate::ir::validate_module(&module).unwrap();
    let accessor = module.class_methods.iter().find(|function|
        function.name == "ReflectionClass::getStaticProperties").unwrap();
    assert!(accessor.instructions.iter().any(|inst| inst.op == Op::ReflectionStaticPropertyInitialized),
        "{target}: generic reflection reads the current initialization state");
    let names = accessor.instructions.iter().filter(|inst|
        inst.op == Op::PropGet && inst.result_php_type == crate::types::PhpType::Str)
        .filter_map(|inst| inst.result).collect::<Vec<_>>();
    assert!(!names.is_empty(), "{target}: runtime class selection borrows the reflector name");
    assert!(names.iter().all(|name| !accessor.instructions.iter().any(|inst|
        inst.op == Op::Release && inst.operands.contains(name))),
        "{target}: an eval-owned reflector must keep its name after every accessor call");
    for acquired in accessor.instructions.iter().filter(|inst|
        matches!(inst.op, Op::Acquire | Op::StrPersist)
            && inst.operands.iter().any(|source| names.contains(source)))
        .filter_map(|inst| inst.result)
    {
        assert!(accessor.instructions.iter().any(|inst|
            inst.op == Op::Release && inst.operands == [acquired]),
            "{target}: an explicitly stabilized name copy needs its own cleanup");
    }
    assert_eq!(module.class_infos.get("ReflectionEnum").unwrap().parent.as_deref(),
        Some("ReflectionClass"), "{target}: enum reflectors satisfy class parameter contracts");
    let enum_owner = &module.class_infos["ReflectionEnum"];
    for method in ["getmethods", "getproperties", "getconstants", "getstaticproperties"] {
        assert_eq!(enum_owner.method_declaring_classes.get(method).map(String::as_str),
            Some("ReflectionEnum"), "{target}: {method} must read the populated enum slots");
    }
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    let constructor = assembly.split_once("@fn name=constructStaticLifetimeReflector ").unwrap().1
        .split_once("@endfn name=constructStaticLifetimeReflector").unwrap().0;
    let symbol = crate::names::static_property_symbol("ReflectionStaticLifetimeSubject", "value");
    assert!(!constructor.contains(&symbol), "{target}: declaration metadata cannot own a static snapshot");
}

supported_target_cases!(reflection_static_metadata_lifetimes, verify_reflection_static_metadata_lifetimes);
