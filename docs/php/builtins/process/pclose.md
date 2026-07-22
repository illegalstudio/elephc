---
title: "pclose()"
description: "Closes process file pointer."
sidebar:
  order: 327
---

## pclose()

```php
function pclose(resource $handle): int
```

Closes process file pointer.

**Parameters**:
- `$handle` (`resource`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/pclose.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/pclose.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `pclose` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/pclose.md).
