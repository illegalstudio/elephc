---
title: "clearstatcache()"
description: "Clears file status cache."
sidebar:
  order: 109
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

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/clearstatcache.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/clearstatcache.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `clearstatcache` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/clearstatcache.md).
