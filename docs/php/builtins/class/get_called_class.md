---
title: "get_called_class()"
description: "Returns the late-static-binding class name."
sidebar:
  order: 83
---

## get_called_class()

```php
function get_called_class(): string
```

Returns the late-static-binding class name.

**Parameters**: none.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_called_class.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_called_class.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_called_class` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_called_class.md).
