use super::{CompileError, CompileWarning};

pub fn print_error(error: &CompileError) {
    if error.span.line > 0 {
        eprintln!(
            "error[{}:{}]: {}",
            error.span.line, error.span.col, error.message
        );
    } else {
        eprintln!("error: {}", error.message);
    }
    for related in &error.related {
        print_error(related);
    }
}

pub fn print_warning(warning: &CompileWarning) {
    if warning.span.line > 0 {
        eprintln!(
            "warning[{}:{}]: {}",
            warning.span.line, warning.span.col, warning.message
        );
    } else {
        eprintln!("warning: {}", warning.message);
    }
}
