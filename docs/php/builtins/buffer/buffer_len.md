---
title: "buffer_len()"
description: "Returns the logical element count of a buffer<T>."
sidebar:
  order: 65
---

## buffer_len()

```php
function buffer_len(buffer $buffer): int
```

Returns the logical element count of a buffer<T>.

**Parameters**:
- `$buffer` (`buffer`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_len.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_len.rs)).
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `buffer_len` is implemented in the compiler, see [the internals page](../../../internals/builtins/buffer/buffer_len.md).
