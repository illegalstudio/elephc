---
title: "clearstatcache()"
description: "Clears file status cache."
sidebar:
  order: 103
---

## clearstatcache()

```php
function clearstatcache(bool $clear_realpath_cache = false, string $filename = ''): void
```

Clears file status cache.

**Parameters**:
- `$clear_realpath_cache` (`bool`), default `false`, optional
- `$filename` (`string`), default `''`, optional

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `clearstatcache` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/clearstatcache.md).

