---
title: "array_key_exists()"
description: "Checks if the given key or index exists in the array."
---

## array_key_exists()

```php
function array_key_exists(string $key, array $array): bool
```

Checks if the given key or index exists in the array.

**Parameters**:
- `$key` (`string`)
- `$array` (`array`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_key_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_key_exists.md).
