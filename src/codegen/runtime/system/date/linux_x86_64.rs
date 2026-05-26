//! Purpose:
//! Emits the `__rt_date`, `__rt_date_have_time_linux_x86_64` runtime helper assembly for linux Linux x86 64.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::system::date::emit_date()` for Linux x86_64 targets.
//!
//! Key details:
//! - Formatting reads libc tm fields and fixed date tables using Linux x86_64 register conventions.

use crate::codegen::emit::Emitter;

/// Emits the `__rt_date` and `__rt_date_have_time_linux_x86_64` runtime helpers for Linux x86_64.
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
/// byte-by-byte dispatching each token ('Y', 'm', 'd', 'H', 'i', 's', 'j', 'n', 'G',
/// 'g', 'N', 'A', 'a', 'U', 'l', 'D', 'F', 'M') to a dedicated helper that appends
/// the formatted field to the shared concat buffer. Non-token bytes are copied
/// verbatim. The global concat-buffer offset is updated atomically before return.
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
    emitter.comment("--- runtime: date ---");
    emitter.label_global("__rt_date");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the date formatter uses stack-backed locals and helpers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved timestamp, format metadata, and decimal scratch buffer
    emitter.instruction("sub rsp, 128");                                        // reserve aligned local storage for the formatter state plus a small decimal scratch buffer

    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the requested Unix timestamp so helper paths can reload it after libc and formatting calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the format-string pointer so the main loop can reload it without depending on caller-saved registers
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the format-string length so the loop bound survives helper calls
    emitter.instruction("cmp rax, -1");                                         // check whether the builtin requested "current time" instead of an explicit timestamp
    emitter.instruction("jne __rt_date_have_time_linux_x86_64");                // skip the libc time() query when the caller already supplied an explicit Unix timestamp
    emitter.instruction("xor edi, edi");                                        // pass NULL to libc time() so it only returns the current Unix timestamp value
    emitter.instruction("call time");                                           // query libc for the current Unix timestamp when PHP date() was called without an explicit timestamp
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // store the current Unix timestamp so the rest of the formatter can treat both code paths uniformly

    emitter.label("__rt_date_have_time_linux_x86_64");
    emitter.instruction("lea rdi, [rbp - 8]");                                  // pass a pointer to the saved Unix timestamp as the first argument to libc localtime()
    emitter.instruction("call localtime");                                      // decompose the Unix timestamp into libc's struct tm fields in the current local timezone
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the returned struct tm pointer so each format-token branch can reload the decomposed calendar fields

    emitter.instruction("mov r8, QWORD PTR [rip + _concat_off]");               // load the current concat-buffer offset before appending the formatted date output
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // preserve the original concat-buffer offset for the final global offset update
    emitter.instruction("lea r9, [rip + _concat_buf]");                         // load the base address of the shared concat buffer used for transient string results
    emitter.instruction("add r9, r8");                                          // compute the initial write cursor inside the concat buffer from the saved relative offset
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save the live write cursor so every token helper can append to the same destination buffer
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the formatted string start pointer for the final return value
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // start scanning the format string at byte index zero

    emitter.label("__rt_date_loop_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the current format-string byte index before checking for loop completion
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // stop once the format-string byte index reaches the saved format length
    emitter.instruction("jae __rt_date_done_linux_x86_64");                     // finish the formatter once every format byte has been consumed
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the format-string pointer before reading the current format character
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // load the current format character as an unsigned byte for the token dispatch ladder

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
    emitter.instruction("lea r9, [rip + _day_names]");                          // load the base address of the runtime weekday-name lookup table
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
    emitter.instruction("lea r9, [rip + _day_names]");                          // load the base address of the runtime weekday-name lookup table
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
    emitter.instruction("lea r9, [rip + _month_names]");                        // load the base address of the runtime month-name lookup table
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
    emitter.instruction("lea r9, [rip + _month_names]");                        // load the base address of the runtime month-name lookup table
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

    emitter.label("__rt_date_next_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the current format-string byte index before stepping to the next token or literal
    emitter.instruction("add rcx, 1");                                          // advance the format-string byte index after consuming one token or literal character
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // publish the advanced format-string byte index for the next loop iteration
    emitter.instruction("jmp __rt_date_loop_linux_x86_64");                     // continue scanning the format string until every byte has been consumed

    emitter.label("__rt_date_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return the formatted-string start pointer in the standard x86_64 string result register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the live output cursor so the final string length can be computed from the written byte count
    emitter.instruction("sub rdx, rax");                                        // compute the formatted-string length from the distance between the output cursor and the start pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // reload the original concat-buffer offset that was active before formatting started
    emitter.instruction("add r8, rdx");                                         // advance the global concat-buffer offset by the number of bytes written by the formatter
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r8");               // publish the updated concat-buffer offset for later transient string helpers
    emitter.instruction("add rsp, 128");                                        // release the formatter locals and decimal scratch buffer before returning
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
}
