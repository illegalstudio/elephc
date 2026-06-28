---
title: "lchown()"
description: "Lowers `lchown(path, owner)` for integer UIDs and string user names without following symlinks."
sidebar:
  order: 113
---

## lchown()

```php
function lchown(string $filename, int $user): bool
```

Lowers `lchown(path, owner)` for integer UIDs and string user names without following symlinks.

**Parameters**:
- `$filename` (`string`)
- `$user` (`int`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `lchown` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/lchown.md).

