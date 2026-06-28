---
title: "clearstatcache()"
description: "Lowers `clearstatcache(...)` as an ordered no-op after EIR operand evaluation."
sidebar:
  order: 86
---

## clearstatcache()

```php
function clearstatcache(bool $clear_realpath_cache, string $filename): void
```

Lowers `clearstatcache(...)` as an ordered no-op after EIR operand evaluation.

**Parameters**:
- `$clear_realpath_cache` (`bool`), optional
- `$filename` (`string`), optional

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `clearstatcache` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/clearstatcache.md).

