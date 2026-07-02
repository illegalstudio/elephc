//! Purpose:
//! Pure WebAssembly text (WAT) module builder for the wasm32-wasi backend.
//! Provides types and methods to construct WAT s-expressions without EIR dependency.
//!
//! Called from:
//! - `crate::codegen_wasm` module setup and the per-function lowering.
//!
//! Key details:
//! - No type section: signatures are inline on each func/import.
//! - All names are internal symbols without leading '$'; the builder adds '$' during render.
//! - Data byte escaping follows WAT conventions with \HH hex escapes.

use std::fmt::Write;

/// WebAssembly value type used to declare params, results, locals, and globals.
///
/// The wasm32-wasi backend models PHP values as i32 (pointers / linear-memory
/// offsets), i64 (PHP ints, string lengths, tagged payloads), and f64 (PHP
/// floats); f32 is never a value-type slot, so it is intentionally absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValType {
    I32,
    I64,
    F64,
}

impl ValType {
    /// Returns the WAT string representation of this value type.
    ///
    /// # Returns
    /// - `"i32"` for `I32`
    /// - `"i64"` for `I64`
    /// - `"f64"` for `F64`
    pub fn as_str(&self) -> &'static str {
        match self {
            ValType::I32 => "i32",
            ValType::I64 => "i64",
            ValType::F64 => "f64",
        }
    }
}

/// An imported function (e.g., WASI host calls).
pub struct FuncImport {
    /// The module name, e.g., "wasi_snapshot_preview1".
    pub module: String,
    /// The field name, e.g., "fd_write".
    pub field: String,
    /// The internal symbol name without leading '$', e.g., "wasi_fd_write".
    pub internal: String,
    /// Parameter types.
    pub params: Vec<ValType>,
    /// Result types.
    pub results: Vec<ValType>,
}

/// A module global variable.
pub struct Global {
    /// The internal symbol name without leading '$'.
    pub name: String,
    /// The value type.
    pub ty: ValType,
    /// Whether the global is mutable.
    pub mutable: bool,
    /// The constant initializer value.
    pub init: i64,
}

/// An active data segment placed at a constant byte offset in memory 0.
pub struct DataSegment {
    /// The byte offset in memory.
    pub offset: u32,
    /// The raw bytes to place.
    pub bytes: Vec<u8>,
}

/// A function definition under construction.
pub struct FuncBuilder {
    name: String,
    params: Vec<(String, ValType)>,
    results: Vec<ValType>,
    exports: Vec<String>,
    locals: Vec<(String, ValType)>,
    body: Vec<String>,
}

impl FuncBuilder {
    /// Creates a new function builder with the given internal name.
    ///
    /// # Arguments
    /// * `internal_name` - The function name without leading '$'.
    ///
    /// # Returns
    /// A new `FuncBuilder` ready to have parameters, locals, and instructions added.
    pub fn new(internal_name: &str) -> Self {
        FuncBuilder {
            name: internal_name.to_string(),
            params: Vec::new(),
            results: Vec::new(),
            exports: Vec::new(),
            locals: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Declares a parameter for this function.
    ///
    /// # Arguments
    /// * `name` - The parameter name without leading '$'.
    /// * `ty` - The value type of the parameter.
    ///
    /// # Returns
    /// The internal reference string `"$name"` for use in the function body.
    pub fn param(&mut self, name: &str, ty: ValType) -> String {
        self.params.push((name.to_string(), ty));
        format!("${}", name)
    }

    /// Appends a result type to this function's signature.
    ///
    /// # Arguments
    /// * `ty` - The value type of the result.
    pub fn result(&mut self, ty: ValType) {
        self.results.push(ty);
    }

    /// Marks this function for export with the given name.
    ///
    /// # Arguments
    /// * `export_name` - The export name, e.g., "_start".
    pub fn export(&mut self, export_name: &str) {
        self.exports.push(export_name.to_string());
    }

    /// Declares a local variable for this function.
    ///
    /// # Arguments
    /// * `name` - The local name without leading '$'.
    /// * `ty` - The value type of the local.
    ///
    /// # Returns
    /// The internal reference string `"$name"` for use in the function body.
    pub fn local(&mut self, name: &str, ty: ValType) -> String {
        self.locals.push((name.to_string(), ty));
        format!("${}", name)
    }

    /// Pushes an instruction line with an optional trailing comment.
    ///
    /// # Arguments
    /// * `code` - The WAT instruction code.
    /// * `comment` - A comment to append after `;;`. If empty, no comment is added.
    pub fn ins(&mut self, code: &str, comment: &str) {
        if comment.is_empty() {
            self.body.push(code.to_string());
        } else {
            self.body.push(format!("{}   ;; {}", code, comment));
        }
    }

    /// Pushes a raw line verbatim into the function body.
    ///
    /// Use for structural WAT like `(block ...)`, `(loop ...)`, `(if ...)`, `(else)`, `(end)`.
    ///
    /// # Arguments
    /// * `line` - The raw line to append.
    pub fn raw(&mut self, line: &str) {
        self.body.push(line.to_string());
    }

    /// Pushes a standalone comment line.
    ///
    /// # Arguments
    /// * `text` - The comment text (will be prefixed with `;;`).
    pub fn comment(&mut self, text: &str) {
        self.body.push(format!(";; {}", text));
    }

    /// Renders this function as a complete `(func ...)` s-expression.
    ///
    /// # Arguments
    /// * `indent` - The base indentation string for the entire function.
    ///
    /// # Returns
    /// A string containing the full `(func ...)` s-expression.
    pub(crate) fn render(&self, indent: &str) -> String {
        let mut out = String::new();

        // First line: (func $name (export "...")? (param $pname ty)* (result ty)*
        let _ = write!(out, "{}(func ${}", indent, self.name);
        for exp in &self.exports {
            let _ = write!(out, " (export \"{}\")", exp);
        }
        for (pname, ty) in &self.params {
            let _ = write!(out, " (param ${} {})", pname, ty.as_str());
        }
        for ty in &self.results {
            let _ = write!(out, " (result {})", ty.as_str());
        }
        let _ = writeln!(out);

        // Local declarations
        for (lname, ty) in &self.locals {
            let _ = writeln!(out, "{}  (local ${} {})", indent, lname, ty.as_str());
        }

        // Body lines
        for line in &self.body {
            let _ = writeln!(out, "{}  {}", indent, line);
        }

        // Closing paren
        let _ = writeln!(out, "{})", indent);

        out
    }
}

/// The whole module under construction.
pub struct WatModule {
    imports: Vec<FuncImport>,
    memory: Option<(u32, Option<String>)>,
    globals: Vec<Global>,
    data_segments: Vec<DataSegment>,
    functions: Vec<FuncBuilder>,
    /// Pre-written `(func ...)` s-expressions for the hand-authored WAT runtime.
    raw_functions: Vec<String>,
}

impl Default for WatModule {
    /// Returns an empty module, equivalent to `WatModule::new()`.
    fn default() -> Self {
        Self::new()
    }
}

impl WatModule {
    /// Creates a new empty WAT module.
    ///
    /// # Returns
    /// A new `WatModule` with no imports, default memory (1 page, exported as "memory"),
    /// no globals, no data, and no functions.
    pub fn new() -> Self {
        WatModule {
            imports: Vec::new(),
            memory: None,
            globals: Vec::new(),
            data_segments: Vec::new(),
            functions: Vec::new(),
            raw_functions: Vec::new(),
        }
    }

    /// Adds an imported function to this module.
    ///
    /// # Arguments
    /// * `imp` - The function import specification.
    pub fn import_func(&mut self, imp: FuncImport) {
        self.imports.push(imp);
    }

    /// Sets the memory configuration for this module.
    ///
    /// # Arguments
    /// * `min_pages` - The minimum number of 64KB pages.
    /// * `export_name` - Optional export name. If `None`, memory is not exported.
    ///
    /// If never called, defaults to 1 page exported as "memory".
    pub fn set_memory(&mut self, min_pages: u32, export_name: Option<&str>) {
        self.memory = Some((min_pages, export_name.map(|s| s.to_string())));
    }

    /// Adds a global variable to this module.
    ///
    /// # Arguments
    /// * `g` - The global specification.
    // Consumed by the WAT runtime (heap pointer/GC globals) in a later phase.
    #[allow(dead_code)]
    pub fn add_global(&mut self, g: Global) {
        self.globals.push(g);
    }

    /// Adds a data segment to this module.
    ///
    /// # Arguments
    /// * `seg` - The data segment specification.
    // Consumed by string-literal / runtime data emission in a later phase.
    #[allow(dead_code)]
    pub fn add_data(&mut self, seg: DataSegment) {
        self.data_segments.push(seg);
    }

    /// Adds a function definition to this module.
    ///
    /// # Arguments
    /// * `f` - The function builder to add.
    pub fn add_func(&mut self, f: FuncBuilder) {
        self.functions.push(f);
    }

    /// Adds a pre-written `(func ...)` s-expression (the hand-authored WAT runtime).
    ///
    /// # Arguments
    /// * `wat` - A complete `(func ...)` block. It is emitted verbatim in the
    ///   function section, so it must be valid WAT.
    pub fn add_raw_func(&mut self, wat: &str) {
        self.raw_functions.push(wat.to_string());
    }

    /// Renders this module as a complete `(module ...)` s-expression.
    ///
    /// # Returns
    /// A string containing the full WAT module, newline-terminated.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "(module");

        // 1. Imported functions
        for imp in &self.imports {
            let _ = write!(
                out,
                "  (import \"{}\" \"{}\" (func ${}",
                imp.module, imp.field, imp.internal
            );
            for ty in &imp.params {
                let _ = write!(out, " (param {})", ty.as_str());
            }
            for ty in &imp.results {
                let _ = write!(out, " (result {})", ty.as_str());
            }
            let _ = writeln!(out, "))");
        }

        // 2. Memory
        let (min_pages, export_name) = self
            .memory
            .clone()
            .unwrap_or((1, Some("memory".to_string())));
        if let Some(name) = export_name {
            let _ = writeln!(out, "  (memory (export \"{}\") {})", name, min_pages);
        } else {
            let _ = writeln!(out, "  (memory {})", min_pages);
        }

        // 3. Globals
        for g in &self.globals {
            if g.mutable {
                let _ = writeln!(
                    out,
                    "  (global ${} (mut {}) ({}.const {}))",
                    g.name,
                    g.ty.as_str(),
                    g.ty.as_str(),
                    g.init
                );
            } else {
                let _ = writeln!(
                    out,
                    "  (global ${} {} ({}.const {}))",
                    g.name,
                    g.ty.as_str(),
                    g.ty.as_str(),
                    g.init
                );
            }
        }

        // 4. Data segments
        for seg in &self.data_segments {
            let escaped = escape_wat_bytes(&seg.bytes);
            let _ = writeln!(out, "  (data (i32.const {}) \"{}\")", seg.offset, escaped);
        }

        // 5. Hand-authored raw runtime functions (emitted verbatim).
        for raw in &self.raw_functions {
            for line in raw.lines() {
                let _ = writeln!(out, "  {}", line);
            }
        }

        // 6. Lowered functions.
        for func in &self.functions {
            let rendered = func.render("  ");
            out.push_str(&rendered);
        }

        let _ = writeln!(out, ")");
        out
    }
}

/// Escapes bytes for WAT string literals.
///
/// Returns the content inside the quotes. Printable ASCII (0x20-0x7E) except
/// `"` and `\` is emitted as-is; all other bytes are emitted as `\HH` with
/// lowercase hex digits.
///
/// # Arguments
/// * `bytes` - The raw bytes to escape.
///
/// # Returns
/// The escaped string content (without surrounding quotes).
pub(crate) fn escape_wat_bytes(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        match b {
            // Printable ASCII except " and \
            0x20..=0x21 | 0x23..=0x5B | 0x5D..=0x7E => {
                out.push(b as char);
            }
            // Everything else: \HH
            _ => {
                let _ = write!(out, "\\{:02x}", b);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the WAT module/function builder and byte escaping.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Render tests assert key substrings rather than full text so the builder's
    //!   incidental whitespace can evolve without breaking the tests.

    use super::*;

    /// Verifies escaping of individual special bytes (quote, backslash, control, high byte).
    #[test]
    fn test_escape_wat_bytes_basic() {
        // Test printable ASCII
        assert_eq!(escape_wat_bytes(b"hello"), "hello");
        // Test quote
        assert_eq!(escape_wat_bytes(b"\""), "\\22");
        // Test backslash
        assert_eq!(escape_wat_bytes(b"\\"), "\\5c");
        // Test newline
        assert_eq!(escape_wat_bytes(&[0x0a]), "\\0a");
        // Test high byte
        assert_eq!(escape_wat_bytes(&[0xC3]), "\\c3");
    }

    /// Verifies a combined byte string escapes each special byte in order.
    #[test]
    fn test_escape_wat_bytes_combined() {
        // String with quote, backslash, newline, and high byte
        let input: Vec<u8> = vec![b'"', b'\\', 0x0a, 0xC3];
        let expected = "\\22\\5c\\0a\\c3";
        assert_eq!(escape_wat_bytes(&input), expected);
    }

    /// Verifies a full module renders the expected import/memory/global/func substrings.
    #[test]
    fn test_module_render() {
        let mut module = WatModule::new();

        // Add imported function
        module.import_func(FuncImport {
            module: "wasi_snapshot_preview1".to_string(),
            field: "fd_write".to_string(),
            internal: "wasi_fd_write".to_string(),
            params: vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
            results: vec![ValType::I32],
        });

        // Add global
        module.add_global(Global {
            name: "g".to_string(),
            ty: ValType::I64,
            mutable: true,
            init: 42,
        });

        // Add data segment
        module.add_data(DataSegment {
            offset: 0,
            bytes: b"hello".to_vec(),
        });

        // Add function
        let mut func = FuncBuilder::new("_start");
        func.export("_start");
        func.param("x", ValType::I32);
        func.result(ValType::I32);
        func.local("tmp", ValType::I64);
        func.ins("i32.const 0", "return value");
        func.ins("call $wasi_fd_write", "call imported function");
        func.raw("(block");
        func.ins("  i32.const 1", "inside block");
        func.raw(")");
        module.add_func(func);

        let rendered = module.render();

        // Check key substrings
        assert!(rendered.contains("(module"), "should contain (module");
        assert!(
            rendered.contains("(import \"wasi_snapshot_preview1\" \"fd_write\""),
            "should contain import"
        );
        assert!(
            rendered.contains("(memory (export \"memory\") 1)"),
            "should contain default memory"
        );
        assert!(rendered.contains("(global $g (mut i64)"), "should contain global");
        assert!(
            rendered.contains("(func $_start (export \"_start\")"),
            "should contain func with export"
        );
        assert!(rendered.contains("call $"), "should contain call instruction");
        assert!(rendered.contains("(param $x i32)"), "should contain param");
        assert!(rendered.contains("(result i32)"), "should contain result");
        assert!(rendered.contains("(local $tmp i64)"), "should contain local");
        assert!(rendered.ends_with(")\n"), "should end with )\\n");
    }

    /// Verifies each `ValType` maps to its WAT spelling.
    #[test]
    fn test_valtype_as_str() {
        assert_eq!(ValType::I32.as_str(), "i32");
        assert_eq!(ValType::I64.as_str(), "i64");
        assert_eq!(ValType::F64.as_str(), "f64");
    }

    /// Verifies `param` returns the `$name` reference form.
    #[test]
    fn test_func_builder_param_returns_dollar_name() {
        let mut func = FuncBuilder::new("test");
        let pname = func.param("x", ValType::I32);
        assert_eq!(pname, "$x");
    }

    /// Verifies `local` returns the `$name` reference form.
    #[test]
    fn test_func_builder_local_returns_dollar_name() {
        let mut func = FuncBuilder::new("test");
        let lname = func.local("tmp", ValType::I64);
        assert_eq!(lname, "$tmp");
    }

    /// Verifies an instruction with a comment renders the trailing `;;` form.
    #[test]
    fn test_func_builder_ins_with_comment() {
        let mut func = FuncBuilder::new("test");
        func.ins("i32.const 42", "answer");
        let rendered = func.render("");
        assert!(rendered.contains("i32.const 42   ;; answer"));
    }

    /// Verifies an instruction with an empty comment renders no `;;`.
    #[test]
    fn test_func_builder_ins_without_comment() {
        let mut func = FuncBuilder::new("test");
        func.ins("i32.const 42", "");
        let rendered = func.render("");
        assert!(rendered.contains("i32.const 42\n"));
        assert!(!rendered.contains(";;"));
    }

    /// Verifies a non-exported memory renders without an export clause.
    #[test]
    fn test_module_memory_no_export() {
        let mut module = WatModule::new();
        module.set_memory(2, None);
        let rendered = module.render();
        assert!(rendered.contains("(memory 2)"));
        assert!(!rendered.contains("(export"));
    }

    /// Verifies a custom memory export name renders correctly.
    #[test]
    fn test_module_memory_custom_export() {
        let mut module = WatModule::new();
        module.set_memory(4, Some("mem"));
        let rendered = module.render();
        assert!(rendered.contains("(memory (export \"mem\") 4)"));
    }

    /// Verifies an immutable global renders without the `(mut ...)` wrapper.
    #[test]
    fn test_immutable_global() {
        let mut module = WatModule::new();
        module.add_global(Global {
            name: "constant".to_string(),
            ty: ValType::I32,
            mutable: false,
            init: 123,
        });
        let rendered = module.render();
        assert!(rendered.contains("(global $constant i32 (i32.const 123))"));
        assert!(!rendered.contains("(mut i32)"));
    }
}
