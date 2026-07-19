---
title: "class_parents()"
description: "Returns the parent classes of the given class."
sidebar:
  order: 72
---

## class_parents()

```php
function class_parents(mixed $object_or_class, bool $autoload = true): mixed
```

Returns the parent classes of the given class.

**Parameters**:
- `$object_or_class` (`mixed`)
- `$autoload` (`bool`), default `true`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/class_parents.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/class_parents.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `class_parents` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/class_parents.md).

