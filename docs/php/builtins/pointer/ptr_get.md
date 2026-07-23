---
title: "ptr_get()"
description: "Reads one machine word through a raw pointer and returns it as an integer."
sidebar:
  order: 306
---

## ptr_get()

```php
function ptr_get(pointer $pointer): int
```

Reads one machine word through a raw pointer and returns it as an integer.

**Parameters**:
- `$pointer` (`pointer`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_get.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_get.rs)).
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_get.md).
