---
title: "zip_entry_read()"
description: "Reads up to len bytes from an open zip entry."
---

## zip_entry_read()

```php
function zip_entry_read(mixed $zip_entry, int $len = 1024): string|false
```

Reads up to len bytes from an open zip entry.

**Parameters**:
- `$zip_entry` (`mixed`)
- `$len` (`int`), default `1024`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `zip_entry_read` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/zip_entry_read.md).
