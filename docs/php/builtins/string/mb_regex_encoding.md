---
title: "mb_regex_encoding()"
description: "Reads or changes the multibyte regex encoding independently of the internal text encoding."
sidebar:
  order: 834
---

## mb_regex_encoding()

```php
function mb_regex_encoding(?string $encoding = null): string|bool
```

Reads or changes the multibyte regex encoding independently of the internal text encoding.

**Parameters**:
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string|bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_regex_encoding.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_regex_encoding.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_regex_encoding` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_regex_encoding.md).
