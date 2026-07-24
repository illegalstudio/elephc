---
title: "spl_autoload_register()"
description: "Register given function as __autoload() implementation."
sidebar:
  order: 352
---

## spl_autoload_register()

```php
function spl_autoload_register(callable $callback = null, bool $throw = true, bool $prepend = false): bool
```

Register given function as __autoload() implementation.

**Parameters**:
- `$callback` (`callable`), default `null`, optional
- `$throw` (`bool`), default `true`, optional
- `$prepend` (`bool`), default `false`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/spl_autoload_register.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/spl_autoload_register.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_autoload_register` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload_register.md).
