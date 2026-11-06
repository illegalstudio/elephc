---
title: "opcache_get_configuration()"
description: "Returns the OPcache directives, blacklist, and version, or false when API access is restricted."
---

## opcache_get_configuration()

```php
function opcache_get_configuration(): array|false
```

Returns the OPcache directives, blacklist, and version, or false when API access is restricted.

**Parameters**: none.

**Returns**: `array|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_get_configuration` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_get_configuration.md).
