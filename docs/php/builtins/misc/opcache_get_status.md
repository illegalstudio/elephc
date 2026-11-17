---
title: "opcache_get_status()"
description: "Returns OPcache memory, statistics, and optionally the cached scripts."
---

## opcache_get_status()

```php
function opcache_get_status(mixed $include_scripts = true): mixed
```

Returns OPcache memory, statistics, and optionally the cached scripts.

**Parameters**:
- `$include_scripts` (`mixed`), default `true`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_get_status` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_get_status.md).
