---
title: "opcache_jit_blacklist()"
description: "Excludes a function from JIT compilation."
---

## opcache_jit_blacklist()

```php
function opcache_jit_blacklist(mixed $closure): void
```

Excludes a function from JIT compilation.

**Parameters**:
- `$closure` (`mixed`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_jit_blacklist` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_jit_blacklist.md).
