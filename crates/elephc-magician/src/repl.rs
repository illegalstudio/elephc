//! Purpose:
//! Provides terminal input and parser-backed submission for the cached PHP REPL host.
//!
//! Called from:
//! - Private `__elephc_repl_*` extern calls in the compiler's embedded bootstrap.
//!
//! Key details:
//! - Only the PHP host executes code, through ordinary dynamic eval in one scope.
//! - Returned source is thread-local and stays valid until the next input request.

use std::cell::RefCell;
use std::ffi::{c_char, CString};
use std::io::{self, IsTerminal};
use std::path::PathBuf;

use rustyline::error::ReadlineError;
use rustyline::{Config, DefaultEditor};

use crate::parser::repl::{classify, Input};

const MAX_INPUT_BYTES: usize = 1024 * 1024;
const HELP: &str = "Enter PHP without <?php. Expressions display their result.\n\
Continue incomplete code at ...; Ctrl-C cancels the current input.\n\
:help shows this help; :quit or Ctrl-D exits.\n\
Eval errors return to the prompt; PHP exit() ends the session.\n";

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

/// Input state kept outside the PHP scope so commands cannot overwrite loop bookkeeping.
struct Session {
    editor: Option<DefaultEditor>,
    history: Option<PathBuf>,
    source: CString,
    submission: Option<bool>,
    status: i64,
}

impl Session {
    /// Initializes line editing only on a terminal and loads its bounded command history.
    fn new() -> Result<Self, String> {
        let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
        let mut editor = if interactive {
            // PHP output need not end with a newline. Start the next prompt on a fresh
            // line when necessary, before the editor clears and redraws its input row.
            let config = Config::builder().check_cursor_position(true)
                .history_ignore_space(true).max_history_size(1000)
                .map_err(|error| error.to_string())?.build();
            Some(DefaultEditor::with_config(config).map_err(|error| error.to_string())?)
        } else {
            None
        };
        let history = editor.as_ref().and_then(|_| std::env::var_os("ELEPHC_REPL_HISTORY"))
            .filter(|path| !path.is_empty()).map(PathBuf::from);
        if let (Some(editor), Some(path)) = (&mut editor, &history) {
            if path.exists() {
                if let Err(error) = editor.load_history(path) {
                    eprintln!("elephc repl: cannot load history: {error}");
                }
            }
        }
        if interactive && std::env::var_os("ELEPHC_REPL_QUIET").is_none() {
            println!("Elephc REPL. Type :help for help, :quit to exit.");
        }
        Ok(Self { editor, history, source: CString::default(), submission: None, status: 0 })
    }

    /// Reads complete submissions, recovering from parse errors without invoking eval.
    fn next(&mut self) -> bool {
        let mut buffer = String::new();
        loop {
            let line = match self.read_line(if buffer.is_empty() { ">>> " } else { "... " }) {
                Ok(Some(line)) => line,
                Ok(None) => {
                    if !buffer.is_empty() {
                        self.error("incomplete input at end of file");
                        self.status = 1;
                    }
                    return false;
                }
                Err(ReadlineError::Interrupted) => {
                    buffer.clear();
                    continue;
                }
                Err(error) => {
                    self.error(&format!("cannot read input: {error}"));
                    self.status = 1;
                    return false;
                }
            };
            if buffer.is_empty() {
                match line.trim() {
                    ":quit" | ":q" => return false,
                    ":help" | ":h" => { print!("{HELP}"); continue; }
                    _ => {}
                }
            }
            buffer.push_str(&line);
            buffer.push('\n');
            if buffer.len() > MAX_INPUT_BYTES {
                self.error("input exceeds the 1 MiB limit");
                buffer.clear();
                continue;
            }
            if buffer.contains('\0') {
                self.error("input contains a NUL byte");
                buffer.clear();
                continue;
            }
            match classify(&buffer) {
                Input::Empty => buffer.clear(),
                Input::Incomplete => {}
                Input::Invalid { error, line } => {
                    self.error(&format!("{} on input line {line}", parse_error_message(error)));
                    buffer.clear();
                }
                Input::Ready { source, display } => {
                    self.remember(&buffer);
                    self.source = CString::new(source).expect("input was checked for NUL");
                    self.submission = Some(display);
                    return true;
                }
            }
        }
    }

    /// Reports an input error and makes piped sessions fail after processing later commands.
    fn error(&mut self, message: &str) {
        eprintln!("elephc repl: {message}");
        if self.editor.is_none() { self.status = 1; }
    }

    /// Appends accepted interactive commands without overwriting another session's history.
    fn remember(&mut self, source: &str) {
        let Some(editor) = &mut self.editor else { return; };
        if let Err(error) = editor.add_history_entry(source.trim_end()) {
            eprintln!("elephc repl: cannot record history: {error}");
        }
        if let Some(path) = &self.history {
            if let Err(error) = editor.append_history(path) {
                eprintln!("elephc repl: cannot save history: {error}");
            }
        }
    }

    /// Avoids buffering piped PHP input past a newline, leaving stdin available to evaluated code.
    fn read_line(&mut self, prompt: &str) -> Result<Option<String>, ReadlineError> {
        if let Some(editor) = &mut self.editor {
            return match editor.readline(prompt) {
                Ok(line) => Ok(Some(line)),
                Err(ReadlineError::Eof) => Ok(None),
                Err(error) => Err(error),
            };
        }
        let mut bytes = Vec::new();
        loop {
            let mut byte = 0_u8;
            let read = unsafe { libc::read(libc::STDIN_FILENO, (&mut byte as *mut u8).cast(), 1) };
            if read < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted { continue; }
                return Err(error.into());
            }
            if read == 0 || byte == b'\n' {
                if read == 0 && bytes.is_empty() { return Ok(None); }
                return String::from_utf8(bytes).map(Some).map_err(|error| {
                    io::Error::new(io::ErrorKind::InvalidData, error).into()
                });
            }
            bytes.push(byte);
            if bytes.len() > MAX_INPUT_BYTES {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "input exceeds the 1 MiB limit").into());
            }
        }
    }
}

/// Translates parser categories into concise input diagnostics without exposing Rust enum names.
fn parse_error_message(error: crate::errors::EvalParseError) -> &'static str {
    use crate::errors::EvalParseError;
    match error {
        EvalParseError::UnsupportedConstruct => "construct is not supported by eval",
        EvalParseError::ExpectedSemicolon => "syntax error: expected a semicolon",
        EvalParseError::ExpectedVariable => "syntax error: expected a variable",
        EvalParseError::InvalidNumber => "syntax error: invalid number",
        EvalParseError::InvalidUtf8 => "input is not valid UTF-8",
        _ => "syntax error: unexpected token",
    }
}

/// Lazily initializes input and returns whether the native host should execute another fragment.
#[no_mangle]
pub extern "C" fn __elephc_repl_next() -> i64 {
    std::panic::catch_unwind(|| SESSION.with(|cell| {
        let mut state = cell.borrow_mut();
        if state.is_none() {
            match Session::new() {
                Ok(session) => *state = Some(session),
                Err(error) => { eprintln!("elephc repl: {error}"); return 0; }
            }
        }
        i64::from(state.as_mut().expect("initialized session").next())
    })).unwrap_or_else(|_| {
        eprintln!("elephc repl: input handler failed");
        SESSION.with(|cell| {
            if let Ok(mut session) = cell.try_borrow_mut() {
                if let Some(session) = session.as_mut() { session.status = 1; }
            }
        });
        0
    })
}

/// Returns a borrowed NUL-terminated fragment valid until the next `__elephc_repl_next` call.
#[no_mangle]
pub extern "C" fn __elephc_repl_source() -> *const c_char {
    std::panic::catch_unwind(|| SESSION.with(|cell| {
        cell.borrow().as_ref().map_or(c"".as_ptr(), |state| state.source.as_ptr())
    })).unwrap_or(c"".as_ptr())
}

/// Identifies the outer submission, including statements that do not display a result.
/// Nested eval calls must neither display intermediate values nor recover prematurely.
#[cfg(not(test))]
pub(crate) fn take_submission_request() -> Option<bool> {
    SESSION.with(|cell| {
        cell.borrow_mut().as_mut().and_then(|state| state.submission.take())
    })
}

/// Marks a caught PHP Throwable as a failed submission in a noninteractive session.
#[no_mangle]
pub extern "C" fn __elephc_repl_failed() {
    let _ = std::panic::catch_unwind(|| SESSION.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            if state.editor.is_none() { state.status = 1; }
        }
    }));
}

/// Returns the session exit status, failing closed if terminal initialization failed.
#[no_mangle]
pub extern "C" fn __elephc_repl_status() -> i64 {
    std::panic::catch_unwind(|| SESSION.with(|cell| {
        cell.borrow().as_ref().map_or(1, |state| state.status)
    })).unwrap_or(1)
}
