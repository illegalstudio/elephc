---
title: "rename()"
description: "Renames a file or directory."
---

## rename()

```php
function rename(string $from, string $to): bool
```

Renames a file or directory.

**Parameters**:
- `$from` (`string`)
- `$to` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `rename` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/rename.md).
