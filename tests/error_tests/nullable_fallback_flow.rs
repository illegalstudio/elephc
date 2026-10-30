//! Purpose:
//! Checks nullable fallback facts through boolean guards and local coalescing assignments.
//!
//! Called from:
//! - The `error_tests` integration test harness.
//!
//! Key details:
//! - Positive fixtures retain declared return contracts; negative fixtures test unsafe joins.

use super::*;

/// Loops that must execute publish their actual exits without a zero-iteration alternative.
#[test]
fn test_nullable_fallback_flow_review_must_run_loops() {
    for body in [
        "do { if ($value === null) { return 0; } } while (false); return $value;",
        "do { if ($value === null) { return 0; } break; } while (true); return $value;",
        "while (true) { if ($value === null) { return 0; } break; } return $value;",
        "while (1) { if ($value === null) { return 0; } break; } return $value;",
        "for (;;) { if ($value === null) { return 0; } break; } return $value;",
        "switch ($key) { case 1: if ($value === null) { return 0; } break; default: if ($value === null) { return 0; } break; } return $value;",
    ] {
        let source = format!("<?php function run(?int $value, int $key): int {{ {body} }}");
        check_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
    }
    expect_error("<?php function run(?int $value, bool $flag): int { while ($flag) { if ($value === null) { return 0; } break; } return $value; }", "return type expects int");
}

/// A finally input includes pending loop and switch exits before its body is checked.
#[test]
fn test_nullable_fallback_flow_review_finally_inputs() {
    for coalesce in ["$value ??= 1;", "$value = $value ?? 1;"] {
        for body in [
            "foreach ([1] as $i) { try { if ($flag) { $value = null; break; } } finally { need($value); } }",
            "switch ($key) { case 1: try { if ($flag) { $value = null; break; } } finally { need($value); } }",
            "try { if ($value !== null) { $value = null; return; } } finally { need($value); }",
            "try { if ($value !== null) { $value = null; throw new RuntimeException('failed'); } } finally { need($value); }",
            "try { if ($value !== null) { $value = null; (throw new RuntimeException('failed')); } } finally { need($value); }",
            "try { try { throw new RuntimeException('initial'); } catch (RuntimeException $error) { if ($value !== null) { $value = null; throw new RuntimeException('failed'); } } } finally { need($value); }",
        ] {
            let source = format!("<?php function need(int $value): void {{}} function run(?int $value, bool $flag, int $key): void {{ {coalesce} {body} }}");
            expect_error(&source, "expects int");
        }
    }
    for body in [
        "try { if ($value !== null) { $value = null; throw new RuntimeException('failed'); } } catch (RuntimeException $error) { $value ??= 2; } finally { need($value); }",
        "try { try { if ($value !== null) { $value = null; throw new RuntimeException('failed'); } } catch (RuntimeException $error) { $value ??= 2; } } finally { need($value); }",
        "try { $noop = function (?int $value): void { $value = null; return; }; $noop(null); } finally { need($value); }",
        "try { return; $value = null; return; } finally { need($value); }",
    ] {
        let source = format!("<?php function need(int $value): void {{}} function run(?int $value): void {{ $value ??= 1; {body} }}");
        check_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
    }
}

/// Caught and implicit throws retain writes at each operation's source-order boundary.
#[test]
fn test_nullable_fallback_flow_review_catch_unwind() {
    check_source("<?php class UnrelatedRetirementFlow { public function __destruct() { throw new LogicException('drop'); } } function failRetirementFlow(): never { throw new RuntimeException('fail'); } function needRetirementFlow(int $value): void {} function run(?int $value): void { $value ??= 4; try { if ($value !== null) { $value = null; FAILRETIREMENTFLOW(); } } catch (RuntimeException $e) { $value ??= 2; } finally { needRetirementFlow($value); } }").unwrap();
    for coalesce in ["$value ??= 4;", "$value = $value ?? 4;"] {
        for body in [
            "try { if ($value !== null) { $value = null; throw new RuntimeException('x'); } } catch (RuntimeException $e) {} finally { need($value); }",
            "try { if ($value !== null) { $value = null; throw new RuntimeException('x'); } } catch (Exception $e) {} need($value);",
            "try { if ($value !== null) { $value = null; throw new RuntimeException('x'); } } catch (RuntimeException $e) { need($value); }",
            "try { if ($value !== null) { $value = null; mayThrow(); return; } } catch (RuntimeException $e) {} need($value);",
            "try { if ($value !== null) { $value = null; fail(); } } finally { need($value); }",
            "foreach ([1] as $i) { try { if ($value !== null) { $value = null; fail(); } } finally { need($value); } }",
            "try { if ($value !== null) { $value = null; $zero = 0; $out = 1 / $zero; return; } } catch (Throwable $e) {} need($value);",
        ] {
            let source = format!("<?php function need(int $v): void {{}} function fail(): never {{ throw new RuntimeException('x'); }} function mayThrow(): void {{ throw new RuntimeException('x'); }} function run(?int $value): void {{ {coalesce} {body} }}");
            expect_error(&source, "expects int");
        }
    }
    for body in [
        "try { if ($value === null) { return; } throw new RuntimeException('x'); } catch (RuntimeException $e) { need($value); }",
        "try { $value = null; throw new RuntimeException('x'); } catch (RuntimeException $e) { $value ??= 2; need($value); }",
        "try { $value = null; mayThrow(); } catch (RuntimeException $e) { $value ??= 2; need($value); }",
        "$value ??= 1; try { if ($value !== null) { $value = null; FAIL(); } } catch (RuntimeException $e) { $value ??= 2; } finally { need($value); }",
        "try { $value ??= 1; pureCall(); } catch (RuntimeException $e) {}",
        "$value ??= 1; try { if ($value !== null) { $value = null; exit(); } } finally { need($value); }",
        "$value ??= 1; try { if ($value !== null) { $value = null; die(); } } finally { need($value); }",
        "$value ??= 1; try { try { pureArgs(throwsArg(), $value = null); } catch (RuntimeException $e) { need($value); } } finally {}",
        "$value ??= 1; try { if ($flag) { $value = null; throw new RuntimeException('x'); } throw new LogicException('x'); } catch (RuntimeException $e) { $value ??= 2; } catch (LogicException $e) { need($value); } finally { need($value); }",
    ] {
        let source = format!("<?php function need(int $v): void {{}} function pureCall(): void {{}} function fail(): never {{ throw new RuntimeException('x'); }} function pureArgs(int $a, ?int $b): void {{}} function throwsArg(): int {{ throw new RuntimeException('x'); }} function mayThrow(): void {{ throw new RuntimeException('x'); }} function run(?int $value, bool $flag): void {{ {body} }}");
        check_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
    }
}

/// Statement callbacks and iterator resumptions contribute their actual catch input states.
#[test]
fn test_nullable_fallback_flow_review_statement_exceptions() {
    for coalesce in ["$value ??= 5;", "$value = $value ?? 5;"] {
        for body in [
            "try { mayThrow(false); $value = null; echo $label; $value ??= 6; } catch (RuntimeException $e) { return $value; } return 0;",
            "try { echo $label; } catch (RuntimeException $e) { $value = null; } return $value;",
            "try { mayThrow(false); $value = null; $bag->missing = 1; $value ??= 6; } catch (RuntimeException $e) { return $value; } return 0;",
            "try { mayThrow(false); foreach (rows(true) as $row) { $value = null; } $value ??= 6; } catch (RuntimeException $e) { return $value; } return 0;",
            "try { mayThrow(false); foreach (rows(true) as $row) { $value = null; continue; } $value ??= 6; } catch (RuntimeException $e) { return $value; } return 0;",
            "try { ($value = null) |> passValue(...); } catch (RuntimeException $e) {} return $value;",
        ] {
            let source = format!("<?php class Label {{ public function __toString(): string {{ throw new RuntimeException('label'); }} }} class Bag {{ public function __set(string $name, mixed $value): void {{ throw new RuntimeException('set'); }} }} function mayThrow(bool $flag): void {{ if ($flag) {{ throw new RuntimeException('call'); }} }} function passValue(?int $value): int {{ return 0; }} function rows(bool $fail): iterable {{ yield 1; if ($fail) {{ throw new RuntimeException('rows'); }} }} function run(?int $value, Label $label, Bag $bag): int {{ {coalesce} {body} }}");
            expect_error(&source, "return type expects int");
        }
    }
    for setup in ["", "$old = null; $set = function () use (&$old) { $old = new RetiredFlow(); }; $set();"] {
        let source = format!("<?php class RetiredFlow {{ public function __destruct() {{ throw new RuntimeException('drop'); }} }} function mayThrow(bool $flag): void {{ if ($flag) {{ throw new RuntimeException('call'); }} }} function run(?int $value, ?RetiredFlow $old): int {{ $value ??= 5; {setup} try {{ mayThrow(false); $value = null; $old = null; $value ??= 6; }} catch (Throwable $e) {{ return $value; }} return 0; }}");
        expect_error(&source, "return type expects int");
    }

}

/// Reference operands and declaration initializers cannot silently discard a throwing catch.
#[test]
fn test_nullable_fallback_flow_review_initializer_effects() {
    for coalesce in ["$value ??= 4;", "$value = $value ?? 4;"] {
        for body in [
            "$r = &$arr[failInit()];",
            "$r = &$holder->item();",
            "$r = &makeHolderInit()->slot;",
            "$r = &refInit();",
            "$r = &HolderInit::staticItem();",
            "int $local = failInit();",
            "static $saved = failInit();",
            "const FLOW_INIT = failInit();",
            "int $local = ($value = null) ?? 7;",
            "static $saved = ($value = null) ?? 7;",
        ] {
            let source = format!("<?php function failInit(): int {{ throw new RuntimeException('x'); }} function &refInit(): int {{ throw new RuntimeException('x'); }} function makeHolderInit(): HolderInit {{ throw new RuntimeException('x'); }} class HolderInit {{ public int $slot = 1; public function &item(): int {{ throw new RuntimeException('x'); }} public static function &staticItem(): int {{ throw new RuntimeException('x'); }} }} function run(?int $value, HolderInit $holder): int {{ {coalesce} $arr = [1, 2]; try {{ {body} }} catch (RuntimeException $e) {{ $value = null; }} return $value; }}");
            expect_error(&source, "return type expects int");
        }
    }
    expect_error("<?php class RefRetiredInit { public function __destruct() { throw new RuntimeException('drop'); } } function mayThrowRefInit(bool $flag): void { if ($flag) { throw new RuntimeException('earlier'); } } function run(?int $value, mixed $old): int { $value ??= 4; $source = 1; try { mayThrowRefInit(false); $value = null; $old = &$source; $value ??= 6; } catch (Throwable $e) { return $value; } return 0; }", "return type expects int");
}

/// Eager operands and nonliteral callbacks cannot conceal a store from guarded reads.
#[test]
fn test_nullable_fallback_flow_review_eager_operands() {
    for coalesce in ["$value ??= 4;", "$value = $value ?? 4;"] {
        for body in [
            "if (($value = null) instanceof OperandFlow) {} return $value;",
            "new $className($value = null); return $value;",
            "$copy = clone (($value = null) ?? new OperandFlow()); return $value;",
            "$kept = array_filter([1, 0, 2], $value = null); return $value;",
            "$kept = array_filter(callback: $value = null, array: [1, 0, 2]); return $value;",
            "$kept = array_filter([1], (($value = null) ?? new OperandFlow())->accept(...)); return $value;",
        ] {
            let source = format!("<?php class OperandFlow {{ public function __construct(mixed $arg = null) {{}} public function accept(int $value): bool {{ return true; }} }} function run(?int $value, string $className): int {{ {coalesce} {body} }}");
            let error = check_source(&source).expect_err("hidden null store must fail").to_string();
            assert!(error.contains("return type expects int"), "{source}: {error}");
        }
    }
    expect_error("<?php class OperandFlow {} function run(?string $raw, ?int $value): int { if ($raw === null && $value !== null) { if (($value = null) instanceof OperandFlow) {} return $value; } return 0; }", "return type expects int");
    check_source("<?php function run(): array { return array_filter(['one', ''], fn($text) => strlen($text) > 0); }").unwrap();
    for expression in ["yield ($value = null);", "yield from [($value = null)];"] {
        let source = format!("<?php function needYieldOperand(int $value): void {{}} function values(?int $value): iterable {{ $value ??= 4; {expression} needYieldOperand($value); }}");
        expect_error(&source, "expects int");
    }

}

/// Conditional access and match labels retain both their skipped and evaluated local states.
#[test]
fn test_nullable_fallback_flow_review_conditional_operands() {
    let mut admitted = Vec::new();
    for coalesce in ["$value ??= 4", "$value = $value ?? 4"] {
        let source = format!("<?php class ConditionalOperandFlow {{ public function take(int $arg): void {{}} }} function run(?int $value, ?ConditionalOperandFlow $receiver): int {{ $receiver?->take({coalesce}); return $value; }}");
        match check_source(&source) {
            Ok(_) => admitted.push(source),
            Err(error) => assert!(error.to_string().contains("return type expects int"), "{source}: {error}"),
        }
    }
    for body in [
        "return match ($key) { ($value = null) => 0, default => needOperand($value) };",
        "$holder->{$property} = ($value = null); return $value;",
        "$holder->{(($value = null) ?? 'item')} = 1; return $value;",
        "try { $holder->{$property} = 1; } catch (RuntimeException $e) { $value = null; } return $value;",
        "try { $holder->{$property} = resetOperand($value); } catch (RuntimeException $e) { return $value; } return 0;",
    ] {
        let source = format!("<?php class ConditionalOperandFlow {{ public mixed $item = null; public function __set(string $name, mixed $value): void {{ throw new RuntimeException('set'); }} }} function resetOperand(?int &$value): int {{ $value = null; throw new RuntimeException('reset'); }} function needOperand(int $value): int {{ return $value; }} function run(?int $value, int $key, ConditionalOperandFlow $holder, string $property): int {{ $value ??= 4; {body} }}");
        match check_source(&source) {
            Ok(_) => admitted.push(source),
            Err(error) => assert!(error.to_string().contains("expects int"), "{source}: {error}"),
        }
    }
    assert!(admitted.is_empty(), "unsafe admitted operands: {admitted:#?}");
    expect_error("<?php function needLabelOperand(int $value): int { return $value; } function run(?int $value, int $key): int { return match ($key) { 1, ($value ??= 4) => needLabelOperand($value), default => 0 }; }", "expects int");
    check_source("<?php class NonNullOperandFlow { public function take(int $value): void {} } function run(?int $value, NonNullOperandFlow $receiver): int { $receiver?->take($value ??= 4); return $value; }").unwrap();

}

/// A captured reference's stale null type cannot prove that the try always returns early.
#[test]
fn test_nullable_fallback_flow_review_captured_null_reachability() {
    for coalesce in ["$value ??= 5;", "$value = $value ?? 5;"] {
        let source = format!("<?php function run(?int $value): int {{ {coalesce} $flag = null; $set = function () use (&$flag) {{ $flag = 1; }}; try {{ $set(); if ($flag === null) {{ return -1; }} $value = null; }} catch (RuntimeException $e) {{}} return $value; }}");
        expect_error(&source, "return type expects int");
    }
}

/// Broken switch arms cannot narrow a later arm that can be entered directly.
#[test]
fn test_nullable_fallback_flow_review_switch_arm_inputs() {
    for guard in ["$closed || $value === null", "$value === null"] {
        for first_exit in ["break;", "continue;", "return 1;"] {
            let source = format!("<?php function run(?int $value, int $key, bool $closed): int {{ switch ($key) {{ case 1: if ({guard}) {{ return -1; }} {first_exit} case 2: return $value; }} return 0; }}");
            expect_error(&source, "return type expects int");
        }
    }
    check_source("<?php function run(?int $value, int $key): ?int { switch ($key) { case 1: $value = null; case 2: return $value; default: return $value; } }").unwrap();
}

/// Non-null object references preserve the established Mixed output view of boxed callers.
#[test]
fn test_nullable_fallback_flow_review_non_null_object_reference() {
    check_source("<?php class Box { public int $value = 1; } function swapBox(Box &$box): void { $box = new Box(); $box->value = 9; } function run(?Box $box): int { if ($box === null) { return -1; } swapBox($box); return $box->value; }").unwrap();
}

/// Numeric storage preparation must not replace an unguarded nullable read with Mixed.
#[test]
fn test_nullable_fallback_flow_review_plain_loop_nullability() {
    for rhs in ["$flag ? null : 1", "$flag ? 1.5 : 1", "$flag ? 's' : 1", "$value + 1", "'x'", "[1]", "$x"] {
        let source = format!("<?php function need(int $value): void {{}} function run(?int $value, array $xs, bool $flag): void {{ foreach ($xs as $x) {{ need($value); $value = {rhs}; }} }}");
        expect_error(&source, "expects int");
    }
    expect_error("<?php function run(?int $value, bool $flag): int { for ($i = 0; $i < 2; $i++) { $value = $flag ? null : $i; } return $value; }", "return type expects int");
    check_source("<?php function resetValue(?int &$value): void { $value = null; } function run(?int $value, array $xs, bool $flag): void { foreach ($xs as $x) { $value = $flag ? null : 1; } resetValue($value); }").unwrap();
}

/// Loop exits preserve the complement on the path that reaches the following statement.
#[test]
fn test_nullable_fallback_flow_review_loop_exit_complements() {
    let _strict = elephc::strict_php::scoped_enable();
    for header in ["foreach ($xs as $x)", "while ($flag)", "for (; $flag; $flag = false)", "do"] {
        for exit in ["continue", "break"] {
            let tail = if header == "do" { " while ($flag);" } else { "" };
            for store in ["$value = 0;", "$value = null;", ""] {
                let source = format!("<?php function run(?int $value, array $xs, bool $flag): int {{ {header} {{ if ($value === null) {{ {store} {exit}; }} return $value; }}{tail} return 0; }}");
                check_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
            }
        }
    }
    check_source("<?php function run(?int $value, array $xs): int { foreach ($xs as $x) { while (true) { if ($value === null) { $value = 0; continue 2; } return $value; } } return 0; }").unwrap();
}

/// Nullable writes on back edges retire both spellings of an earlier coalescing view.
#[test]
fn test_nullable_fallback_flow_review_loop_entry_writes() {
    let _strict = elephc::strict_php::scoped_enable();
    for coalesce in ["$value ??= 1;", "$value = $value ?? 1;"] {
        for body in [
            "foreach ($xs as $x) { need($value); $value = null; }",
            "foreach ($xs as $x) { need($value); $value = $flag ? null : 1; }",
            "while ($flag) { need($value); $value = null; $flag = false; }",
            "for (; $flag; $value = null) { need($value); $flag = false; }",
            "do { need($value); $value = null; } while ($flag);",
            "foreach ($xs as $x) { need($value); if ($flag) { $value = null; continue; } }",
            "foreach ($xs as $x) { need($value); resetValue($value); }",
            "foreach ($xs as $x) { need($value); resetValue(value: $value); }",
            "foreach ($xs as $x) { need($value); $reset = function () use (&$value) { $value = null; }; $reset(); }",
            "foreach ($xs as $x) { need($value); $alias =& $value; $alias = null; }",
            "foreach ($xs as $x) { need($value); switch ($x) { default: $value = null; case 1: continue 2; } return; }",
            "foreach ($xs as $x) { need($value); while (true) { $value = null; continue 2; } }",
            "foreach ($xs as $x) { need($value); try { continue; } finally { $value = null; } }",
        ] {
            let source = format!("<?php function need(int $value): void {{}} function resetValue(?int &$value): void {{ $value = null; }} function run(?int $value, array $xs, bool $flag): void {{ {coalesce} {body} }}");
            let error = check_source(&source).expect_err(&source);
            assert!(error.contains("expects int") && error.contains("null"), "{source}: {error}");
        }
        for body in [
            "foreach ($xs as $x) { need($value); $value = 2; }",
            "foreach ($xs as $x) { need($value); $value = null; break; }",
            "foreach ($xs as $x) { need($value); while (true) { $value = null; break 2; } }",
            "foreach ($xs as $x) { if ($flag) { $value = null; break; } need($value); }",
            "foreach ($xs as $x) { need($value); $value = null; return; }",
            "foreach ($xs as $x) { need($value); try { $value = null; continue; } finally { return; } }",
        ] {
            let source = format!("<?php function need(int $value): void {{}} function run(?int $value, array $xs, bool $flag): void {{ {coalesce} {body} }}");
            check_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
        }
    }
    for (ty, default) in [("bool", "true"), ("string", "'ok'"), ("Service", "new Service()")] {
        for coalesce in [format!("$value ??= {default};"), format!("$value = $value ?? {default};")] {
            for write in ["$value = null;", "resetValue($value);"] {
                let source = format!("<?php final class Service {{}} function need({ty} $value): void {{}} function resetValue(?{ty} &$value): void {{ $value = null; }} function run(?{ty} $value, array $xs): void {{ {coalesce} foreach ($xs as $x) {{ need($value); {write} }} }}");
                let error = check_source(&source).expect_err(&source);
                assert!(error.contains(&format!("expects {ty}")) && error.contains("null"), "{source}: {error}");
            }
        }
    }
}

/// Tail bindings survive the loop and continue edges retain nullable reference storage.
#[test]
fn test_nullable_fallback_flow_review_loop_tail_bindings() {
    for source in [
        "<?php for ($i = 0; $i < 3; $prev = $i, $i++) { if ($i > 0) { echo $prev; } }",
        "<?php for ($i = 0, $acc = 0; $i < 3; $i++, $total = $acc) { $acc += $i; } echo $total;",
        "<?php function nextLine(): ?string { return null; } do { $k = 1; } while (($line = nextLine()) !== null); var_dump($line);",
        "<?php function bump(?int &$x): void { $x = ($x ?? 0) + 1; } function run(?int $value): void { for ($i = 0; $i < 3; $i++, bump($value)) { if ($value !== null) { continue; } $value = null; break; } var_dump($value); } run(5); run(null);",
    ] { check_source(source).unwrap_or_else(|error| panic!("{source}: {error}")); }
}

/// Switch breaks and level-one continues publish terminal writes to the switch exit.
#[test]
fn test_nullable_fallback_flow_review_switch_exits() {
    let _strict = elephc::strict_php::scoped_enable();
    for prefix in ["$value ??= 3;", "$value = $value ?? 3;", "if ($value === null) { return 0; }"] {
        for exit in ["break", "continue"] {
            let source = format!("<?php function run(?int $value, int $key): int {{ {prefix} switch ($key) {{ case 1: if ($key > 0) {{ $value = null; {exit}; }} break; }} return $value; }}");
            expect_error(&source, "return type expects int");
        }
        expect_error(&format!("<?php function run(?int $value, int $key): int {{ {prefix} switch ($key) {{ default: $value = null; case 1: return $value; }} return 0; }}"), "return type expects int");
        expect_error(&format!("<?php function need(int $value): void {{}} function run(?int $value, int $key): int {{ {prefix} switch ($key) {{ default: $value = null; case 1: need($value); }} return 0; }}"), "expects int");
        let source = format!("<?php function run(?int $value, int $key): ?int {{ {prefix} foreach ([1, 2] as $i) {{ switch ($key) {{ case 1: if ($key > 0) {{ $value = null; continue 2; }} break; }} }} return $value; }}");
        check_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
    }
}

/// Break values remain visible after the loop and retain their reference ABI.
#[test]
fn test_nullable_fallback_flow_review_loop_exit_storage() {
    let _strict = elephc::strict_php::scoped_enable();
    for body in [
        "foreach ($xs as $x) { if (is_int($value)) { $value = null; break; } } need($value);",
        "foreach ($xs as $x) { if ($value === null) { $value = 0; continue; } } need($value);",
        "foreach ($xs as $x) { try { if ($value === null) { $value = 0; break; } } finally { $value = null; } } need($value);",
    ] {
        expect_error(&format!("<?php function need(int $value): void {{}} function run(?int $value, array $xs): void {{ {body} }}"), "expects int");
    }
    for exit in ["break", "continue"] {
        let source = format!("<?php function widen(mixed &$value): void {{ $value = 'x'; }} function resetValue(?int &$value): void {{ $value = null; }} function run(?int $value, array $xs): void {{ foreach ($xs as $x) {{ if ($value === null) {{ widen($value); {exit}; }} resetValue($value); }} }}");
        let error = check_source(&source).expect_err(&source);
        assert!(error.contains("by-reference storage") || error.contains("got mixed"), "{error}");
    }
}

/// Compound and nested guards expose non-null integer and boolean fallback parameters.
#[test]
fn test_nullable_fallback_flow_and_guards() {
    let _strict = elephc::strict_php::scoped_enable();
    for (ty, fallback) in [("int", "0"), ("bool", "false")] {
        for guard in [
            "if ($raw === null && $default !== null) { return $default; }",
            "if ($default !== null && $raw === null) { return $default; }",
            "if ($raw === null) { if ($default !== null) { return $default; } }",
            "if (!($raw !== null || $default === null)) { return $default; }",
            "if ($raw === null && !($default === null)) { return $default; }",
        ] {
            let source = format!("<?php function queryFallback(?string $raw, ?{ty} $default = null): {ty} {{ {guard} return {fallback}; }}");
            check_source(&source).unwrap_or_else(|error| panic!("{guard}: {error}"));
        }
    }
}

/// Every operand of a false OR guard holds on its non-divergent fallthrough path.
#[test]
fn test_nullable_fallback_flow_or_fallthrough() {
    let _strict = elephc::strict_php::scoped_enable();
    for guard in [
        "$closed || $value === null",
        "$value === null || $closed",
        "!(!$closed && $value !== null)",
        "$closed || ($value === null || $other === null)",
    ] {
        let source = format!("<?php final class Service {{}} function service(?Service $value, bool $closed, ?int $other): Service {{ if ({guard}) {{ throw new LogicException('unavailable'); }} return $value; }}");
        check_source(&source).unwrap_or_else(|error| panic!("{guard}: {error}"));
    }
}

/// A coalesced receiver assignment removes only the null member, including in expression form.
#[test]
fn test_nullable_fallback_flow_coalesced_receiver() {
    let _strict = elephc::strict_php::scoped_enable();
    for assignment in ["$options = $options ?? new Options();", "$options ??= new Options();", "consume(($options = $options ?? new Options())->path);"] {
        let source = format!("<?php final class Options {{ public readonly string $path; public function __construct() {{ $this->path = 'ok'; }} }} function consume(string $path): void {{}} function run(?Options $options = null): void {{ {assignment} consume($options->path); }}");
        check_source(&source).unwrap_or_else(|error| panic!("{assignment}: {error}"));
    }
}

/// OR-true and AND-false paths do not prove that either nullable operand is non-null.
#[test]
fn test_nullable_fallback_flow_unsafe_joins() {
    let _strict = elephc::strict_php::scoped_enable();
    for body in [
        "if ($flag || $value !== null) { return $value; } return 0;",
        "if ($flag && $value === null) { throw new Exception('missing'); } return $value;",
        "if ($flag && $value !== null) {} return $value;",
        "$value = $value ?? null; return $value;",
        "if ($flag) { $value ??= 0; } return $value;",
        "$value ??= 0; $value = null; return $value;",
    ] {
        let source = format!("<?php function fallback(?int $value, bool $flag): int {{ {body} }}");
        let error = check_source(&source).expect_err(body);
        assert!(error.contains("return type expects int"), "{body}: {error}");
    }
}

/// A later operand may replace a local directly or through a reference-taking call.
#[test]
fn test_nullable_fallback_flow_operand_mutation() {
    let _strict = elephc::strict_php::scoped_enable();
    for condition in [
        "$value !== null && (($value = null) === null)",
        "$value !== null && resetValue($value)",
    ] {
        expect_error(&format!("<?php final class Service {{}} function resetValue(?Service &$value): bool {{ $value = null; return true; }} function fallback(?Service $value): Service {{ if ({condition}) {{ return $value; }} return new Service(); }}"), "return type expects Service");
    }
    expect_error(
        "<?php final class Service {} class Box { public ?Service $value = null; public function __get(string $name): bool { $this->value = null; return true; } } function fallback(Box $box, mixed $proxy): Service { if ($box->value !== null && $proxy->flag) { return $box->value; } return new Service(); }",
        "return type expects Service",
    );
}

/// Short-circuit operands, ternaries, elseif complements and while bodies share guard facts.
#[test]
fn test_nullable_fallback_flow_expression_paths() {
    let _strict = elephc::strict_php::scoped_enable();
    for body in [
        "return $value !== null && positive($value);",
        "return $value === null || positive($value);",
        "return $raw === null && $value !== null ? positive($value) : false;",
        "if ($raw !== null || $value === null) { return false; } elseif (positive($value)) { return true; } return false;",
        "while ($raw === null && $value !== null) { return positive($value); } return false;",
        "if ($raw === null && $value !== null) { $value = null; } return false;",
    ] {
        let source = format!("<?php function positive(int $value): bool {{ return $value > 0; }} function fallback(?string $raw, ?int $value): bool {{ {body} }}");
        check_source(&source).unwrap_or_else(|error| panic!("{body}: {error}"));
    }
}

/// Refining a nullable binding does not forbid assigning null again later.
#[test]
fn test_nullable_fallback_flow_coalesce_reassignment() {
    let _strict = elephc::strict_php::scoped_enable();
    check_source("<?php function fallback(?int $value): int { $value ??= 0; $value = null; $value ??= 1; return $value; }").unwrap();
}

/// Reference-taking calls see the underlying nullable slot, including after coalescing.
#[test]
fn test_nullable_fallback_flow_reference_storage() {
    let _strict = elephc::strict_php::scoped_enable();
    check_source("<?php function resetValue(?int &$value): bool { $value = null; return true; } function fallback(?int $value): bool { return $value !== null && resetValue($value); }").unwrap();
    check_source("<?php function resetValue(?int &$value): void { $value = null; } function fallback(?int $value): void { $value ??= 0; resetValue($value); }").unwrap();
}

/// A scalar read view cannot turn a nullable boxed binding into raw reference storage.
#[test]
fn test_nullable_fallback_flow_review_reference_boundaries() {
    let _strict = elephc::strict_php::scoped_enable();
    for (ty, value) in [("string", "'changed'"), ("float", "3.0"), ("int", "2"), ("bool", "true")] {
        for body in [
            "if ($closed || $value === null) { return; } change($value);",
            "$result = $raw === null && $value !== null ? change($value) : false;",
            "if ($value !== null) { change($value); }",
            "while ($value !== null) { change($value); break; }",
        ] {
            let source = format!("<?php function change({ty} &$value): bool {{ $value = {value}; return true; }} function run(?{ty} $value, bool $closed, ?string $raw): void {{ {body} }}");
            let error = check_source(&source).expect_err(&source);
            assert!(error.contains("by-reference storage") || error.contains(&format!("got {ty}|null")), "{source}: {error}");
        }
    }
}

/// Unset, foreach destinations and reference rebinding end the previous coalescing contract.
#[test]
fn test_nullable_fallback_flow_review_binding_lifecycle() {
    let _strict = elephc::strict_php::scoped_enable();
    for body in [
        "unset($value); $value = 'abc'; return $value;",
        "foreach (['a'] as $value) {} $value = 'b'; return $value;",
        "foreach (['a' => 1] as $value => $item) {} $value = 'b'; return $value;",
        "$other = 'a'; $value =& $other; change($value); return $value;",
    ] {
        let source = format!("<?php function change(string &$value): void {{ $value .= '!'; }} function run(?int $value): string {{ $value ??= 0; {body} }}");
        check_source(&source).unwrap_or_else(|error| panic!("{body}: {error}"));
    }
}

/// Retained read facts do not replace the binding's strict assignment or widening decisions.
#[test]
fn test_nullable_fallback_flow_review_widening_contract() {
    let _strict = elephc::strict_php::scoped_enable();
    let source = "<?php function run(?int $value): void { $value ??= 0; $value = 1.5; echo $value; }";
    let error = check_source_strict(source).unwrap_err();
    assert!(error.contains("cannot reassign $value from int|null to float"), "{error}");
    let result = check_source_full(source).unwrap();
    assert!(result.warnings.iter().any(|warning| warning.message.contains("changes type from int|null to float")));
    check_source("<?php final class P { public string $p = 'ok'; } function run(?int $value, mixed $other): void { $value ??= 0; $value = $other; echo $value->p; }").unwrap();
}

/// Aliases and global/static rebinding cannot retain a coalescing non-null guarantee.
#[test]
fn test_nullable_fallback_flow_review_alias_barriers() {
    let _strict = elephc::strict_php::scoped_enable();
    for body in [
        "$value ??= new Service(); $alias =& $value; $alias = null; return $value;",
        "$alias =& $value; $value ??= new Service(); $alias = null; return $value;",
        "$value ??= new Service(); global $value; return $value;",
        "$value ??= new Service(); static $value = null; return $value;",
    ] {
        expect_error(&format!("<?php final class Service {{}} function run(?Service $value): Service {{ {body} }}"), "return type expects Service");
    }
}

/// Long associative chains retain every guaranteed fact without repeated prefix inference.
#[test]
fn test_nullable_fallback_flow_review_long_chains() {
    let _strict = elephc::strict_php::scoped_enable();
    let params = (0..400).map(|index| format!("?int $a{index}")).collect::<Vec<_>>().join(", ");
    for (operator, comparison, body) in [
        (" && ", " !== null", "if (CONDITION && $closed) { return $a399; } return 0;"),
        (" || ", " === null", "if (CONDITION || $closed) { throw new Exception('missing'); } return $a399;"),
    ] {
        let condition = (0..400).map(|index| format!("$a{index}{comparison}"))
            .collect::<Vec<_>>().join(operator);
        let source = format!("<?php function run({params}, bool $closed): int {{ {} }}", body.replace("CONDITION", &condition));
        check_source(&source).unwrap_or_else(|error| panic!("{operator}: {error}"));
    }
}

/// Guard-complement storage does not undo compatible ordinary stores or their precise read types.
#[test]
fn test_nullable_fallback_flow_review_guarded_stores() {
    let _strict = elephc::strict_php::scoped_enable();
    for source in [
        "<?php function run(?string $value): string { if ($value === null) { return ''; } $value = trim($value); return $value; }",
        "<?php function run(?int $value): int { if ($value === null) { throw new Exception(); } $value = 0; return $value; }",
        "<?php final class S { public function next(): S { return new S(); } } function run(?S $value): S { if ($value === null) { throw new Exception(); } $value = $value->next(); return $value; }",
        "<?php function source(): ?string { return 'ok'; } function run(): string { $value = source(); if ($value === null) { return ''; } $value = strtoupper($value); return $value; }",
        "<?php function run(int|false $value): int { if ($value === false) { return 0; } $value = 4; return $value; }",
        "<?php function run(?array $value): array { if ($value === null) { return []; } $value = array_values($value); return $value; }",
    ] {
        check_source(source).unwrap_or_else(|error| panic!("{source}: {error}"));
    }
    expect_error("<?php function run(?int $a, ?int $b, bool $closed): int { if ($closed || $a === null) { return 0; } $a = $b; return $a; }", "return type expects int");
}

/// Object alternatives retain a usable Throwable supertype on ambiguous boolean outcomes.
#[test]
fn test_nullable_fallback_flow_review_throwable_joins() {
    let _strict = elephc::strict_php::scoped_enable();
    for body in [
        "if ($error instanceof A || $error instanceof B) { throw $error; }",
        "if ($error instanceof A && $closed) { return; } throw $error;",
        "if ($error instanceof A && $closed) { return; } else { throw $error; }",
    ] {
        let source = format!("<?php class A extends Exception {{}} class B extends Exception {{}} function run(Throwable $error, bool $closed): void {{ {body} }}");
        check_source(&source).unwrap_or_else(|error| panic!("{body}: {error}"));
    }
}

/// Both direct and nested reference captures invalidate facts in their enclosing frame.
#[test]
fn test_nullable_fallback_flow_review_reference_captures() {
    let _strict = elephc::strict_php::scoped_enable();
    for body in [
        "$callback = function () use (&$value) { $value = null; }; $callback();",
        "$callbacks = [function () use (&$value) { $value = null; }]; $callbacks[0]();",
    ] {
        let source = format!("<?php final class S {{}} function run(?S $value): S {{ $value ??= new S(); {body} return $value; }}");
        expect_error(&source, "return type expects S");
    }
}

/// Widening effects cannot leave a nullable reference contract attached to a boxed value.
#[test]
fn test_nullable_fallback_flow_review_storage_effects() {
    let _strict = elephc::strict_php::scoped_enable();
    for effect in ["widen($value);", "settype($value, 'string');", "++$value;", "$value++;", "--$value;", "$value--;"] {
        for body in [
            "$value ??= 1; EFFECT resetValue($value);",
            "if ($value === null) { return; } EFFECT resetValue($value);",
            "if ($value !== null) { EFFECT resetValue($value); }",
        ] {
            let source = format!("<?php function widen(mixed &$value): void {{ $value = null; }} function resetValue(?int &$value): void {{ $value = null; }} function run(?int $value): void {{ {} }}", body.replace("EFFECT", effect));
            let error = check_source(&source).expect_err(&source);
            assert!(error.contains("by-reference storage"), "{source}: {error}");
        }
    }
}

/// Declared-int return verification keeps TypeError handlers in the continuing type join.
#[test]
fn test_nullable_fallback_flow_review_return_boundary_catches() {
    for prefix in ["$value ??= 1;", "$value = $value ?? 1;", "if ($value === null) { return 0; }"] {
        for call in ["returnChecked($input)", "ReturnBoundaryBox::read($input)"] {
            let source = format!("<?php function returnChecked(mixed $input): int {{ return $input; }} class ReturnBoundaryBox {{ public static function read(mixed $input): int {{ return $input; }} }} function run(?int $value, mixed $input): int {{ {prefix} $ignored = 0; try {{ $ignored = {call}; }} catch (TypeError $error) {{ $value = null; }} echo $ignored; return $value; }}");
            expect_error(&source, "return type expects int");
        }
    }
}

/// A nullable array write cannot promise an unboxed array argument without a source guard.
#[test]
fn test_nullable_fallback_flow_review_nullable_container_reads() {
    for write in ["$value[] = 5;", "$value[0] = 5;", "$ignored = ($value[0] = 5);"] {
        for (consumer, message) in [
            ("max($value)", "must be of type array"),
            ("array_map(fn($item) => $item * 2, $value)", "second argument must be array"),
        ] {
            let source = format!("<?php function run(?array $value): void {{ {write} {consumer}; }}");
            expect_error(&source, message);
        }
    }
    for body in [
        "$value[] = 5; $value = null;",
        "if ($flag) { $value[] = 5; } else { $value = null; }",
        "$value[] = 5; resetArray($value);",
    ] {
        let source = format!("<?php function resetArray(?array &$value): void {{ $value = null; }} function run(?array $value, bool $flag): int {{ try {{ if ($value === null) {{ return 0; }} }} finally {{ {body} }} return max($value); }}");
        expect_error(&source, "must be of type array");
    }
}

/// Container writes in finally update the element view rather than restoring stale entry facts.
#[test]
fn test_nullable_fallback_flow_review_finally_container_writes() {
    let mut failures = Vec::new();
    for write in ["$value[] = 'abc';", "$value[2] = 'abc';", "$ignored = ($value[2] = 'abc');"] {
        for guarded_inside in [true, false] {
            let before = if guarded_inside { "" } else { "if ($value === null) { return 0; }" };
            let body = if guarded_inside { "if ($value === null) { return 0; }" } else { "echo '';" };
            let source = format!("<?php function run(bool $flag): int {{ $value = $flag ? [1, 2] : null; {before} try {{ {body} }} finally {{ {write} }} return strlen($value[2]); }}");
            if let Err(error) = check_source(&source) { failures.push(format!("{source}: {error}")); }
        }
    }
    for source in [
        "<?php function run(?array $value): array { try { if ($value === null) { return []; } } finally { $value[] = 1; } return $value; }",
        "<?php function run(bool $flag): int { $value = $flag ? ['first' => 1] : null; try { if ($value === null) { return 0; } } finally { $value['next'] = 'abc'; } return strlen($value['next']); }",
        "<?php function resetArray(?array &$value): void { $value = null; } function run(?array $value): void { if ($value === null) { return; } try { echo ''; } finally { $value[] = 1; } resetArray($value); }",
    ] {
        if let Err(error) = check_source(source) { failures.push(format!("{source}: {error}")); }
    }
    assert!(failures.is_empty(), "container flow failures: {failures:#?}");
}

/// Read-only finally blocks preserve the continuing path's guard facts after terminal exits.
#[test]
fn test_nullable_fallback_flow_review_finally_continuation() {
    for source in [
        "<?php function run(?int $value): int { try { echo 'try'; } finally { if ($value === null) { return 0; } } return $value; }",
        "<?php function run(?int $value): int { try { if ($value === null) { return 0; } } finally { $value = 7; } return $value; }",
        "<?php function run(?int &$value): int { try { if ($value === null) { return 0; } } finally { echo 'done'; } return $value; }",
        "<?php function run(?int $value, ?int $other): int { try { if ($value === null) { return 0; } if ($other === null) { return 0; } } finally { $value = $other; } return $value; }",
        "<?php function run(?int $value, ?int $other): int { try { if ($value === null) { return 0; } if ($other === null) { return 0; } } finally { $other = 7; $value = $other; } return $value; }",
    ] { check_source(source).unwrap_or_else(|error| panic!("{source}: {error}")); }
    for guard in ["$value === null", "$closed || $value === null"] {
        for exit in ["return 0;", "throw new RuntimeException('exit');"] {
            let source = format!("<?php function run(?int $value, bool $closed): int {{ try {{ if ({guard}) {{ {exit} }} }} finally {{ echo 'done'; }} return $value; }}");
            check_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
        }
        for exit in ["continue;", "break;"] {
            let source = format!("<?php function needFinallyContinuation(int $value): void {{}} function run(?int $value, bool $closed): void {{ foreach ([1, 2] as $index) {{ try {{ if ({guard}) {{ {exit} }} }} finally {{ echo 'done'; }} needFinallyContinuation($value); }} }}");
            check_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
        }
    }
    check_source("<?php final class FinallyServiceFlow {} function run(?FinallyServiceFlow $value, bool $closed): FinallyServiceFlow { try { if ($closed || $value === null) { throw new LogicException('exit'); } } finally { echo 'done'; } return $value; }").unwrap();
    for prefix in ["$value ??= 1;", "$value = $value ?? 1;"] {
        let source = format!("<?php function run(?int $value, bool $flag): int {{ {prefix} try {{ if ($flag) {{ $value = null; return 0; }} }} finally {{ echo 'done'; }} return $value; }}");
        check_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
    }
    for body in ["$value = null;", "resetFinallyContinuation($value);", "$write = function () use (&$value) { $value = null; }; $write();"] {
        let source = format!("<?php function resetFinallyContinuation(?int &$value): void {{ $value = null; }} function run(?int $value): int {{ try {{ if ($value === null) {{ return 0; }} }} finally {{ {body} }} return $value; }}");
        expect_error(&source, "return type expects int");
    }
    expect_error("<?php function needFinallyContinuation(int $value): void {} function run(?int $value): void { try { if ($value === null) { return; } } finally { needFinallyContinuation($value); } }", "expects int");
    expect_error("<?php function run(?int $value, ?int $other): int { try { if ($value === null) { return 0; } if ($other === null) { return 0; } } finally { $other = null; $value = $other; } return $value; }", "return type expects int");
    expect_error("<?php function run(?int $value, ?int $other): int { try { if ($value === null) { return 0; } } finally { $value = $other; $other ??= 1; } return $value; }", "return type expects int");
}

/// Conditional writes invalidate a non-null read on every expression control-flow surface.
#[test]
fn test_nullable_fallback_flow_review_conditional_effects() {
    let _strict = elephc::strict_php::scoped_enable();
    for prefix in ["$value ??= 1;", "if ($n > 0 || $value === null) { return 0; }"] {
        for body in [
            "$flag && resetValue($value);", "$flag || resetValue($value);",
            "$flag and resetValue($value);", "$flag or resetValue($value);",
            "$r = $flag ? resetValue($value) : false;",
            "$r = $flag ?: resetValue($value);",
            "$r = $other ?? resetValue($value);",
            "$r = match ($n) { 1 => resetValue($value), default => false };",
        ] {
            let source = format!("<?php function resetValue(?int &$value): bool {{ $value = null; return true; }} function run(?int $value, bool $flag, int $n, ?bool $other): int {{ {prefix} {body} return $value; }}");
            let error = check_source(&source).expect_err(&source);
            assert!(error.contains("return type expects int"), "{source}: {error}");
        }
    }
    expect_error("<?php final class S {} function resetValue(?S &$value): bool { $value = null; return true; } function run(?S $value, bool $flag): S { $value ??= new S(); $r = $flag ? resetValue($value) : false; return $value; }", "return type expects S");
}

/// A conditional coalescing default carries its writes to other previously narrowed locals.
#[test]
fn test_nullable_fallback_flow_review_coalesce_default_effects() {
    for prefix in [
        "$value ??= 1;", "$value = $value ?? 1;",
        "if ($closed || $value === null) { return 0; }",
        "if ($value === null) { return 0; }",
    ] {
        for body in [
            "$other ??= resetDefaultFlow($value);",
            "$other = $other ?? resetDefaultFlow($value);",
            "consumeDefaultFlow($other ??= resetDefaultFlow($value));",
            "$other ??= (($value = null) === null);",
        ] {
            let source = format!("<?php function resetDefaultFlow(?int &$value): bool {{ $value = null; return true; }} function consumeDefaultFlow(bool $flag): void {{}} function run(?int $value, ?bool $other, bool $closed): int {{ {prefix} {body} return $value; }}");
            expect_error(&source, "return type expects int");
        }
    }
    check_source("<?php function resetSkippedFlow(?int &$value): bool { $value = null; return true; } function run(?int $value, bool $other): int { $value ??= 1; $other ??= resetSkippedFlow($value); return $value; }").unwrap();
}

/// Alternatives retain entry storage for reference checks and preserve their result precision.
#[test]
fn test_nullable_fallback_flow_review_conditional_reference_contracts() {
    let _strict = elephc::strict_php::scoped_enable();
    for body in [
        "$r = $flag ? widen($value) : change($value);",
        "$r = $flag ? $value++ : change($value);",
        "$r = match ($n) { 1 => widen($value), default => change($value) };",
        "$r = $flag ? widen($value) : false; change($value);",
        "$r = ($flag && widen($value)) || change($value);",
        "$flag && widen($value); resetValue($value);",
    ] {
        let source = format!("<?php function widen(mixed &$value): bool {{ $value = 'str'; return true; }} function change(int &$value): bool {{ ++$value; return true; }} function resetValue(?int &$value): void {{ $value = null; }} function run(?int $value, bool $flag, int $n): void {{ $value ??= 1; {body} }}");
        let error = check_source(&source).expect_err(&source);
        assert!(error.contains("by-reference storage") || error.contains("got int|null"), "{source}: {error}");
    }
    for source in [
        "<?php function widen(mixed &$value): bool { $value = 'str'; return true; } function run(?int $value, int $n): void { $value ??= 1; $r = match ($n) { 1 => widen($value), default => false }; }",
        "<?php function run(?int $value, bool $flag): int { $value ??= 1; return $flag ? ($value = 2) : $value; }",
        "<?php function run(?int $value, int $n): int { $value ??= 1; return match ($n) { 1 => ($value = 2), default => $value }; }",
    ] { check_source(source).unwrap_or_else(|error| panic!("{source}: {error}")); }
}

/// Destructuring and catch bindings retire the previous guard before validating later stores.
#[test]
fn test_nullable_fallback_flow_review_rebinding_destinations() {
    let _strict = elephc::strict_php::scoped_enable();
    for prefix in [
        "$value ??= 1;", "if ($value === null) { return; }",
        "if ($value !== null) { echo ''; }",
    ] {
        for binding in [
            "[$value] = ['str'];", "list($value) = ['str'];",
            "foreach ([['str']] as [$value]) {}",
        ] {
            let stores = format!("<?php function run(?int $value): void {{ {prefix} {binding} $value = 7; }}");
            let result = check_source_full(&stores).unwrap();
            assert!(result.warnings.iter().any(|warning| warning.message.contains("changes type from string to int")), "{stores}");
            let error = check_source_strict(&stores).expect_err(&stores);
            assert!(error.contains("cannot reassign $value from string to int"), "{stores}: {error}");
            let unsafe_ref = format!("<?php function resetValue(?int &$value): void {{ $value = null; }} function run(?int $value): void {{ {prefix} {binding} $value = 7; resetValue($value); }}");
            expect_error(&unsafe_ref, "requires a variable with mixed/union/nullable storage");
        }
        for binding in ["[$value] = ['str'];", "['key' => $value] = ['key' => 'str'];"] {
            let source = format!("<?php function appendValue(string &$value): void {{ $value .= '!'; }} function run(?int $value): void {{ {prefix} {binding} appendValue($value); }}");
            check_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
        }
    }
    check_source("<?php function run(?int $value): void { $value ??= 1; try { throw new Exception(); } catch (Exception $value) { $value = new Exception(); echo get_class($value); } }").unwrap();
}

/// Stores under a redundant guard must invalidate a coalesced non-null read view.
#[test]
fn test_nullable_fallback_flow_review_guarded_null_stores() {
    let _strict = elephc::strict_php::scoped_enable();
    for prefix in ["$value ??= 1;", "if ($value === null) { return 0; }"] {
        for body in [
            "if (is_int($value)) { $value = null; }",
            "if ($value !== null) { $value = null; }",
            "if (is_int($value)) { $value = null; } else { $value = 2; }",
            "while (is_int($value)) { $value = null; }",
            "while (is_int($value)) { $value = null; break; }",
            "$r = is_int($value) ? ($value = null) : 0;",
            "is_int($value) && (($value = null) === null);",
        ] {
            let source = format!("<?php function run(?int $value): int {{ {prefix} {body} return $value; }}");
            expect_error(&source, "return type expects int");
        }
    }
    expect_error("<?php class S {} final class ChildS extends S {} function run(?S $value): S { $value ??= new ChildS(); if ($value instanceof ChildS) { $value = null; } return $value; }", "return type expects S");
    for body in [
        "if (is_int($value)) { $value = 2; }",
        "if (is_int($value)) { $value = 2; } else { $value = 3; }",
        "while (is_int($value)) { $value = 2; break; }",
    ] {
        let source = format!("<?php function run(?int $value): int {{ $value ??= 1; {body} return $value; }}");
        check_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
    }
    let incompatible = "<?php function run(?int $value): void { $value ??= 1; if (is_int($value)) { $value = 1.5; } }";
    let error = check_source_strict(incompatible).expect_err(incompatible);
    assert!(error.contains("cannot reassign"), "{error}");
    for body in ["if (is_int($value)) { widen($value); }", "while (is_int($value)) { widen($value); break; }"] {
        let source = format!("<?php function widen(mixed &$value): void {{ $value = 'str'; }} function resetValue(?int &$value): void {{ $value = null; }} function run(?int $value): void {{ $value ??= 1; {body} resetValue($value); }}");
        expect_error(&source, "by-reference storage");
    }
}

/// A nested guard keeps the nullable slot ABI for raw and matching nullable references.
#[test]
fn test_nullable_fallback_flow_review_nested_reference_storage() {
    let _strict = elephc::strict_php::scoped_enable();
    for (ty, default, predicate) in [
        ("int", "1", "is_int"), ("float", "1.5", "is_float"),
        ("bool", "true", "is_bool"), ("string", "'str'", "is_string"),
    ] {
        for prefix in [format!("$value ??= {default};"), "if ($value === null) { return; }".to_string()] {
            for body in [
                format!("if ({predicate}($value)) {{ change($value); }}"),
                format!("while ({predicate}($value)) {{ change($value); break; }}"),
                format!("$r = {predicate}($value) ? change($value) : false;"),
                format!("{predicate}($value) && change($value);"),
            ] {
                let raw = format!("<?php function change({ty} &$value): bool {{ $value = {default}; return true; }} function run(?{ty} $value): void {{ {prefix} {body} }}");
                expect_error(&raw, "by-reference");
                let matching = raw.replace(&format!("change({ty} &"), &format!("change(?{ty} &"));
                check_source(&matching).unwrap_or_else(|error| panic!("{matching}: {error}"));
            }
        }
    }
}

/// A fresh guard cannot restore tagged reference storage after an increment boxed the local.
#[test]
fn test_nullable_fallback_flow_review_fresh_guard_widening() {
    let _strict = elephc::strict_php::scoped_enable();
    for update in ["$value++;", "++$value;", "$value--;", "--$value;"] {
        for body in [
            "if ($value !== null) { UPDATE }",
            "while ($value !== null) { UPDATE break; }",
            "$r = $value !== null ? (UPDATE) : 0;",
            "if ($value !== null && (UPDATE) > 100) { echo ''; }",
        ] {
            let expression = update.trim_end_matches(';');
            let body = if body.contains("(UPDATE)") { body.replace("UPDATE", expression) }
                else { body.replace("UPDATE", update) };
            let source = format!("<?php function resetValue(?int &$value): void {{ $value = null; }} function run(?int $value): void {{ {body} resetValue($value); }}");
            expect_error(&source, "by-reference storage");
        }
    }
    // Pure guards retain their physical contract, including a kept terminal complement.
    for body in ["if ($value === null) { return; }", "if ($value !== null) { echo ''; }"] {
        let source = format!("<?php function resetValue(?int &$value): void {{ $value = null; }} function run(?int $value): void {{ {body} resetValue($value); }}");
        check_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
    }
}

/// Float-bearing scalar unions keep the same explicit-conversion guidance as a float.
#[test]
fn test_nullable_fallback_flow_review_union_coercion_diagnostic() {
    let _strict = elephc::strict_php::scoped_enable();
    let source = "<?php function toInt(int $value): int { return $value; } function run(mixed $value): int { if (is_float($value) || is_bool($value)) { return toInt($value); } return 0; }";
    let error = check_source(source).expect_err(source);
    assert!(error.contains("got float|bool") || error.contains("got bool|float"), "{error}");
    assert!(error.contains("add an explicit cast at the call site"), "{error}");
    check_source(&source.replace("toInt($value)", "toInt((int)$value)")).unwrap();
}

/// Loop headers reject tagged references before a later checked update boxes the same slot.
#[test]
fn test_nullable_fallback_flow_review_loop_reference_storage() {
    let _strict = elephc::strict_php::scoped_enable();
    for prefix in ["", "$value ??= 1;"] {
        for (header, tail) in [
            ("foreach ([1, 2, 3] as $i)", ""),
            ("while ($flag)", "$flag = false;"),
            ("for ($i = 0; $i < 3; $i++)", ""),
            ("do", ""),
        ] {
            for update in ["$value++;", "++$value;", "$value--;", "--$value;"] {
                let body = format!("showValue($value); $value = 1; if ($value !== null) {{ {update} }} {tail}");
                let loop_body = if header == "do" { format!("do {{ {body} }} while (false);") }
                    else { format!("{header} {{ {body} }}") };
                let source = format!("<?php function showValue(?int &$value): void {{ var_dump($value); }} function run(?int $value, bool $flag): void {{ {prefix} {loop_body} }}");
                expect_error(&source, "by-reference storage");
            }
        }
    }
    // Integer-only stores keep the compact nullable reference contract.
    check_source("<?php function showValue(?int &$value): void { var_dump($value); } function run(?int $value): void { foreach ([1, 2] as $i) { showValue($value); $value = $i; } }").unwrap();
    expect_error("<?php function run(?int $value): void { foreach ([1] as $i) { $value++; } }", "Cannot increment");
}

/// Fixed tagged updates remain compatible with later nullable reference calls and captures.
#[test]
fn test_nullable_fallback_flow_review_fixed_reference_continuation() {
    let _strict = elephc::strict_php::scoped_enable();
    for update in ["$value++;", "++$value;", "$value--;", "--$value;"] {
        let source = format!("<?php function resetValue(?int &$value): void {{ $value = null; }} function incoming(?int &$value): void {{ if ($value !== null) {{ {update} }} resetValue($value); }} function captured(?int $value): void {{ $show = function () use (&$value) {{ var_dump($value); }}; if ($value !== null) {{ {update} }} resetValue($value); $show(); }} function capturedLoop(?int $value): void {{ $show = function () use (&$value) {{ var_dump($value); }}; foreach ([1, 2] as $i) {{ resetValue($value); $value = $i; if ($value !== null) {{ {update} }} $show(); }} }}");
        check_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
    }
    let late_capture = "<?php function resetValue(?int &$value): void { $value = null; } function run(?int $value): void { if ($value !== null) { $value++; } $show = function () use (&$value) { var_dump($value); }; resetValue($value); }";
    expect_error(late_capture, "by-reference storage");
    let detached_capture = "<?php function nullableValue(?int $value): ?int { return $value; } function resetValue(?int &$value): void { $value = null; } function run(?int $value): void { $show = function () use (&$value) { var_dump($value); }; unset($value); $value = nullableValue(5); if ($value !== null) { $value++; } resetValue($value); }";
    expect_error(detached_capture, "by-reference storage");
    check_source("<?php function resetValue(?int &$value): void { $value = null; } function run(?int $value, bool $flag): void { if ($flag) { $show = function () use (&$value) { var_dump($value); }; $show(); } if ($value !== null) { $value++; } resetValue($value); }").unwrap();
}
