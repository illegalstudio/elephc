---
title: "get_defined_vars()"
description: "Returns variables visible in the current scope."
sidebar:
  order: 337
---

## get_defined_vars()

```php
function get_defined_vars(): array
```

Returns variables visible in the current scope.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/get_defined_vars.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/get_defined_vars.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_defined_vars` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_defined_vars.md).
