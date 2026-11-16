---
title: "sha1()"
description: "Calculates the SHA-1 hash of a string."
---

## sha1()

```php
function sha1(string $string, bool $binary = false): string
```

Calculates the SHA-1 hash of a string.

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

For how `sha1` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/sha1.md).
