//! Purpose:
//! Pins the ownership of string offset results before their receiver is retired.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Exercises the shared lowering and assembly generation for every supported target.

/// Review regressions preserve transient receiver releases and retire keys before suspension.
#[test]
fn string_offset_review_boundaries_on_all_targets() {
    use crate::ir::{Op, Ownership};
    let source = r#"<?php
function reviewConcatByte(int $index): string { return (str_repeat("a", 70000) . "b")[$index]; }
function reviewKey(array<mixed> &$items, string $bytes): void { $items[$bytes[0]] = 1; }
function reviewYield(string $bytes) { yield $bytes[0] => 1; }
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let yielded = module.functions.iter().find(|function| function.name == "reviewYield").unwrap();
        let suspend = yielded.instructions.iter().position(|inst| inst.op == Op::GeneratorYield).unwrap();
        let key = yielded.instructions[suspend].operands[0];
        assert_eq!(yielded.values[key.as_raw() as usize].ownership, Ownership::Owned, "{target}");
        assert!(yielded.instructions[..suspend].iter().any(|inst| inst.op == Op::MixedBox), "{target}");
        assert!(yielded.instructions[..suspend].iter().any(|inst| inst.op == Op::PopCallOperandOwner), "{target}");
        assert!(yielded.instructions[..suspend].iter().any(|inst| inst.op == Op::ReleaseLocalSlot), "{target}");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("release_transient_skip"), "{target}: release unadopted concat storage");
    }
}

/// An offset view is copied before an owned receiver can be released on every target.
#[test]
fn string_offset_lifetime_stabilizes_views_on_all_targets() {
    use crate::ir::{Immediate, Op};
    let source = r#"<?php
class OffsetReader {
    public string $bytes;
    public function __construct(string $s) { $this->bytes = $s; }
    public function at(int $i): int { return ord($this->bytes[$i]); }
    public function emptyAt(int $i): bool { return empty($this->bytes[$i]); }
}
class OffsetStore {
    public string $typed = "";
    public $loose = "";
}
function offsetArray(array $items, int $i): string { return $items[0][$i]; }
function replaceOffsetBytes(OffsetReader $reader): int {
    $reader->bytes = "0";
    return 0;
}
function offsetStores(string $bytes): void {
    $local = $bytes . "!";
    $items = [""];
    $appended = [];
    $mixed = [1, ""];
    $hash = ["value" => ""];
    $keyed = [];
    $object = new OffsetStore();
    $items[0] = $local[8];
    $appended[] = $local[8];
    $mixed[0] = $local[8];
    $hash["value"] = $local[8];
    $keyed[$local[8]] = 1;
    $object->typed = $local[8];
    $object->loose = $local[8];
}
function offsetAcrossEffects(string $bytes, bool $select): void {
    $local = $bytes . "!";
    $keyed = [];
    $keyed[$local[8]] = ($local = "replaced");
    $local = $bytes . "!";
    echo $local[8] . ($local = "replaced");
    $local = $bytes . "!";
    echo $local[8] === ($local = "Z");
    $local = $bytes . "!";
    $joined = $select ? $local[8] : 88;
    $items = [];
    $items[] = $joined;
    $literalReader = new OffsetReader($bytes . "!");
    $literal = [$literalReader->bytes[8] => replaceOffsetBytes($literalReader)];
    $mixedLiteralReader = new OffsetReader($bytes . "!");
    $mixedLiteral = [0, $mixedLiteralReader->bytes[8] => replaceOffsetBytes($mixedLiteralReader)];
    $local = $bytes . "!";
    echo match ($local[8]) { ($local = "Z") => "yes", default => "no" };
    $reader = new OffsetReader($bytes . "!");
    echo $reader->bytes[8][replaceOffsetBytes($reader)];
}
class OffsetStaticStore {
    public static stdClass $object;
    public static mixed $kept;
    public static string $typed = "";
    public static $inferred = "";
    public static mixed $nested = ["seed" => ["child" => 0]];
}
function offsetCallableAfterChange(string $unused, callable $callback): callable { return $callback; }
function offsetYield(string $bytes) {
    $local = $bytes . "!";
    yield $local[8];
    $local = $bytes . "!";
    yield $local[8] => ($local = "no");
}
class OffsetAliasReturnReader {
    public function same(string $byte): string { return $byte; }
    public static function staticSame(string $byte): string { return $byte; }
}
#[AllowDynamicProperties]
class OffsetDynamicByteBag {}
function offsetAliasIdentity(string $byte): string { return $byte; }
function offsetAliasReturns(string $bytes, ?OffsetAliasReturnReader $nullable): void {
    $reader = new OffsetAliasReturnReader();
    $local = $bytes . "!";
    $kept = offsetAliasIdentity($local[8]);
    $local = "no";
    echo $kept;
    $local = $bytes . "!";
    $kept = $reader->same($local[8]);
    $local = "no";
    echo $kept;
    $local = $bytes . "!";
    $kept = OffsetAliasReturnReader::staticSame($local[8]);
    $local = "no";
    echo $kept;
    $local = $bytes . "!";
    $kept = $nullable?->same($local[8]);
    $local = "no";
    echo $kept;
    $local = $bytes . "!";
    $kept = strval($local[8]);
    $local = "no";
    echo $kept;
}
function offsetCallableAliasReturns(string $bytes): void {
    $local = $bytes . "!";
    $kept = (["OffsetAliasReturnReader", "staticSame"])($local[8]);
    $local = "no";
    echo $kept;
}
function offsetDynamicPropertyValues(string $bytes): void {
    $local = $bytes . "!";
    $object = new stdClass();
    $object->kept = $local[8];
    $local = $bytes . "!";
    $bag = new OffsetDynamicByteBag();
    $bag->kept = $local[8];
    $local = $bytes . "!";
    $name = "kept";
    $object->{$name} = $local[8];
}
function offsetReflectionStaticValues(string $bytes): void {
    $local = $bytes . "!";
    $class = new ReflectionClass(OffsetStaticStore::class);
    $class->setStaticPropertyValue("kept", $local[8]);
    $local = $bytes . "!";
    $property = new ReflectionProperty(OffsetStaticStore::class, "kept");
    $property->setValue(null, $local[8]);
    $local = $bytes . "!";
    $class->setStaticPropertyValue("typed", $local[8]);
    $local = $bytes . "!";
    $typed = new ReflectionProperty(OffsetStaticStore::class, "typed");
    $typed->setValue(null, $local[8]);
    $local = $bytes . "!";
    $class->setStaticPropertyValue("inferred", $local[8]);
    $local = $bytes . "!";
    $inferred = new ReflectionProperty(OffsetStaticStore::class, "inferred");
    $inferred->setValue(null, $local[8]);
}
function offsetNestedUpdateKey(string $bytes): void {
    $local = $bytes . "!";
    OffsetStaticStore::$nested[$local[8]]["child"]++;
}
function offsetBoundaries(string $bytes): void {
    $local = $bytes . "!";
    switch ($local[8]) {
        case ($local = "no"): echo "bad"; break;
        case "other": echo "bad"; break;
        default: echo "default";
    }
    $local = $bytes . "!";
    echo $local[8] |> (function (string $c): int { echo ord($c), ":"; return 1; });
    $callback = function (string $c): int { echo ord($c), ":"; return 1; };
    $local = $bytes . "!";
    echo $local[8] |> offsetCallableAfterChange($local = "no", $callback);
    $object = new stdClass();
    $local = $bytes . "!";
    $object->{$local[8]} = 1;
    $local = $bytes . "!";
    $object->{$local[8]} = ($local = "no");
    OffsetStaticStore::$object = new stdClass();
    $local = $bytes . "!";
    OffsetStaticStore::$object->{$local[8]} = ($local = "no");
    foreach (offsetYield($bytes) as $key => $value) { echo $key, $value; }
}
echo (new OffsetReader("hello"))->at($argc), offsetArray(["world"], $argc),
    (new OffsetReader("0a"))->emptyAt($argc);
offsetStores("\x07aaaaaaaZ");
offsetAcrossEffects("\x07aaaaaaaZ", $argc >= 0);
offsetAliasReturns("\x07aaaaaaaZ", new OffsetAliasReturnReader());
offsetCallableAliasReturns("\x07aaaaaaaZ");
offsetDynamicPropertyValues("\x07aaaaaaaZ");
offsetReflectionStaticValues("\x07aaaaaaaZ");
offsetNestedUpdateKey("\x07aaaaaaaZ");
offsetBoundaries("\x07aaaaaaaZ");
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let mut reads = 0;
        let mut store_reads = 0;
        let mut persisted_reads = 0;
        for function in module.functions.iter().chain(module.class_methods.iter()) {
            // Inspect this fixture's owners. Injected preludes may read borrowed bytes
            // for immediate consumers such as ord(), which need no independent copy.
            if !function.name.starts_with("offset") && !function.name.starts_with("Offset") {
                continue;
            }
            if function.name == "offsetNestedUpdateKey" {
                let byte_results = function.instructions.iter()
                    .filter(|inst| inst.op == Op::StrCharAt)
                    .filter_map(|inst| inst.result)
                    .collect::<Vec<_>>();
                assert_eq!(byte_results.len(), 2,
                    "{target}: the static nested update reads and writes the parent key");
                let stabilized = byte_results.iter().filter(|result| {
                    function.instructions.iter().any(|inst| {
                        inst.op == Op::Acquire && inst.operands == [**result]
                    })
                }).count();
                assert_eq!(stabilized, 1,
                    "{target}: persist only the nested write key that can reach hash insertion");
                let pinned = byte_results.iter().filter(|result| {
                    let mut acquired = vec![**result];
                    for inst in &function.instructions {
                        if inst.op == Op::Acquire && inst.operands.iter().any(|value| acquired.contains(value)) {
                            if let Some(value) = inst.result { acquired.push(value); }
                        }
                    }
                    if acquired.len() == 1 { return false; }
                    let slots = function.instructions.iter().filter_map(|inst| {
                        (inst.op == Op::StoreLocal
                            && inst.operands.iter().any(|value| acquired.contains(value)))
                            .then_some(inst.immediate.as_ref())
                            .flatten()
                            .and_then(|immediate| match immediate {
                                Immediate::LocalSlot(slot) => Some(*slot),
                                _ => None,
                            })
                    });
                    slots.into_iter().any(|slot| {
                        function.instructions.iter().any(|inst| {
                            inst.op == Op::PushCallOperandOwner
                                && inst.immediate == Some(Immediate::LocalSlot(slot))
                        }) && function.instructions.iter().any(|inst| {
                            inst.op == Op::PopCallOperandOwner
                                && inst.immediate == Some(Immediate::LocalSlot(slot))
                        })
                    })
                }).count();
                assert_eq!(pinned, 1,
                    "{target}: publish the persisted nested key through every throwing writer");
                reads += byte_results.len();
                persisted_reads += stabilized;
                continue;
            }
            if function.name == "offsetBoundaries" {
                assert!(function.blocks.iter().any(|block| block.name == "switch.matched"),
                    "{target}: exercise dynamic switch dispatch without single-case normalization");
            }
            for (index, read) in function.instructions.iter().enumerate() {
                if read.op != Op::StrCharAt { continue; }
                reads += 1;
                if function.name == "offsetStores" {
                    store_reads += 1;
                    persisted_reads += 1;
                    continue;
                }
                if matches!(
                    function.name.as_str(),
                    "offsetAcrossEffects" | "offsetAliasReturns" | "offsetBoundaries"
                ) {
                    assert!(function.instructions[index + 1..].iter().any(|inst| {
                        matches!(inst.op, Op::Acquire | Op::MixedBox)
                            && inst.operands.contains(&read.result.unwrap())
                    }), "{target}: {} byte at {:?} must survive the following effect", function.name, read.span);
                    persisted_reads += 1;
                    continue;
                }
                if function.name == "offsetYield" {
                    assert!(function.instructions[index + 1..].iter().any(|inst| {
                        matches!(inst.op, Op::Acquire | Op::GeneratorYield)
                            && inst.operands.contains(&read.result.unwrap())
                    }), "{target}: preserve yielded bytes and explicit keys");
                    persisted_reads += 1;
                    continue;
                }
                if function.name == "offsetCallableAliasReturns" {
                    assert!(function.instructions[index + 1..].iter().any(|inst| {
                        inst.op == Op::ExprCall && inst.operands.contains(&read.result.unwrap())
                    }), "{target}: descriptor argument must preserve the direct byte producer");
                    persisted_reads += 1;
                    continue;
                }
                if function.name == "offsetDynamicPropertyValues" {
                    let stored = function.instructions[index + 1..].iter().any(|inst| {
                        matches!(inst.op, Op::PropSet | Op::DynamicPropSet)
                            && inst.operands.contains(&read.result.unwrap())
                    });
                    if stored {
                        persisted_reads += 1;
                    } else {
                        assert!(function.instructions[index + 1..].iter().any(|inst| {
                            inst.op == Op::Release && inst.operands.contains(&read.result.unwrap())
                        }), "{target}: duplicated dynamic-write byte reads must only feed cleanup");
                    }
                    continue;
                }
                if function.name == "offsetReflectionStaticValues" {
                    assert!(function.instructions[index + 1..].iter().any(|inst| {
                        inst.op == Op::StoreReflectionStaticProperty
                            && inst.operands.contains(&read.result.unwrap())
                    }), "{target}: reflection static stores must preserve the direct byte producer");
                    persisted_reads += 1;
                    continue;
                }
                if function.name.ends_with("::emptyAt") {
                    // Silent empty stages the receiver in a local that outlives the boolean test.
                    assert!(function.instructions.iter().any(|inst| inst.op == Op::IsTruthy));
                    continue;
                }
                let copied = function.instructions[index + 1..].iter().position(|inst| {
                    inst.op == Op::Acquire && inst.operands == [read.result.unwrap()]
                }).expect("an offset view needs an independent byte owner");
                let released = function.instructions[index + 1..].iter().position(|inst| {
                    inst.op == Op::Release && inst.operands == [read.operands[0]]
                }).expect("the owning receiver must be retired");
                assert!(copied < released, "{target}: copy the byte before freeing its source");
                persisted_reads += 1;
            }
        }
        assert_eq!(reads, 44, "{target}: reads and persistent stores exercise StrCharAt");
        assert_eq!(store_reads, 7,
            "{target}: cover indexed, mixed, hash-key, hash-value and property stores");
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("str_byte_persist_ready"), "{target}: byte persistence must bypass heap header detection");
        let stage = if target == "linux-x86_64" { "mov BYTE PTR [rsp], r10b" } else { "strb w9, [sp]" };
        assert!(asm.matches(stage).count() >= persisted_reads,
            "{target}: copy every persisted byte to unmanaged stack storage");
    }
}

/// Non-byte pending strings are published before throwing calls and direct throw expressions.
#[test]
fn string_offset_pending_whole_owners_on_all_targets() {
    use crate::ir::{Immediate, Op};
    let source = r#"<?php
function failpending(): string { throw new Exception("pending"); }
function pendingconcat(string $a, string $b): void { echo ($a . $b) . failpending(); }
function pendingkey(): void { $items = ["seed" => 0]; $items[str_repeat("k", 1000)] = failpending(); }
function pendingdirect(string $a, string $b, Exception $error, bool $flag): void {
    echo ($a . $b) . ($flag ? throw $error : "");
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["pendingconcat", "pendingkey", "pendingdirect"] {
            let function = module.functions.iter().find(|f| f.name.eq_ignore_ascii_case(name)).unwrap();
            let mut protected = 0;
            for block in &function.blocks {
                for (position, id) in block.instructions.iter().enumerate() {
                    let call = function.instruction(*id).unwrap();
                    let Some(Immediate::Data(callee)) = call.immediate else { continue; };
                    if call.op != Op::Call || !module.data.function_names[callee.as_raw() as usize].eq_ignore_ascii_case("failpending") { continue; }
                    assert!(block.instructions[..position].iter().any(|id| function.instruction(*id).unwrap().op == Op::PushCallOperandOwner), "{target}: {name}");
                    protected += 1;
                }
            }
            if name == "pendingdirect" {
                assert!(function.instructions.iter().any(|inst| inst.op == Op::ThrowException), "{target}: direct throw expression");
                assert!(function.instructions.iter().any(|inst| inst.op == Op::PushCallOperandOwner), "{target}: direct throw owner");
            } else { assert_eq!(protected, 1, "{target}: {name}"); }
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}


/// Warning-only byte reads publish earlier owners before calling a throwing user handler.
#[test]
fn string_offset_pending_warning_owners_on_all_targets() {
    use crate::ir::Op;
    let source = r#"<?php
function warning_concat(string $s, int $i): void { echo $s[0] . $s[$i]; }
function warning_key(string $s, int $i): void { $items = ["seed" => 0]; $items[$s . $s] = $s[$i]; }
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["warning_concat", "warning_key"] {
            let function = module.functions.iter().find(|f| f.name.eq_ignore_ascii_case(name)).unwrap();
            let (block, position) = function.blocks.iter().find_map(|block| {
                block.instructions.iter().enumerate().rev().find_map(|(position, id)| {
                    (function.instruction(*id).unwrap().op == Op::StrCharAt).then_some((block, position))
                })
            }).expect("the fixture retains its warning-capable RHS byte read");
            assert!(block.instructions[..position].iter().any(|id| {
                function.instruction(*id).unwrap().op == Op::PushCallOperandOwner
            }), "{target}: {name} must protect its pending operand before the warning");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// A nested index call sees the persisted byte receiver published until the outer read completes.
#[test]
fn string_offset_nested_index_owner_on_all_targets() {
    use crate::ir::Op;
    let source = r#"<?php
function throwing_nested_offset_index(): int { throw new Exception("index"); }
function nested_offset_read(string $s): void { echo $s[0][throwing_nested_offset_index()]; }
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter()
            .find(|function| function.name.eq_ignore_ascii_case("nested_offset_read"))
            .unwrap();
        let (block, call_position) = function.blocks.iter().find_map(|block| {
            block.instructions.iter().enumerate().find_map(|(position, id)| {
                let call = function.instruction(*id).unwrap();
                let Some(crate::ir::Immediate::Data(callee)) = call.immediate else { return None; };
                (call.op == Op::Call
                    && module.data.function_names[callee.as_raw() as usize]
                        .eq_ignore_ascii_case("throwing_nested_offset_index"))
                    .then_some((block, position))
            })
        }).expect("the nested offset fixture retains its throwing index call");
        let outer_read = block.instructions[call_position + 1..].iter().position(|id| {
            function.instruction(*id).unwrap().op == Op::StrCharAt
        }).map(|position| call_position + 1 + position)
            .expect("the outer string read follows the nested index call");
        assert_eq!(function.instructions.iter().filter(|instruction| {
            instruction.op == Op::PushCallOperandOwner
        }).count(), 1, "{target}: the receiver needs one owner record");
        assert_eq!(function.instructions.iter().filter(|instruction| {
            instruction.op == Op::PopCallOperandOwner
        }).count(), 1, "{target}: the receiver owner record must retire once");
        assert!(block.instructions[..call_position].iter().any(|id| {
            function.instruction(*id).unwrap().op == Op::PushCallOperandOwner
        }), "{target}: the byte receiver must be published before the nested index call");
        assert!(block.instructions[outer_read + 1..].iter().any(|id| {
            function.instruction(*id).unwrap().op == Op::PopCallOperandOwner
        }), "{target}: the byte receiver must stay published through the outer read");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
