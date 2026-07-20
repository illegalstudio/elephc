---
title: "realpath_cache_size()"
description: "Returns the amount of memory used by the realpath cache."
sidebar:
  order: 145
---

## realpath_cache_size()

```php
function realpath_cache_size(): int
```

Returns the amount of memory used by the realpath cache.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/realpath_cache_size.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/realpath_cache_size.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `realpath_cache_size` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/realpath_cache_size.md).

