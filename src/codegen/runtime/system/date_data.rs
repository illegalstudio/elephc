/// Emit day and month name lookup tables as data.
pub(crate) fn emit_date_data() -> String {
    let mut out = String::new();
    // Day names: 7 entries, each 12 bytes (10 chars + 1 length + 1 padding)
    // Sunday=0, Monday=1, ..., Saturday=6
    out.push_str(".globl _day_names\n_day_names:\n");
    let days = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    for day in &days {
        let mut padded = day.to_string();
        while padded.len() < 10 {
            padded.push('\0');
        }
        out.push_str(&format!("    .ascii \"{}\"\n", padded.replace('\0', "\\0")));
        out.push_str(&format!("    .byte {}\n", day.len()));
        out.push_str("    .byte 0\n");
    }

    // Month names: 12 entries, each 12 bytes (10 chars + 1 length + 1 padding)
    // January=0, ..., December=11
    out.push_str(".globl _month_names\n_month_names:\n");
    let months = ["January", "February", "March", "April", "May", "June",
                  "July", "August", "September", "October", "November", "December"];
    for month in &months {
        let mut padded = month.to_string();
        while padded.len() < 10 {
            padded.push('\0');
        }
        out.push_str(&format!("    .ascii \"{}\"\n", padded.replace('\0', "\\0")));
        out.push_str(&format!("    .byte {}\n", month.len()));
        out.push_str("    .byte 0\n");
    }

    out
}
