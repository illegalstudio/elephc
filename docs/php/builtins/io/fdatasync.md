---
title: "fdatasync()"
description: "Synchronizes data (but not meta-data) to file."
---

## fdatasync()

```php
function fdatasync(resource $stream): bool
```

Synchronizes data (but not meta-data) to file.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fdatasync` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fdatasync.md).
