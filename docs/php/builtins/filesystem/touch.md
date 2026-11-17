---
title: "touch()"
description: "Sets access and modification time of a file."
---

## touch()

```php
function touch(string $filename, ?int $mtime = null, ?int $atime = null): bool
```

Sets access and modification time of a file.

**Parameters**:
- `$filename` (`string`)
- `$mtime` (`?int`), default `null`, optional
- `$atime` (`?int`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `touch` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/touch.md).
