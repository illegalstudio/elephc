---
title: "opcache_jit_blacklist()"
description: "Implemented by the compiler-injected OPcache prelude."
sidebar:
  order: 618
---

## opcache_jit_blacklist()

```php
function opcache_jit_blacklist(mixed $closure): void
```

Implemented by the compiler-injected OPcache prelude.

**Parameters**:
- `$closure` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_jit_blacklist` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_jit_blacklist.md).
