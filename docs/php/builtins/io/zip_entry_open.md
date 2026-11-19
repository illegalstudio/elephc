---
title: "zip_entry_open()"
description: "Opens a zip entry for reading."
---

## zip_entry_open()

```php
function zip_entry_open(mixed $zip_dp, mixed $zip_entry, string $mode = 'rb'): bool
```

Opens a zip entry for reading.

**Parameters**:
- `$zip_dp` (`mixed`)
- `$zip_entry` (`mixed`)
- `$mode` (`string`), default `'rb'`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `zip_entry_open` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/zip_entry_open.md).
