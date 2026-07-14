---
title: "function_exists()"
description: "Returns true if the given function has been defined."
sidebar:
  order: 75
---

## function_exists()

```php
function function_exists(string $function): bool
```

Returns true if the given function has been defined.

**Parameters**:
- `$function` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/function_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/function_exists.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `function_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/function_exists.md).

