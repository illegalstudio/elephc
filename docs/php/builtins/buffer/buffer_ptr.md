---
title: "buffer_ptr()"
description: "Returns the payload address of a buffer<T>, valid until the buffer is freed."
sidebar:
  order: 76
---

## buffer_ptr()

```php
function buffer_ptr(buffer $buffer): pointer
```

Returns the payload address of a buffer<T>, valid until the buffer is freed.

**Parameters**:
- `$buffer` (`buffer`)

**Returns**: `pointer`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_ptr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_ptr.rs)).
- **Strict PHP mode**: hidden - this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `buffer_ptr` is implemented in the compiler, see [the internals page](../../../internals/builtins/buffer/buffer_ptr.md).
