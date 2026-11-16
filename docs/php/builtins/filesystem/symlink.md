---
title: "symlink()"
description: "Creates a symbolic link."
---

## symlink()

```php
function symlink(string $target, string $link): bool
```

Creates a symbolic link.

**Parameters**:
- `$target` (`string`)
- `$link` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `symlink` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/symlink.md).
