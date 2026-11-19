---
title: "zip_open()"
description: "Opens a zip archive and returns a directory resource, or a libzip error code."
---

## zip_open()

```php
function zip_open(string $filename): mixed
```

Opens a zip archive and returns a directory resource, or a libzip error code.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `zip_open` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/zip_open.md).
