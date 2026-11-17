---
title: "readlink()"
description: "Returns the target of a symbolic link."
---

## readlink()

```php
function readlink(string $path): mixed
```

Returns the target of a symbolic link.

**Parameters**:
- `$path` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `readlink` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/readlink.md).
