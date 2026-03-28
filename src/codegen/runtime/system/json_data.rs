/// Emit JSON string constants for the data section.
pub(crate) fn emit_json_data() -> String {
    let mut out = String::new();
    out.push_str("_json_true:\n    .ascii \"true\"\n");
    out.push_str("_json_false:\n    .ascii \"false\"\n");
    out.push_str("_json_null:\n    .ascii \"null\"\n");
    out
}
