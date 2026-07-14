---
title: "ptr_null()"
description: "Returns a null raw pointer."
sidebar:
  order: 292
---

## ptr_null()

```php
function ptr_null(): mixed
```

Returns a null raw pointer.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_null.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_null.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_null` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_null.md).

