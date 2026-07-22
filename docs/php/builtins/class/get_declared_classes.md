---
title: "get_declared_classes()"
description: "Returns an array of the names of the defined classes."
sidebar:
  order: 80
---

## get_declared_classes()

```php
function get_declared_classes(): array
```

Returns an array of the names of the defined classes.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_declared_classes.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_declared_classes.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `get_declared_classes` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_declared_classes.md).
