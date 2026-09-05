---
title: "gc_enable()"
description: "Enables automatic collection of circular references."
sidebar:
  order: 331
---

## gc_enable()

```php
function gc_enable(): void
```

Enables automatic collection of circular references.

**Parameters**: none.

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/gc_enable.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/gc_enable.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gc_enable` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/gc_enable.md).
