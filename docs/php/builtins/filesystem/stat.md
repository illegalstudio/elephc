---
title: "stat()"
description: "Gives information about a file."
---

## stat()

```php
function stat(string $filename): mixed
```

Gives information about a file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stat` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/stat.md).
