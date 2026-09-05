---
title: "opcache_is_script_cached()"
description: "Implemented by the compiler-injected OPcache prelude."
sidebar:
  order: 616
---

## opcache_is_script_cached()

```php
function opcache_is_script_cached(mixed $filename): bool
```

Implemented by the compiler-injected OPcache prelude.

**Parameters**:
- `$filename` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_is_script_cached` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_is_script_cached.md).
