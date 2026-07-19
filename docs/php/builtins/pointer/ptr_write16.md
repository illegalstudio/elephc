---
title: "ptr_write16()"
description: "Writes one 16-bit word through a raw pointer."
sidebar:
  order: 300
---

## ptr_write16()

```php
function ptr_write16(pointer $pointer, int $value): void
```

Writes one 16-bit word through a raw pointer.

**Parameters**:
- `$pointer` (`pointer`)
- `$value` (`int`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_write16.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_write16.rs)).
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_write16` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_write16.md).

