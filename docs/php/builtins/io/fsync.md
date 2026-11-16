---
title: "fsync()"
description: "Synchronizes changes to the file (including meta-data)."
---

## fsync()

```php
function fsync(resource $stream): bool
```

Synchronizes changes to the file (including meta-data).

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fsync` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fsync.md).
