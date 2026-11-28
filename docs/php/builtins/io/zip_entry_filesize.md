---
title: "zip_entry_filesize()"
description: "Returns the uncompressed size of a zip entry."
---

## zip_entry_filesize()

```php
function zip_entry_filesize(mixed $zip_entry): int|false
```

Returns the uncompressed size of a zip entry.

**Parameters**:
- `$zip_entry` (`mixed`)

**Returns**: `int|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `zip_entry_filesize` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/zip_entry_filesize.md).
