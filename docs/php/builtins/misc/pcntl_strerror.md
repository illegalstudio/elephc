---
title: "pcntl_strerror()"
description: "Returns the system message for a PCNTL errno value."
sidebar:
  order: 347
---

## pcntl_strerror()

```php
function pcntl_strerror(int $error_code): string
```

Returns the system message for a PCNTL errno value.

**Parameters**:
- `$error_code` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_strerror.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_strerror.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_strerror` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_strerror.md).
