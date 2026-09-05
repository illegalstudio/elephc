---
title: "gc_disable()"
description: "Disables automatic collection of circular references."
sidebar:
  order: 330
---

## gc_disable()

```php
function gc_disable(): void
```

Disables automatic collection of circular references.

**Parameters**: none.

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/gc_disable.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/gc_disable.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gc_disable` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/gc_disable.md).
