---
title: "ob_list_handlers()"
description: "Lists all output handlers in use."
sidebar:
  order: 200
---

## ob_list_handlers()

```php
function ob_list_handlers(): array
```

Lists all output handlers in use.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/ob_list_handlers.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_list_handlers.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ob_list_handlers` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_list_handlers.md).

