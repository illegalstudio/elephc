---
title: "opcache_reset()"
description: "Implemented by the compiler-injected OPcache prelude."
sidebar:
  order: 619
---

## opcache_reset()

```php
function opcache_reset(): bool
```

Implemented by the compiler-injected OPcache prelude.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_reset` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_reset.md).
