---
title: "spl_autoload_call()"
description: "Try all registered __autoload() functions to load the requested class."
sidebar:
  order: 329
---

## spl_autoload_call()

```php
function spl_autoload_call(string $class): void
```

Try all registered __autoload() functions to load the requested class.

**Parameters**:
- `$class` (`string`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/spl_autoload_call.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/spl_autoload_call.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_autoload_call` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload_call.md).

