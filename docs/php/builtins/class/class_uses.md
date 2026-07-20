---
title: "class_uses()"
description: "Returns the traits used by the given class."
sidebar:
  order: 73
---

## class_uses()

```php
function class_uses(mixed $object_or_class, bool $autoload = true): mixed
```

Returns the traits used by the given class.

**Parameters**:
- `$object_or_class` (`mixed`)
- `$autoload` (`bool`), default `true`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/class_uses.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/class_uses.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `class_uses` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/class_uses.md).

