---
title: "rawurlencode()"
description: "URL-encodes a string using RFC 3986 percent-encoding (no '+' for spaces)."
sidebar:
  order: 382
---

## rawurlencode()

```php
function rawurlencode(string $string): string
```

URL-encodes a string using RFC 3986 percent-encoding (no '+' for spaces).

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/rawurlencode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/rawurlencode.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `rawurlencode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/rawurlencode.md).

