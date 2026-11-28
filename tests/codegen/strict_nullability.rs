//! Purpose:
//! Verifies public non-null contracts across native, interpreted and included source.
//!
//! Called from:
//! - Focused CLI integration and profile admission tests.
//!
//! Key details:
//! - Real opaque eval covers runtime reflection and native callback ownership.

use crate::support::*;

/// Public native contracts preserve original values, source order and heap ownership.
#[test]
fn test_strict_nullability_public_native_contracts_bundle() {
    for (source, expected) in [
        (include_str!("../fixtures/strict/nullable_object_arguments.php"), "LN|LC|LV|V|S|1:1:NULL"),
        (include_str!("../fixtures/strict/nullable_boundaries.php"), "LP|R|S:kept|NULL:7:8:NULL:NULL"),
        (include_str!("../fixtures/strict/nullable_builtins.php"), "DNSCAFIRLTJ|bc:1:NULL:0"),
        (include_str!("../fixtures/strict/nullable_aliases.php"), "R:kept|S:saved|P:kept|string:7|NULL"),
        (include_str!("../fixtures/strict/nullable_descriptors.php"), "SFIBNVWGHLP|7:NULL:NULL:NULL"),
        (include_str!("../fixtures/strict/nullable_variadic_contracts.php"), "AIC|1:1:1:NULL:NULL"),
    ] {
        for optimization in ["--ir-opt=off", "--ir-opt=on"] {
            let output = StrictFixture::new(source).capture(&["--strict-nullability", "--php-only", optimization, "--heap-debug"]);
            let errors = String::from_utf8_lossy(&output.stderr);
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{optimization}: {errors}");
            assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
        }
    }
}

/// Source-selected eval exercises the actual native catalog, runtime reflection and property aliases.
#[test]
fn test_strict_nullability_public_opaque_eval_contracts_bundle() {
    let fixtures = [
        (include_str!("../fixtures/strict/nullable_receivers.php"), "KN|K7|q:q|ON|KT"),
        (include_str!("../fixtures/strict/nullable_builtins.php"), "DNSCAFIRLTJ|bc:1:NULL:0"),
        (include_str!("../fixtures/strict/nullable_eval_aliases.php"), "R:kept|S:saved|P:kept|string:7|NULL"),
        (include_str!("../fixtures/strict/nullable_descriptors.php"), "SFIBNVWGHLP|7:NULL:NULL:NULL"),
        (include_str!("../fixtures/strict/nullable_variadic_contracts.php"), "AIC|1:1:1:NULL:NULL"),
        (include_str!("../fixtures/strict/nullable_object_arguments.php"), "LN|LC|LV|V|S|1:1:NULL"),
    ];
    let fragment = fixtures.iter().map(|(source, _)| source.strip_prefix("<?php").unwrap()
        .replace("declare(strict_types=0);", "")).collect::<Vec<_>>().join(";");
    let fragment = fragment.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $fragment=$argc>0?'{fragment}':'';eval($fragment);");
    let output = StrictFixture::new(&source).capture(&["--strict-nullability", "--php-only", "--heap-debug"]);
    let errors = String::from_utf8_lossy(&output.stderr);
    let expected = fixtures.iter().map(|(_, expected)| *expected).collect::<String>();
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{errors}");
    assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
}

/// Original source errors precede folding, and a disabled profile retains its own cached runtime.
#[test]
fn test_strict_nullability_public_source_probes_and_profile_identity() {
    for source in ["<?php function f(int $value=null){}", "<?php if(false){echo null[0];}",
        "<?php eval('function f(int $value=null){}');",
        "<?php class User {} function f(User $value=null){}",
        "<?php class User {} function f(User|int $value=null){}",
        "<?php eval('class User {} function f(User $value=null){}');",
        "<?php function f(string $value){} f(value:null);", "<?php if(false){strlen(null);}"] {
        StrictFixture::new(source).assert_error(&["--strict-nullability"], "strict-nullability");
    }
    let nullable = StrictFixture::new(
        "<?php class User {} function f(?User $user=null,mixed $value=null,ptr $p=null):void{} f();",
    );
    nullable.assert_compiles(&["--strict-nullability", "--check"]);
    let fixture = StrictFixture::new("<?php function probe(mixed $value):mixed{return $value[0];} try{echo gettype(probe(null));}catch(TypeError $error){echo 'N';}echo null[0]??'q';");
    assert_eq!(fixture.run(&["--strict-nullability", "--no-strict-nullability"]), "NULLq");
    assert_eq!(fixture.run(&["--strict-nullability"]), "Nq");
    assert_eq!(fixture.run(&["--strict-nullability", "--no-strict-nullability"]), "NULLq");
}

/// A global rebinding loses the local non-null proof and uses the class return guard.
#[test]
fn test_global_rebinding_checks_a_class_return_after_local_coalescing() {
    let fixture = StrictFixture::new(r#"<?php
final class Service {}
function selectService(?Service $value): Service {
    $value ??= new Service();
    global $value;
    return $value;
}
function publishService(mixed $incoming): void {
    global $value;
    $value = $incoming;
}
publishService(null);
try { selectService(null); echo 'bad-null'; }
catch (TypeError $error) { echo 'null:'; }
publishService(new stdClass());
try { selectService(null); echo 'bad-class'; }
catch (TypeError $error) { echo 'class:'; }
publishService(new Service());
echo get_class(selectService(null));
"#);
    for optimization in ["--ir-opt=on", "--ir-opt=off"] {
        assert_eq!(fixture.run(&["--php-only", optimization]), "null:class:Service");
        assert_eq!(
            fixture.run(&["--php-only", "--strict-nullability", optimization]),
            "null:class:Service",
        );
    }
    fixture.assert_error(&["--php-only", "--strict-mixed"], "--strict-mixed: narrow mixed");
}

/// Included LFC declarations and the PHP-only example retain independent non-null contracts.
#[test]
fn test_strict_nullability_public_lfc_and_example() {
    let fixture = StrictFixture::new("<?php require 'values.lfc';");
    fixture.file("values.lfc", "function absent():mixed{return null;}try{int $value=absent();}catch(TypeError $error){echo 'L';}");
    assert_eq!(fixture.run(&["--strict-nullability", "--php-only"]), "L");
    let source = include_str!("../../examples/strict-nullability/main.php");
    let output = StrictFixture::new(source).capture(&["--strict-nullability", "--php-only", "--heap-debug"]);
    let errors = String::from_utf8_lossy(&output.stderr);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "anonymous\nAda\nA name is required\n", "{errors}");
    assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
}

/// Registered borrowed builtins keep interpreter dispatch and the caller's independent profile.
fn verify_reflection_registry_builtin_dispatch(flags: &[&str], expected: &str) {
    let fragment = "$reflection=new ReflectionFunction('strlen');echo $reflection->invoke('abc'),':',$reflection->invokeArgs(['d']),':'; \
        $keys=new ReflectionFunction('array_key_exists');echo (int)$keys->invoke('1',[1=>'value']);";
    let fragment = fragment.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $fragment=$argc>0?'{fragment}':'';eval($fragment);");
    let fixture = StrictFixture::new(&source);
    let mut args = vec!["--php-only", "--heap-debug"];
    args.extend_from_slice(flags);
    let output = fixture.capture(&args);
    let errors = String::from_utf8_lossy(&output.stderr);
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}: {errors}");
    assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
}

/// Checks Reflection builtin dispatch under ordinary PHP semantics.
#[test]
fn test_reflection_registry_builtin_dispatch_permissive() {
    verify_reflection_registry_builtin_dispatch(&[], "3:1:1");
}

/// Checks Reflection builtin dispatch with standalone nullability checks.
#[test]
fn test_reflection_registry_builtin_dispatch_strict_nullability() {
    verify_reflection_registry_builtin_dispatch(&["--strict-nullability"], "3:1:1");
}

/// Checks Reflection builtin dispatch with standalone exact array keys.
#[test]
fn test_reflection_registry_builtin_dispatch_strict_array_keys() {
    verify_reflection_registry_builtin_dispatch(&["--strict-array-keys"], "3:1:0");
}
