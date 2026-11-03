---
title: "preg_last_error_msg()"
description: "Returns the error message of the last PCRE regex execution."
sidebar:
  order: 779
---

## preg_last_error_msg()

```php
function preg_last_error_msg(): string
```

Returns the error message of the last PCRE regex execution.

**Parameters**: none.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/regex/preg_last_error.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_last_error.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_last_error_msg` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/preg_last_error_msg.md).
