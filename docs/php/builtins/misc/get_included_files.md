---
title: "get_included_files()"
description: "Returns the files included by the current program."
sidebar:
  order: 339
---

## get_included_files()

```php
function get_included_files(): array
```

Returns the files included by the current program.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/get_included_files.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/get_included_files.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_included_files` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_included_files.md).
