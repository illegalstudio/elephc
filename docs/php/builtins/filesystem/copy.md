---
title: "copy()"
description: "Copies a file."
sidebar:
  order: 108
---

## copy()

```php
function copy(string $from, string $to): bool
```

Copies a file.

**Parameters**:
- `$from` (`string`)
- `$to` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/copy.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/copy.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `copy` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/copy.md).

