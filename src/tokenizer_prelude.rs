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
///
/// The source carries the program's compiled heap capacity because a tokenizer-only
/// program has no Core INI state; that capacity is the fallback `memory_limit`.
pub(crate) fn tokenizer_declarations(heap_size: usize, ini_source: IniMemorySource) -> Program {
    let body = match ini_source {
        IniMemorySource::Core => INI_MEMORY_LIMIT_BODY,
        IniMemorySource::None => "    return __ELEPHC_TOKENIZER_HEAP_LIMIT__;",
    };
    let source = TOKENIZER_PRELUDE_SRC
        .replace("__ELEPHC_TOKENIZER_MEMORY_LIMIT_BODY__", body)
        .replace(
            "__ELEPHC_TOKENIZER_HEAP_LIMIT__",
            &heap_size.to_string(),
        );
    let tokens = crate::lexer::tokenize(&source).expect("tokenizer prelude must tokenize");
    crate::parser::parse_internal(&tokens).expect("tokenizer prelude must parse")
}

/// Injects tokenizer and accounts for the web prelude's Core INI state.
pub fn inject_if_used_with_web(
    program: Program,
    force: bool,
    heap_size: usize,
    web: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        let used = force || program_uses_surface(&program);
        if !used { return program; }
        let mut declarations =
            tokenizer_declarations(heap_size, ini_memory_source(&program, web));
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
    function elephc_tokenizer_warning_count(ptr $scan): int;
    function elephc_tokenizer_warning_message(ptr $scan, int $index): string;
    function elephc_tokenizer_warning_line(ptr $scan, int $index): int;
    function elephc_tokenizer_warning_pos(ptr $scan, int $index): int;
    function elephc_tokenizer_warning_severity(ptr $scan, int $index): int;
    function elephc_tokenizer_release(ptr $scan): void;
    function elephc_tokenizer_name(int $id): string;
}

function __elephc_tokenizer_memory_limit(): int {
__ELEPHC_TOKENIZER_MEMORY_LIMIT_BODY__
}

function token_get_all(string $code, int $flags = 0): array {
    $scan = elephc_tokenizer_scan($code, strlen($code), $flags);
    try {
        $count = elephc_tokenizer_count($scan);
        if ($count < 0) {
            // PHP emits the warnings that precede a parse failure before throwing.
            $warningCount = elephc_tokenizer_warning_count($scan);
            for ($w = 0; $w < $warningCount; $w++) {
                $message = elephc_tokenizer_warning_message($scan, $w);
                if (elephc_tokenizer_warning_severity($scan, $w) === 2) {
                    __elephc_builtin_warning_at($message, elephc_tokenizer_warning_line($scan, $w));
                } else {
                    __elephc_builtin_deprecated_at($message, elephc_tokenizer_warning_line($scan, $w));
                }
            }
            $message = elephc_tokenizer_error($scan);
            // A nested __halt_compiler() is a CompileError. ParseError extends that class, so
            // catch (ParseError) must not observe it. The message is the only class selector.
            if ($message === '__HALT_COMPILER() can only be used from the outermost scope') {
                $error = new \CompileError($message);
            } else {
                $error = new \ParseError($message);
            }
            __elephc_tokenizer_error_location($error, elephc_tokenizer_error_line($scan));
            throw $error;
        }
        $limit = __elephc_tokenizer_memory_limit();
        // Approximation of PHP's token-array footprint: a flat per-token cost that covers the
        // array slot and one-character tokens. It is a guard, not an allocator.
        $need = strlen($code) + $count * 256;
        if ($limit >= 0 && $need > $limit) {
            $frames = debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 4);
            $origin = $frames[0] ?? [];
            foreach ($frames as $frame) {
                if (($frame['class'] ?? '') === 'PhpToken' && ($frame['function'] ?? '') === 'tokenize') {
                    $origin = $frame;
                    break;
                }
            }
            echo "Fatal error: Allowed memory size of ", $limit, " bytes exhausted (tried to allocate ", $need, " bytes) in ", ($origin['file'] ?? 'unknown'), " on line ", ($origin['line'] ?? 0), "\n";
            exit(255);
        }
        $warningCount = elephc_tokenizer_warning_count($scan);
        for ($w = 0; $w < $warningCount; $w++) {
            $message = elephc_tokenizer_warning_message($scan, $w);
            if (elephc_tokenizer_warning_severity($scan, $w) === 2) {
                __elephc_builtin_warning_at($message, elephc_tokenizer_warning_line($scan, $w));
            } else {
                __elephc_builtin_deprecated_at($message, elephc_tokenizer_warning_line($scan, $w));
            }
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

    // Declared `mixed` so this prelude method can reproduce the internal
    // `Z_PARAM_ARRAY_OR_STR_OR_LONG` rejection of bool/float/null/object. A
    // userland union type would weakly coerce those values before the body ran.
    public function is(mixed $kind): bool {
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

/// Reads the shared allocator-backed INI value and parses its quantity.
const INI_MEMORY_LIMIT_BODY: &str = r#"    $limit = __elephc_memory_limit_raw();
    if ($limit === '' || $limit === '-1') { return -1; }
    return __elephc_ini_quantity($limit);"#;

/// How the tokenizer prelude can read PHP's active `memory_limit`.
pub(crate) enum IniMemorySource {
    /// The shared Core INI prelude provides the allocator-backed value.
    Core,
    /// No Core INI surface, so the compiled heap capacity is the only limit.
    None,
}

/// Uses the internal Core INI store when the CLI or web prelude provides it.
fn ini_memory_source(program: &[Stmt], web: bool) -> IniMemorySource {
    if web
        || program.iter().any(|stmt| {
            stmt.source_mode == crate::source::SourceMode::Internal
                && matches!(
                    &stmt.kind,
                    crate::parser::ast::StmtKind::FunctionDecl { name, .. }
                        if name == "__elephc_memory_limit_raw"
                )
        })
    {
        return IniMemorySource::Core;
    }
    IniMemorySource::None
}

/// Detects tokenizer references in entry files and the probed autoload closure.
///
/// Returns whether the program needs the tokenizer prelude, dynamic callers included.
pub(crate) fn program_uses_surface(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    usage.introspects || usage.dynamic_function_call || usage.constructs_dynamic_class
        || ["token_get_all", "token_name"].iter().any(|name| usage.references(name) || usage.literals.contains(*name))
        || usage.classes.contains("phptoken") || usage.literals.contains("phptoken")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses a declaration-only program the way the prelude injector does.
    fn internal_program(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        crate::parser::parse_internal(&tokens).expect("parse")
    }

    /// The memory guard chooses the shared limit for CLI and web INI state.
    #[test]
    fn memory_guard_reads_core_ini_for_every_injected_ini_surface() {
        let core = internal_program(
            "<?php function __elephc_memory_limit_raw(): string { return '4M'; } token_get_all('<?php ');",
        );
        assert!(matches!(ini_memory_source(&core, false), IniMemorySource::Core));

        let none = internal_program("<?php echo 1;");
        assert!(matches!(ini_memory_source(&none, false), IniMemorySource::None));
        assert!(matches!(ini_memory_source(&none, true), IniMemorySource::Core));

        // A computed tokenizer call still reaches the injected prelude at runtime, so the
        // guard must keep reading the declared Core INI surface.
        let dynamic = internal_program(
            "<?php function __elephc_memory_limit_raw(): string { return '4M'; } eval($code);",
        );
        assert!(matches!(ini_memory_source(&dynamic, false), IniMemorySource::Core));
    }
}
