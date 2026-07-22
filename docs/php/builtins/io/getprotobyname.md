---
title: "getprotobyname()"
description: "Gets the protocol number associated with the given protocol name."
sidebar:
  order: 186
---

## getprotobyname()

```php
function getprotobyname(string $protocol): mixed
```

Gets the protocol number associated with the given protocol name.

**Parameters**:
- `$protocol` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/getprotobyname.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/getprotobyname.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `getprotobyname` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/getprotobyname.md).
