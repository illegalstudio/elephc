---
title: "gc_status()"
description: "Returns live collector counters, candidate roots, and phase timings. The unbuffered collector reports threshold and buffer_size as 0 and full as false."
sidebar:
  order: 334
---

## gc_status()

```php
function gc_status(): mixed
```

Returns live collector counters, candidate roots, and phase timings. The unbuffered collector reports threshold and buffer_size as 0 and full as false.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/gc_status.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/gc_status.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gc_status` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/gc_status.md).
