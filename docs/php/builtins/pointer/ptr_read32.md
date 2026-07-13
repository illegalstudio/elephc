---
title: "ptr_read32()"
description: "Reads one unsigned 32-bit word through a raw pointer and returns it as an integer."
sidebar:
  order: 295
---

## ptr_read32()

```php
function ptr_read32(pointer $pointer): int
```

Reads one unsigned 32-bit word through a raw pointer and returns it as an integer.

**Parameters**:
- `$pointer` (`pointer`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_read32.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_read32.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_read32` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_read32.md).

