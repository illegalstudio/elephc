---
title: "buffer_new()"
description: "Allocates a raw byte buffer."
sidebar:
  order: 341
---

## buffer_new()

```php
function buffer_new(int $length): mixed
```

Allocates a raw byte buffer.

**Parameters**:
- `$length` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through a dedicated AST/EIR syntax path.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_new.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_new.rs)).
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `buffer_new` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/buffer_new.md).
