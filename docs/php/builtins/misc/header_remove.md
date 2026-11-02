---
title: "header_remove()"
description: "Removes one or all pending HTTP response headers."
sidebar:
  order: 716
---

## header_remove()

```php
function header_remove(string $name = null): void
```

Removes one or all pending HTTP response headers.

**Parameters**:
- `$name` (`string`), default `null`, optional

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/header_remove.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/header_remove.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `header_remove` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/header_remove.md).
