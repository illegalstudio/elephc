# Native references

This bridge owns the weak property type-source registry and the original-RHS
validation coordinator. Its versioned C exports are shared by native and eval
publishers. The compiler links `libelephc_reference.a` for the
`native_reference_validation` capability.

The crate uses Rust's standard library and imports no interpreter hooks. Native
callbacks must contain PHP exceptions before returning through Rust. Publishers
keep the original value and destination reference rooted throughout validation,
then publish or retire every owned candidate. Registry records never retain or
dereference receivers or cells.

The PHP surface remains in the compiler and eval backends. The bridge exposes
no PHP functions or classes. Unit coverage checks the C record layouts, weak
source lifecycle, independent coercion, conflicts, re-entry and rollback.
