---
title: "headers_list()"
description: "Returns the pending response headers as `Name: value` lines."
sidebar:
  order: 721
---

## headers_list()

```php
function headers_list(): array
```

Returns the pending response headers as `Name: value` lines.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/headers_list.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/headers_list.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `headers_list` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/headers_list.md).
