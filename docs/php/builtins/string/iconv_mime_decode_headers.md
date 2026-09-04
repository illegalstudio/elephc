---
title: "iconv_mime_decode_headers()"
description: "Decodes a block of MIME header fields into an associative array."
sidebar:
  order: 481
---

## iconv_mime_decode_headers()

```php
function iconv_mime_decode_headers(string $headers, int $mode = 0, string $encoding = null): mixed
```

Decodes a block of MIME header fields into an associative array.

**Parameters**:
- `$headers` (`string`)
- `$mode` (`int`), default `0`, optional
- `$encoding` (`string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/iconv_mime_decode_headers.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/iconv_mime_decode_headers.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_mime_decode_headers` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_mime_decode_headers.md).
