---
title: "pcntl_unshare()"
description: "Disassociates selected Linux process execution contexts."
---

## pcntl_unshare()

```php
function pcntl_unshare(int $flags): bool
```

Disassociates selected Linux process execution contexts.

**Parameters**:
- `$flags` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_unshare` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_unshare.md).
