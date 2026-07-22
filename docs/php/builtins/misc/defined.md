---
title: "defined()"
description: "Checks whether a given named constant exists."
sidebar:
  order: 292
---

## defined()

```php
function defined(string $constant_name): bool
```

Checks whether a given named constant exists.

**Parameters**:
- `$constant_name` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/defined.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/defined.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `defined` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/defined.md).
