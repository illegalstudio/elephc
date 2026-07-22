---
title: "class_alias()"
description: "Creates an alias for a class."
sidebar:
  order: 66
---

## class_alias()

```php
function class_alias(string $class, string $alias, bool $autoload = true): bool
```

Creates an alias for a class.

**Parameters**:
- `$class` (`string`)
- `$alias` (`string`)
- `$autoload` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/class_alias.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/class_alias.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `class_alias` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/class_alias.md).
