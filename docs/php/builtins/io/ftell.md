---
title: "ftell()"
description: "Returns the current position of the file read/write pointer."
---

## ftell()

```php
function ftell(resource $stream): int
```

Returns the current position of the file read/write pointer.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ftell` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ftell.md).
