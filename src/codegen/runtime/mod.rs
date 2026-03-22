mod arrays;
mod strings;
mod system;

use super::emit::Emitter;

pub fn emit_runtime(emitter: &mut Emitter) {
    strings::emit_itoa(emitter);
    strings::emit_ftoa(emitter);
    strings::emit_concat(emitter);
    strings::emit_atoi(emitter);
    strings::emit_str_eq(emitter);
    strings::emit_number_format(emitter);
    system::emit_build_argv(emitter);
    arrays::emit_heap_alloc(emitter);
    arrays::emit_array_new(emitter);
    arrays::emit_array_push_int(emitter);
    arrays::emit_array_push_str(emitter);
    arrays::emit_sort_int(emitter, false);
    arrays::emit_sort_int(emitter, true);
}

pub fn emit_runtime_data() -> String {
    let mut out = String::new();
    out.push_str(".comm _concat_buf, 65536, 3\n");
    out.push_str(".comm _concat_off, 8, 3\n");
    out.push_str(".comm _global_argc, 8, 3\n");
    out.push_str(".comm _global_argv, 8, 3\n");
    out.push_str(".comm _heap_buf, 1048576, 3\n");
    out.push_str(".comm _heap_off, 8, 3\n");
    out.push_str("_fmt_g:\n    .asciz \"%.14G\"\n");
    out
}
