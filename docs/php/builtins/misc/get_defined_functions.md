---
title: "get_defined_functions()"
description: "Returns internal and user-defined function names. Elephc has no disable_functions configuration, so exclude_disabled is accepted but does not change the result."
sidebar:
  order: 336
---

## get_defined_functions()

```php
function get_defined_functions(bool $exclude_disabled = true): array
```

Returns internal and user-defined function names. Elephc has no disable_functions configuration, so exclude_disabled is accepted but does not change the result.

**Parameters**:
- `$exclude_disabled` (`bool`), default `true`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/get_defined_functions.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/get_defined_functions.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_defined_functions` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_defined_functions.md).
