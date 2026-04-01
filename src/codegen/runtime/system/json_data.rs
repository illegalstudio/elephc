/// Emit JSON string constants for the data section.
pub(crate) fn emit_json_data() -> String {
    let mut out = String::new();
    out.push_str(".globl _json_true\n_json_true:\n    .ascii \"true\"\n");
    out.push_str(".globl _json_false\n_json_false:\n    .ascii \"false\"\n");
    out.push_str(".globl _json_null\n_json_null:\n    .ascii \"null\"\n");
    out
}
