---
title: "mb_encode_mimeheader()"
description: "Encodes a MIME header with language defaults, Base64 or Q transfer, and folded lines."
sidebar:
  order: 807
---

## mb_encode_mimeheader()

```php
function mb_encode_mimeheader(string $string, ?string $charset = null, ?string $transfer_encoding = null, string $newline = "\r\n", int $indent = 0): string
```

Encodes a MIME header with language defaults, Base64 or Q transfer, and folded lines.

**Parameters**:
- `$string` (`string`)
- `$charset` (`?string`), default `null`, optional
- `$transfer_encoding` (`?string`), default `null`, optional
- `$newline` (`string`), default `"\r\n"`, optional
- `$indent` (`int`), default `0`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_encode_mimeheader.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_encode_mimeheader.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_encode_mimeheader` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_encode_mimeheader.md).
