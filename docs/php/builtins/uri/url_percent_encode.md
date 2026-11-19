---
title: "url_percent_encode()"
description: "Percent-encodes a string with a WHATWG URL encoding set."
---

## url_percent_encode()

```php
function url_percent_encode(string $input, mixed $mode): string
```

Percent-encodes a string with a WHATWG URL encoding set.

**Parameters**:
- `$input` (`string`)
- `$mode` (`mixed`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `url_percent_encode` is implemented in the compiler, see [the internals page](../../../internals/builtins/uri/url_percent_encode.md).
