---
title: "ptr_write_string()"
description: "Copies PHP string bytes into raw memory at the given pointer."
sidebar:
  order: 303
---

## ptr_write_string()

```php
function ptr_write_string(pointer $pointer, string $string): int
```

Copies PHP string bytes into raw memory at the given pointer.

**Parameters**:
- `$pointer` (`pointer`)
- `$string` (`string`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_write_string.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_write_string.rs)).
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_write_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_write_string.md).

