---
title: "chdir()"
description: "Changes the current directory."
---

## chdir()

```php
function chdir(string $directory): bool
```

Changes the current directory.

**Parameters**:
- `$directory` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `chdir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/chdir.md).
