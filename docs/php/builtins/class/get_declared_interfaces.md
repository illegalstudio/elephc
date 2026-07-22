---
title: "get_declared_interfaces()"
description: "Returns an array of all declared interfaces."
sidebar:
  order: 81
---

## get_declared_interfaces()

```php
function get_declared_interfaces(): array
```

Returns an array of all declared interfaces.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_declared_interfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_declared_interfaces.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `get_declared_interfaces` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_declared_interfaces.md).
