---
title: "get_declared_traits()"
description: "Returns an array of all declared traits."
sidebar:
  order: 82
---

## get_declared_traits()

```php
function get_declared_traits(): array
```

Returns an array of all declared traits.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_declared_traits.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_declared_traits.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `get_declared_traits` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_declared_traits.md).
