---
title: "feof()"
description: "Tests for end-of-file on a file pointer."
---

## feof()

```php
function feof(resource $stream): bool
```

Tests for end-of-file on a file pointer.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `feof` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/feof.md).
