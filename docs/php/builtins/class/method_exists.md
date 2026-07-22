---
title: "method_exists()"
description: "Checks whether a class method exists."
sidebar:
  order: 88
---

## method_exists()

```php
function method_exists(mixed $object_or_class, string $method): bool
```

Checks whether a class method exists.

**Parameters**:
- `$object_or_class` (`mixed`)
- `$method` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/method_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/method_exists.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `method_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/method_exists.md).
