---
title: "array_chunk()"
description: "Splits an array into chunks of the given size."
---

## array_chunk()

```php
function array_chunk(array $array, int $length, bool $preserve_keys = false): array
```

Splits an array into chunks of the given size.

**Parameters**:
- `$array` (`array`)
- `$length` (`int`)
- `$preserve_keys` (`bool`), default `false`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_chunk` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_chunk.md).
