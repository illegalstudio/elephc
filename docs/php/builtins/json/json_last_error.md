---
title: "json_last_error()"
description: "Returns the last error (if any) occurred during the last JSON encoding/decoding."
sidebar:
  order: 249
---

## json_last_error()

```php
function json_last_error(): int
```

Returns the last error (if any) occurred during the last JSON encoding/decoding.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/json/json_last_error.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/json/json_last_error.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `json_last_error` is implemented in the compiler, see [the internals page](../../../internals/builtins/json/json_last_error.md).

