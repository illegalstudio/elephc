---
title: "ptr()"
description: "Returns a raw pointer to the given variable."
sidebar:
  order: 304
---

## ptr()

```php
function ptr(mixed $value): mixed
```

Returns a raw pointer to the given variable.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/raw_memory/ptr.rs)).
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr.md).
