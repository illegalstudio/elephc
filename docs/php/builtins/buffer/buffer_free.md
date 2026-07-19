---
title: "buffer_free()"
description: "Frees a buffer<T> and nulls the local variable that held it."
sidebar:
  order: 64
---

## buffer_free()

```php
function buffer_free(buffer $buffer): void
```

Frees a buffer<T> and nulls the local variable that held it.

**Parameters**:
- `$buffer` (`buffer`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_free.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_free.rs)).
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `buffer_free` is implemented in the compiler, see [the internals page](../../../internals/builtins/buffer/buffer_free.md).

