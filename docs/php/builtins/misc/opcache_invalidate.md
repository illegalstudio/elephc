---
title: "opcache_invalidate()"
description: "Implemented by the compiler-injected OPcache prelude."
sidebar:
  order: 615
---

## opcache_invalidate()

```php
function opcache_invalidate(mixed $filename, mixed $force = false): bool
```

Implemented by the compiler-injected OPcache prelude.

**Parameters**:
- `$filename` (`mixed`)
- `$force` (`mixed`), default `false`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_invalidate` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_invalidate.md).
