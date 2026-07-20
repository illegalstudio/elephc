---
title: "ptr_read_string()"
description: "Copies raw bytes from a pointer into a PHP string of the given length."
sidebar:
  order: 310
---

## ptr_read_string()

```php
function ptr_read_string(pointer $pointer, int $length): string
```

Copies raw bytes from a pointer into a PHP string of the given length.

**Parameters**:
- `$pointer` (`pointer`)
- `$length` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_read_string.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_read_string.rs)).
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_read_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_read_string.md).

