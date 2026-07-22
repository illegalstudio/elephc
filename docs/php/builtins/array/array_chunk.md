---
title: "array_chunk()"
description: "Splits an array into chunks of the given size."
sidebar:
  order: 3
---

## array_chunk()

```php
function array_chunk(array $array, int $length): array
```

Splits an array into chunks of the given size.

**Parameters**:
- `$array` (`array`)
- `$length` (`int`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_chunk.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_chunk.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_chunk` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_chunk.md).
