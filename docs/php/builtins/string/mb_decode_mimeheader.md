---
title: "mb_decode_mimeheader()"
description: "Decodes MIME header words into the current internal encoding and unfolds whitespace."
sidebar:
  order: 803
---

## mb_decode_mimeheader()

```php
function mb_decode_mimeheader(string $string): string
```

Decodes MIME header words into the current internal encoding and unfolds whitespace.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_decode_mimeheader.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_decode_mimeheader.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_decode_mimeheader` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_decode_mimeheader.md).
