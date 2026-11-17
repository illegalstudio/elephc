---
title: "implode()"
description: "Joins array elements into a single string using a separator."
---

## implode()

```php
function implode(string $separator, array $array = null): string
```

Joins array elements into a single string using a separator.

**Parameters**:
- `$separator` (`string`)
- `$array` (`array`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `implode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/implode.md).
