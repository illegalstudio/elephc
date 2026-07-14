---
title: "ptr_offset()"
description: "Returns a new pointer offset from the given pointer by the given byte count."
sidebar:
  order: 293
---

## ptr_offset()

```php
function ptr_offset(pointer $pointer, int $offset): mixed
```

Returns a new pointer offset from the given pointer by the given byte count.

**Parameters**:
- `$pointer` (`pointer`)
- `$offset` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_offset.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_offset.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_offset` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_offset.md).

