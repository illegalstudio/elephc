---
title: "interface_exists()"
description: "Checks if the interface has been defined."
sidebar:
  order: 85
---

## interface_exists()

```php
function interface_exists(string $interface, bool $autoload = true): bool
```

Checks if the interface has been defined.

**Parameters**:
- `$interface` (`string`)
- `$autoload` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/interface_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/interface_exists.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `interface_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/interface_exists.md).

