---
title: "define()"
description: "Defines a named constant at runtime."
sidebar:
  order: 276
---

## define()

```php
function define(string $constant_name, mixed $value): bool
```

Defines a named constant at runtime.

**Parameters**:
- `$constant_name` (`string`)
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/define.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/define.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `define` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/define.md).

