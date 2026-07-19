---
title: "umask()"
description: "Changes the current umask."
sidebar:
  order: 155
---

## umask()

```php
function umask(int $mask = null): int
```

Changes the current umask.

**Parameters**:
- `$mask` (`int`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/umask.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/umask.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `umask` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/umask.md).

