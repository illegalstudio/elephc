---
title: "mb_strcut()"
description: "Selects complete encoded characters within a byte offset and length."
sidebar:
  order: 841
---

## mb_strcut()

```php
function mb_strcut(string $string, int $start, ?int $length = null, ?string $encoding = null): string
```

Selects complete encoded characters within a byte offset and length.

**Parameters**:
- `$string` (`string`)
- `$start` (`int`)
- `$length` (`?int`), default `null`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_strcut.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_strcut.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strcut` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strcut.md).
