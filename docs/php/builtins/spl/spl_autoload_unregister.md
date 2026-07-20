---
title: "spl_autoload_unregister()"
description: "Unregister given function as __autoload() implementation."
sidebar:
  order: 346
---

## spl_autoload_unregister()

```php
function spl_autoload_unregister(callable $callback): bool
```

Unregister given function as __autoload() implementation.

**Parameters**:
- `$callback` (`callable`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/spl_autoload_unregister.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/spl_autoload_unregister.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_autoload_unregister` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload_unregister.md).

