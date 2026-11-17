---
title: "fstat()"
description: "Gets information about a file using an open file pointer."
---

## fstat()

```php
function fstat(resource $stream): mixed
```

Gets information about a file using an open file pointer.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fstat` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fstat.md).
