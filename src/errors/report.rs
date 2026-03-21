use super::CompileError;

pub fn print_error(error: &CompileError) {
    if error.line > 0 {
        eprintln!(
            "error[{}:{}]: {}",
            error.line, error.col, error.message
        );
    } else {
        eprintln!("error: {}", error.message);
    }
}
