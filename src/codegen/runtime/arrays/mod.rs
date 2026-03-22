mod heap_alloc;
mod array_new;
mod array_push_int;
mod array_push_str;
mod sort_int;

pub use heap_alloc::emit_heap_alloc;
pub use array_new::emit_array_new;
pub use array_push_int::emit_array_push_int;
pub use array_push_str::emit_array_push_str;
pub use sort_int::emit_sort_int;
