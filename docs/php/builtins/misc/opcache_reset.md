---
title: "opcache_reset()"
description: "Clears the whole opcode cache."
sidebar:
  order: 644
---

## opcache_reset()

```php
function opcache_reset(): bool
```

Clears the whole opcode cache.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through native OPcache prelude declarations or dedicated interpreter handlers; see [OPcache](../../opcache.md).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_reset` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_reset.md).
