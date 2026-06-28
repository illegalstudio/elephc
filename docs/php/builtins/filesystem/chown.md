---
title: "chown()"
description: "Lowers `chown(path, owner)` for integer UIDs and string user names."
sidebar:
  order: 85
---

## chown()

```php
function chown(string $filename, int $user): bool
```

Lowers `chown(path, owner)` for integer UIDs and string user names.

**Parameters**:
- `$filename` (`string`)
- `$user` (`int`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `chown` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/chown.md).

