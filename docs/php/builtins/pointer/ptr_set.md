---
title: "ptr_set()"
description: "Writes one machine word through a raw pointer."
sidebar:
  order: 298
---

## ptr_set()

```php
function ptr_set(pointer $pointer, mixed $value): void
```

Writes one machine word through a raw pointer.

**Parameters**:
- `$pointer` (`pointer`)
- `$value` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_set.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_set.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_set.md).

