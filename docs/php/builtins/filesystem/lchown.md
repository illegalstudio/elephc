---
title: "lchown()"
description: "Changes user ownership of a symlink."
---

## lchown()

```php
function lchown(string $filename, string $user): bool
```

Changes user ownership of a symlink.

**Parameters**:
- `$filename` (`string`)
- `$user` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `lchown` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/lchown.md).
