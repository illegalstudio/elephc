---
title: "spl_autoload_functions()"
description: "Return all registered __autoload() functions."
sidebar:
  order: 351
---

## spl_autoload_functions()

```php
function spl_autoload_functions(): array
```

Return all registered __autoload() functions.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/spl_autoload_functions.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/spl_autoload_functions.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_autoload_functions` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload_functions.md).
