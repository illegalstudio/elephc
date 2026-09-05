---
title: "opcache_get_configuration()"
description: "Implemented by the compiler-injected OPcache prelude."
sidebar:
  order: 613
---

## opcache_get_configuration()

```php
function opcache_get_configuration(): array
```

Implemented by the compiler-injected OPcache prelude.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_get_configuration` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_get_configuration.md).
