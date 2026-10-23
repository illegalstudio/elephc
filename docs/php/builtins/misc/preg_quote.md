---
title: "preg_quote()"
description: "Quotes regular expression characters."
sidebar:
  order: 683
---

## preg_quote()

```php
function preg_quote(string $str, ?string $delimiter = null): string
```

Quotes regular expression characters.

**Parameters**:
- `$str` (`string`)
- `$delimiter` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/preg_quote.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/preg_quote.rs)).

**Examples**:

// Full example: examples/string-ops/main.php
echo preg_quote("1.5*2=3?", "/");  // 1\.5\*2\=3\?

## Internals

For how `preg_quote` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/preg_quote.md).
