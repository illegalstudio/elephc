---
title: "token_name()"
description: "Get the symbolic name of a PHP token identifier."
sidebar:
  order: 1018
---

## token_name()

```php
function token_name(int $id): string
```

Get the symbolic name of a PHP token identifier.

**Parameters**:
- `$id` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected tokenizer prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/tokenizer/token_name.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/tokenizer/token_name.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `token_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/token_name.md).
