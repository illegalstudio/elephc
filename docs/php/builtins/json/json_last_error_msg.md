---
title: "json_last_error_msg()"
description: "Returns the error string of the last json_encode() or json_decode() call."
sidebar:
  order: 252
---

## json_last_error_msg()

```php
function json_last_error_msg(): string
```

Returns the error string of the last json_encode() or json_decode() call.

**Parameters**: none.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/json/json_last_error_msg.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/json/json_last_error_msg.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `json_last_error_msg` is implemented in the compiler, see [the internals page](../../../internals/builtins/json/json_last_error_msg.md).
