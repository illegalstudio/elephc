---
title: "get_required_files()"
description: "Returns the files included or required by the current program."
sidebar:
  order: 342
---

## get_required_files()

```php
function get_required_files(): array
```

Returns the files included or required by the current program.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/get_required_files.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/get_required_files.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_required_files` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_required_files.md).
