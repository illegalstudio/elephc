use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("gettype()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        let (integer_label, integer_len) = data.add_string(b"integer");
        let (double_label, double_len) = data.add_string(b"double");
        let (string_label, string_len) = data.add_string(b"string");
        let (boolean_label, boolean_len) = data.add_string(b"boolean");
        let (null_label, null_len) = data.add_string(b"NULL");
        let (array_label, array_len) = data.add_string(b"array");
        let (object_label, object_len) = data.add_string(b"object");
        let integer_case = ctx.next_label("builtin_gettype_mixed_integer");
        let double_case = ctx.next_label("builtin_gettype_mixed_double");
        let string_case = ctx.next_label("builtin_gettype_mixed_string");
        let boolean_case = ctx.next_label("builtin_gettype_mixed_boolean");
        let null_case = ctx.next_label("builtin_gettype_mixed_null");
        let array_case = ctx.next_label("builtin_gettype_mixed_array");
        let object_case = ctx.next_label("builtin_gettype_mixed_object");
        let done = ctx.next_label("builtin_gettype_mixed_done");

        // -- mixed gettype() unwraps the payload and dispatches on its concrete runtime tag --
        emitter.instruction("bl __rt_mixed_unbox");                             // resolve the boxed payload tag before selecting the PHP type string
        emitter.instruction("cmp x0, #0");                                      // does the mixed payload hold an int?
        emitter.instruction(&format!("b.eq {}", integer_case));                 // ints map to PHP's integer type name
        emitter.instruction("cmp x0, #1");                                      // does the mixed payload hold a string?
        emitter.instruction(&format!("b.eq {}", string_case));                  // strings map to PHP's string type name
        emitter.instruction("cmp x0, #2");                                      // does the mixed payload hold a float?
        emitter.instruction(&format!("b.eq {}", double_case));                  // floats map to PHP's double type name
        emitter.instruction("cmp x0, #3");                                      // does the mixed payload hold a bool?
        emitter.instruction(&format!("b.eq {}", boolean_case));                 // bools map to PHP's boolean type name
        emitter.instruction("cmp x0, #4");                                      // does the mixed payload hold an indexed array?
        emitter.instruction(&format!("b.eq {}", array_case));                   // arrays map to PHP's array type name
        emitter.instruction("cmp x0, #5");                                      // does the mixed payload hold an associative array?
        emitter.instruction(&format!("b.eq {}", array_case));                   // associative arrays also map to array
        emitter.instruction("cmp x0, #6");                                      // does the mixed payload hold an object?
        emitter.instruction(&format!("b.eq {}", object_case));                  // objects map to PHP's object type name
        emitter.instruction(&format!("b {}", null_case));                       // null and unknown tags fall back to PHP's NULL type name

        emitter.label(&integer_case);
        emitter.instruction(&format!("adrp x1, {}@PAGE", integer_label));       // load page address of the integer type name
        emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", integer_label)); // resolve the integer type name address
        emitter.instruction(&format!("mov x2, #{}", integer_len));              // load the integer type name length
        emitter.instruction(&format!("b {}", done));                            // finish after selecting the integer type string

        emitter.label(&double_case);
        emitter.instruction(&format!("adrp x1, {}@PAGE", double_label));        // load page address of the double type name
        emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", double_label));  // resolve the double type name address
        emitter.instruction(&format!("mov x2, #{}", double_len));               // load the double type name length
        emitter.instruction(&format!("b {}", done));                            // finish after selecting the double type string

        emitter.label(&string_case);
        emitter.instruction(&format!("adrp x1, {}@PAGE", string_label));        // load page address of the string type name
        emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", string_label));  // resolve the string type name address
        emitter.instruction(&format!("mov x2, #{}", string_len));               // load the string type name length
        emitter.instruction(&format!("b {}", done));                            // finish after selecting the string type string

        emitter.label(&boolean_case);
        emitter.instruction(&format!("adrp x1, {}@PAGE", boolean_label));       // load page address of the boolean type name
        emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", boolean_label)); // resolve the boolean type name address
        emitter.instruction(&format!("mov x2, #{}", boolean_len));              // load the boolean type name length
        emitter.instruction(&format!("b {}", done));                            // finish after selecting the boolean type string

        emitter.label(&null_case);
        emitter.instruction(&format!("adrp x1, {}@PAGE", null_label));          // load page address of the NULL type name
        emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", null_label));    // resolve the NULL type name address
        emitter.instruction(&format!("mov x2, #{}", null_len));                 // load the NULL type name length
        emitter.instruction(&format!("b {}", done));                            // finish after selecting the NULL type string

        emitter.label(&array_case);
        emitter.instruction(&format!("adrp x1, {}@PAGE", array_label));         // load page address of the array type name
        emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", array_label));   // resolve the array type name address
        emitter.instruction(&format!("mov x2, #{}", array_len));                // load the array type name length
        emitter.instruction(&format!("b {}", done));                            // finish after selecting the array type string

        emitter.label(&object_case);
        emitter.instruction(&format!("adrp x1, {}@PAGE", object_label));        // load page address of the object type name
        emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", object_label));  // resolve the object type name address
        emitter.instruction(&format!("mov x2, #{}", object_len));               // load the object type name length
        emitter.label(&done);
        return Some(PhpType::Str);
    }

    let type_str = match &ty {
        PhpType::Int => "integer",
        PhpType::Float => "double",
        PhpType::Str => "string",
        PhpType::Bool => "boolean",
        PhpType::Void => "NULL",
        PhpType::Array(_) | PhpType::AssocArray { .. } => "array",
        PhpType::Callable => "callable",
        PhpType::Object(_) => "object",
        PhpType::Pointer(_) => "pointer",
        PhpType::Buffer(_) => "buffer",
        PhpType::Packed(_) => "packed",
        PhpType::Mixed | PhpType::Union(_) => unreachable!("mixed handled above"),
    };
    let (label, len) = data.add_string(type_str.as_bytes());
    emitter.instruction(&format!("adrp x1, {}@PAGE", label));                   // load page address of type name string
    emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", label));             // add page offset to get full address
    emitter.instruction(&format!("mov x2, #{}", len));                          // load string length into x2
    Some(PhpType::Str)
}
