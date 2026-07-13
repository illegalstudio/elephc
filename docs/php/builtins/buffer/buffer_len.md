---
title: "buffer_len()"
description: "Lowers `buffer_len()` through the direct buffer opcode helper."
sidebar:
  order: 65
---

## buffer_len()

```php
function buffer_len(buffer $buffer): int
```

Lowers `buffer_len()` through the direct buffer opcode helper.

**Parameters**:
- `$buffer` (`buffer`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_len.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_len.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `buffer_len` is implemented in the compiler, see [the internals page](../../../internals/builtins/buffer/buffer_len.md).

