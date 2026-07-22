---
title: "getprotobynumber()"
description: "Gets the protocol name associated with the given protocol number."
sidebar:
  order: 187
---

## getprotobynumber()

```php
function getprotobynumber(int $protocol): mixed
```

Gets the protocol name associated with the given protocol number.

**Parameters**:
- `$protocol` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/getprotobynumber.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/getprotobynumber.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `getprotobynumber` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/getprotobynumber.md).
