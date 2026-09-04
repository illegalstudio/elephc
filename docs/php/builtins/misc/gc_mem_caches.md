---
title: "gc_mem_caches()"
description: "Reclaims memory used by the runtime allocator caches."
sidebar:
  order: 333
---

## gc_mem_caches()

```php
function gc_mem_caches(): int
```

Reclaims memory used by the runtime allocator caches.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/gc_mem_caches.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/gc_mem_caches.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gc_mem_caches` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/gc_mem_caches.md).
