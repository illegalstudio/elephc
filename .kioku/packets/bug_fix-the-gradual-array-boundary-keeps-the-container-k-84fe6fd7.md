---
id: bug_fix-the-gradual-array-boundary-keeps-the-container-k-84fe6fd7
type: bug_fix
title: "The gradual array boundary keeps the container kind instead of forcing a hash"
description: "A Mixed value narrowed to a bare array return became associative storage wearing an indexed type, and in_array, array_filter, array_pop and min segfaulted on it"
created: 2026-09-17
sources:
  - path: src/ir_lower/gradual_coercions.rs
    blob: f2eef64633d47389db963416c4484c137234b068
  - path: src/codegen/lower_inst/arrays.rs
    blob: c7d0acee4d83f6b1586c6b31a3e6f0bc543770ca
  - path: src/ir_lower/stmt/return_coercions.rs
    blob: 65f171ed959898adcf8ab53301c208b066ddfb11
  - path: src/codegen/lower_inst/builtins/strings/split.rs
    blob: b26ff16f4d39282d9922dd769dfbc6529ea0c9a5
---

# The gradual array boundary keeps the container kind instead of forcing a hash

## Fact

The gradual array boundary keeps the CONTAINER KIND: `Array(Mixed)` is a list target, not a hash
target, and only a genuinely associative source still becomes a hash.

`coerce_gradual_value_to_boundary` answered an `Array(Mixed)` target with `MixedToHash` typed
`AssocArray<Mixed, Mixed>`, so an INDEXED source came out associative. `coerce_container_to_return_type`
then restamped that hash as the declared bare `array` contract WITHOUT converting — deliberate,
because converting would discard string keys — and the static type therefore lied about the storage.

Reproducer (two implementers are required; with one the guarded call is concrete and correct):

```php
interface ParserInterface { public function getName(): string; }
abstract class AbstractParser implements ParserInterface {
    public function getOperatorTokens(): array { return ['op-' . $this->getName(), 'zz']; }
}
class Rich extends AbstractParser { public function getName(): string { return 'rich'; } }
class Plain implements ParserInterface { public function getName(): string { return 'plain'; } }
function tokensFor(ParserInterface $p): array {
    if (method_exists($p, 'getOperatorTokens')) { return $p->getOperatorTokens(); }
    return [$p->getName()];
}
```

Measured on one such value against `php -n` 8.5: `count()`, `$a[0]`, `foreach`, `json_encode()`,
`serialize()`, `array_keys()`, `array_values()`, `array_merge()`, `array_slice()`,
`array_reverse()`, `array_unique()`, `sort()` and `var_export()` were RIGHT — they ask
`__rt_heap_kind` — while `in_array()`, `array_filter()`, `array_pop()` and `min()` SEGFAULTED,
`array_map()` died, `array_shift()` answered `NULL`, `current()`/`end()` answered nothing and
`print_r()` printed the keys in place of the values.

`Op::MixedToHash` keeps its name and its operand contract (a `Heap(Mixed)` cell, runtime-tag
checked, PHP `TypeError` for a non-array); what its RESULT TYPE asks for now decides which owned
container `lower_mixed_to_hash` builds. Tag 4 takes the widening `ArrayToMixed` uses — incref so
`__rt_array_ensure_unique` sees a shared array and splits, then `__rt_array_to_mixed` — and tag 5
still goes to `__rt_mixed_to_owned_hash`. Every other emitter of the op asks for `AssocArray` and is
unchanged.

`implode()` separately learned to ask `__rt_heap_kind` for an `array<mixed>` operand, because the
restamp still happens for a genuinely associative source; both branches leave an owned array so one
`__rt_decref_array` balances either.

Tests: `codegen::regressions::arrays::test_gradual_array_return_keeps_its_container_kind` and
`…::test_implode_reads_a_hash_stored_under_a_list_contract`. The `codegen::arrays` module's failure
set was identical before and after (29 names, unchanged).
