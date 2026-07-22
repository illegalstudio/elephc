---
title: "array_filter()"
description: "Filters elements of an array using a callback function."
sidebar:
  order: 11
---

## array_filter()

```php
function array_filter(array $array, callable $callback = null, int $mode = 0): array
```

Filters elements of an array using a callback function.

**Parameters**:
- `$array` (`array`)
- `$callback` (`callable`), default `null`, optional
- `$mode` (`int`), default `0`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_filter.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_filter.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_filter` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_filter.md).
