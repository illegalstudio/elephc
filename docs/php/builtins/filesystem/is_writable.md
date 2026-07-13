---
title: "is_writable()"
description: "Tells whether the filename is writable."
sidebar:
  order: 131
---

## is_writable()

```php
function is_writable(string $filename): bool
```

Tells whether the filename is writable.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/is_writable.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/is_writable.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_writable` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_writable.md).

