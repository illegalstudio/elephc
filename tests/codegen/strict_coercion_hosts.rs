//! Purpose:
//! Verifies exact value bindings across Reflection and real native/interpreter calls.
//!
//! Called from:
//! - Focused Coercions host admission tests.
//!
//! Key details:
//! - Reflection must preserve original arguments before either backend can coerce them.
//! - Runtime-selected source reaches Magician and every program checks final ownership.

use crate::support::*;

/// Shared native invocation source also exercises independently owned string results.
const NATIVE_REFLECTION_SOURCE: &str = r#"<?php
function reflectionExactFloat(float $value): void { echo 'F'; }
function reflectionMixedValue(mixed $value): mixed { return $value; }
function reflectionOwnedResult(int $count): string { return str_repeat('v', $count); }
class ReflectionExactMethod {
    public function method(float $value): void { echo 'M'; }
    public static function shared(float $value): void { echo 'S'; }
    public function __destruct() { echo 'D'; }
}
$object = new ReflectionExactMethod();
try { (new ReflectionFunction('reflectionExactFloat'))->invoke(reflectionMixedValue(1)); }
catch (TypeError $error) { echo 'T'; unset($error); }
try { (new ReflectionFunction('reflectionExactFloat'))->invokeArgs([reflectionMixedValue(1)]); }
catch (TypeError $error) { echo 'T'; unset($error); }
try { (new ReflectionMethod('ReflectionExactMethod', 'method'))->invoke($object, reflectionMixedValue(1)); }
catch (TypeError $error) { echo 'T'; unset($error); }
try { (new ReflectionMethod('ReflectionExactMethod', 'shared'))->invoke(null, reflectionMixedValue(1)); }
catch (TypeError $error) { echo 'T'; unset($error); }
echo '|', (new ReflectionFunction('reflectionOwnedResult'))->invoke($argc), '|'; unset($object);
gc_collect_cycles();
"#;

/// Direct native Reflection uses exact signatures while permissive invocation retains PHP widening.
#[test]
fn test_strict_coercions_native_reflection_contracts_bundle() {
    let source = NATIVE_REFLECTION_SOURCE;
    for selected in [false, true] {
        for optimization in ["--ir-opt=off", "--ir-opt=on"] {
            let mut flags = vec![optimization, "--heap-debug"];
            if selected { flags.push("--strict-coercions"); }
            let output = StrictFixture::new(source).capture(&flags);
            assert_eq!(String::from_utf8_lossy(&output.stdout), if selected { "TTTT|v|D" } else { "FFMS|v|D" },
                "strict={selected} {optimization}: {}", String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "strict={selected} {optimization}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Eval and AOT signatures, return guards, descriptors and Reflection share the installed profile.
#[test]
fn test_strict_coercions_opaque_native_eval_hosts_bundle() {
    let source = r#"<?php
function nativeExactFloat(float $value): void { echo 'N'; }
function nativeExactResult(mixed $value): float { return $value; }
class NativeExactMethod { public function method(float $value): void { echo 'M'; } }
$callback = nativeExactFloat(...);
$source = $argc > 0 ? '
function evalExactFloat(float $value): void { echo "E"; }
function evalExactResult(mixed $value): float { return $value; }
class EvalExactMethod { public function method(float $value): void { echo "C"; } }
try { nativeExactFloat(1); } catch (TypeError $error) { echo "1"; unset($error); }
try { evalExactFloat(1); } catch (TypeError $error) { echo "2"; unset($error); }
try { nativeExactResult(1); echo "X"; } catch (TypeError $error) { echo "3"; unset($error); }
try { evalExactResult(1); echo "X"; } catch (TypeError $error) { echo "4"; unset($error); }
try { (new ReflectionFunction("nativeExactFloat"))->invoke(1); } catch (TypeError $error) { echo "5"; unset($error); }
try { (new ReflectionFunction("evalExactFloat"))->invoke(1); } catch (TypeError $error) { echo "6"; unset($error); }
try { (new ReflectionMethod("NativeExactMethod", "method"))->invoke(new NativeExactMethod(), 1); } catch (TypeError $error) { echo "7"; unset($error); }
try { (new ReflectionMethod("EvalExactMethod", "method"))->invoke(new EvalExactMethod(), 1); } catch (TypeError $error) { echo "8"; unset($error); }
try { (new ReflectionFunction("nativeExactFloat"))->invokeArgs([1]); } catch (TypeError $error) { echo "9"; unset($error); }
try { $callback(1); } catch (TypeError $error) { echo "A"; unset($error); }
try { call_user_func("nativeExactFloat", 1); } catch (TypeError $error) { echo "B"; unset($error); }
try { strlen(1); echo "X"; } catch (TypeError $error) { echo "C"; unset($error); }
try { str_repeat("a", 2.0); echo "X"; } catch (TypeError $error) { echo "D"; unset($error); }
echo "|";
nativeExactFloat(2.0); evalExactFloat(2.0);
(new NativeExactMethod())->method(2.0); (new EvalExactMethod())->method(2.0);
' : '';
eval($source);
gc_collect_cycles();
"#;
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&["--strict-coercions", optimization, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "123456789ABCD|NEMC",
            "{optimization}: {}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
            "{}", String::from_utf8_lossy(&output.stderr));
    }
}

/// The same receiver roots and result pins lower through every supported target ABI.
#[test]
fn test_strict_coercions_reflection_emits_on_all_supported_targets() {
    let fixture = StrictFixture::new(NATIVE_REFLECTION_SOURCE);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        fixture.assert_compiles(&["--strict-coercions", "--emit-asm", "--emit", "staticlib", "--target", name]);
    }
}
