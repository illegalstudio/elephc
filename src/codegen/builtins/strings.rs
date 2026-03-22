use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match name {
        "strlen" => {
            emitter.comment("strlen()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- return the string length as an integer --
            emitter.instruction("mov x0, x2");                                  // move string length to return register

            Some(PhpType::Int)
        }
        "intval" => {
            emitter.comment("intval()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty == PhpType::Str {
                // -- convert string to integer --
                emitter.instruction("bl __rt_atoi");                            // call runtime: parse string as integer into x0
            }
            Some(PhpType::Int)
        }
        "number_format" => {
            emitter.comment("number_format()");
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            // -- prepare the numeric value as a float --
            if t0 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert signed int to double-precision float
            emitter.instruction("str d0, [sp, #-16]!");                         // push float value onto stack (pre-decrement sp by 16)

            // -- prepare decimals argument --
            if args.len() >= 2 {
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("str x0, [sp, #-16]!");                     // push decimal places count onto stack
            } else {
                emitter.instruction("str xzr, [sp, #-16]!");                    // push 0 decimals (default) onto stack
            }

            // -- prepare decimal point character --
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                emitter.instruction("ldrb w0, [x1]");                           // load first byte of decimal separator string
                emitter.instruction("str x0, [sp, #-16]!");                     // push decimal separator char onto stack
            } else {
                emitter.instruction("mov x0, #46");                             // load ASCII '.' as default decimal separator
                emitter.instruction("str x0, [sp, #-16]!");                     // push default decimal separator onto stack
            }

            // -- prepare thousands separator character --
            if args.len() >= 4 {
                emit_expr(&args[3], emitter, ctx, data);
                emitter.instruction("cbz x2, 1f");                              // if separator string is empty, jump to use zero
                emitter.instruction("ldrb w0, [x1]");                           // load first byte of thousands separator string
                emitter.instruction("b 2f");                                    // skip over the zero-fallback
                emitter.raw("1:");
                emitter.instruction("mov x0, #0");                              // use zero (no separator) for empty string
                emitter.raw("2:");
                emitter.instruction("str x0, [sp, #-16]!");                     // push thousands separator onto stack
            } else {
                emitter.instruction("mov x0, #44");                             // load ASCII ',' as default thousands separator
                emitter.instruction("str x0, [sp, #-16]!");                     // push default thousands separator onto stack
            }

            // -- pop all args from stack into registers and call runtime --
            emitter.instruction("ldr x3, [sp], #16");                           // pop thousands separator into x3
            emitter.instruction("ldr x2, [sp], #16");                           // pop decimal separator into x2
            emitter.instruction("ldr x1, [sp], #16");                           // pop decimal places count into x1
            emitter.instruction("ldr d0, [sp], #16");                           // pop float value into d0
            emitter.instruction("bl __rt_number_format");                       // call runtime: format number as string

            Some(PhpType::Str)
        }
        "substr" => {
            emitter.comment("substr()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- save string and evaluate offset --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push string ptr and length onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");                         // push offset value onto stack
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                emitter.instruction("mov x3, x0");                              // move length argument to x3
            } else {
                emitter.instruction("mov x3, #-1");                             // set sentinel -1: use all remaining characters
            }
            // -- restore offset and string from stack --
            emitter.instruction("ldr x0, [sp], #16");                           // pop offset into x0
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop string ptr into x1, length into x2
            // -- handle negative offset --
            emitter.instruction("cmp x0, #0");                                  // check if offset is negative
            emitter.instruction("b.ge 1f");                                     // skip adjustment if offset >= 0
            emitter.instruction("add x0, x2, x0");                              // convert negative offset: offset = length + offset
            emitter.instruction("cmp x0, #0");                                  // check if adjusted offset is still negative
            emitter.instruction("csel x0, xzr, x0, lt");                        // clamp to 0 if offset went below zero
            emitter.raw("1:");
            // -- clamp offset to string length --
            emitter.instruction("cmp x0, x2");                                  // compare offset to string length
            emitter.instruction("csel x0, x2, x0, gt");                         // clamp offset to length if it exceeds it
            // -- adjust pointer and compute result length --
            emitter.instruction("add x1, x1, x0");                              // advance string pointer by offset bytes
            emitter.instruction("sub x2, x2, x0");                              // remaining = length - offset
            // -- apply optional length argument --
            emitter.instruction("cmn x3, #1");                                  // test if x3 == -1 (no length arg given)
            emitter.instruction("b.eq 2f");                                     // skip length clamping if no length arg
            emitter.instruction("cmp x3, #0");                                  // check if length arg is negative
            emitter.instruction("csel x3, xzr, x3, lt");                        // clamp negative length to 0
            emitter.instruction("cmp x3, x2");                                  // compare length arg to remaining chars
            emitter.instruction("csel x2, x3, x2, lt");                         // result length = min(length arg, remaining)
            emitter.raw("2:");

            Some(PhpType::Str)
        }
        "strpos" => {
            emitter.comment("strpos()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- save haystack, evaluate needle --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push haystack ptr and length onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move needle pointer to x3
            emitter.instruction("mov x4, x2");                                  // move needle length to x4
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop haystack ptr into x1, length into x2
            emitter.instruction("bl __rt_strpos");                              // call runtime: find needle in haystack, result in x0

            Some(PhpType::Int)
        }
        "strrpos" => {
            emitter.comment("strrpos()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- save haystack, evaluate needle --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push haystack ptr and length onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move needle pointer to x3
            emitter.instruction("mov x4, x2");                                  // move needle length to x4
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop haystack ptr into x1, length into x2
            emitter.instruction("bl __rt_strrpos");                             // call runtime: find last occurrence of needle, result in x0

            Some(PhpType::Int)
        }
        "strstr" => {
            emitter.comment("strstr()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- save haystack, evaluate needle --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push haystack ptr and length onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move needle pointer to x3
            emitter.instruction("mov x4, x2");                                  // move needle length to x4
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop haystack ptr into x1, length into x2
            // -- find needle position in haystack --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push haystack again (needed after strpos call)
            emitter.instruction("bl __rt_strpos");                              // call runtime: find needle position in haystack
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop saved haystack ptr and length
            // -- return substring from match position, or empty if not found --
            let found = ctx.next_label("strstr_found");
            emitter.instruction("cmp x0, #0");                                  // check if strpos returned a valid position
            emitter.instruction(&format!("b.ge {}", found));                    // branch to found if position >= 0
            emitter.instruction("mov x2, #0");                                  // set length to 0 (return empty string)
            let end = ctx.next_label("strstr_end");
            emitter.instruction(&format!("b {}", end));                         // jump to end, skipping found logic
            emitter.label(&found);
            emitter.instruction("add x1, x1, x0");                              // advance haystack ptr to match position
            emitter.instruction("sub x2, x2, x0");                              // result length = haystack length - position
            emitter.label(&end);

            Some(PhpType::Str)
        }
        "strtolower" => {
            emitter.comment("strtolower()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- convert all characters to lowercase --
            emitter.instruction("bl __rt_strtolower");                          // call runtime: lowercase string in-place, result in x1/x2

            Some(PhpType::Str)
        }
        "strtoupper" => {
            emitter.comment("strtoupper()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- convert all characters to uppercase --
            emitter.instruction("bl __rt_strtoupper");                          // call runtime: uppercase string in-place, result in x1/x2

            Some(PhpType::Str)
        }
        "ucfirst" => {
            emitter.comment("ucfirst()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- copy string then uppercase the first character --
            emitter.instruction("bl __rt_strcopy");                             // call runtime: copy string to mutable buffer
            emitter.instruction("cbz x2, 1f");                                  // skip if string is empty (length == 0)
            emitter.instruction("ldrb w9, [x1]");                               // load first byte of copied string
            emitter.instruction("cmp w9, #97");                                 // compare with ASCII 'a' (start of lowercase range)
            emitter.instruction("b.lt 1f");                                     // skip if char < 'a' (not lowercase)
            emitter.instruction("cmp w9, #122");                                // compare with ASCII 'z' (end of lowercase range)
            emitter.instruction("b.gt 1f");                                     // skip if char > 'z' (not lowercase)
            emitter.instruction("sub w9, w9, #32");                             // convert to uppercase by subtracting 32
            emitter.instruction("strb w9, [x1]");                               // store uppercased byte back to string
            emitter.raw("1:");

            Some(PhpType::Str)
        }
        "lcfirst" => {
            emitter.comment("lcfirst()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- copy string then lowercase the first character --
            emitter.instruction("bl __rt_strcopy");                             // call runtime: copy string to mutable buffer
            emitter.instruction("cbz x2, 1f");                                  // skip if string is empty (length == 0)
            emitter.instruction("ldrb w9, [x1]");                               // load first byte of copied string
            emitter.instruction("cmp w9, #65");                                 // compare with ASCII 'A' (start of uppercase range)
            emitter.instruction("b.lt 1f");                                     // skip if char < 'A' (not uppercase)
            emitter.instruction("cmp w9, #90");                                 // compare with ASCII 'Z' (end of uppercase range)
            emitter.instruction("b.gt 1f");                                     // skip if char > 'Z' (not uppercase)
            emitter.instruction("add w9, w9, #32");                             // convert to lowercase by adding 32
            emitter.instruction("strb w9, [x1]");                               // store lowercased byte back to string
            emitter.raw("1:");

            Some(PhpType::Str)
        }
        "trim" => {
            emitter.comment("trim()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- strip whitespace from both ends --
            emitter.instruction("bl __rt_trim");                                // call runtime: trim whitespace from both sides

            Some(PhpType::Str)
        }
        "ltrim" => {
            emitter.comment("ltrim()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- strip whitespace from the left --
            emitter.instruction("bl __rt_ltrim");                               // call runtime: trim whitespace from start of string

            Some(PhpType::Str)
        }
        "rtrim" => {
            emitter.comment("rtrim()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- strip whitespace from the right --
            emitter.instruction("bl __rt_rtrim");                               // call runtime: trim whitespace from end of string

            Some(PhpType::Str)
        }
        "str_repeat" => {
            emitter.comment("str_repeat()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- save string, evaluate repeat count --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push string ptr and length onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x0");                                  // move repeat count to x3
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop string ptr into x1, length into x2
            emitter.instruction("bl __rt_str_repeat");                          // call runtime: repeat string x3 times, result in x1/x2

            Some(PhpType::Str)
        }
        "strrev" => {
            emitter.comment("strrev()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- reverse the string --
            emitter.instruction("bl __rt_strrev");                              // call runtime: reverse string, result in x1/x2

            Some(PhpType::Str)
        }
        "ord" => {
            emitter.comment("ord()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- return ASCII value of first character --
            emitter.instruction("ldrb w0, [x1]");                               // load first byte from string ptr as unsigned int

            Some(PhpType::Int)
        }
        "chr" => {
            emitter.comment("chr()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- convert ASCII code to single-character string --
            emitter.instruction("bl __rt_chr");                                 // call runtime: write byte x0 to buffer, return x1/x2

            Some(PhpType::Str)
        }
        "strcmp" => {
            emitter.comment("strcmp()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- save first string, evaluate second --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push first string ptr and length onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move second string pointer to x3
            emitter.instruction("mov x4, x2");                                  // move second string length to x4
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop first string ptr into x1, length into x2
            emitter.instruction("bl __rt_strcmp");                              // call runtime: compare strings, result <0, 0, or >0

            Some(PhpType::Int)
        }
        "strcasecmp" => {
            emitter.comment("strcasecmp()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- save first string, evaluate second --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push first string ptr and length onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move second string pointer to x3
            emitter.instruction("mov x4, x2");                                  // move second string length to x4
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop first string ptr into x1, length into x2
            emitter.instruction("bl __rt_strcasecmp");                          // call runtime: case-insensitive compare, result <0, 0, or >0

            Some(PhpType::Int)
        }
        "str_contains" => {
            emitter.comment("str_contains()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- save haystack, evaluate needle --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push haystack ptr and length onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move needle pointer to x3
            emitter.instruction("mov x4, x2");                                  // move needle length to x4
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop haystack ptr into x1, length into x2
            emitter.instruction("bl __rt_strpos");                              // call runtime: find needle in haystack
            // -- convert strpos result to boolean --
            emitter.instruction("cmp x0, #0");                                  // check if position >= 0 (needle found)
            emitter.instruction("cset x0, ge");                                 // set x0 to 1 if found, 0 if not

            Some(PhpType::Bool)
        }
        "str_starts_with" => {
            emitter.comment("str_starts_with()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- save haystack, evaluate prefix --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push haystack ptr and length onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move prefix pointer to x3
            emitter.instruction("mov x4, x2");                                  // move prefix length to x4
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop haystack ptr into x1, length into x2
            emitter.instruction("bl __rt_str_starts_with");                     // call runtime: check if haystack starts with prefix

            Some(PhpType::Bool)
        }
        "str_ends_with" => {
            emitter.comment("str_ends_with()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- save haystack, evaluate suffix --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push haystack ptr and length onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move suffix pointer to x3
            emitter.instruction("mov x4, x2");                                  // move suffix length to x4
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop haystack ptr into x1, length into x2
            emitter.instruction("bl __rt_str_ends_with");                       // call runtime: check if haystack ends with suffix

            Some(PhpType::Bool)
        }
        "str_replace" => {
            emitter.comment("str_replace()");
            // str_replace($search, $replace, $subject)
            emit_expr(&args[0], emitter, ctx, data);
            // -- save search and replace strings, evaluate subject --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push search ptr and length onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push replace ptr and length onto stack
            emit_expr(&args[2], emitter, ctx, data);
            // -- arrange all args into registers for runtime call --
            emitter.instruction("mov x5, x1");                                  // move subject pointer to x5
            emitter.instruction("mov x6, x2");                                  // move subject length to x6
            emitter.instruction("ldp x3, x4, [sp], #16");                       // pop replace ptr into x3, length into x4
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop search ptr into x1, length into x2
            emitter.instruction("bl __rt_str_replace");                         // call runtime: replace all occurrences, result in x1/x2

            Some(PhpType::Str)
        }
        "explode" => {
            emitter.comment("explode()");
            // explode($delimiter, $string)
            emit_expr(&args[0], emitter, ctx, data);
            // -- save delimiter, evaluate string --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push delimiter ptr and length onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move string pointer to x3
            emitter.instruction("mov x4, x2");                                  // move string length to x4
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop delimiter ptr into x1, length into x2
            emitter.instruction("bl __rt_explode");                             // call runtime: split string by delimiter into array

            Some(PhpType::Array(Box::new(PhpType::Str)))
        }
        "implode" => {
            emitter.comment("implode()");
            // implode($glue, $array)
            emit_expr(&args[0], emitter, ctx, data);
            // -- save glue, evaluate array --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push glue ptr and length onto stack
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x0");                                  // move array pointer to x3
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop glue ptr into x1, length into x2
            emitter.instruction("bl __rt_implode");                             // call runtime: join array elements with glue string

            Some(PhpType::Str)
        }
        "ucwords" => {
            emitter.comment("ucwords()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_ucwords");                             // call runtime: uppercase first letter of each word
            Some(PhpType::Str)
        }
        "str_ireplace" => {
            emitter.comment("str_ireplace()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                    // push search string
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                    // push replace string
            emit_expr(&args[2], emitter, ctx, data);
            emitter.instruction("mov x5, x1");                                 // subject ptr
            emitter.instruction("mov x6, x2");                                 // subject len
            emitter.instruction("ldp x3, x4, [sp], #16");                      // pop replace
            emitter.instruction("ldp x1, x2, [sp], #16");                      // pop search
            emitter.instruction("bl __rt_str_ireplace");                        // call runtime: case-insensitive replace
            Some(PhpType::Str)
        }
        "substr_replace" => {
            emitter.comment("substr_replace()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                    // push subject string
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                    // push replacement string
            emit_expr(&args[2], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");                         // push offset
            if args.len() >= 4 {
                emit_expr(&args[3], emitter, ctx, data);
                emitter.instruction("mov x7, x0");                             // length arg
            } else {
                emitter.instruction("mov x7, #-1");                             // sentinel: replace to end
            }
            emitter.instruction("ldr x0, [sp], #16");                          // pop offset
            emitter.instruction("ldp x3, x4, [sp], #16");                      // pop replacement
            emitter.instruction("ldp x1, x2, [sp], #16");                      // pop subject
            // x1/x2=subject, x3/x4=replacement, x0=offset, x7=length
            emitter.instruction("bl __rt_substr_replace");                      // call runtime: replace substring
            Some(PhpType::Str)
        }
        "str_pad" => {
            emitter.comment("str_pad()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                    // push input string
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");                         // push target length
            // pad_string (arg 3, default " ")
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                emitter.instruction("stp x1, x2, [sp, #-16]!");                // push pad string
            } else {
                let (label, len) = data.add_string(b" ");
                emitter.instruction(&format!("adrp x1, {}@PAGE", label));       // load default pad string " "
                emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", label)); // resolve address
                emitter.instruction(&format!("mov x2, #{}", len));              // pad string length = 1
                emitter.instruction("stp x1, x2, [sp, #-16]!");                // push pad string
            }
            // pad_type (arg 4, default 1 = STR_PAD_RIGHT)
            if args.len() >= 4 {
                emit_expr(&args[3], emitter, ctx, data);
                emitter.instruction("mov x7, x0");                             // pad type
            } else {
                emitter.instruction("mov x7, #1");                              // STR_PAD_RIGHT
            }
            emitter.instruction("ldp x3, x4, [sp], #16");                      // pop pad string
            emitter.instruction("ldr x5, [sp], #16");                           // pop target length
            emitter.instruction("ldp x1, x2, [sp], #16");                      // pop input string
            // x1/x2=input, x3/x4=pad_str, x5=target_len, x7=pad_type
            emitter.instruction("bl __rt_str_pad");                             // call runtime: pad string
            Some(PhpType::Str)
        }
        "str_split" => {
            emitter.comment("str_split()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                    // push string
            if args.len() >= 2 {
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("mov x3, x0");                             // chunk length
            } else {
                emitter.instruction("mov x3, #1");                              // default chunk = 1
            }
            emitter.instruction("ldp x1, x2, [sp], #16");                      // pop string
            emitter.instruction("bl __rt_str_split");                           // call runtime: split string into chunks
            Some(PhpType::Array(Box::new(PhpType::Str)))
        }
        "addslashes" => {
            emitter.comment("addslashes()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_addslashes");                          // call runtime: escape quotes and backslashes
            Some(PhpType::Str)
        }
        "stripslashes" => {
            emitter.comment("stripslashes()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_stripslashes");                        // call runtime: remove escape backslashes
            Some(PhpType::Str)
        }
        "nl2br" => {
            emitter.comment("nl2br()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_nl2br");                               // call runtime: insert <br /> before newlines
            Some(PhpType::Str)
        }
        "wordwrap" => {
            emitter.comment("wordwrap()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                    // push string
            // width (arg 2, default 75)
            if args.len() >= 2 {
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("mov x3, x0");                             // width
            } else {
                emitter.instruction("mov x3, #75");                             // default width
            }
            // break string (arg 3, default "\n")
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                emitter.instruction("mov x4, x1");                             // break ptr
                emitter.instruction("mov x5, x2");                             // break len
            } else {
                let (label, len) = data.add_string(b"\n");
                emitter.instruction(&format!("adrp x4, {}@PAGE", label));       // load default break "\n"
                emitter.instruction(&format!("add x4, x4, {}@PAGEOFF", label)); // resolve address
                emitter.instruction(&format!("mov x5, #{}", len));              // break length = 1
            }
            emitter.instruction("ldp x1, x2, [sp], #16");                      // pop input string
            emitter.instruction("bl __rt_wordwrap");                            // call runtime: wrap text at word boundaries
            Some(PhpType::Str)
        }
        "bin2hex" => {
            emitter.comment("bin2hex()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_bin2hex");                             // call runtime: convert bytes to hex string
            Some(PhpType::Str)
        }
        "hex2bin" => {
            emitter.comment("hex2bin()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_hex2bin");                             // call runtime: convert hex string to bytes
            Some(PhpType::Str)
        }
        _ => None,
    }
}
