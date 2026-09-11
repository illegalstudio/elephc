---
title: "get_called_class()"
description: "Returns the late-static-binding class name in eval context."
sidebar:
  order: 84
---

## get_called_class()

```php
function get_called_class(): mixed
```

Returns the late-static-binding class name in eval context.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: not available — compiled programs cannot call this builtin (`eval-only-reflection`).
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_called_class.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_called_class.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_called_class` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_called_class.md).
