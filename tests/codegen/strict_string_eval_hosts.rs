//! Purpose:
//! Verifies strict strings across the actual native Magician boundary.
//!
//! Called from:
//! - Focused codegen host, callable, reference and formatting integration tests.
//!
//! Key details:
//! - Runtime-selected source exercises walker and bytecode rather than literal eval.
//! - Default and strict profiles preserve native owners and visible scope aliases.

use crate::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Native descriptors and scope references use the same string rules in opaque eval.
#[test]
fn test_strict_string_opaque_native_hosts_bundle() {
    let source = r#"<?php
function nativeStringMissing(int $index): mixed { $parts = ['a']; return $parts[$index]; }
function nativeStringJoin(mixed $value): string { return $value . 'x'; }
class NativeStringFormat {
    public int $count = 1;
    public function __toString(): string { echo 'H'; return str_repeat('v', $this->count); }
    public function __destruct() { echo 'D'; }
}
class NativeStringFormatThrow {
    public function __toString(): string { throw new ValueError('format'); }
    public function __destruct() { echo 'Q'; }
}
function nativeStringScope(?string $missing, int $selector): void {
    $alias =& $missing;
    $number = 7; $list = [1]; $map = ['k' => 1];
    $object = new NativeStringFormat(); $object->count = $selector;
    $throwing = new NativeStringFormatThrow();
    $callback = nativeStringJoin(...);
    $source = $selector > 0 ? '
echo gettype($missing), ":", gettype($alias), ":", gettype(nativeStringMissing(4));
echo "|";
try { echo $missing . "x"; } catch (TypeError $error) { echo "T"; unset($error); }
echo "|";
try { echo $alias . "x"; } catch (TypeError $error) { echo "T"; unset($error); }
echo "|";
try { echo $number . "x"; } catch (TypeError $error) { echo "T"; unset($error); }
echo "|";
try { echo $object . "x"; } catch (TypeError $error) { echo "T"; unset($error); }
echo "|", "[$number]:[$object]:[$missing]";
echo "|";
try { echo $list; } catch (TypeError $error) { echo "T"; unset($error); }
echo ":";
try { print $map; } catch (TypeError $error) { echo "T"; unset($error); }
echo "|";
try { echo $callback(null); } catch (TypeError $error) { echo "T"; unset($error); }
try { echo call_user_func($callback, null); } catch (TypeError $error) { echo "T"; unset($error); }
try { echo call_user_func_array($callback, [null]); } catch (TypeError $error) { echo "T"; unset($error); }
echo "|";
try { echo "[$throwing]"; } catch (ValueError $formatError) { echo "T"; unset($formatError); }
echo "|";
try { $alias .= "x"; echo "A:"; } catch (TypeError $error) { echo "T:"; unset($error); }
echo gettype($missing), ":", (string) $missing;
' : '';
    eval($source);
    echo '|native:', gettype($missing), ':', (string) $missing;
    $vmSource = $selector > 0 ? 'return $missing . "x";' : '';
    echo '|vm:';
    try { echo eval($vmSource); }
    catch (TypeError $nativeError) { echo 'T'; unset($nativeError); }
    unset($object); echo '|'; unset($throwing); echo '|end';
}
nativeStringScope(null, $argc);
gc_collect_cycles();
"#;
    for selected in [false, true] {
        for optimized in [false, true] {
            let output = StrictFixture::new(source).capture_staged_eval_runtime(
                SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, selected), optimized);
            assert!(output.success, "strict={selected} opt={optimized}: {} {} {}", output.status, output.stdout, output.stderr);
            let expected = if selected {
                "NULL:NULL:NULL|T|T|T|T|H[7]:[v]:[]|T:T|TTT|T|T:NULL:|native:NULL:|vm:TD|Q|end"
            } else {
                "NULL:NULL:NULL|x|x|7x|Hvx|H[7]:[v]:[]|Array:Array|xxx|T|A:string:x|native:string:x|vm:xxD|Q|end"
            };
            assert_eq!(output.stdout, expected, "strict={selected} opt={optimized}: {}", output.stderr);
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
        }
    }
}
