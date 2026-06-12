//! Purpose:
//! Emits the `__rt_date` / `__rt_gmdate` runtime helper assembly for Linux x86_64.
//! `__rt_gmdate` shares the formatter body and only swaps `localtime` for `gmtime` (UTC).
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::system::date::emit_date()` for Linux x86_64 targets.
//!
//! Key details:
//! - Formatting reads libc tm fields and fixed date tables using Linux x86_64 register conventions.

use crate::codegen::emit::Emitter;
use crate::codegen::abi;

/// Emits the `__rt_date` and `__rt_gmdate` runtime helpers for Linux x86_64.
///
/// Both entry points share one body; `__rt_gmdate` sets the UTC flag so the timestamp
/// is decomposed with `gmtime` (UTC), while `__rt_date` uses `localtime`.
///
/// # Input ABI
/// - `rax`: Unix timestamp (i64); pass -1 to query current time via libc `time()`.
/// - `rdi`: pointer to the PHP date format string.
/// - `rsi`: byte length of the format string.
///
/// # Output ABI
/// - `rax`: pointer to the formatted date string inside the concat buffer.
/// - `rdx`: byte length of the formatted string.
///
/// # Behavior
/// Decomposes the timestamp via libc `localtime()`, then scans the format string
/// byte-by-byte dispatching each token ('Y', 'y', 'm', 'n', 'd', 'j', 'D', 'l',
/// 'N', 'w', 'F', 'M', 'H', 'G', 'h', 'g', 'i', 's', 'A', 'a', 'U', 'S', 'z', 't',
/// 'L', 'W', 'o') to a dedicated helper that appends the formatted field to the shared concat
/// buffer. A backslash escapes the next byte so it is emitted literally (a lone trailing
/// backslash emits nothing); other non-token bytes are copied verbatim. The global
/// concat-buffer offset is updated atomically before return.
///
/// # Local frame layout (128 bytes, aligned)
/// - `[rbp - 8]`: saved timestamp (rax)
/// - `[rbp - 16]`: saved format string pointer (rdi)
/// - `[rbp - 24]`: saved format string length (rsi)
/// - `[rbp - 32]`: pointer to libc `struct tm` returned by `localtime()`
/// - `[rbp - 40]`: live concat-buffer write cursor
/// - `[rbp - 48]`: formatted-string start pointer (returned as result)
/// - `[rbp - 56]`: format string scan index
/// - `[rbp - 64]`: original concat-buffer offset (for global update)
/// - `[rbp - 96..rbp - 128]`: scratch area for decimal digit staging in `__rt_date_write_int64_linux_x86_64`
pub(super) fn emit_date_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: date / gmdate ---");
    // gmdate() shares this formatter, entering with the UTC flag set so the timestamp
    // is decomposed with gmtime() instead of localtime().
    emitter.label_global("__rt_gmdate");
    emitter.instruction("mov r10d, 1");                                         // select UTC decomposition (gmtime)
    emitter.instruction("jmp __rt_date_entry_linux_x86_64");                    // share the formatter body
    emitter.label_global("__rt_date");
    emitter.instruction("mov r10d, 0");                                         // select local decomposition (localtime)
    emitter.label("__rt_date_entry_linux_x86_64");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the date formatter uses stack-backed locals and helpers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved timestamp, format metadata, and decimal scratch buffer
    emitter.instruction("sub rsp, 160");                                        // formatter locals + decimal scratch + the c/r format-include save slots

    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the requested Unix timestamp so helper paths can reload it after libc and formatting calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the format-string pointer so the main loop can reload it without depending on caller-saved registers
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the format-string length so the loop bound survives helper calls
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // save the UTC-vs-local decomposition flag across the libc calls below
    emitter.instruction("cmp rax, -1");                                         // check whether the builtin requested "current time" instead of an explicit timestamp
    emitter.instruction("jne __rt_date_have_time_linux_x86_64");                // skip the libc time() query when the caller already supplied an explicit Unix timestamp
    emitter.instruction("xor edi, edi");                                        // pass NULL to libc time() so it only returns the current Unix timestamp value
    emitter.instruction("call time");                                           // query libc for the current Unix timestamp when PHP date() was called without an explicit timestamp
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // store the current Unix timestamp so the rest of the formatter can treat both code paths uniformly

    emitter.label("__rt_date_have_time_linux_x86_64");
    emitter.instruction("call __rt_tz_init_utc");                               // default the timezone to UTC once the timestamp is resolved (PHP-compatible) unless set
    emitter.instruction("lea rdi, [rbp - 8]");                                  // pass a pointer to the saved Unix timestamp as the first argument to libc localtime()/gmtime()
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the UTC-vs-local decomposition flag
    emitter.instruction("cmp rax, 0");                                          // check whether UTC decomposition was requested
    emitter.instruction("jne __rt_date_use_gmtime_linux_x86_64");               // nonzero flag → decompose as UTC
    emitter.instruction("call localtime");                                      // decompose the Unix timestamp into libc's struct tm fields in the current local timezone
    emitter.instruction("jmp __rt_date_decomposed_linux_x86_64");               // skip the UTC decomposition path
    emitter.label("__rt_date_use_gmtime_linux_x86_64");
    emitter.instruction("call gmtime");                                         // decompose the Unix timestamp into libc's struct tm fields in UTC
    emitter.label("__rt_date_decomposed_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the returned struct tm pointer so each format-token branch can reload the decomposed calendar fields

    abi::emit_load_symbol_to_reg(emitter, "r8", "_concat_off", 0);              // load the current concat-buffer offset before appending the formatted date output
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // preserve the original concat-buffer offset for the final global offset update
    abi::emit_symbol_address(emitter, "r9", "_concat_buf");                     // load the base address of the shared concat buffer used for transient string results
    emitter.instruction("add r9, r8");                                          // compute the initial write cursor inside the concat buffer from the saved relative offset
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save the live write cursor so every token helper can append to the same destination buffer
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the formatted string start pointer for the final return value
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // start scanning the format string at byte index zero
    emitter.instruction("mov QWORD PTR [rbp - 136], 0");                        // format-include save slot empty (not inside a c/r sub-format)

    emitter.label("__rt_date_loop_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the current format-string byte index before checking for loop completion
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // stop once the byte index reaches the saved format length
    emitter.instruction("jae __rt_date_check_pop_linux_x86_64");                // reached the end: pop a c/r sub-format or finish
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the format-string pointer before reading the current format character
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // load the current format character as an unsigned byte for the token dispatch ladder

    emitter.instruction("cmp al, 92");                                          // check whether the current token is '\\' which escapes the next character
    emitter.instruction("je __rt_date_escape_linux_x86_64");                    // emit the next character literally through the escape helper path
    emitter.instruction("cmp al, 89");                                          // check whether the current token is 'Y' for a four-digit Gregorian year
    emitter.instruction("je __rt_date_fmt_Y_linux_x86_64");                     // handle the four-digit Gregorian year token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 109");                                         // check whether the current token is 'm' for a zero-padded month number
    emitter.instruction("je __rt_date_fmt_m_linux_x86_64");                     // handle the zero-padded month token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 100");                                         // check whether the current token is 'd' for a zero-padded day-of-month number
    emitter.instruction("je __rt_date_fmt_d_linux_x86_64");                     // handle the zero-padded day token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 72");                                          // check whether the current token is 'H' for a zero-padded 24-hour clock value
    emitter.instruction("je __rt_date_fmt_H_linux_x86_64");                     // handle the zero-padded 24-hour token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 105");                                         // check whether the current token is 'i' for a zero-padded minute value
    emitter.instruction("je __rt_date_fmt_i_linux_x86_64");                     // handle the zero-padded minute token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 115");                                         // check whether the current token is 's' for a zero-padded second value
    emitter.instruction("je __rt_date_fmt_s_linux_x86_64");                     // handle the zero-padded second token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 106");                                         // check whether the current token is 'j' for an unpadded day-of-month number
    emitter.instruction("je __rt_date_fmt_j_linux_x86_64");                     // handle the unpadded day token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 110");                                         // check whether the current token is 'n' for an unpadded month number
    emitter.instruction("je __rt_date_fmt_n_linux_x86_64");                     // handle the unpadded month token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 71");                                          // check whether the current token is 'G' for an unpadded 24-hour clock value
    emitter.instruction("je __rt_date_fmt_G_linux_x86_64");                     // handle the unpadded 24-hour token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 103");                                         // check whether the current token is 'g' for an unpadded 12-hour clock value
    emitter.instruction("je __rt_date_fmt_g_linux_x86_64");                     // handle the unpadded 12-hour token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 78");                                          // check whether the current token is 'N' for the ISO weekday number
    emitter.instruction("je __rt_date_fmt_N_linux_x86_64");                     // handle the ISO weekday token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 65");                                          // check whether the current token is 'A' for the uppercase AM/PM marker
    emitter.instruction("je __rt_date_fmt_A_linux_x86_64");                     // handle the uppercase AM/PM token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 97");                                          // check whether the current token is 'a' for the lowercase am/pm marker
    emitter.instruction("je __rt_date_fmt_a_linux_x86_64");                     // handle the lowercase am/pm token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 85");                                          // check whether the current token is 'U' for the Unix timestamp decimal form
    emitter.instruction("je __rt_date_fmt_U_linux_x86_64");                     // handle the Unix timestamp token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 108");                                         // check whether the current token is 'l' for the full weekday name
    emitter.instruction("je __rt_date_fmt_l_linux_x86_64");                     // handle the full weekday-name token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 68");                                          // check whether the current token is 'D' for the short weekday name
    emitter.instruction("je __rt_date_fmt_D_linux_x86_64");                     // handle the short weekday-name token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 70");                                          // check whether the current token is 'F' for the full month name
    emitter.instruction("je __rt_date_fmt_F_linux_x86_64");                     // handle the full month-name token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 77");                                          // check whether the current token is 'M' for the short month name
    emitter.instruction("je __rt_date_fmt_M_linux_x86_64");                     // handle the short month-name token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 121");                                         // check whether the current token is 'y' for a two-digit year
    emitter.instruction("je __rt_date_fmt_y_linux_x86_64");                     // handle the two-digit year token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 104");                                         // check whether the current token is 'h' for a zero-padded 12-hour clock value
    emitter.instruction("je __rt_date_fmt_h_linux_x86_64");                     // handle the zero-padded 12-hour token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 119");                                         // check whether the current token is 'w' for the numeric weekday
    emitter.instruction("je __rt_date_fmt_w_linux_x86_64");                     // handle the numeric weekday token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 122");                                         // check whether the current token is 'z' for the day of year
    emitter.instruction("je __rt_date_fmt_z_linux_x86_64");                     // handle the day-of-year token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 83");                                          // check whether the current token is 'S' for the English ordinal suffix
    emitter.instruction("je __rt_date_fmt_S_linux_x86_64");                     // handle the ordinal-suffix token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 116");                                         // check whether the current token is 't' for the number of days in the month
    emitter.instruction("je __rt_date_fmt_t_linux_x86_64");                     // handle the days-in-month token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 76");                                          // check whether the current token is 'L' for the leap-year flag
    emitter.instruction("je __rt_date_fmt_L_linux_x86_64");                     // handle the leap-year-flag token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 87");                                          // check whether the current token is 'W' for the ISO-8601 week number
    emitter.instruction("je __rt_date_fmt_W_linux_x86_64");                     // handle the ISO week-number token through the dedicated x86_64 helper path
    emitter.instruction("cmp al, 111");                                         // check whether the current token is 'o' for the ISO-8601 week-numbering year
    emitter.instruction("je __rt_date_fmt_o_linux_x86_64");                     // handle the ISO year token through the dedicated x86_64 helper path

    emitter.instruction("cmp al, 79");                                          // check whether the current token is 'O' for a +hhmm UTC offset
    emitter.instruction("je __rt_date_fmt_O_linux_x86_64");                     // handle the +hhmm timezone-offset token
    emitter.instruction("cmp al, 80");                                          // check whether the current token is 'P' for a +hh:mm UTC offset
    emitter.instruction("je __rt_date_fmt_P_linux_x86_64");                     // handle the +hh:mm timezone-offset token
    emitter.instruction("cmp al, 90");                                          // check whether the current token is 'Z' for the UTC offset in seconds
    emitter.instruction("je __rt_date_fmt_Z_linux_x86_64");                     // handle the offset-in-seconds token

    emitter.instruction("cmp al, 101");                                         // check whether the current token is 'e' (timezone identifier)
    emitter.instruction("je __rt_date_fmt_e_linux_x86_64");                     // handle the timezone-identifier token
    emitter.instruction("cmp al, 84");                                          // check whether the current token is 'T' (timezone abbreviation)
    emitter.instruction("je __rt_date_fmt_T_linux_x86_64");                     // handle the timezone-abbreviation token
    emitter.instruction("cmp al, 73");                                          // check whether the current token is 'I' (daylight saving flag)
    emitter.instruction("je __rt_date_fmt_I_linux_x86_64");                     // handle the DST-flag token
    emitter.instruction("cmp al, 117");                                         // check whether the current token is 'u' (microseconds)
    emitter.instruction("je __rt_date_fmt_u_linux_x86_64");                     // handle the microseconds token
    emitter.instruction("cmp al, 118");                                         // check whether the current token is 'v' (milliseconds)
    emitter.instruction("je __rt_date_fmt_v_linux_x86_64");                     // handle the milliseconds token
    emitter.instruction("cmp al, 99");                                          // check whether the current token is 'c' (ISO 8601 composite)
    emitter.instruction("je __rt_date_fmt_c_linux_x86_64");                     // handle the ISO 8601 composite token
    emitter.instruction("cmp al, 114");                                         // check whether the current token is 'r' (RFC 2822 composite)
    emitter.instruction("je __rt_date_fmt_r_linux_x86_64");                     // handle the RFC 2822 composite token
    emitter.instruction("cmp al, 112");                                         // check whether the current token is 'p' (offset with Z-for-UTC)
    emitter.instruction("je __rt_date_fmt_p_linux_x86_64");                     // handle the Z-for-UTC offset token
    emitter.instruction("cmp al, 66");                                          // check whether the current token is 'B' (Swatch Internet Time)
    emitter.instruction("je __rt_date_fmt_B_linux_x86_64");                     // handle the Swatch beats token


    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor for literal bytes that are copied directly from the format string
    emitter.instruction("mov BYTE PTR [r9], al");                               // copy the current non-token literal format byte into the output buffer unchanged
    emitter.instruction("add r9, 1");                                           // advance the live output cursor after writing one literal byte
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the advanced output cursor after the literal-byte append
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after copying a literal character

    emitter.label("__rt_date_fmt_Y_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the saved year-since-1900 field
    emitter.instruction("mov eax, DWORD PTR [r8 + 20]");                        // load tm_year from the libc struct tm
    emitter.instruction("add eax, 1900");                                       // convert the libc year-since-1900 encoding into a full Gregorian year
    emitter.instruction("call __rt_date_write_4digit_linux_x86_64");            // append the four-digit Gregorian year to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the year token

    emitter.label("__rt_date_fmt_m_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the zero-based month field
    emitter.instruction("mov eax, DWORD PTR [r8 + 16]");                        // load tm_mon from the libc struct tm
    emitter.instruction("add eax, 1");                                          // convert the libc zero-based month encoding into PHP's 1-based calendar month
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // append the zero-padded calendar month to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the month token

    emitter.label("__rt_date_fmt_d_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the day-of-month field
    emitter.instruction("mov eax, DWORD PTR [r8 + 12]");                        // load tm_mday from the libc struct tm
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // append the zero-padded day-of-month to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the day token

    emitter.label("__rt_date_fmt_H_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the 24-hour field
    emitter.instruction("mov eax, DWORD PTR [r8 + 8]");                         // load tm_hour from the libc struct tm
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // append the zero-padded 24-hour clock value to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the hour token

    emitter.label("__rt_date_fmt_i_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the minute field
    emitter.instruction("mov eax, DWORD PTR [r8 + 4]");                         // load tm_min from the libc struct tm
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // append the zero-padded minute value to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the minute token

    emitter.label("__rt_date_fmt_s_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the second field
    emitter.instruction("mov eax, DWORD PTR [r8 + 0]");                         // load tm_sec from the libc struct tm
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // append the zero-padded second value to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the second token

    emitter.label("__rt_date_fmt_j_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the day-of-month field
    emitter.instruction("mov eax, DWORD PTR [r8 + 12]");                        // load tm_mday from the libc struct tm
    emitter.instruction("call __rt_date_write_num_linux_x86_64");               // append the unpadded day-of-month to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the unpadded day token

    emitter.label("__rt_date_fmt_n_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the zero-based month field
    emitter.instruction("mov eax, DWORD PTR [r8 + 16]");                        // load tm_mon from the libc struct tm
    emitter.instruction("add eax, 1");                                          // convert the libc zero-based month encoding into PHP's 1-based calendar month
    emitter.instruction("call __rt_date_write_num_linux_x86_64");               // append the unpadded calendar month to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the unpadded month token

    emitter.label("__rt_date_fmt_G_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the 24-hour field
    emitter.instruction("mov eax, DWORD PTR [r8 + 8]");                         // load tm_hour from the libc struct tm
    emitter.instruction("call __rt_date_write_num_linux_x86_64");               // append the unpadded 24-hour clock value to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the unpadded hour token

    emitter.label("__rt_date_fmt_g_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the 24-hour field for 12-hour conversion
    emitter.instruction("mov eax, DWORD PTR [r8 + 8]");                         // load tm_hour from the libc struct tm
    emitter.instruction("cmp eax, 0");                                          // detect midnight so PHP's 12-hour token can print 12 instead of 0
    emitter.instruction("je __rt_date_g_midnight_linux_x86_64");                // map midnight to 12 before appending the unpadded 12-hour clock value
    emitter.instruction("cmp eax, 12");                                         // detect afternoon hours that need the 13-23 -> 1-11 conversion
    emitter.instruction("jle __rt_date_g_write_linux_x86_64");                  // keep morning and noon values unchanged when they are already in the 1-12 range
    emitter.instruction("sub eax, 12");                                         // convert afternoon hours from the 24-hour range into the PHP 12-hour range
    emitter.instruction("jmp __rt_date_g_write_linux_x86_64");                  // append the converted 12-hour value after subtracting the noon offset
    emitter.label("__rt_date_g_midnight_linux_x86_64");
    emitter.instruction("mov eax, 12");                                         // map midnight to 12 so PHP's 'g' token matches the expected 12-hour clock convention
    emitter.label("__rt_date_g_write_linux_x86_64");
    emitter.instruction("call __rt_date_write_num_linux_x86_64");               // append the unpadded 12-hour clock value to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the 12-hour token

    emitter.label("__rt_date_fmt_N_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the weekday field
    emitter.instruction("mov eax, DWORD PTR [r8 + 24]");                        // load tm_wday where libc uses Sunday=0 and Monday=1
    emitter.instruction("cmp eax, 0");                                          // detect Sunday so PHP's ISO weekday token can remap it to 7
    emitter.instruction("jne __rt_date_N_write_linux_x86_64");                  // keep Monday-Saturday unchanged because libc already stores them as 1-6
    emitter.instruction("mov eax, 7");                                          // remap Sunday from libc's 0 to PHP's ISO weekday value 7
    emitter.label("__rt_date_N_write_linux_x86_64");
    emitter.instruction("call __rt_date_write_num_linux_x86_64");               // append the ISO weekday number to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the ISO weekday token

    emitter.label("__rt_date_fmt_A_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the hour for the AM/PM decision
    emitter.instruction("mov eax, DWORD PTR [r8 + 8]");                         // load tm_hour from the libc struct tm
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the AM/PM marker
    emitter.instruction("cmp eax, 12");                                         // distinguish morning hours from afternoon hours for PHP's uppercase AM/PM token
    emitter.instruction("jge __rt_date_A_pm_linux_x86_64");                     // choose the PM branch when the hour is 12 or later
    emitter.instruction("mov BYTE PTR [r9 + 0], 65");                           // append 'A' for the uppercase morning marker
    emitter.instruction("mov BYTE PTR [r9 + 1], 77");                           // append 'M' for the uppercase morning marker
    emitter.instruction("add r9, 2");                                           // advance the output cursor after writing the two-byte uppercase AM marker
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the updated output cursor after the uppercase AM append
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the uppercase AM token
    emitter.label("__rt_date_A_pm_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9 + 0], 80");                           // append 'P' for the uppercase afternoon marker
    emitter.instruction("mov BYTE PTR [r9 + 1], 77");                           // append 'M' for the uppercase afternoon marker
    emitter.instruction("add r9, 2");                                           // advance the output cursor after writing the two-byte uppercase PM marker
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the updated output cursor after the uppercase PM append
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the uppercase PM token

    emitter.label("__rt_date_fmt_a_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the hour for the am/pm decision
    emitter.instruction("mov eax, DWORD PTR [r8 + 8]");                         // load tm_hour from the libc struct tm
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the lowercase am/pm marker
    emitter.instruction("cmp eax, 12");                                         // distinguish morning hours from afternoon hours for PHP's lowercase am/pm token
    emitter.instruction("jge __rt_date_a_pm_linux_x86_64");                     // choose the lowercase pm branch when the hour is 12 or later
    emitter.instruction("mov BYTE PTR [r9 + 0], 97");                           // append 'a' for the lowercase morning marker
    emitter.instruction("mov BYTE PTR [r9 + 1], 109");                          // append 'm' for the lowercase morning marker
    emitter.instruction("add r9, 2");                                           // advance the output cursor after writing the two-byte lowercase am marker
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the updated output cursor after the lowercase am append
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the lowercase am token
    emitter.label("__rt_date_a_pm_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9 + 0], 112");                          // append 'p' for the lowercase afternoon marker
    emitter.instruction("mov BYTE PTR [r9 + 1], 109");                          // append 'm' for the lowercase afternoon marker
    emitter.instruction("add r9, 2");                                           // advance the output cursor after writing the two-byte lowercase pm marker
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the updated output cursor after the lowercase pm append
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the lowercase pm token

    emitter.label("__rt_date_fmt_U_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the original Unix timestamp so the decimal formatter can append it directly to the output buffer
    emitter.instruction("call __rt_date_write_int64_linux_x86_64");             // append the full Unix timestamp as an unpadded decimal integer without disturbing the global concat cursor
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the Unix timestamp token

    emitter.label("__rt_date_fmt_l_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the weekday index for the full weekday name table
    emitter.instruction("mov eax, DWORD PTR [r8 + 24]");                        // load tm_wday where libc uses Sunday=0 and Saturday=6
    emitter.instruction("imul rax, rax, 12");                                   // convert the weekday index into the 12-byte table stride used by the runtime day-name data
    abi::emit_symbol_address(emitter, "r9", "_day_names");                      // load the base address of the runtime weekday-name lookup table
    emitter.instruction("add r9, rax");                                         // advance to the selected weekday-name entry inside the runtime lookup table
    emitter.instruction("movzx ecx, BYTE PTR [r9 + 10]");                       // load the selected weekday-name length from the table metadata byte
    emitter.instruction("xor r10, r10");                                        // start a byte-copy index at zero before appending the full weekday name
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the live output cursor before copying the selected weekday-name bytes
    emitter.label("__rt_date_l_copy_linux_x86_64");
    emitter.instruction("cmp r10, rcx");                                        // stop once every byte of the selected full weekday name has been copied
    emitter.instruction("jae __rt_date_l_done_linux_x86_64");                   // finish the full weekday-name copy once the saved length has been exhausted
    emitter.instruction("mov al, BYTE PTR [r9 + r10]");                         // load one byte from the selected full weekday-name entry
    emitter.instruction("mov BYTE PTR [r11 + r10], al");                        // write that byte into the current output buffer position
    emitter.instruction("add r10, 1");                                          // advance the full weekday-name copy index after moving one byte
    emitter.instruction("jmp __rt_date_l_copy_linux_x86_64");                   // continue copying bytes until the full weekday name is exhausted
    emitter.label("__rt_date_l_done_linux_x86_64");
    emitter.instruction("add r11, rcx");                                        // advance the live output cursor by the copied weekday-name byte count
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // publish the updated output cursor after copying the full weekday name
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the full weekday-name token

    emitter.label("__rt_date_fmt_D_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the weekday index for the short weekday name table
    emitter.instruction("mov eax, DWORD PTR [r8 + 24]");                        // load tm_wday where libc uses Sunday=0 and Saturday=6
    emitter.instruction("imul rax, rax, 12");                                   // convert the weekday index into the 12-byte table stride used by the runtime day-name data
    abi::emit_symbol_address(emitter, "r9", "_day_names");                      // load the base address of the runtime weekday-name lookup table
    emitter.instruction("add r9, rax");                                         // advance to the selected weekday-name entry inside the runtime lookup table
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the live output cursor before appending the three-byte short weekday name
    emitter.instruction("mov al, BYTE PTR [r9 + 0]");                           // load the first byte of the selected weekday name
    emitter.instruction("mov BYTE PTR [r11 + 0], al");                          // write the first byte of the short weekday name into the output buffer
    emitter.instruction("mov al, BYTE PTR [r9 + 1]");                           // load the second byte of the selected weekday name
    emitter.instruction("mov BYTE PTR [r11 + 1], al");                          // write the second byte of the short weekday name into the output buffer
    emitter.instruction("mov al, BYTE PTR [r9 + 2]");                           // load the third byte of the selected weekday name
    emitter.instruction("mov BYTE PTR [r11 + 2], al");                          // write the third byte of the short weekday name into the output buffer
    emitter.instruction("add r11, 3");                                          // advance the output cursor by the fixed three-byte short weekday-name width
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // publish the updated output cursor after copying the short weekday name
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the short weekday-name token

    emitter.label("__rt_date_fmt_F_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the month index for the full month-name table
    emitter.instruction("mov eax, DWORD PTR [r8 + 16]");                        // load tm_mon where libc uses January=0 and December=11
    emitter.instruction("imul rax, rax, 12");                                   // convert the month index into the 12-byte table stride used by the runtime month-name data
    abi::emit_symbol_address(emitter, "r9", "_month_names");                    // load the base address of the runtime month-name lookup table
    emitter.instruction("add r9, rax");                                         // advance to the selected month-name entry inside the runtime lookup table
    emitter.instruction("movzx ecx, BYTE PTR [r9 + 10]");                       // load the selected month-name length from the table metadata byte
    emitter.instruction("xor r10, r10");                                        // start a byte-copy index at zero before appending the full month name
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the live output cursor before copying the selected month-name bytes
    emitter.label("__rt_date_F_copy_linux_x86_64");
    emitter.instruction("cmp r10, rcx");                                        // stop once every byte of the selected full month name has been copied
    emitter.instruction("jae __rt_date_F_done_linux_x86_64");                   // finish the full month-name copy once the saved length has been exhausted
    emitter.instruction("mov al, BYTE PTR [r9 + r10]");                         // load one byte from the selected full month-name entry
    emitter.instruction("mov BYTE PTR [r11 + r10], al");                        // write that byte into the current output buffer position
    emitter.instruction("add r10, 1");                                          // advance the full month-name copy index after moving one byte
    emitter.instruction("jmp __rt_date_F_copy_linux_x86_64");                   // continue copying bytes until the full month name is exhausted
    emitter.label("__rt_date_F_done_linux_x86_64");
    emitter.instruction("add r11, rcx");                                        // advance the live output cursor by the copied month-name byte count
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // publish the updated output cursor after copying the full month name
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the full month-name token

    emitter.label("__rt_date_fmt_M_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the month index for the short month-name table
    emitter.instruction("mov eax, DWORD PTR [r8 + 16]");                        // load tm_mon where libc uses January=0 and December=11
    emitter.instruction("imul rax, rax, 12");                                   // convert the month index into the 12-byte table stride used by the runtime month-name data
    abi::emit_symbol_address(emitter, "r9", "_month_names");                    // load the base address of the runtime month-name lookup table
    emitter.instruction("add r9, rax");                                         // advance to the selected month-name entry inside the runtime lookup table
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the live output cursor before appending the three-byte short month name
    emitter.instruction("mov al, BYTE PTR [r9 + 0]");                           // load the first byte of the selected month name
    emitter.instruction("mov BYTE PTR [r11 + 0], al");                          // write the first byte of the short month name into the output buffer
    emitter.instruction("mov al, BYTE PTR [r9 + 1]");                           // load the second byte of the selected month name
    emitter.instruction("mov BYTE PTR [r11 + 1], al");                          // write the second byte of the short month name into the output buffer
    emitter.instruction("mov al, BYTE PTR [r9 + 2]");                           // load the third byte of the selected month name
    emitter.instruction("mov BYTE PTR [r11 + 2], al");                          // write the third byte of the short month name into the output buffer
    emitter.instruction("add r11, 3");                                          // advance the output cursor by the fixed three-byte short month-name width
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // publish the updated output cursor after copying the short month name
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the short month-name token

    emitter.label("__rt_date_fmt_y_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the year-since-1900 field
    emitter.instruction("mov eax, DWORD PTR [r8 + 20]");                        // load tm_year from the libc struct tm
    emitter.instruction("add eax, 1900");                                       // convert the libc year-since-1900 encoding into a full Gregorian year
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before reducing the year modulo 100
    emitter.instruction("mov ecx, 100");                                        // load the divisor used to keep only the last two digits of the year
    emitter.instruction("div ecx");                                             // divide the year by 100 so edx holds the two-digit year remainder
    emitter.instruction("mov eax, edx");                                        // move the two-digit year remainder into the value register for the writer helper
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // append the zero-padded two-digit year to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the two-digit year token

    emitter.label("__rt_date_fmt_h_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the 24-hour field for 12-hour conversion
    emitter.instruction("mov eax, DWORD PTR [r8 + 8]");                         // load tm_hour from the libc struct tm
    emitter.instruction("cmp eax, 0");                                          // detect midnight so PHP's zero-padded 12-hour token can print 12 instead of 0
    emitter.instruction("je __rt_date_h_midnight_linux_x86_64");                // map midnight to 12 before appending the zero-padded 12-hour clock value
    emitter.instruction("cmp eax, 12");                                         // detect afternoon hours that need the 13-23 -> 1-11 conversion
    emitter.instruction("jle __rt_date_h_write_linux_x86_64");                  // keep morning and noon values unchanged when they are already in the 1-12 range
    emitter.instruction("sub eax, 12");                                         // convert afternoon hours from the 24-hour range into the PHP 12-hour range
    emitter.instruction("jmp __rt_date_h_write_linux_x86_64");                  // append the converted 12-hour value after subtracting the noon offset
    emitter.label("__rt_date_h_midnight_linux_x86_64");
    emitter.instruction("mov eax, 12");                                         // map midnight to 12 so PHP's 'h' token matches the expected 12-hour clock convention
    emitter.label("__rt_date_h_write_linux_x86_64");
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // append the zero-padded 12-hour clock value to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the zero-padded 12-hour token

    emitter.label("__rt_date_fmt_w_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the weekday field
    emitter.instruction("mov eax, DWORD PTR [r8 + 24]");                        // load tm_wday where libc already uses Sunday=0 matching PHP's 'w' token
    emitter.instruction("call __rt_date_write_num_linux_x86_64");               // append the single-digit numeric weekday to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the numeric weekday token

    emitter.label("__rt_date_fmt_z_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the day-of-year field
    emitter.instruction("mov eax, DWORD PTR [r8 + 28]");                        // load tm_yday (0-365) zero-extended into rax for the decimal writer
    emitter.instruction("call __rt_date_write_int64_linux_x86_64");             // append the unpadded day-of-year value to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the day-of-year token

    emitter.label("__rt_date_fmt_S_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the day-of-month field for the ordinal suffix
    emitter.instruction("mov eax, DWORD PTR [r8 + 12]");                        // load tm_mday (1-31) used to select the English ordinal suffix
    emitter.instruction("cmp eax, 11");                                         // 11th always takes the "th" suffix regardless of its last digit
    emitter.instruction("je __rt_date_S_th_linux_x86_64");                      // route 11 to the "th" suffix branch
    emitter.instruction("cmp eax, 12");                                         // 12th always takes the "th" suffix regardless of its last digit
    emitter.instruction("je __rt_date_S_th_linux_x86_64");                      // route 12 to the "th" suffix branch
    emitter.instruction("cmp eax, 13");                                         // 13th always takes the "th" suffix regardless of its last digit
    emitter.instruction("je __rt_date_S_th_linux_x86_64");                      // route 13 to the "th" suffix branch
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before extracting the last digit
    emitter.instruction("mov ecx, 10");                                         // load the divisor used to isolate the day's last decimal digit
    emitter.instruction("div ecx");                                             // divide the day by ten so edx holds its last digit for the suffix decision
    emitter.instruction("cmp edx, 1");                                          // a last digit of 1 takes the "st" suffix
    emitter.instruction("je __rt_date_S_st_linux_x86_64");                      // route last-digit 1 to the "st" suffix branch
    emitter.instruction("cmp edx, 2");                                          // a last digit of 2 takes the "nd" suffix
    emitter.instruction("je __rt_date_S_nd_linux_x86_64");                      // route last-digit 2 to the "nd" suffix branch
    emitter.instruction("cmp edx, 3");                                          // a last digit of 3 takes the "rd" suffix
    emitter.instruction("je __rt_date_S_rd_linux_x86_64");                      // route last-digit 3 to the "rd" suffix branch
    emitter.label("__rt_date_S_th_linux_x86_64");
    emitter.instruction("mov r10b, 116");                                       // stage 't' as the first byte of the "th" suffix
    emitter.instruction("mov r11b, 104");                                       // stage 'h' as the second byte of the "th" suffix
    emitter.instruction("jmp __rt_date_S_emit_linux_x86_64");                   // emit the staged "th" suffix
    emitter.label("__rt_date_S_st_linux_x86_64");
    emitter.instruction("mov r10b, 115");                                       // stage 's' as the first byte of the "st" suffix
    emitter.instruction("mov r11b, 116");                                       // stage 't' as the second byte of the "st" suffix
    emitter.instruction("jmp __rt_date_S_emit_linux_x86_64");                   // emit the staged "st" suffix
    emitter.label("__rt_date_S_nd_linux_x86_64");
    emitter.instruction("mov r10b, 110");                                       // stage 'n' as the first byte of the "nd" suffix
    emitter.instruction("mov r11b, 100");                                       // stage 'd' as the second byte of the "nd" suffix
    emitter.instruction("jmp __rt_date_S_emit_linux_x86_64");                   // emit the staged "nd" suffix
    emitter.label("__rt_date_S_rd_linux_x86_64");
    emitter.instruction("mov r10b, 114");                                       // stage 'r' as the first byte of the "rd" suffix
    emitter.instruction("mov r11b, 100");                                       // stage 'd' as the second byte of the "rd" suffix
    emitter.label("__rt_date_S_emit_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the two-byte ordinal suffix
    emitter.instruction("mov BYTE PTR [r9 + 0], r10b");                         // append the first suffix byte to the output buffer
    emitter.instruction("mov BYTE PTR [r9 + 1], r11b");                         // append the second suffix byte to the output buffer
    emitter.instruction("add r9, 2");                                           // advance the output cursor after writing the two-byte suffix
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the updated output cursor after the ordinal-suffix append
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the ordinal-suffix token

    emitter.label("__rt_date_fmt_t_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the month index for the days-in-month table
    emitter.instruction("movsxd r10, DWORD PTR [r8 + 16]");                     // load tm_mon (0-based) sign-extended for table indexing
    emitter.instruction("lea r9, [rip + _days_in_month]");                      // load the base address of the days-in-month lookup table
    emitter.instruction("movzx r11d, BYTE PTR [r9 + r10]");                     // r11d = base number of days for the selected month
    emitter.instruction("cmp r10, 1");                                          // detect February (index 1) which needs a leap-year adjustment
    emitter.instruction("jne __rt_date_t_write_linux_x86_64");                  // non-February months use the table value directly
    emitter.instruction("mov eax, DWORD PTR [r8 + 20]");                        // load tm_year for the February leap-year decision
    emitter.instruction("add eax, 1900");                                       // convert the libc year-since-1900 encoding into a full Gregorian year
    emitter.instruction("mov r10d, eax");                                       // save the full year for the repeated modulo checks
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the divide-by-4 step
    emitter.instruction("mov ecx, 4");                                          // load the divisor used to test divisibility by 4
    emitter.instruction("div ecx");                                             // divide the year by 4 so edx holds year mod 4
    emitter.instruction("cmp edx, 0");                                          // years not divisible by 4 are common years
    emitter.instruction("jne __rt_date_t_write_linux_x86_64");                  // not divisible by 4 → February keeps 28 days
    emitter.instruction("mov eax, r10d");                                       // reload the saved year for the divide-by-100 step
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the divide-by-100 step
    emitter.instruction("mov ecx, 100");                                        // load the divisor used to test divisibility by 100
    emitter.instruction("div ecx");                                             // divide the year by 100 so edx holds year mod 100
    emitter.instruction("cmp edx, 0");                                          // years divisible by 4 but not 100 are leap years
    emitter.instruction("jne __rt_date_t_feb29_linux_x86_64");                  // divisible by 4, not 100 → leap → 29 days
    emitter.instruction("mov eax, r10d");                                       // reload the saved year for the divide-by-400 step
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the divide-by-400 step
    emitter.instruction("mov ecx, 400");                                        // load the divisor used to test divisibility by 400
    emitter.instruction("div ecx");                                             // divide the year by 400 so edx holds year mod 400
    emitter.instruction("cmp edx, 0");                                          // years divisible by 100 are leap only when also divisible by 400
    emitter.instruction("jne __rt_date_t_write_linux_x86_64");                  // divisible by 100, not 400 → February keeps 28 days
    emitter.label("__rt_date_t_feb29_linux_x86_64");
    emitter.instruction("mov r11d, 29");                                        // leap February has 29 days
    emitter.label("__rt_date_t_write_linux_x86_64");
    emitter.instruction("mov eax, r11d");                                       // move the day count into the writer's value register
    emitter.instruction("call __rt_date_write_num_linux_x86_64");               // append the 1- or 2-digit days-in-month value to the output buffer
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the days-in-month token

    emitter.label("__rt_date_fmt_L_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the year for the leap-year decision
    emitter.instruction("mov eax, DWORD PTR [r8 + 20]");                        // load tm_year from the libc struct tm
    emitter.instruction("add eax, 1900");                                       // convert the libc year-since-1900 encoding into a full Gregorian year
    emitter.instruction("mov r10d, eax");                                       // save the full year for the repeated modulo checks
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the divide-by-4 step
    emitter.instruction("mov ecx, 4");                                          // load the divisor used to test divisibility by 4
    emitter.instruction("div ecx");                                             // divide the year by 4 so edx holds year mod 4
    emitter.instruction("cmp edx, 0");                                          // years not divisible by 4 are common years
    emitter.instruction("jne __rt_date_L_no_linux_x86_64");                     // not divisible by 4 → not a leap year
    emitter.instruction("mov eax, r10d");                                       // reload the saved year for the divide-by-100 step
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the divide-by-100 step
    emitter.instruction("mov ecx, 100");                                        // load the divisor used to test divisibility by 100
    emitter.instruction("div ecx");                                             // divide the year by 100 so edx holds year mod 100
    emitter.instruction("cmp edx, 0");                                          // years divisible by 4 but not 100 are leap years
    emitter.instruction("jne __rt_date_L_yes_linux_x86_64");                    // divisible by 4, not 100 → leap year
    emitter.instruction("mov eax, r10d");                                       // reload the saved year for the divide-by-400 step
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the divide-by-400 step
    emitter.instruction("mov ecx, 400");                                        // load the divisor used to test divisibility by 400
    emitter.instruction("div ecx");                                             // divide the year by 400 so edx holds year mod 400
    emitter.instruction("cmp edx, 0");                                          // years divisible by 400 are leap years
    emitter.instruction("je __rt_date_L_yes_linux_x86_64");                     // divisible by 400 → leap year
    emitter.label("__rt_date_L_no_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the leap-year flag
    emitter.instruction("mov BYTE PTR [r9], 48");                               // append '0' for a non-leap year
    emitter.instruction("add r9, 1");                                           // advance the output cursor after writing the flag byte
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the updated output cursor after the non-leap flag append
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the leap-year token
    emitter.label("__rt_date_L_yes_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the leap-year flag
    emitter.instruction("mov BYTE PTR [r9], 49");                               // append '1' for a leap year
    emitter.instruction("add r9, 1");                                           // advance the output cursor after writing the flag byte
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the updated output cursor after the leap flag append
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte after the leap-year token

    emitter.label("__rt_date_fmt_I_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer to read the DST flag
    emitter.instruction("mov eax, DWORD PTR [r8 + 32]");                        // load tm_isdst from the libc struct tm
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor
    emitter.instruction("mov edx, 48");                                         // default ASCII '0' (not in DST)
    emitter.instruction("cmp eax, 0");                                          // is the DST flag positive?
    emitter.instruction("jle __rt_date_I_store_linux_x86_64");                  // <= 0 means not in DST, keep 0
    emitter.instruction("mov edx, 49");                                         // ASCII '1' (DST in effect)
    emitter.label("__rt_date_I_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r9], dl");                               // write the DST flag digit
    emitter.instruction("add r9, 1");                                           // advance the output cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the advanced output cursor
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte

    emitter.label("__rt_date_fmt_u_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor
    emitter.instruction("mov BYTE PTR [r9], 48");                               // write microsecond digit 1 ('0')
    emitter.instruction("mov BYTE PTR [r9 + 1], 48");                           // write microsecond digit 2
    emitter.instruction("mov BYTE PTR [r9 + 2], 48");                           // write microsecond digit 3
    emitter.instruction("mov BYTE PTR [r9 + 3], 48");                           // write microsecond digit 4
    emitter.instruction("mov BYTE PTR [r9 + 4], 48");                           // write microsecond digit 5
    emitter.instruction("mov BYTE PTR [r9 + 5], 48");                           // write microsecond digit 6
    emitter.instruction("add r9, 6");                                           // advance past the 6 digits
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the advanced output cursor
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte

    emitter.label("__rt_date_fmt_v_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor
    emitter.instruction("mov BYTE PTR [r9], 48");                               // write millisecond digit 1 ('0')
    emitter.instruction("mov BYTE PTR [r9 + 1], 48");                           // write millisecond digit 2
    emitter.instruction("mov BYTE PTR [r9 + 2], 48");                           // write millisecond digit 3
    emitter.instruction("add r9, 3");                                           // advance past the 3 digits
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the advanced output cursor
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte

    // -- format: c (ISO 8601): switch to the sub-format and reuse the main loop --
    emitter.label("__rt_date_fmt_c_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // current main format pointer
    emitter.instruction("mov QWORD PTR [rbp - 136], rax");                      // save it (also marks that a sub-format is active)
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // current main format length
    emitter.instruction("mov QWORD PTR [rbp - 144], rax");                      // save the main format length
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // current main index (at this token)
    emitter.instruction("mov QWORD PTR [rbp - 152], rax");                      // save the main index
    emitter.instruction("lea rax, [rip + _date_fmt_c]");                        // address of the ISO 8601 sub-format
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // switch the format pointer to the sub-format
    emitter.instruction("mov QWORD PTR [rbp - 24], 13");                        // ISO 8601 sub-format length
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // restart the index for the sub-format
    emitter.instruction("jmp __rt_date_loop_linux_x86_64");                     // process the sub-format through the main loop

    // -- format: r (RFC 2822): switch to the sub-format and reuse the main loop --
    emitter.label("__rt_date_fmt_r_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // current main format pointer
    emitter.instruction("mov QWORD PTR [rbp - 136], rax");                      // save it (also marks that a sub-format is active)
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // current main format length
    emitter.instruction("mov QWORD PTR [rbp - 144], rax");                      // save the main format length
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // current main index (at this token)
    emitter.instruction("mov QWORD PTR [rbp - 152], rax");                      // save the main index
    emitter.instruction("lea rax, [rip + _date_fmt_r]");                        // address of the RFC 2822 sub-format
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // switch the format pointer to the sub-format
    emitter.instruction("mov QWORD PTR [rbp - 24], 16");                        // RFC 2822 sub-format length
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // restart the index for the sub-format
    emitter.instruction("jmp __rt_date_loop_linux_x86_64");                     // process the sub-format through the main loop

    emitter.label("__rt_date_escape_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // load the format index that currently points at the backslash
    emitter.instruction("add rcx, 1");                                          // advance to the escaped character following the backslash
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // publish the advanced index so the escaped char is consumed
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // check whether a character actually follows the backslash
    emitter.instruction("jae __rt_date_loop_linux_x86_64");                     // a lone trailing backslash emits nothing
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the format-string pointer to read the escaped character
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // load the escaped character to emit verbatim
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the escaped character
    emitter.instruction("mov BYTE PTR [r9], al");                               // append the escaped character literally
    emitter.instruction("add r9, 1");                                           // advance the output cursor past the escaped character
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the advanced output cursor
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // the next-step +1 advances past the escaped character

    emitter.label("__rt_date_next_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the current format-string byte index before stepping to the next token or literal
    emitter.instruction("add rcx, 1");                                          // advance the format-string byte index after consuming one token or literal character
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // publish the advanced format-string byte index for the next loop iteration
    emitter.instruction("jmp __rt_date_loop_linux_x86_64");                     // continue scanning the format string until every byte has been consumed


    // -- end of (sub-)format: resume a pending c/r sub-format, or finish --
    emitter.label("__rt_date_check_pop_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 136]");                      // saved main format ptr (0 if not inside a sub-format)
    emitter.instruction("test rax, rax");                                       // is a c/r sub-format active?
    emitter.instruction("jz __rt_date_done_linux_x86_64");                      // no -> the whole format string is done
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // restore the main format pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 144]");                      // saved main format length
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // restore the main format length
    emitter.instruction("mov rax, QWORD PTR [rbp - 152]");                      // saved main index (at the c/r token)
    emitter.instruction("add rax, 1");                                          // advance past the c/r token
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // restore the main index, advanced
    emitter.instruction("mov QWORD PTR [rbp - 136], 0");                        // clear the in-sub marker
    emitter.instruction("jmp __rt_date_loop_linux_x86_64");                     // resume the main format
    emitter.label("__rt_date_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return the formatted-string start pointer in the standard x86_64 string result register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the live output cursor so the final string length can be computed from the written byte count
    emitter.instruction("sub rdx, rax");                                        // compute the formatted-string length from the distance between the output cursor and the start pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // reload the original concat-buffer offset that was active before formatting started
    emitter.instruction("add r8, rdx");                                         // advance the global concat-buffer offset by the number of bytes written by the formatter
    abi::emit_store_reg_to_symbol(emitter, "r8", "_concat_off", 0);             // publish the updated concat-buffer offset for later transient string helpers
    emitter.instruction("add rsp, 160");                                        // release the formatter locals, scratch, and c/r save slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the formatted date string
    emitter.instruction("ret");                                                 // return the formatted date string pointer and length through the standard x86_64 string result registers

    emitter.label("__rt_date_write_2digit_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the zero-padded two-digit decimal field
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the unsigned divide-by-10 step
    emitter.instruction("mov ecx, 10");                                         // load the constant decimal divisor used to split the value into tens and ones digits
    emitter.instruction("div ecx");                                             // divide the input value by ten so eax=quotient and edx=remainder for decimal digit emission
    emitter.instruction("add al, 48");                                          // convert the tens digit quotient to its ASCII decimal character
    emitter.instruction("mov BYTE PTR [r8 + 0], al");                           // append the tens digit to the output buffer
    emitter.instruction("add dl, 48");                                          // convert the ones digit remainder to its ASCII decimal character
    emitter.instruction("mov BYTE PTR [r8 + 1], dl");                           // append the ones digit to the output buffer
    emitter.instruction("add r8, 2");                                           // advance the live output cursor after appending the two decimal digits
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // publish the updated output cursor after the two-digit append
    emitter.instruction("ret");                                                 // return to the caller token branch after appending the zero-padded two-digit field

    emitter.label("__rt_date_write_4digit_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the four-digit decimal field
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the divide-by-1000 step
    emitter.instruction("mov ecx, 1000");                                       // load the constant decimal divisor used to extract the thousands digit
    emitter.instruction("div ecx");                                             // split the input into the thousands digit in eax and the remaining three digits in edx
    emitter.instruction("add al, 48");                                          // convert the thousands digit to its ASCII decimal character
    emitter.instruction("mov BYTE PTR [r8 + 0], al");                           // append the thousands digit to the output buffer
    emitter.instruction("mov eax, edx");                                        // move the remaining three digits into the dividend register for the hundreds extraction step
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the divide-by-100 step
    emitter.instruction("mov ecx, 100");                                        // load the constant decimal divisor used to extract the hundreds digit
    emitter.instruction("div ecx");                                             // split the remaining three digits into the hundreds digit in eax and the remaining two digits in edx
    emitter.instruction("add al, 48");                                          // convert the hundreds digit to its ASCII decimal character
    emitter.instruction("mov BYTE PTR [r8 + 1], al");                           // append the hundreds digit to the output buffer
    emitter.instruction("mov eax, edx");                                        // move the remaining two digits into the dividend register for the final divide-by-10 step
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the divide-by-10 step
    emitter.instruction("mov ecx, 10");                                         // load the constant decimal divisor used to extract the tens and ones digits
    emitter.instruction("div ecx");                                             // split the remaining two digits into the tens digit in eax and the ones digit in edx
    emitter.instruction("add al, 48");                                          // convert the tens digit to its ASCII decimal character
    emitter.instruction("mov BYTE PTR [r8 + 2], al");                           // append the tens digit to the output buffer
    emitter.instruction("add dl, 48");                                          // convert the ones digit to its ASCII decimal character
    emitter.instruction("mov BYTE PTR [r8 + 3], dl");                           // append the ones digit to the output buffer
    emitter.instruction("add r8, 4");                                           // advance the live output cursor after appending the four decimal digits
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // publish the updated output cursor after the four-digit append
    emitter.instruction("ret");                                                 // return to the caller token branch after appending the four-digit field

    emitter.label("__rt_date_write_num_linux_x86_64");
    emitter.instruction("cmp eax, 10");                                         // check whether the decimal value fits in a single digit before choosing the emission path
    emitter.instruction("jl __rt_date_write_num_single_linux_x86_64");          // use the single-digit path when the value is strictly smaller than ten
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // reuse the zero-padded two-digit helper when the value naturally occupies two decimal digits
    emitter.instruction("ret");                                                 // return to the caller token branch after appending the two-digit decimal field
    emitter.label("__rt_date_write_num_single_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the single-digit decimal field
    emitter.instruction("add al, 48");                                          // convert the single decimal digit to its ASCII character representation
    emitter.instruction("mov BYTE PTR [r8 + 0], al");                           // append the single decimal digit to the output buffer
    emitter.instruction("add r8, 1");                                           // advance the live output cursor after appending one decimal digit
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // publish the updated output cursor after the single-digit append
    emitter.instruction("ret");                                                 // return to the caller token branch after appending the single-digit field

    emitter.label("__rt_date_write_int64_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the live output cursor before appending the variable-width decimal integer field
    emitter.instruction("lea r9, [rbp - 96]");                                  // point at the local scratch buffer used to stage decimal digits in reverse order
    emitter.instruction("xor rcx, rcx");                                        // start the decimal scratch length at zero before extracting any digits
    emitter.instruction("cmp rax, 0");                                          // check whether the integer to append is exactly zero before entering the division loop
    emitter.instruction("jne __rt_date_write_int64_loop_linux_x86_64");         // skip the dedicated zero case when at least one non-zero digit must be extracted
    emitter.instruction("mov BYTE PTR [r9 + 0], 48");                           // stage the single ASCII digit '0' in the decimal scratch buffer for the zero value
    emitter.instruction("mov rcx, 1");                                          // record that the zero case staged exactly one decimal digit in the scratch buffer
    emitter.instruction("jmp __rt_date_write_int64_copy_linux_x86_64");         // skip the division loop once the dedicated zero case has staged its single digit

    emitter.label("__rt_date_write_int64_loop_linux_x86_64");
    emitter.instruction("xor edx, edx");                                        // clear the implicit high half of the dividend before the unsigned divide-by-10 step
    emitter.instruction("mov r10, 10");                                         // load the constant decimal divisor used to peel off one least-significant digit at a time
    emitter.instruction("div r10");                                             // divide the integer by ten so rax=quotient and rdx=remainder for decimal digit extraction
    emitter.instruction("add dl, 48");                                          // convert the extracted least-significant digit remainder to its ASCII decimal character
    emitter.instruction("mov BYTE PTR [r9 + rcx], dl");                         // stage the extracted decimal digit into the reverse-order scratch buffer
    emitter.instruction("add rcx, 1");                                          // advance the scratch-buffer length after staging one more extracted decimal digit
    emitter.instruction("test rax, rax");                                       // stop the extraction loop once no higher-order decimal digits remain
    emitter.instruction("jne __rt_date_write_int64_loop_linux_x86_64");         // continue extracting digits until the quotient reaches zero

    emitter.label("__rt_date_write_int64_copy_linux_x86_64");
    emitter.instruction("cmp rcx, 0");                                          // stop once every staged decimal digit has been copied back out in forward order
    emitter.instruction("je __rt_date_write_int64_done_linux_x86_64");          // finish the decimal integer append once the reverse-order scratch buffer is exhausted
    emitter.instruction("sub rcx, 1");                                          // step backward through the reverse-order scratch buffer to restore forward decimal order
    emitter.instruction("mov al, BYTE PTR [r9 + rcx]");                         // load the next forward-order decimal digit from the reverse-order scratch buffer
    emitter.instruction("mov BYTE PTR [r8 + 0], al");                           // append the next forward-order decimal digit to the output buffer
    emitter.instruction("add r8, 1");                                           // advance the live output cursor after appending one decimal digit
    emitter.instruction("jmp __rt_date_write_int64_copy_linux_x86_64");         // continue copying digits out until the scratch buffer has been fully drained

    emitter.label("__rt_date_write_int64_done_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // publish the updated output cursor after appending the variable-width decimal integer
    emitter.instruction("ret");                                                 // return to the caller token branch after appending the full decimal integer field

    // -- format: W (ISO-8601 week number, zero-padded 2 digits) --
    emitter.label("__rt_date_fmt_W_linux_x86_64");
    emitter.instruction("call __rt_date_iso_week_linux_x86_64");                // eax = ISO week, r9d = ISO year
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // append the zero-padded 2-digit week
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte

    // -- format: o (ISO-8601 week-numbering year, 4 digits) --
    emitter.label("__rt_date_fmt_o_linux_x86_64");
    emitter.instruction("call __rt_date_iso_week_linux_x86_64");                // eax = ISO week, r9d = ISO year
    emitter.instruction("mov eax, r9d");                                        // move the ISO year into the writer register
    emitter.instruction("call __rt_date_write_4digit_linux_x86_64");            // append the 4-digit ISO year
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte

    // -- format: Z (timezone offset in seconds, e.g. 7200 or -18000) --
    emitter.label("__rt_date_fmt_Z_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the gmt offset
    emitter.instruction("mov rax, QWORD PTR [r8 + 40]");                        // load tm_gmtoff (signed seconds east of UTC, offset 40)
    emitter.instruction("cmp rax, 0");                                          // is the UTC offset negative?
    emitter.instruction("jge __rt_date_Z_mag_linux_x86_64");                    // non-negative offset prints without a sign
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the live output cursor
    emitter.instruction("mov BYTE PTR [r11], 45");                              // write the leading minus sign
    emitter.instruction("add r11, 1");                                          // advance the live output cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // publish the advanced output cursor
    emitter.instruction("neg rax");                                             // format the magnitude of the negative offset
    emitter.label("__rt_date_Z_mag_linux_x86_64");
    emitter.instruction("call __rt_date_write_int64_linux_x86_64");             // append the offset magnitude as unpadded decimal
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte

    // -- format: O / P (timezone offset as +hhmm or +hh:mm) --
    emitter.label("__rt_date_fmt_O_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // colon flag = 0 (no ':' separator for 'O')
    emitter.instruction("jmp __rt_date_OP_common_linux_x86_64");                // share the offset body with 'P'
    emitter.label("__rt_date_fmt_P_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 80], 1");                         // colon flag = 1 (insert ':' separator for 'P')
    emitter.label("__rt_date_OP_common_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the gmt offset
    emitter.instruction("mov rax, QWORD PTR [r8 + 40]");                        // load tm_gmtoff (signed seconds east of UTC, offset 40)
    emitter.instruction("mov r10d, 43");                                        // assume a '+' sign (43)
    emitter.instruction("cmp rax, 0");                                          // is the UTC offset negative?
    emitter.instruction("jge __rt_date_OP_sign_linux_x86_64");                  // non-negative -> keep the '+' sign
    emitter.instruction("mov r10d, 45");                                        // '-' (45) for a negative offset
    emitter.instruction("neg rax");                                             // format the magnitude of the negative offset
    emitter.label("__rt_date_OP_sign_linux_x86_64");
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the live output cursor
    emitter.instruction("mov BYTE PTR [r11], r10b");                            // write the sign character
    emitter.instruction("add r11, 1");                                          // advance the live output cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // publish the advanced output cursor
    emitter.instruction("xor edx, edx");                                        // clear the high half before the unsigned divide
    emitter.instruction("mov r10, 3600");                                       // seconds per hour
    emitter.instruction("div r10");                                             // rax = hours, rdx = remaining seconds
    emitter.instruction("mov r9, rax");                                         // keep the hours value across the minutes division
    emitter.instruction("mov rax, rdx");                                        // remaining seconds -> dividend
    emitter.instruction("xor edx, edx");                                        // clear the high half before the unsigned divide
    emitter.instruction("mov r10, 60");                                         // seconds per minute
    emitter.instruction("div r10");                                             // rax = minutes
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // save minutes across the 2-digit writer call
    emitter.instruction("mov rax, r9");                                         // hours -> 2-digit writer input register
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // write zero-padded 2-digit hours
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // reload the colon flag
    emitter.instruction("cmp rax, 0");                                          // does this specifier use a ':' separator?
    emitter.instruction("je __rt_date_OP_min_linux_x86_64");                    // no -> skip the colon
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the live output cursor
    emitter.instruction("mov BYTE PTR [r11], 58");                              // write the ':' separator
    emitter.instruction("add r11, 1");                                          // advance the live output cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // publish the advanced output cursor
    emitter.label("__rt_date_OP_min_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // reload minutes
    emitter.instruction("call __rt_date_write_2digit_linux_x86_64");            // write zero-padded 2-digit minutes
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte

    // -- format: p (timezone offset as +hh:mm, or the literal 'Z' when UTC) --
    emitter.label("__rt_date_fmt_p_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer before reading the gmt offset
    emitter.instruction("mov rax, QWORD PTR [r8 + 40]");                        // load tm_gmtoff (signed seconds east of UTC, offset 40)
    emitter.instruction("test rax, rax");                                       // is the UTC offset zero?
    emitter.instruction("jnz __rt_date_p_offset_linux_x86_64");                 // non-zero offset → render exactly like 'P'
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the live output cursor
    emitter.instruction("mov BYTE PTR [r11], 90");                              // write the 'Z' (90) marking a zero UTC offset
    emitter.instruction("add r11, 1");                                          // advance the live output cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // publish the advanced output cursor
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte
    emitter.label("__rt_date_p_offset_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 80], 1");                         // colon flag = 1 (insert ':' separator like 'P')
    emitter.instruction("jmp __rt_date_OP_common_linux_x86_64");                // share the offset body with 'O'/'P'

    // -- format: B (Swatch Internet Time: beats of the UTC+1 day, 000-999) --
    emitter.label("__rt_date_fmt_B_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // load the original Unix timestamp (UTC-based)
    emitter.instruction("add rax, 3600");                                       // shift to Biel Mean Time (UTC+1)
    emitter.instruction("mov r10, 86400");                                      // seconds per day
    emitter.instruction("cqo");                                                 // sign-extend the BMT timestamp for the signed divide
    emitter.instruction("idiv r10");                                            // rdx = remainder (carries the dividend's sign)
    emitter.instruction("mov rax, rdx");                                        // keep only the remainder seconds
    emitter.instruction("test rax, rax");                                       // negative remainder (pre-epoch timestamp)?
    emitter.instruction("jge __rt_date_B_scaled_linux_x86_64");                 // non-negative → already the seconds of the BMT day
    emitter.instruction("add rax, r10");                                        // floor-mod into [0, 86400)
    emitter.label("__rt_date_B_scaled_linux_x86_64");
    emitter.instruction("imul rax, rax, 10");                                   // scale so beats = seconds*10/864 (one beat = 86.4 s)
    emitter.instruction("xor edx, edx");                                        // clear the high half before the unsigned divide
    emitter.instruction("mov r10, 864");                                        // scaled divisor for one beat
    emitter.instruction("div r10");                                             // rax = beats 0-999
    emitter.instruction("xor edx, edx");                                        // clear the high half before the hundreds divide
    emitter.instruction("mov r10, 100");                                        // split off the hundreds digit
    emitter.instruction("div r10");                                             // rax = hundreds digit, rdx = beats % 100
    emitter.instruction("mov r9, rax");                                         // keep the hundreds digit across the next divide
    emitter.instruction("mov rax, rdx");                                        // beats % 100 → dividend
    emitter.instruction("xor edx, edx");                                        // clear the high half before the tens divide
    emitter.instruction("mov r10, 10");                                         // split tens and units
    emitter.instruction("div r10");                                             // rax = tens digit, rdx = units digit
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the live output cursor
    emitter.instruction("add r9, 48");                                          // hundreds digit → ASCII
    emitter.instruction("mov BYTE PTR [r11], r9b");                             // write the hundreds digit
    emitter.instruction("add rax, 48");                                         // tens digit → ASCII
    emitter.instruction("mov BYTE PTR [r11 + 1], al");                          // write the tens digit
    emitter.instruction("add rdx, 48");                                         // units digit → ASCII
    emitter.instruction("mov BYTE PTR [r11 + 2], dl");                          // write the units digit
    emitter.instruction("add r11, 3");                                          // advance the output cursor past the three digits
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // publish the advanced output cursor
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte

    // -- format: T (timezone abbreviation from tm_zone, e.g. CEST/CET/UTC) --
    emitter.label("__rt_date_fmt_T_linux_x86_64");
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // reload the struct tm pointer
    emitter.instruction("mov rsi, QWORD PTR [r8 + 48]");                        // load tm_zone (char* abbreviation, offset 48)
    emitter.instruction("test rsi, rsi");                                       // no abbreviation available?
    emitter.instruction("jz __rt_date_T_done_linux_x86_64");                    // yes → emit nothing
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor
    emitter.label("__rt_date_T_copy_linux_x86_64");
    emitter.instruction("mov al, BYTE PTR [rsi]");                              // load one abbreviation byte
    emitter.instruction("test al, al");                                         // NUL terminator?
    emitter.instruction("jz __rt_date_T_save_linux_x86_64");                    // yes → finish
    emitter.instruction("mov BYTE PTR [r9], al");                               // store the byte into the output buffer
    emitter.instruction("add r9, 1");                                           // advance the output cursor
    emitter.instruction("add rsi, 1");                                          // advance the abbreviation pointer
    emitter.instruction("jmp __rt_date_T_copy_linux_x86_64");                   // continue copying
    emitter.label("__rt_date_T_save_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the advanced output cursor
    emitter.label("__rt_date_T_done_linux_x86_64");
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte

    // -- format: e (timezone identifier: gmdate→UTC, else the configured default zone) --
    emitter.label("__rt_date_fmt_e_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // load the UTC-vs-local flag
    emitter.instruction("test rax, rax");                                       // gmdate()?
    emitter.instruction("jnz __rt_date_e_utc_linux_x86_64");                    // yes → always report UTC
    emitter.instruction("lea rsi, [rip + _php_default_tz_len]");                // address of the configured identifier length
    emitter.instruction("mov rdx, QWORD PTR [rsi]");                            // load the configured identifier length
    emitter.instruction("test rdx, rdx");                                       // none configured?
    emitter.instruction("jz __rt_date_e_utc_linux_x86_64");                     // yes → UTC
    emitter.instruction("lea rsi, [rip + _php_tz_env]");                        // address of the configured TZ env buffer
    emitter.instruction("add rsi, 3");                                          // skip the "TZ=" prefix → identifier pointer
    emitter.instruction("jmp __rt_date_e_copy_linux_x86_64");                   // copy the configured identifier
    emitter.label("__rt_date_e_utc_linux_x86_64");
    emitter.instruction("lea rsi, [rip + _php_tz_utc]");                        // address of the literal "UTC"
    emitter.instruction("mov rdx, 3");                                          // length of "UTC"
    emitter.label("__rt_date_e_copy_linux_x86_64");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live output cursor
    emitter.instruction("xor rcx, rcx");                                        // copy index = 0
    emitter.label("__rt_date_e_loop_linux_x86_64");
    emitter.instruction("cmp rcx, rdx");                                        // copied every byte?
    emitter.instruction("jae __rt_date_e_done_linux_x86_64");                   // yes → finish
    emitter.instruction("mov al, BYTE PTR [rsi + rcx]");                        // load one identifier byte
    emitter.instruction("mov BYTE PTR [r9 + rcx], al");                         // store it into the output buffer
    emitter.instruction("add rcx, 1");                                          // advance the copy index
    emitter.instruction("jmp __rt_date_e_loop_linux_x86_64");                   // continue copying
    emitter.label("__rt_date_e_done_linux_x86_64");
    emitter.instruction("add r9, rdx");                                         // advance the output cursor by the identifier length
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // publish the advanced output cursor
    emitter.instruction("jmp __rt_date_next_linux_x86_64");                     // continue with the next format byte

    // -- helper: ISO-8601 week + year from struct tm → eax = week, r9d = year --
    emitter.label("__rt_date_iso_week_linux_x86_64");
    emitter.instruction("sub rsp, 24");                                         // scratch + 16-byte alignment for nested calls
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // struct tm pointer
    emitter.instruction("mov ecx, DWORD PTR [r8 + 24]");                        // tm_wday (0=Sunday)
    emitter.instruction("mov edx, DWORD PTR [r8 + 28]");                        // tm_yday (0-based day of year)
    emitter.instruction("mov eax, DWORD PTR [r8 + 20]");                        // tm_year (years since 1900)
    emitter.instruction("add eax, 1900");                                       // full Gregorian year
    emitter.instruction("mov DWORD PTR [rsp + 0], eax");                        // save year
    emitter.instruction("test ecx, ecx");                                       // is it Sunday?
    emitter.instruction("jne __rt_date_iso_dow_ok_linux_x86_64");               // no → keep tm_wday
    emitter.instruction("mov ecx, 7");                                          // Sunday maps to ISO weekday 7
    emitter.label("__rt_date_iso_dow_ok_linux_x86_64");
    emitter.instruction("add edx, 1");                                          // ordinal day = tm_yday + 1
    emitter.instruction("sub edx, ecx");                                        // ordinal - iso_dow
    emitter.instruction("add edx, 10");                                         // + 10 (ISO week offset)
    emitter.instruction("mov eax, edx");                                        // dividend = ordinal - iso_dow + 10
    emitter.instruction("xor edx, edx");                                        // clear the high half before dividing
    emitter.instruction("mov ecx, 7");                                          // days per week
    emitter.instruction("div ecx");                                             // eax = candidate ISO week number
    emitter.instruction("mov DWORD PTR [rsp + 8], eax");                        // save candidate week
    emitter.instruction("cmp eax, 1");                                          // week < 1 → previous year
    emitter.instruction("jl __rt_date_iso_prev_linux_x86_64");                  // handle early-January case
    emitter.instruction("mov edi, DWORD PTR [rsp + 0]");                        // weeks_in_year argument = this year
    emitter.instruction("call __rt_date_weeks_in_year_linux_x86_64");           // eax = weeks in this year
    emitter.instruction("cmp DWORD PTR [rsp + 8], eax");                        // candidate week > weeks_in_year ?
    emitter.instruction("jg __rt_date_iso_next_linux_x86_64");                  // handle late-December case
    emitter.instruction("mov eax, DWORD PTR [rsp + 8]");                        // ISO week = candidate
    emitter.instruction("mov r9d, DWORD PTR [rsp + 0]");                        // ISO year = this year
    emitter.instruction("jmp __rt_date_iso_done_linux_x86_64");                 // done
    emitter.label("__rt_date_iso_prev_linux_x86_64");
    emitter.instruction("mov eax, DWORD PTR [rsp + 0]");                        // this year
    emitter.instruction("sub eax, 1");                                          // previous year
    emitter.instruction("mov DWORD PTR [rsp + 0], eax");                        // save previous year
    emitter.instruction("mov edi, eax");                                        // weeks_in_year argument = previous year
    emitter.instruction("call __rt_date_weeks_in_year_linux_x86_64");           // eax = ISO week (last week of prev year)
    emitter.instruction("mov r9d, DWORD PTR [rsp + 0]");                        // ISO year = previous year
    emitter.instruction("jmp __rt_date_iso_done_linux_x86_64");                 // done (eax already holds the week)
    emitter.label("__rt_date_iso_next_linux_x86_64");
    emitter.instruction("mov eax, 1");                                          // ISO week = 1
    emitter.instruction("mov r9d, DWORD PTR [rsp + 0]");                        // this year ...
    emitter.instruction("add r9d, 1");                                          // ... ISO year = next year
    emitter.label("__rt_date_iso_done_linux_x86_64");
    emitter.instruction("add rsp, 24");                                         // release the scratch frame
    emitter.instruction("ret");                                                 // return eax = week, r9d = year

    // -- helper: number of ISO weeks in a year (edi = year → eax = 52 or 53) --
    emitter.label("__rt_date_weeks_in_year_linux_x86_64");
    emitter.instruction("sub rsp, 24");                                         // scratch + 16-byte alignment for nested calls
    emitter.instruction("mov DWORD PTR [rsp + 0], edi");                        // save the year argument
    emitter.instruction("call __rt_date_dow_dec31_linux_x86_64");               // eax = weekday of 31 Dec of this year
    emitter.instruction("cmp eax, 4");                                          // Thursday? → 53-week year
    emitter.instruction("je __rt_date_wiy_53_linux_x86_64");                    // yes → 53 weeks
    emitter.instruction("mov edi, DWORD PTR [rsp + 0]");                        // reload the year
    emitter.instruction("sub edi, 1");                                          // previous year
    emitter.instruction("call __rt_date_dow_dec31_linux_x86_64");               // eax = weekday of 31 Dec of previous year
    emitter.instruction("cmp eax, 3");                                          // Wednesday? → 53-week year (leap)
    emitter.instruction("je __rt_date_wiy_53_linux_x86_64");                    // yes → 53 weeks
    emitter.instruction("mov eax, 52");                                         // otherwise 52 weeks
    emitter.instruction("jmp __rt_date_wiy_done_linux_x86_64");                 // done
    emitter.label("__rt_date_wiy_53_linux_x86_64");
    emitter.instruction("mov eax, 53");                                         // 53-week year
    emitter.label("__rt_date_wiy_done_linux_x86_64");
    emitter.instruction("add rsp, 24");                                         // release the scratch frame
    emitter.instruction("ret");                                                 // return eax = 52 or 53

    // -- helper: weekday of 31 December (edi = year → eax = 0..6, 0=Sunday) --
    emitter.label("__rt_date_dow_dec31_linux_x86_64");
    emitter.instruction("mov r8d, edi");                                        // accumulator = year
    emitter.instruction("mov eax, edi");                                        // dividend = year
    emitter.instruction("xor edx, edx");                                        // clear the high half
    emitter.instruction("mov ecx, 4");                                          // divisor 4
    emitter.instruction("div ecx");                                             // eax = year / 4
    emitter.instruction("add r8d, eax");                                        // + leap-year contribution
    emitter.instruction("mov eax, edi");                                        // dividend = year
    emitter.instruction("xor edx, edx");                                        // clear the high half
    emitter.instruction("mov ecx, 100");                                        // divisor 100
    emitter.instruction("div ecx");                                             // eax = year / 100
    emitter.instruction("sub r8d, eax");                                        // - century contribution
    emitter.instruction("mov eax, edi");                                        // dividend = year
    emitter.instruction("xor edx, edx");                                        // clear the high half
    emitter.instruction("mov ecx, 400");                                        // divisor 400
    emitter.instruction("div ecx");                                             // eax = year / 400
    emitter.instruction("add r8d, eax");                                        // + 400-year contribution
    emitter.instruction("mov eax, r8d");                                        // dividend = accumulator
    emitter.instruction("xor edx, edx");                                        // clear the high half
    emitter.instruction("mov ecx, 7");                                          // days per week
    emitter.instruction("div ecx");                                             // edx = accumulator mod 7
    emitter.instruction("mov eax, edx");                                        // weekday of 31 Dec
    emitter.instruction("ret");                                                 // return eax = 0..6
}
