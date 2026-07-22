---
title: "array_map()"
description: "Applies a callback to the elements of an array."
sidebar:
  order: 22
---

## array_map()

```php
function array_map(callable $callback, array $array, ...$arrays): array
```

Applies a callback to the elements of an array.

**Parameters**:
- `$callback` (`callable`)
- `$array` (`array`)
- `...$arrays` — variadic: collects excess arguments into `$arrays`.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_map.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_map.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_map` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_map.md).
