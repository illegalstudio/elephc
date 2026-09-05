---
title: "gc_collect_cycles()"
description: "Forces collection of any existing garbage cycles."
sidebar:
  order: 329
---

## gc_collect_cycles()

```php
function gc_collect_cycles(): int
```

Forces collection of any existing garbage cycles.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/gc_collect_cycles.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/gc_collect_cycles.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gc_collect_cycles` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/gc_collect_cycles.md).
