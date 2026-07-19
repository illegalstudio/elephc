---
title: "class_exists()"
description: "Checks whether the given class has been defined."
sidebar:
  order: 69
---

## class_exists()

```php
function class_exists(string $class, bool $autoload = true): bool
```

Checks whether the given class has been defined.

**Parameters**:
- `$class` (`string`)
- `$autoload` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/class_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/class_exists.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `class_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/class_exists.md).

