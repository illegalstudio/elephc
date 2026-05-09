use super::*;

#[test]
fn test_error_file_ownership_builtins_reject_invalid_principals() {
    expect_error(
        r#"<?php chmod("file.txt", "0644");"#,
        "chmod() mode must be int",
    );
    expect_error(
        r#"<?php chown("file.txt", null);"#,
        "chown() owner/group must be int or string",
    );
    expect_error(
        r#"<?php chgrp("file.txt", null);"#,
        "chgrp() owner/group must be int or string",
    );
}

#[test]
fn test_error_umask_wrong_args() {
    expect_error("<?php umask(1, 2);", "umask() takes 0 or 1 arguments");
}

// --- v0.6: switch/match/array errors ---

#[test]
fn test_error_global_missing_var() {
    expect_error("<?php global ;", "Expected variable after 'global'");
}
