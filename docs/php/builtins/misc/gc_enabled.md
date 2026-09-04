---
title: "gc_enabled()"
description: "Reports whether automatic cycle collection is enabled."
sidebar:
  order: 332
---

## gc_enabled()

```php
function gc_enabled(): bool
```

Reports whether automatic cycle collection is enabled.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/gc_enabled.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/gc_enabled.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gc_enabled` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/gc_enabled.md).
