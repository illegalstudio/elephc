use super::CompileError;

pub fn print_error(error: &CompileError) {
    if error.span.line > 0 {
        eprintln!(
            "error[{}:{}]: {}",
            error.span.line, error.span.col, error.message
        );
    } else {
        eprintln!("error: {}", error.message);
    }
}
