---
title: "proc_nice()"
description: "Changes the priority of the current process."
sidebar:
  order: 786
---

## proc_nice()

```php
function proc_nice(int $priority): bool
```

Changes the priority of the current process.

**Parameters**:
- `$priority` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected proc prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_nice.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_nice.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `proc_nice` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/proc_nice.md).
