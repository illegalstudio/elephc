---
title: "is_resource()"
description: "Checks whether a variable is a resource."
sidebar:
  order: 460
---

## is_resource()

```php
function is_resource(mixed $value): bool
```

Checks whether a variable is a resource.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_resource.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_resource.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_resource` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_resource.md).
