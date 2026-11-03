---
title: "preg_last_error()"
description: "Returns the error code of the last PCRE regex execution."
sidebar:
  order: 778
---

## preg_last_error()

```php
function preg_last_error(): int
```

Returns the error code of the last PCRE regex execution.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/regex/preg_last_error.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_last_error.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_last_error` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/preg_last_error.md).
