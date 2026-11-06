//! Purpose:
//! Pins the private storage and reference ABI used by WeakMap entry places.
//!
//! Called from:
//! - The EIR lowering unit test module.
//!
//! Key details:
//! - Public offsetGet remains a value-returning method on every supported target.
//! - The compiler-only helper returns a managed cell and survives reachability pruning.

use crate::codegen::platform::Target;

/// Every target retains and calls the managed entry helper without changing public reflection.
#[test]
fn weak_map_reference_places_preserve_private_cell_abi_on_all_targets() {
    let source = r#"<?php
        class WeakMapHolder {
            public WeakMap $map;
            public static WeakMap $shared;
            public function __construct() { $this->map = new WeakMap(); }
            public function take(): WeakMap { return $this->map; }
            public static function takeStatic(WeakMapHolder $holder): WeakMap { return $holder->map; }
        }
        function replaceWeakMapEntry(mixed &$value): void { $value = 2; }
        $key = new stdClass();
        $map = new WeakMap();
        $map[$key] = ['n' => 0];
        $reference = &$map[$key];
        $reference['n'] = 1;
        echo $map[$key]['n'];
        $holder = new WeakMapHolder();
        $holder->map[$key] = ['n' => 0];
        $propertyReference = &$holder->map[$key];
        $methodReference = &$holder->take()[$key];
        $staticReference = &WeakMapHolder::takeStatic($holder)[$key];
        $staticReference['n'] = 1;
        echo $propertyReference['n'], $methodReference['n'];
        $map[$key] = 0;
        replaceWeakMapEntry($map[$key]);
        $map[$key]++;
        WeakMapHolder::$shared = new WeakMap();
        WeakMapHolder::$shared[$key] = 0;
        $sharedReference = &WeakMapHolder::$shared[$key];
        WeakMapHolder::$shared[$key]++;
        replaceWeakMapEntry(WeakMapHolder::$shared[$key]);
        echo $sharedReference;
        $before = $map[$key]++;
        $after = ++$map[$key];
        echo $before, $after;
        $get = fn(): WeakMap => $map;
        $closureReference = &$get()[$key];
        $maps = [$map];
        $containerReference = &$maps[0][$key];
        $expressionReference = &($get)()[$key];
        $literalReference = &(fn(): WeakMap => $map)()[$key];
        echo $containerReference, $expressionReference, $literalReference;
        settype($map[$key], 'string');
        echo $closureReference;
        $map[$key] = [1];
        $arrayReference = &$map[$key];
        $map[$key][] = 2;
        echo count($arrayReference);
        try { $map[] = ['v' => 1]; } catch (Error $e) { echo $e->getMessage(); }
        try { $holder->map[] = ['v' => 1]; } catch (Error $e) { echo $e->getMessage(); }
        try { WeakMapHolder::$shared[] = ['v' => 1]; } catch (Error $e) { echo $e->getMessage(); }
    "#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source,
            std::path::Path::new("main.php"), std::path::Path::new("."), Target::parse(target).unwrap());
        let class = &module.class_infos["WeakMap"];
        assert!(!class.methods["offsetget"].by_ref_return, "{target}");
        assert!(class.methods["__elephc_reference"].by_ref_return, "{target}");
        assert!(class.owned_reference_properties.contains("__elephc_entries"), "{target}");
        assert!(module.class_methods.iter().any(|method|
            method.name == "WeakMap::__elephc_reference" && method.flags.by_ref_return), "{target}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}
