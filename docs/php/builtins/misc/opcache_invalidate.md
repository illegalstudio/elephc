---
title: "opcache_invalidate()"
description: "Invalidates a script's cached opcodes, optionally without checking its timestamp."
sidebar:
  order: 644
---

## opcache_invalidate()

```php
function opcache_invalidate(mixed $filename, mixed $force = false): bool
```

Invalidates a script's cached opcodes, optionally without checking its timestamp.

**Parameters**:
- `$filename` (`mixed`)
- `$force` (`mixed`), default `false`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through native OPcache prelude declarations or dedicated interpreter handlers; see [OPcache](../../opcache.md).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_invalidate` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_invalidate.md).
