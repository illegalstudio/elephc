---
title: "filectime()"
description: "Gets inode change time of file."
---

## filectime()

```php
function filectime(string $filename): mixed
```

Gets inode change time of file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `filectime` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/filectime.md).
