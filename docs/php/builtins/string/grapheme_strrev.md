---
title: "grapheme_strrev()"
description: "Reverses a string by grapheme cluster, returning false on failure."
---

## grapheme_strrev()

```php
function grapheme_strrev(string $string): mixed
```

Reverses a string by grapheme cluster, returning false on failure.

**Parameters**:
- `$string` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `grapheme_strrev` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/grapheme_strrev.md).
