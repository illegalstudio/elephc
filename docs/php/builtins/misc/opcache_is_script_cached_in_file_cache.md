---
title: "opcache_is_script_cached_in_file_cache()"
description: "Reports whether a script is in the on-disk file cache."
sidebar:
  order: 643
---

## opcache_is_script_cached_in_file_cache()

```php
function opcache_is_script_cached_in_file_cache(mixed $filename): bool
```

Reports whether a script is in the on-disk file cache.

**Parameters**:
- `$filename` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_is_script_cached_in_file_cache` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_is_script_cached_in_file_cache.md).
