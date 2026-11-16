---
title: "mkdir()"
description: "Makes a directory."
---

## mkdir()

```php
function mkdir(string $directory, int $permissions = 511, bool $recursive = false): bool
```

Makes a directory.

**Parameters**:
- `$directory` (`string`)
- `$permissions` (`int`), default `511`, optional
- `$recursive` (`bool`), default `false`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mkdir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/mkdir.md).
