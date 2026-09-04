---
title: "opcache_get_status()"
description: "Implemented by the compiler-injected OPcache prelude."
sidebar:
  order: 614
---

## opcache_get_status()

```php
function opcache_get_status(mixed $include_scripts = true): mixed
```

Implemented by the compiler-injected OPcache prelude.

**Parameters**:
- `$include_scripts` (`mixed`), default `true`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_get_status` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_get_status.md).
