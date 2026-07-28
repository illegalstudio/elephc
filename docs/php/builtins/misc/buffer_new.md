---
title: "buffer_new()"
description: "buffer_new() — misc builtin supported by Elephc."
sidebar:
  order: 291
---

## buffer_new()

```php
function buffer_new(int $length): mixed
```

`buffer_new()` is a misc builtin supported by Elephc. Behavior matches the PHP manual unless noted below.

**Parameters**:
- `$length` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_new.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_new.rs)).
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._
