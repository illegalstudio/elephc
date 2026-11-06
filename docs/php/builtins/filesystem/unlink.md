---
title: "unlink()"
description: "Deletes a file."
---

## unlink()

```php
function unlink(string $filename): bool
```

Deletes a file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `unlink` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/unlink.md).
