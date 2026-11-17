---
title: "fpassthru()"
description: "Output all remaining data on a file pointer."
---

## fpassthru()

```php
function fpassthru(resource $stream): int
```

Output all remaining data on a file pointer.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fpassthru` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fpassthru.md).
