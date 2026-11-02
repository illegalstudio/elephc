//! Purpose:
//! Lists the value-layout facts shared by the emulated engine and real-host SDK probes.
//!
//! Called from:
//! - `super::values::value_layout_facts()` and Elephc's PHP extension SDK probe generator.
//!
//! Key details:
//! - Contains no allocator, engine globals, version constants, or host bindings.
//! - The C field spelling is explicit where Rust represents an anonymous C union.

/// Expands the common value-layout contract through caller-supplied size and offset collectors.
macro_rules! value_layout {
    ($size:ident, $align:ident, $offset:ident) => {
        $size!(zval, zend_string, Bucket, HashTable);
        $align!(zval, zend_string, Bucket, HashTable);
        $offset!(zval: value => value, u1 => u1, u2 => u2);
        $offset!(zend_string: gc => gc, h => h, len => len, val => val);
        $offset!(Bucket: val => val, h => h, key => key);
        $offset!(zend_array: gc => gc, u => u, nTableMask => nTableMask,
            data => arData, nNumUsed => nNumUsed, nNumOfElements => nNumOfElements,
            nTableSize => nTableSize, nInternalPointer => nInternalPointer,
            nNextFreeElement => nNextFreeElement, pDestructor => pDestructor);
    };
}

pub(crate) use value_layout;
