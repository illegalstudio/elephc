---
title: "ptr_is_null()"
description: "Returns true if the pointer is null."
sidebar:
  order: 291
---

## ptr_is_null()

```php
function ptr_is_null(pointer $pointer): bool
```

Returns true if the pointer is null.

**Parameters**:
- `$pointer` (`pointer`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_is_null.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr_is_null.rs)).
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_is_null` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_is_null.md).

