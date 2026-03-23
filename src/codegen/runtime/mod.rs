mod arrays;
mod io;
mod strings;
mod system;

use super::emit::Emitter;

pub fn emit_runtime(emitter: &mut Emitter) {
    // String runtime functions
    strings::emit_itoa(emitter);
    strings::emit_ftoa(emitter);
    strings::emit_concat(emitter);
    strings::emit_atoi(emitter);
    strings::emit_str_eq(emitter);
    strings::emit_number_format(emitter);
    strings::emit_strcopy(emitter);
    strings::emit_strtolower(emitter);
    strings::emit_strtoupper(emitter);
    strings::emit_trim(emitter);
    strings::emit_ltrim(emitter);
    strings::emit_rtrim(emitter);
    strings::emit_strpos(emitter);
    strings::emit_strrpos(emitter);
    strings::emit_str_repeat(emitter);
    strings::emit_strrev(emitter);
    strings::emit_chr(emitter);
    strings::emit_strcmp(emitter);
    strings::emit_strcasecmp(emitter);
    strings::emit_str_starts_with(emitter);
    strings::emit_str_ends_with(emitter);
    strings::emit_str_replace(emitter);
    strings::emit_explode(emitter);
    strings::emit_implode(emitter);
    strings::emit_ucwords(emitter);
    strings::emit_str_ireplace(emitter);
    strings::emit_substr_replace(emitter);
    strings::emit_str_pad(emitter);
    strings::emit_str_split(emitter);
    strings::emit_addslashes(emitter);
    strings::emit_stripslashes(emitter);
    strings::emit_nl2br(emitter);
    strings::emit_wordwrap(emitter);
    strings::emit_bin2hex(emitter);
    strings::emit_hex2bin(emitter);
    strings::emit_htmlspecialchars(emitter);
    strings::emit_html_entity_decode(emitter);
    strings::emit_urlencode(emitter);
    strings::emit_urldecode(emitter);
    strings::emit_rawurlencode(emitter);
    strings::emit_base64_encode(emitter);
    strings::emit_base64_decode(emitter);
    strings::emit_sprintf(emitter);
    strings::emit_md5(emitter);
    strings::emit_sha1(emitter);
    strings::emit_hash(emitter);
    strings::emit_sscanf(emitter);

    // System runtime functions
    system::emit_build_argv(emitter);

    // Array runtime functions
    arrays::emit_heap_alloc(emitter);
    arrays::emit_array_new(emitter);
    arrays::emit_array_push_int(emitter);
    arrays::emit_array_push_str(emitter);
    arrays::emit_sort_int(emitter, false);
    arrays::emit_sort_int(emitter, true);
    arrays::emit_hash_fnv1a(emitter);
    arrays::emit_hash_new(emitter);
    arrays::emit_hash_set(emitter);
    arrays::emit_hash_get(emitter);
    arrays::emit_hash_iter(emitter);
    arrays::emit_hash_count(emitter);
    arrays::emit_array_key_exists(emitter);
    arrays::emit_array_search(emitter);
    arrays::emit_array_reverse(emitter);
    arrays::emit_array_sum(emitter);
    arrays::emit_array_product(emitter);
    arrays::emit_array_shift(emitter);
    arrays::emit_array_unshift(emitter);
    arrays::emit_array_merge(emitter);
    arrays::emit_array_slice(emitter);
    arrays::emit_range(emitter);
    arrays::emit_shuffle(emitter);
    arrays::emit_array_unique(emitter);
    arrays::emit_array_rand(emitter);
    arrays::emit_array_fill(emitter);
    arrays::emit_array_pad(emitter);
    arrays::emit_array_diff(emitter);
    arrays::emit_array_intersect(emitter);
    arrays::emit_array_flip(emitter);
    arrays::emit_array_combine(emitter);
    arrays::emit_array_fill_keys(emitter);
    arrays::emit_array_chunk(emitter);
    arrays::emit_array_column(emitter);
    arrays::emit_array_splice(emitter);
    arrays::emit_array_diff_key(emitter);
    arrays::emit_array_intersect_key(emitter);
    arrays::emit_asort(emitter);
    arrays::emit_ksort(emitter);
    arrays::emit_natsort(emitter);

    // I/O runtime functions
    io::emit_cstr(emitter);
    io::emit_fopen(emitter);
    io::emit_fgets(emitter);
    io::emit_feof(emitter);
    io::emit_fread(emitter);
    io::emit_file_get_contents(emitter);
    io::emit_file_put_contents(emitter);
    io::emit_file(emitter);
    io::emit_stat(emitter);
    io::emit_fs(emitter);
    io::emit_getcwd(emitter);
    io::emit_scandir(emitter);
    io::emit_glob(emitter);
    io::emit_tempnam(emitter);
    io::emit_fgetcsv(emitter);
    io::emit_fputcsv(emitter);
}

pub fn emit_runtime_data() -> String {
    let mut out = String::new();
    out.push_str(".comm _concat_buf, 65536, 3\n");
    out.push_str(".comm _concat_off, 8, 3\n");
    out.push_str(".comm _global_argc, 8, 3\n");
    out.push_str(".comm _global_argv, 8, 3\n");
    out.push_str(".comm _heap_buf, 1048576, 3\n");
    out.push_str(".comm _heap_off, 8, 3\n");
    out.push_str(".comm _cstr_buf, 4096, 3\n");
    out.push_str(".comm _cstr_buf2, 4096, 3\n");
    out.push_str(".comm _eof_flags, 256, 3\n");
    out.push_str("_fmt_g:\n    .asciz \"%.14G\"\n");
    // Base64 encode lookup table (A-Z, a-z, 0-9, +, /)
    out.push_str("_b64_encode_tbl:\n    .ascii \"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/\"\n");
    // Base64 decode lookup table (256 bytes, maps ASCII value to 6-bit value)
    out.push_str("_b64_decode_tbl:\n");
    let mut decode_tbl = vec![0u8; 256];
    for (i, &c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/".iter().enumerate() {
        decode_tbl[c as usize] = i as u8;
    }
    out.push_str("    .byte ");
    for (i, val) in decode_tbl.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&val.to_string());
    }
    out.push('\n');
    out
}
