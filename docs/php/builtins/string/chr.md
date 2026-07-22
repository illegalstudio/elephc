---
title: "chr()"
description: "Returns a one-character string from the given byte code point."
sidebar:
  order: 363
---

## chr()

```php
function chr(int $codepoint): string
```

Returns a one-character string from the given byte code point.

**Parameters**:
- `$codepoint` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/chr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/chr.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `chr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/chr.md).
