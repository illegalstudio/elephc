---
title: "iconv_mime_encode()"
description: "Encodes one header field as RFC 2047 encoded-words."
sidebar:
  order: 482
---

## iconv_mime_encode()

```php
function iconv_mime_encode(string $field_name, string $field_value, mixed $options = []): mixed
```

Encodes one header field as RFC 2047 encoded-words.

**Parameters**:
- `$field_name` (`string`)
- `$field_value` (`string`)
- `$options` (`mixed`), default `[]`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/iconv_mime_encode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/iconv_mime_encode.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_mime_encode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_mime_encode.md).
