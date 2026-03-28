# The Lexer

[← Back to Wiki](README.md) | Previous: [The Pipeline](how-elephc-works.md) | Next: [The Parser →](the-parser.md)

---

**Source:** `src/lexer/` — `scan.rs`, `cursor.rs`, `token.rs`, `literals.rs`

The lexer (also called tokenizer or scanner) is the first phase of compilation. It takes raw source text and breaks it into **tokens** — the smallest meaningful units of the language.

## What is a token?

A token is a categorized piece of text. The lexer doesn't care about structure (that's the [parser's](the-parser.md) job) — it just identifies what each piece of text *is*:

```php
<?php $x = 42 + $y;
```

```
OpenTag         "<?php"
Variable("x")   "$x"
Assign           "="
IntLiteral(42)   "42"
Plus             "+"
Variable("y")   "$y"
Semicolon        ";"
Eof
```

Some tokens carry data (the variable name, the number value). Others are just markers (semicolon, plus sign).

## The Cursor

**File:** `src/lexer/cursor.rs`

The cursor is the lowest-level component. It walks through the source text one character at a time, tracking the current position:

```rust
pub struct Cursor<'a> {
    bytes: &'a [u8],    // the source text as bytes
    pos: usize,         // current byte offset
    line: usize,        // current line number (1-indexed)
    col: usize,         // current column (1-indexed)
}
```

It provides three essential operations:

| Method | What it does |
|---|---|
| `peek()` | Look at the current character without moving |
| `advance()` | Move to the next character, return the one we just passed |
| `remaining()` | Get the rest of the source as a string slice |

The cursor automatically tracks line and column — when it sees a `\n`, it increments `line` and resets `col` to 1. This information is stored in a `Span` and attached to every token, so error messages can say "error at line 5, column 12".

## The Scanner

**File:** `src/lexer/scan.rs`

The scanner is the main loop. It uses the cursor to read characters and decides what token each sequence represents:

```rust
pub fn scan_tokens(source: &str) -> Result<Vec<(Token, Span)>, CompileError> {
    // 1. Skip whitespace
    // 2. Must start with <?php
    // 3. Loop: skip whitespace, look at next char, produce a token
    // 4. When EOF reached, push Eof token and return
}
```

### The scanning algorithm

For each token, the scanner looks at the current character and decides:

1. **`"`** → Start of a double-quoted string. May contain interpolation (`$var`). Delegates to `literals::scan_double_string_interpolated()`.

2. **`'`** → Start of a single-quoted string. No interpolation, only `\\` and `\'` escapes.

3. **Digit** → Start of a number. Read all digits. `0x` / `0X` starts a hexadecimal integer literal. If there's a `.` followed by more digits, it's a float. Handle `e`/`E` for scientific notation.

4. **`$`** → Start of a variable. Read the name (letters, digits, underscores).

5. **Letter or `_`** → Start of an identifier or keyword. Read the full word, then check if it's a keyword (`if`, `while`, `echo`, `function`, etc.) or a plain identifier (function name).

6. **Operator characters** (`+`, `-`, `*`, `/`, `=`, `<`, `>`, `!`, `.`, `%`, `&`, `|`, `^`, `~`) → Look ahead to handle multi-character operators (`==`, `===`, `!=`, `!==`, `<=`, `>=`, `<=>`, `<<`, `>>`, `&&`, `||`, `**`, `++`, `--`, `+=`, `-=`, `*=`, `/=`, `.=`, `%=`). Note that `<` may lead to `<=`, `<=>`, `<<`, or `<<<` (heredoc/nowdoc — see [below](#heredoc-and-nowdoc)).

7. **Structural characters** (`(`, `)`, `{`, `}`, `[`, `]`, `;`, `,`, `?`, `:`) → Single-character tokens. Note that `?` followed by another `?` produces the `??` (null coalescing) token instead.

### Whitespace and comments

Before each token, the scanner skips:

- **Whitespace**: spaces, tabs, newlines
- **Line comments**: `//` through end of line
- **Block comments**: `/*` through `*/`

These are discarded entirely — they don't produce tokens.

## Token types

**File:** `src/lexer/token.rs`

The full set of tokens elephc recognizes:

### Literals

| Token | Example | Carries |
|---|---|---|
| `IntLiteral` | `42`, `0`, `0xFF` | `i64` value |
| `FloatLiteral` | `3.14`, `.5`, `1e3` | `f64` value |
| `StringLiteral` | `"hello"`, `'world'` | `String` content (escapes resolved) |

### Variables and identifiers

| Token | Example | Carries |
|---|---|---|
| `Variable` | `$x`, `$name`, `$argc` | Name without the `$` |
| `This` | `$this` | Self-reference inside a class method |
| `Identifier` | `strlen`, `my_func` | The name |

### Keywords

```
echo  if  else  elseif  while  do  for  foreach  as
break  continue  function  return  include  require
include_once  require_once  true  false  null  print
switch  case  default  match  fn  use  extern  const
global  static  class  new  public  private  readonly
```

Each keyword is a distinct token variant (e.g., `Token::If`, `Token::While`, `Token::Switch`).

### Constants (keyword tokens)

```
INF  NAN  PHP_INT_MAX  PHP_INT_MIN  PHP_FLOAT_MAX  PHP_FLOAT_MIN  PHP_FLOAT_EPSILON
M_PI  M_E  M_SQRT2  M_PI_2  M_PI_4  M_LOG2E  M_LOG10E
PHP_EOL  PHP_OS  DIRECTORY_SEPARATOR
STDIN  STDOUT  STDERR
```

These are recognized as distinct tokens by the lexer, not as identifiers.

### Operators

```
+  -  *  **  /  %  .
=  =>  +=  -=  *=  /=  .=  %=
==  ===  !=  !==  <  >  <=  >=  <=>
&&  ||  !
&  |  ^  ~  <<  >>
??  ->  ::
++  --
...
```

### Structural

```
(  )  {  }  [  ]  ;  ,  ?  :
```

### Special

| Token | Meaning |
|---|---|
| `OpenTag` | `<?php` — required at the start of every file |
| `Eof` | End of file — signals the parser to stop |

## String interpolation

Double-quoted strings can contain variables:

```php
"Hello, $name!"
```

The lexer doesn't produce a single string token for this. Instead, it emits a sequence that the parser can assemble:

```
StringLiteral("Hello, ")
Dot                        (implicit concatenation)
Variable("name")
Dot
StringLiteral("!")
```

This is handled by `literals::scan_double_string_interpolated()`, which walks through the string character by character, splitting it whenever it encounters a `$` followed by a valid identifier.

## Escape sequences

In double-quoted strings, the lexer resolves escape sequences during scanning:

| Escape | Becomes |
|---|---|
| `\n` | Newline (0x0A) |
| `\t` | Tab (0x09) |
| `\r` | Carriage return (0x0D) |
| `\\` | Literal backslash |
| `\"` | Literal double quote |
| `\$` | Literal dollar sign |
| `\0` | Null byte (0x00) |
| `\e` | Escape (0x1B) |

Single-quoted strings only support `\\` and `\'` — everything else is literal.

## Heredoc and nowdoc

**File:** `src/lexer/literals.rs` — `scan_heredoc()`

Heredoc and nowdoc are multi-line string syntaxes from PHP. The lexer recognizes `<<<` followed by a label:

```php
$s = <<<EOT
Hello, $name!
Multiple lines here.
EOT;
```

The closing label must appear on its own line (optionally indented). The lexer handles three forms:

- **Heredoc** (`<<<LABEL`): processes escape sequences (like double-quoted strings) and produces a `StringLiteral` token
- **Quoted heredoc** (`<<<"LABEL"`): identical behavior to unquoted heredoc
- **Nowdoc** (`<<<'LABEL'`): no escape processing (like single-quoted strings)

The scanner reads everything between the opening label line and the closing label, strips any common leading indentation (matching the closing label's indent), and returns a single `StringLiteral` token. String interpolation within heredoc strings is handled the same way as in double-quoted strings.

Note that `<<<` is distinguished from `<<` (shift left) by checking whether the third character is also `<`.

## Error handling

If the scanner encounters something it can't tokenize — like an unterminated string or an invalid character — it returns a `CompileError` with the exact position (line and column). The error message is formatted by `src/errors/report.rs`.

## How it connects

The lexer's output — `Vec<(Token, Span)>` — is the input to the [parser](the-parser.md). Every token carries its position in the source, so later phases can point errors back to the exact line and column.

```
Source text → Lexer → [(Token, Span), (Token, Span), ...] → Parser
```

---

Next: [The Parser →](the-parser.md)
