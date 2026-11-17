---
title: "fclose()"
description: "Closes an open file pointer."
---

## fclose()

```php
function fclose(resource $stream): bool
```

Closes an open file pointer.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fclose` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fclose.md).
