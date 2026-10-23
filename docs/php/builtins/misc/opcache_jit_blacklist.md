---
title: "opcache_jit_blacklist()"
description: "Excludes a function from JIT compilation."
sidebar:
  order: 646
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

- **Compiled (AOT)**: supported through the compiler-injected OPcache prelude.
- **`eval()` (magician interpreter)**: supported through native OPcache prelude declarations or dedicated interpreter handlers; see [OPcache](../../opcache.md).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_jit_blacklist` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_jit_blacklist.md).
