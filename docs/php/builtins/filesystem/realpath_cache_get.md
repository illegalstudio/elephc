---
title: "realpath_cache_get()"
description: "Returns realpath cache entries."
sidebar:
  order: 146
---

## realpath_cache_get()

```php
function realpath_cache_get(): array
```

Returns realpath cache entries.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/realpath_cache_get.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/realpath_cache_get.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `realpath_cache_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/realpath_cache_get.md).
