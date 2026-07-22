---
title: "enum_exists()"
description: "Checks if the enum has been defined."
sidebar:
  order: 74
---

## enum_exists()

```php
function enum_exists(string $enum, bool $autoload = true): bool
```

Checks if the enum has been defined.

**Parameters**:
- `$enum` (`string`)
- `$autoload` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/enum_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/enum_exists.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `enum_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/enum_exists.md).
