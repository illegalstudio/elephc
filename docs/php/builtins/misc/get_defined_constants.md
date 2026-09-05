---
title: "get_defined_constants()"
description: "Returns constants visible to the current program."
sidebar:
  order: 335
---

## get_defined_constants()

```php
function get_defined_constants(bool $categorize = false): array
```

Returns constants visible to the current program.

**Parameters**:
- `$categorize` (`bool`), default `false`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/get_defined_constants.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/get_defined_constants.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_defined_constants` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_defined_constants.md).
