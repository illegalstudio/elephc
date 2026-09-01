---
title: "iconv_mime_decode()"
description: "Decodes one MIME header field into the requested character encoding."
sidebar:
  order: 425
---

## iconv_mime_decode()

```php
function iconv_mime_decode(string $string, int $mode = 0, string $encoding = null): mixed
```

Decodes one MIME header field into the requested character encoding.

**Parameters**:
- `$string` (`string`)
- `$mode` (`int`), default `0`, optional
- `$encoding` (`string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/iconv_mime_decode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/iconv_mime_decode.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_mime_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_mime_decode.md).
