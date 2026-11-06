---
title: "array_splice()"
description: "Removes a portion of the array and replaces it with something else."
---

## array_splice()

```php
function array_splice(array $array, int $offset, int $length = null, array $replacement = []): array
```

Removes a portion of the array and replaces it with something else.

**Parameters**:
- `$array` (`array`), passed by reference
- `$offset` (`int`)
- `$length` (`int`), default `null`, optional
- `$replacement` (`array`), default `[]`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_splice` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_splice.md).
