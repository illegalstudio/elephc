//! Purpose:
//! Supplies PHP's tokenizer functions and PhpToken through ordinary PHP declarations.
//!
//! Called from:
//! - The compilation pipeline and codegen harness before name resolution.
//!
//! Key details:
//! - The optional Rust bridge owns PHP lexing and TOKEN_PARSE validation.
//! - This prelude owns PHP arrays, objects, exceptions and snapshot lifetime on all targets.
//! - The shared usage scan covers names, imports, dynamic callables and opaque eval.

use crate::parser::ast::{Program, Stmt};

/// Parses the small declaration-only tokenizer surface in internal source mode.
pub(crate) fn tokenizer_declarations() -> Program {
    let tokens = crate::lexer::tokenize(TOKENIZER_PRELUDE_SRC).expect("tokenizer prelude must tokenize");
    crate::parser::parse_internal(&tokens).expect("tokenizer prelude must parse")
}

/// Injects tokenizer for a referenced symbol, a dynamic symbol consumer, or --with-tokenizer.
pub fn inject_if_used(
    program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        let used = force || program_uses_surface(&program);
        if !used { return program; }
        let mut declarations = tokenizer_declarations();
        inventory.record_program("tokenizer", &declarations);
        declarations.extend(program);
        declarations
    })
}

/// PHP surface shared by every target, compiled through AST, EIR and target-aware extern ABI.
pub(crate) const TOKENIZER_PRELUDE_SRC: &str = r#"<?php
extern "elephc_tokenizer" {
    function elephc_tokenizer_scan(string $source, int $len, int $flags): ptr;
    function elephc_tokenizer_count(ptr $scan): int;
    function elephc_tokenizer_field(ptr $scan, int $index, int $field): int;
    function elephc_tokenizer_error(ptr $scan): string;
    function elephc_tokenizer_error_line(ptr $scan): int;
    function elephc_tokenizer_release(ptr $scan): void;
    function elephc_tokenizer_name(int $id): string;
}

function token_get_all(string $code, int $flags = 0): array {
    $scan = elephc_tokenizer_scan($code, strlen($code), $flags);
    try {
        $count = elephc_tokenizer_count($scan);
        if ($count < 0) {
            $message = elephc_tokenizer_error($scan);
            $error = new \ParseError($message);
            __elephc_tokenizer_error_location($error, elephc_tokenizer_error_line($scan));
            throw $error;
        }
        $result = [];
        for ($i = 0; $i < $count; $i++) {
            $id = elephc_tokenizer_field($scan, $i, 0);
            $pos = elephc_tokenizer_field($scan, $i, 1);
            $len = elephc_tokenizer_field($scan, $i, 2);
            $line = elephc_tokenizer_field($scan, $i, 3);
            $text = substr($code, $pos, $len);
            if ($id < 256) {
                $result[] = $text;
            } else {
                $result[] = [$id, $text, $line];
            }
        }
        return $result;
    } finally {
        elephc_tokenizer_release($scan);
    }
}

function token_name(int $id): string {
    return elephc_tokenizer_name($id);
}

class PhpToken implements \Stringable {
    public int $id;
    public string $text;
    public int $line;
    public int $pos;

    final public function __construct(int $id, string $text, int $line = -1, int $pos = -1) {
        $this->id = $id;
        $this->text = $text;
        $this->line = $line;
        $this->pos = $pos;
    }

    public function getTokenName(): ?string {
        $id = $this->id;
        if ($id < 256) { return chr($id); }
        $name = token_name($id);
        if ($name === "UNKNOWN") { return null; }
        return $name;
    }

    public function is(int|string|array $kind): bool {
        if (is_int($kind)) { return $this->id === $kind; }
        if (is_string($kind)) { return $this->text === $kind; }
        if (is_array($kind)) {
            foreach ($kind as $entry) {
                if (is_int($entry)) {
                    if ($this->id === $entry) { return true; }
                } elseif (is_string($entry)) {
                    if ($this->text === $entry) { return true; }
                } else {
                    throw new \TypeError('PhpToken::is(): Argument #1 ($kind) must only have elements of type string|int, ' . (is_object($entry) ? get_class($entry) : (is_bool($entry) ? ($entry ? "true" : "false") : str_replace("double", "float", strtolower(gettype($entry))))) . " given");
                }
            }
            return false;
        }
        throw new \TypeError('PhpToken::is(): Argument #1 ($kind) must be of type string|int|array, ' . (is_object($kind) ? get_class($kind) : (is_bool($kind) ? ($kind ? "true" : "false") : str_replace("double", "float", strtolower(gettype($kind))))) . " given");
    }

    public function isIgnorable(): bool {
        $id = $this->id;
        return $id === T_WHITESPACE || $id === T_COMMENT || $id === T_DOC_COMMENT || $id === T_OPEN_TAG;
    }

    public function __toString(): string { return $this->text; }

    public static function tokenize(string $code, int $flags = 0): array {
        $called = static::class;
        if (__elephc_class_is_abstract($called)) { throw new \Error("Cannot instantiate abstract class " . $called); }
        $tokens = token_get_all($code, $flags);
        $result = [];
        $line = 1;
        $pos = 0;
        foreach ($tokens as $token) {
            if (is_array($token)) {
                $id = (int)$token[0];
                $text = (string)$token[1];
                $line = (int)$token[2];
            } else {
                $text = (string)$token;
                $id = ord($text[strlen($text) - 1]);
            }
            $result[] = new static($id, $text, $line, $pos);
            $pos += strlen($text);
            $line += substr_count($text, "\n") + substr_count($text, "\r") - substr_count($text, "\r\n");
        }
        return $result;
    }
}
"#;

/// Detects tokenizer references in entry files and the probed autoload closure.
pub(crate) fn program_uses_surface(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    usage.introspects || usage.dynamic_function_call || usage.constructs_dynamic_class
        || ["token_get_all", "token_name"].iter().any(|name| usage.references(name) || usage.literals.contains(*name))
        || usage.classes.contains("phptoken") || usage.literals.contains("phptoken")
}
