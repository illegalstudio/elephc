---
title: "grapheme_strrev()"
description: "Reverses a string by grapheme cluster, returning false on failure."
sidebar:
  order: 795
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

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/grapheme_strrev.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/grapheme_strrev.rs)).
- **Strict PHP mode**: hidden - this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `grapheme_strrev` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/grapheme_strrev.md).
