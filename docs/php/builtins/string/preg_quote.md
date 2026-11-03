---
title: "preg_quote()"
description: "Escapes the PCRE metacharacters in a string, plus the delimiter when one is given."
sidebar:
  order: 1024
---

## preg_quote()

```php
function preg_quote(string $str, string $delimiter = null): string
```

Escapes the PCRE metacharacters in a string, plus the delimiter when one is given.

**Parameters**:
- `$str` (`string`)
- `$delimiter` (`string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/preg_quote.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/preg_quote.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_quote` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/preg_quote.md).
