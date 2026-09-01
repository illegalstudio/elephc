---
title: "error_reporting()"
description: "Gets or sets the active error-reporting mask."
sidebar:
  order: 324
---

## error_reporting()

```php
function error_reporting(int $error_level = null): int
```

Gets or sets the active error-reporting mask.

**Parameters**:
- `$error_level` (`int`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/error_reporting.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/error_reporting.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `error_reporting` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/error_reporting.md).
