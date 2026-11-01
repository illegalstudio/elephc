---
title: "array_change_key_case()"
description: "Changes the case of all string keys in an array."
sidebar:
  order: 3
---

## array_change_key_case()

```php
function array_change_key_case(array $array, int $case = 0): array
```

Changes the case of all string keys in an array.

**Parameters**:
- `$array` (`array`)
- `$case` (`int`), default `0`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected array_edge prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_change_key_case.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_change_key_case.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_change_key_case` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_change_key_case.md).
