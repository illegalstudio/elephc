---
title: "lstat()"
description: "Gives information about a file or symbolic link."
---

## lstat()

```php
function lstat(string $filename): mixed
```

Gives information about a file or symbolic link.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `lstat` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/lstat.md).
