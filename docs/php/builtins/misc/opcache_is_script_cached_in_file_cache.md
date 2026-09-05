---
title: "opcache_is_script_cached_in_file_cache()"
description: "Implemented by the compiler-injected OPcache prelude."
sidebar:
  order: 617
---

## opcache_is_script_cached_in_file_cache()

```php
function opcache_is_script_cached_in_file_cache(mixed $filename): bool
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

For how `opcache_is_script_cached_in_file_cache` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_is_script_cached_in_file_cache.md).
