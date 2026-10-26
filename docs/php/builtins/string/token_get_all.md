---
title: "token_get_all()"
description: "Split PHP source into tokens while preserving its original bytes."
sidebar:
  order: 949
---

## token_get_all()

```php
function token_get_all(string $code, int $flags = 0): array
```

Split PHP source into tokens while preserving its original bytes.

**Parameters**:
- `$code` (`string`)
- `$flags` (`int`), default `0`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected tokenizer prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/tokenizer/token_get_all.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/tokenizer/token_get_all.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `token_get_all` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/token_get_all.md).
