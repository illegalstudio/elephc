---
title: "opcache_get_configuration()"
description: "Returns the OPcache directives, blacklist, and version."
sidebar:
  order: 639
---

## opcache_get_configuration()

```php
function opcache_get_configuration(): array
```

Returns the OPcache directives, blacklist, and version.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_get_configuration` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_get_configuration.md).
