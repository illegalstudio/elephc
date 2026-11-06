---
title: "rewind()"
description: "Rewind the position of a file pointer."
---

## rewind()

```php
function rewind(resource $stream): bool
```

Rewind the position of a file pointer.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `rewind` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/rewind.md).
