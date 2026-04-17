use super::{CompileError, CompileWarning};

pub fn print_error(error: &CompileError) {
    match (&error.file, error.span.line > 0) {
        (Some(file), true) => {
            eprintln!(
                "error[{}:{}:{}]: {}",
                file, error.span.line, error.span.col, error.message
            );
        }
        (Some(file), false) => {
            eprintln!("error[{}]: {}", file, error.message);
        }
        (None, true) => {
            eprintln!(
                "error[{}:{}]: {}",
                error.span.line, error.span.col, error.message
            );
        }
        (None, false) => {
            eprintln!("error: {}", error.message);
        }
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
