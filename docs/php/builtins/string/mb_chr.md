---
title: "mb_chr()"
description: "Encodes a Unicode codepoint, or returns false when it cannot be represented."
sidebar:
  order: 799
---

## mb_chr()

```php
function mb_chr(int $codepoint, ?string $encoding = null): string|false
```

Encodes a Unicode codepoint, or returns false when it cannot be represented.

**Parameters**:
- `$codepoint` (`int`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_chr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_chr.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_chr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_chr.md).
