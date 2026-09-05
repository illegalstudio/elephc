---
title: "get_resources()"
description: "Returns currently active resources, optionally filtered by type."
sidebar:
  order: 343
---

## get_resources()

```php
function get_resources(mixed $type = null): array
```

Returns currently active resources, optionally filtered by type.

**Parameters**:
- `$type` (`mixed`), default `null`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/get_resources.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/get_resources.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_resources` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_resources.md).
