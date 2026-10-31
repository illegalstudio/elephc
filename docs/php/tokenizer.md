---
title: "Tokenizer"
description: "Inspect PHP source with token_get_all, token_name and PhpToken while preserving source bytes."
sidebar:
  order: 22
---

The tokenizer module exposes both PHP functions, the `PhpToken` class and all 154
constants in the PHP 8.5 compatibility baseline. It works in compiled code and
inside runtime `eval()` on every supported target. Compiled programs link the
Rust tokenizer bridge automatically. `--with-tokenizer` explicitly retains the
surface for indirect use.

## Inspecting source

```php
$source = '<?php /* comment */ echo "hello";';
foreach (PhpToken::tokenize($source) as $token) {
    if (!$token->is([T_COMMENT, T_DOC_COMMENT])) {
        echo $token->text;
    }
}
```

`token_get_all(string $code, int $flags = 0): array` returns a mixture of
single-character strings and arrays containing `[token ID, original text, starting
line]`. Input without an opening PHP tag is inline HTML. Whitespace, comments,
opening and closing tags, interpolation, heredocs and non-UTF-8 source bytes are
preserved. Lexing does not depend on the language subset elephc compiles.

`token_get_all()` checks the current `memory_limit` while materializing the token
array and emits PHP's allocation-exhausted fatal at the caller's file and line.
When the program links the Core INI runtime, the configured `memory_limit` wins
(including `ini_set('memory_limit', ...)`). A tokenizer-only program has no Core
INI state, so the check falls back to the compiled heap capacity (`--heap-size`,
128 MB by default); the guard therefore applies even when the program never
mentions `ini_set`.

`token_name(int $id): string` returns the symbolic `T_*` name, or `"UNKNOWN"` for
punctuation and unknown IDs. Token IDs and the `T_DOUBLE_COLON` alias follow the
vendored PHP 8.5 baseline and do not depend on the machine's installed PHP.

## PhpToken

`PhpToken::tokenize(string $code, int $flags = 0): array` returns a `PhpToken`
object for every token, including punctuation. Its four mutable public properties
are `int $id`, `string $text`, `int $line` and `int $pos`. Lines start at 1 and byte
positions start at 0. CRLF counts as one line break; multibyte characters occupy
multiple bytes.

- `new PhpToken(int $id, string $text, int $line = -1, int $pos = -1)` constructs a token.
- `getTokenName(): ?string` returns a symbolic name, the character for a single-byte
  token, or `null` for an unknown named-token ID.
- `is(int|string|array $kind): bool` compares IDs and text strictly. Arrays are tested
  in order and must contain integer or string candidates.
- `isIgnorable(): bool` recognizes whitespace, comments, doc comments and full opening tags.
- `__toString(): string` returns the current text. The class implements `Stringable`.

The class may be extended. Its constructor is final, and inherited `tokenize()`
constructs instances of the called subclass. Abstract subclasses are rejected even
when the source is empty.

## Parser validation

Passing `TOKEN_PARSE` validates the source through elephc's lexer and AST parser
and reclassifies reserved words used as identifiers in contexts where PHP
permits them, such as class constant names, trait adaptation names and static or
instance member names. Valid PHP constructs outside elephc's own grammar are
normalized in the parser view first, so `goto`, variable variables, dynamic
`::` chains, DNF types, abstract property hooks and first-class callables remain
accepted. Syntax errors throw a catchable `ParseError` with PHP's parser-facing
wording for the grammar elephc models, including heredoc indentation and invalid
escape diagnostics. Because this validation reuses the compiler frontend, a
diagnostic can differ from Zend's exact token name or column on syntax outside
that grammar. Without the flag, incomplete and malformed PHP remains tokenizable.
Other flag bits are ignored, matching PHP.

Non-canonical casts (`integer`, `boolean`, `double` and `binary`) raise
`E_DEPRECATED` only under `TOKEN_PARSE`, exactly as PHP does. The `(real)` cast is
rejected with PHP's removed-cast message.

The optional bridge embeds a native Rust lexer and uses the leaf `elephc-syntax`
frontend for validation. It does not execute PHP or require a PHP installation,
managed native package or third-party PHP grammar at runtime. Tokenization,
parser feedback and constant lookup are shared with runtime `eval()`.

See [the tokenizer example](https://github.com/illegalstudio/elephc/tree/main/examples/tokenizer)
for a source inspector that removes comments without changing the remaining text.

<!-- elephc:generated:symbols:begin -->

## Functions {#functions}

Generated from the shared symbol catalog by `scripts/docs/gen_module_sections.py`; do not edit this section by hand. Each function links to its reference page.

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`token_get_all()`](./builtins/string/token_get_all.md) | `(string $code, int $flags = 0): array` | `array` | ✓ | ✓ |
| [`token_name()`](./builtins/string/token_name.md) | `(int $id): string` | `string` | ✓ | ✓ |

Classes: `PhpToken`.

Constants: 154 constants: `T_*` (153), `TOKEN_*` (1).

<!-- elephc:generated:symbols:end -->
