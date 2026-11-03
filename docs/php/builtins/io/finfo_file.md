---
title: "finfo_file()"
description: "Identifies the type and encoding of a file or stream URL."
---

## finfo_file()

```php
function finfo_file(finfo $finfo, string $filename, int $flags = 0, ?resource $context = null): string|false
```

Identifies the type and encoding of a file or stream URL.

**Parameters**:
- `$finfo` (`finfo`)
- `$filename` (`string`)
- `$flags` (`int`), default `0`, optional
- `$context` (`?resource`), default `null`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `finfo_file` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/finfo_file.md).
