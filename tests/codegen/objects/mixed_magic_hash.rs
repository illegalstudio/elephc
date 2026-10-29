//! Purpose:
//! Verifies dynamic property storage and magic access coexist through Mixed receivers.
//!
//! Called from:
//! - The object codegen integration suite.
//!
//! Key details:
//! - Opaque eval reserves native hash storage, including on the AOT fixture's classes.
//! - Stored null and false entries remain real entries, and existing writes skip __set.

use crate::support::*;

/// A present dynamic key wins; a missing key invokes its class's getter or isset hook.
#[test]
fn test_mixed_magic_hash_entry_precedes_accessors_bundle() {
    let source = r#"<?php
#[AllowDynamicProperties]
class MixedHashMagic {
    public static int $sets = 0;
    public function __set(string $name, mixed $value): void {
        self::$sets++;
        $this->{$name} = $value;
    }
    public function __get(string $name): int { echo 'get:', $name, '|'; return 30; }
    public function __isset(string $name): bool { echo 'isset:', $name, '|'; return true; }
}
function MixedHashExercise(mixed $object): void {
    var_dump($object->storedNull, $object->storedFalse, $object->stored, $object->missing);
    var_dump(isset($object->storedNull), isset($object->missing));
    $object->stored = 8;
    $object->another = 9;
    echo $object->stored, ':', $object->another, ':', MixedHashMagic::$sets, "\n";
}
$object = new MixedHashMagic();
$object->storedNull = null; $object->storedFalse = false; $object->stored = 5;
MixedHashExercise($object);
unset($object);
"#;
    let renamed = source.replace("MixedHash", "EvalMixedHash");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let expected = "get:missing|NULL\nbool(false)\nint(5)\nint(30)\nisset:missing|bool(false)\nbool(true)\n8:9:4\n";
    assert_bundle_with(&[
        BundleCase { name: "mixed_magic_hash_aot", source, expected },
        BundleCase { name: "mixed_magic_hash_eval", source: &eval, expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(!out.stderr.contains("Warning:") && !out.stderr.contains("Deprecated:"), "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        out.stdout
    });
}
