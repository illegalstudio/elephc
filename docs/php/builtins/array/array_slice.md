---
title: "array_slice()"
description: "Extracts a slice of an array."
sidebar:
  order: 38
---

## array_slice()

```php
function array_slice(array $array, int $offset, int $length = null, bool $preserve_keys = false): array
```

Extracts a slice of an array.

**Parameters**:
- `$array` (`array`)
- `$offset` (`int`)
- `$length` (`int`), default `null`, optional
- `$preserve_keys` (`bool`), default `false`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_slice.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_slice.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_slice` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_slice.md).
