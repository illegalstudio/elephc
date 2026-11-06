---
title: "md5()"
description: "Calculates the MD5 hash of a string."
---

## md5()

```php
function md5(string $string, bool $binary = false): string
```

Calculates the MD5 hash of a string.

**Parameters**:
- `$string` (`string`)
- `$binary` (`bool`), default `false`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `md5` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/md5.md).
