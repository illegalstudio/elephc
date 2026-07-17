---
title: "class_implements()"
description: "Returns the interfaces which are implemented by the given class or its parents."
sidebar:
  order: 71
---

## class_implements()

```php
function class_implements(mixed $object_or_class, bool $autoload = true): mixed
```

Returns the interfaces which are implemented by the given class or its parents.

**Parameters**:
- `$object_or_class` (`mixed`)
- `$autoload` (`bool`), default `true`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/class_implements.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/class_implements.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `class_implements` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/class_implements.md).

