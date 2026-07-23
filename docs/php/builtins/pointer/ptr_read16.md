---
title: "ptr_read16()"
description: "Reads one unsigned 16-bit word through a raw pointer and returns it as an integer."
sidebar:
  order: 310
---

## ptr_read16()

```php
function ptr_read16(pointer $pointer): int
```

Reads one unsigned 16-bit word through a raw pointer and returns it as an integer.

**Parameters**:
- `$pointer` (`pointer`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_read16.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_read16.rs)).
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_read16` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_read16.md).
