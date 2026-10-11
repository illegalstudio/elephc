//! Purpose:
//! Focused unit coverage for multibyte mail preparation and transport policy.
//!
//! Called from:
//! - The parent mail ABI module under Rust's test harness.
//!
//! Key details:
//! - Tests pin PHP validation, MIME bytes, envelope privacy, Date injection, and process policy.

use super::*;
use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::{MbArgV1, array::ArrayGraph}};

    /// Prepares one structured header graph without opening a configured transport.
    fn array_mail(graph: ArrayGraph) -> Result<PreparedMail, MbError> {
        let encoded = graph.encode();
        let slots = [MbArgV1::string(b"to@example.test"), MbArgV1::string(b"S"),
            MbArgV1::string(b"body"), MbArgV1::array(&encoded)];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        prepare(&args, &State::default())
    }

    /// Rejects each forbidden line break and NUL with PHP's field-specific ValueError.
    #[test]
    fn mail_array_header_values_match_php_validation() {
        for (value, detail) in [
            (b"hello\r\nBcc: attacker@example.test".as_slice(),
                b"CRLF characters that are used as a line separator and are not allowed in the header".as_slice()),
            (b"hello\nBcc: attacker@example.test", b"LF character that is not allowed in the header"),
            (b"hello\rBcc: attacker@example.test", b"CR character that is not allowed in the header"),
            (b"hello\0Bcc: attacker@example.test", b"NULL character that is not allowed in the header"),
        ] {
            let graph = ArrayGraph::new(0, vec![vec![(Key::String(b"Reply-To".to_vec()),
                Value::String(value.to_vec()))]]).expect("header graph");
            let expected = [b"Header \"Reply-To\" contains ".as_slice(), detail].concat();
            assert_eq!(array_mail(graph).err(), Some(MbError::ValueBytes(expected)));
        }
        let graph = ArrayGraph::new(0, vec![vec![(Key::String(b"X-Test".to_vec()),
            Value::String(b"hello\r\n folded\n\tmore".to_vec()))]]).expect("folded header");
        assert!(array_mail(graph).expect("folded header").bytes.windows(b"X-Test: hello\r\n folded\n\tmore".len())
            .any(|part| part == b"X-Test: hello\r\n folded\n\tmore"));
    }

    /// Rejects invalid names, numeric keys, reserved fields, and typed array members.
    #[test]
    fn mail_array_header_names_and_types_match_php_validation() {
        for (key, value, error) in [
            (Key::String(b"Bad Name".to_vec()), Value::String(b"value".to_vec()),
                MbError::ValueBytes(b"Header name \"Bad Name\" contains invalid characters".to_vec())),
            (Key::String(b"Bad:Name".to_vec()), Value::String(b"value".to_vec()),
                MbError::ValueBytes(b"Header name \"Bad:Name\" contains invalid characters".to_vec())),
            (Key::String(b"Bad\0Name".to_vec()), Value::String(b"value".to_vec()),
                MbError::ValueBytes(b"Header name \"Bad\" contains invalid characters".to_vec())),
            (Key::Int(42), Value::String(b"X-Test: value".to_vec()),
                MbError::TypeBytes(b"Header name cannot be numeric, 42 given".to_vec())),
            (Key::String(b"To".to_vec()), Value::String(b"other@example.test".to_vec()),
                MbError::Value("The additional headers cannot contain the \"To\" header".into())),
            (Key::String(b"SUBJECT".to_vec()), Value::String(b"new".to_vec()),
                MbError::Value("The additional headers cannot contain the \"Subject\" header".into())),
            (Key::String(b"X-Test".to_vec()), Value::Int(12),
                MbError::TypeBytes(b"Header \"X-Test\" must be of type array|string, int given".to_vec())),
            (Key::String(b"Cc".to_vec()), Value::Array(1),
                MbError::TypeBytes(b"Header \"cc\" must be of type string, array given".to_vec())),
        ] {
            let graph = ArrayGraph::new(0, vec![vec![(key, value)], vec![]]).expect("header graph");
            assert_eq!(array_mail(graph).err(), Some(error));
        }
        let invalid_key = ArrayGraph::new(0, vec![
            vec![(Key::String(b"X-Test".to_vec()), Value::Array(1))],
            vec![(Key::String(b"named".to_vec()), Value::String(b"one".to_vec()))],
        ]).expect("nested header");
        assert_eq!(array_mail(invalid_key).err(), Some(MbError::TypeBytes(
            b"Header \"X-Test\" must only contain numeric keys, \"named\" found".to_vec())));
        let invalid_value = ArrayGraph::new(0, vec![
            vec![(Key::String(b"X-Test".to_vec()), Value::Array(1))],
            vec![(Key::Int(0), Value::Int(12))],
        ]).expect("nested header");
        assert_eq!(array_mail(invalid_value).err(), Some(MbError::TypeBytes(
            b"Header \"X-Test\" must only contain values of type string, int found".to_vec())));
        let nested_injection = ArrayGraph::new(0, vec![
            vec![(Key::String(b"X-Test".to_vec()), Value::Array(1))],
            vec![(Key::Int(0), Value::String(b"one\r\nBcc: attacker@example.test".to_vec()))],
        ]).expect("nested header");
        assert_eq!(array_mail(nested_injection).err(), Some(MbError::ValueBytes(
            b"Header \"X-Test\" contains CRLF characters that are used as a line separator and are not allowed in the header".to_vec())));
        let repeated = ArrayGraph::new(0, vec![
            vec![(Key::String(b"X-Test".to_vec()), Value::Array(1))],
            vec![(Key::Int(0), Value::String(b"one".to_vec())),
                (Key::Int(1), Value::String(b"two".to_vec()))],
        ]).expect("nested header");
        assert!(array_mail(repeated).expect("repeated header").bytes.windows(b"X-Test: one\r\nX-Test: two\r\n".len())
            .any(|part| part == b"X-Test: one\r\nX-Test: two\r\n"));
    }

    /// Matches PHP's neutral-language MIME bytes for non-ASCII subject and body text.
    #[test]
    fn neutral_mail_matches_php_mime_output() {
        let slots = [MbArgV1::string(b"a@example.test"), MbArgV1::string("Café".as_bytes()),
            MbArgV1::string("Crème".as_bytes())];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        let message = prepare(&args, &State::default()).expect("MIME message");
        assert_eq!(message.bytes, b"To: a@example.test\r\nSubject: =?UTF-8?B?Q2Fmw6k=?=\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: BASE64\r\n\r\nQ3LDqG1l\r\n");
        assert!(message.warnings.is_empty());
    }

    /// Preserves valid RFC 822 folding while replacing other control bytes in To.
    #[test]
    fn mail_to_keeps_folded_continuations() {
        let slots = [MbArgV1::string(b"a@example.test\r\n \tCc: b@example.test\nInjected: z"),
            MbArgV1::string(b"S"), MbArgV1::string(b"body")];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        let message = prepare(&args, &State::default()).expect("MIME message");
        assert!(message.bytes.starts_with(b"To: a@example.test\r\n \tCc: b@example.test Injected: z\r\nSubject: S\r\n"));
    }

    /// Applies PHP's legacy mixed line-feed setting to the complete mail envelope.
    #[test]
    fn mail_mixed_line_endings_match_php() {
        let slots = [MbArgV1::string(b"a@example.test"), MbArgV1::string("Café".as_bytes()),
            MbArgV1::string("Crème".as_bytes())];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        let (state, diagnostics) = State::with_ini_configuration(&[(b"mail.mixed_lf_and_crlf".to_vec(), b"1".to_vec())],
            crate::state::CoreEncodingDefaults::default(), |_| Ok(()));
        assert!(diagnostics.is_empty());
        let message = prepare(&args, &state).expect("MIME message");
        assert_eq!(message.bytes, b"To: a@example.test\nSubject: =?UTF-8?B?Q2Fmw6k=?=\nMIME-Version: 1.0\nContent-Type: text/plain; charset=UTF-8\nContent-Transfer-Encoding: BASE64\n\nQ3LDqG1l\n");
    }

    /// Gives the configured extra parameters precedence without interpreting shell syntax.
    #[cfg(not(windows))]
    #[test]
    fn mail_force_parameters_replace_call_arguments() {
        let (state, diagnostics) = State::with_ini_configuration(&[
            (b"sendmail_path".to_vec(), b"/bin/true -t".to_vec()),
            (b"mail.force_extra_parameters".to_vec(), b"-f forced@example.test".to_vec()),
        ], crate::state::CoreEncodingDefaults::default(), |_| Ok(()));
        assert!(diagnostics.is_empty());
        let configured = command(&state, Some(b"-f caller@example.test")).expect("mail command");
        assert_eq!(configured.get_program(), "/bin/true");
        assert_eq!(configured.get_args().collect::<Vec<_>>(), ["-t", "-f", "forced@example.test"]);
        assert_eq!(command(&state, Some(b"bad\0parameter")).err(),
            Some(MbError::argument("mb_send_mail", 5, "additional_params", "must not contain any null bytes")));
    }

    /// Keeps shell punctuation literal in forced parameters rather than invoking a shell.
    #[cfg(not(windows))]
    #[test]
    fn mail_force_parameters_use_literal_argv() {
        let (state, _) = State::with_ini_configuration(&[
            (b"sendmail_path".to_vec(), b"/bin/true".to_vec()),
            (b"mail.force_extra_parameters".to_vec(), b"'-f quoted@example.test' ; echo".to_vec()),
        ], crate::state::CoreEncodingDefaults::default(), |_| Ok(()));
        let command = command(&state, None).expect("mail command");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["'-f", "quoted@example.test'", ";", "echo"]);
    }

    /// Matches PHP's Windows command escaping for additional sendmail parameters.
    #[test]
    fn windows_additional_parameters_escape_shell_metacharacters() {
        assert_eq!(windows_escape_shell_cmd(b"-f a&b@example.test"), b"-f a^&b@example.test");
        assert_eq!(windows_escape_shell_cmd(br#"%PATH% "x" !"#), br#"^%PATH^% ^"x^" ^!"#);
        assert_eq!(windows_escape_shell_cmd(b"\\\""), b"^\\^\"");
        assert_eq!(windows_escape_shell_cmd(b"\\\\"), b"^\\^\\");
    }

    /// Matches php-src's literal `popen_ex()` `/s /c "<command>"` tail.
    #[test]
    fn windows_sendmail_uses_raw_php_command_tail() {
        assert_eq!(windows_command_tail(r#""C:\Program Files\Mailer é.exe" "a\"b\\""#.as_bytes()),
            r#"/s /c ""C:\Program Files\Mailer é.exe" "a\"b\\"""#.as_bytes());
    }

    /// Preserves exact PHP SMTP warning text and raw server-response bytes.
    #[test]
    fn smtp_failure_warnings_match_php_families() {
        let response = smtp_warning(smtp::SmtpError::Response(b"554 no relay".to_vec()), b"mail.test", 25);
        assert_eq!(response.diagnostics, b"Warning: mb_send_mail(): SMTP server response: 554 no relay\n");
        let raw_response = smtp_warning(smtp::SmtpError::Response(b"554 invalid \xff".to_vec()), b"mail.test", 25);
        assert_eq!(raw_response.diagnostics, b"Warning: mb_send_mail(): SMTP server response: 554 invalid \xff\n");
        let connect = smtp_warning(smtp::SmtpError::Connect, b"mail.test", 0);
        assert_eq!(connect.diagnostics, b"Warning: mb_send_mail(): Failed to connect to mailserver at \"mail.test\" port 25, verify your \"SMTP\" and \"smtp_port\" setting in php.ini or use ini_set()\n");
        assert_eq!(smtp_warning(smtp::SmtpError::Send, b"", 25).diagnostics, b"Warning: mb_send_mail(): Failed to Send\n");
        assert_eq!(smtp_warning(smtp::SmtpError::Receive, b"", 25).diagnostics, b"Warning: mb_send_mail(): Failed to Receive\n");
        assert_eq!(smtp_warning(smtp::SmtpError::Protocol, b"", 25).diagnostics, b"Warning: mb_send_mail(): Server Error\n");
    }

    /// Rejects a missing Windows envelope sender before attempting any SMTP connection.
    #[cfg(windows)]
    #[test]
    fn windows_smtp_requires_sender_before_connecting() {
        let settings = [
            (b"SMTP".to_vec(), b"203.0.113.1".to_vec()),
            (b"smtp_port".to_vec(), b"9".to_vec()),
        ];
        let state = State::with_ini_configuration(&settings, crate::state::CoreEncodingDefaults::default(), |_| Ok(())).0;
        let slots = [MbArgV1::string(b"to@example.test"), MbArgV1::string(b"subject"), MbArgV1::string(b"body")];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.unwrap();
        let result = send_prepared(prepare(&args, &state).unwrap(), &state, None);
        assert_eq!((result.kind, result.value), (super::super::RESULT_BOOL, 0));
        assert_eq!(result.diagnostics,
            b"Warning: mb_send_mail(): \"sendmail_from\" not set in php.ini or custom \"From:\" header missing\n");
    }

    /// PHP's Windows compatibility path accepts legacy ANSI bytes after strict UTF-8 fails.
    #[cfg(windows)]
    #[test]
    fn windows_mail_transport_accepts_php_ansi_fallback_bytes() {
        assert!(command_argument(b"sendmail-\xff").is_ok());
        assert_eq!(command_argument("sendmail-é-日本語".as_bytes()).unwrap(), "sendmail-é-日本語");
    }

    /// Matches both PHP warnings and the fallback MIME conversion order.
    #[test]
    fn mail_unsupported_headers_warn_before_delivery() {
        use elephc_builtin_contract::mbstring_abi::array::ArrayGraph;
        let graph = ArrayGraph::new(0, vec![vec![
            (Key::String(b"Content-Type".to_vec()), Value::String(b"text/plain; charset=X-NOPE".to_vec())),
            (Key::String(b"Content-Transfer-Encoding".to_vec()), Value::String(b"X-NOPE".to_vec())),
        ]]).expect("headers");
        let encoded = graph.encode();
        let slots = [MbArgV1::string(b"a@example.test"), MbArgV1::string("Café".as_bytes()),
            MbArgV1::string(b"body"), MbArgV1::array(&encoded)];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        let prepared = prepare(&args, &State::default()).expect("MIME message");
        assert_eq!(prepared.warnings, [
            b"mb_send_mail(): Unsupported charset \"X-NOPE\" - will be regarded as ascii".to_vec(),
            b"mb_send_mail(): Unsupported transfer encoding \"X-NOPE\" - will be regarded as 8bit".to_vec(),
        ]);
        assert!(prepared.bytes.starts_with(b"To: a@example.test\r\nSubject: =?US-ASCII?B?Q2FmPw==?=\r\n"));
    }

    /// Rejects embedded NULs before opening a mail transport.
    #[test]
    fn mail_rejects_nul_subject() {
        let slots = [MbArgV1::string(b"a@example.test"), MbArgV1::string(b"a\0b"), MbArgV1::string(b"body")];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        assert!(matches!(prepare(&args, &State::default()), Err(error) if error == MbError::argument("mb_send_mail", 2,
            "subject", "must not contain any null bytes")));
    }

    /// Adds an authoritative Date once while preserving a caller-supplied Date header.
    #[test]
    fn smtp_date_is_added_only_when_absent() {
        let slots = [MbArgV1::string(b"a@example.test"), MbArgV1::string(b"subject"), MbArgV1::string(b"body")];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.unwrap();
        let mut prepared = prepare(&args, &State::default()).unwrap();
        prepared.add_date(b"Thu, 02 Oct 2026 12:34:56 +0200");
        prepared.add_date(b"wrong");
        assert_eq!(prepared.smtp_bytes.windows(6).filter(|part| *part == b"Date: ").count(), 1);
        assert!(prepared.smtp_bytes.starts_with(b"Date: Thu, 02 Oct 2026 12:34:56 +0200\r\n"));
    }

    /// Keeps Bcc envelope recipients private while preserving quoted commas and sender identity.
    #[test]
    fn smtp_envelope_collects_recipients_and_strips_bcc() {
        let graph = ArrayGraph::new(0, vec![vec![
            (Key::String(b"Cc".to_vec()), Value::String(b"\"Doe, Jane\" <jane@example.test>".to_vec())),
            (Key::String(b"Bcc".to_vec()), Value::String(b"hidden@example.test".to_vec())),
            (Key::String(b"From".to_vec()), Value::String(b"Sender <from@example.test>".to_vec())),
        ]]).unwrap();
        let encoded = graph.encode();
        let slots = [MbArgV1::string(b"to@example.test"), MbArgV1::string(b"subject"),
            MbArgV1::string(b"body"), MbArgV1::array(&encoded)];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.unwrap();
        let prepared = prepare(&args, &State::default()).unwrap();
        assert_eq!(prepared.recipients, [b"to@example.test".to_vec(), b"jane@example.test".to_vec(), b"hidden@example.test".to_vec()]);
        assert_eq!(prepared.sender.as_deref(), Some(b"from@example.test".as_slice()));
        assert!(!prepared.smtp_bytes.windows(5).any(|part| part.eq_ignore_ascii_case(b"bcc: ")));
    }

    /// Delivers through a configured harmless process and preserves a true result.
    #[cfg(not(windows))]
    #[test]
    fn mail_uses_configured_transport() {
        let slots = [MbArgV1::string(b"a@example.test"), MbArgV1::string(b"subject"), MbArgV1::string(b"body")];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        let mut state = State::with_ini_configuration(&[(b"sendmail_path".to_vec(), b"/bin/cat".to_vec())],
            crate::state::CoreEncodingDefaults::default(), |_| Ok(())).0;
        let result = dispatch(&args, &mut state);
        assert_eq!(result.kind, super::super::RESULT_BOOL);
        assert_eq!(result.value, 1);
        assert!(result.diagnostics.is_empty());
    }

    /// Routes the Windows default through native SMTP with the authoritative Date bytes.
    #[cfg(windows)]
    #[test]
    fn windows_default_transport_uses_smtp_state_and_date() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            socket.write_all(b"220 ready\r\n").unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut transcript = Vec::new();
            loop {
                let mut line = Vec::new();
                if reader.read_until(b'\n', &mut line).unwrap() == 0 { break; }
                transcript.extend_from_slice(&line);
                if line == b"DATA\r\n" { socket.write_all(b"354 data\r\n").unwrap(); }
                else if line == b".\r\n" { socket.write_all(b"250 queued\r\n").unwrap(); }
                else if line == b"QUIT\r\n" { socket.write_all(b"221 bye\r\n").unwrap(); break; }
                else if line.starts_with(b"HELO ") || line.starts_with(b"MAIL FROM:") || line.starts_with(b"RCPT TO:") { socket.write_all(b"250 ok\r\n").unwrap(); }
            }
            transcript
        });
        let settings = [
            (b"SMTP".to_vec(), b"127.0.0.1".to_vec()),
            (b"smtp_port".to_vec(), port.to_string().into_bytes()),
            (b"sendmail_from".to_vec(), b"sender@example.test".to_vec()),
        ];
        let state = State::with_ini_configuration(&settings, crate::state::CoreEncodingDefaults::default(), |_| Ok(())).0;
        let slots = [MbArgV1::string(b"to@example.test"), MbArgV1::string(b"subject"), MbArgV1::string(b"body")];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.unwrap();
        let mut prepared = prepare(&args, &state).unwrap();
        prepared.add_date(b"Thu, 02 Oct 2026 12:34:56 +0200");
        let result = send_prepared(prepared, &state, None);
        assert_eq!((result.kind, result.value), (super::super::RESULT_BOOL, 1));
        let transcript = server.join().unwrap();
        let date = b"Date: Thu, 02 Oct 2026 12:34:56 +0200\r\n";
        let from = b"From: sender@example.test\r\n";
        assert!(transcript.windows(date.len()).any(|part| part == date));
        assert!(transcript.windows(from.len()).any(|part| part == from));
    }

    /// Preserves a quoted configured Windows command and escapes only appended parameters.
    #[cfg(windows)]
    #[test]
    fn windows_configured_sendmail_uses_cmd_command_line() {
        let settings = [
            (b"sendmail_path".to_vec(), br#""C:\Program Files\mailer.exe" --mode smtp"#.to_vec()),
            (b"mail.force_extra_parameters".to_vec(), b"-f a&b@example.test".to_vec()),
        ];
        let state = State::with_ini_configuration(&settings, crate::state::CoreEncodingDefaults::default(), |_| Ok(())).0;
        let command = command(&state, Some(b"ignored")).unwrap();
        assert_eq!(command.get_program(), "cmd.exe");
        assert_eq!(command.get_args().collect::<Vec<_>>(),
            ["/s /c \"\"C:\\Program Files\\mailer.exe\" --mode smtp -f a^&b@example.test\""]);
    }

    /// Executes the configured Windows process transport and streams the prepared message.
    #[cfg(windows)]
    #[test]
    fn windows_configured_process_transport_runs_to_completion() {
        let settings = [(b"sendmail_path".to_vec(), b"more > NUL".to_vec())];
        let state = State::with_ini_configuration(&settings, crate::state::CoreEncodingDefaults::default(), |_| Ok(())).0;
        let slots = [MbArgV1::string(b"to@example.test"), MbArgV1::string(b"subject"), MbArgV1::string(b"body")];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.unwrap();
        let result = send_prepared(prepare(&args, &state).unwrap(), &state, None);
        assert_eq!((result.kind, result.value), (super::super::RESULT_BOOL, 1));
        assert!(result.diagnostics.is_empty());
    }

    /// Executes php-src's raw cmd tail through a quoted Unicode path containing spaces.
    #[cfg(windows)]
    #[test]
    fn windows_quoted_unicode_sendmail_path_executes() {
        let directory = std::env::temp_dir().join(format!(
            "elephc mail été {}", std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let script = directory.join("mailer 日本語.cmd");
        std::fs::write(&script, b"@more > NUL\r\n@exit /b 0\r\n").unwrap();
        let setting = format!("\"{}\"", script.display()).into_bytes();
        let state = State::with_ini_configuration(
            &[(b"sendmail_path".to_vec(), setting)],
            crate::state::CoreEncodingDefaults::default(),
            |_| Ok(()),
        ).0;
        let slots = [MbArgV1::string(b"to@example.test"), MbArgV1::string(b"subject"),
            MbArgV1::string(b"body")];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.unwrap();
        let result = send_prepared(prepare(&args, &state).unwrap(), &state, None);
        let _ = std::fs::remove_dir_all(&directory);
        assert_eq!((result.kind, result.value), (super::super::RESULT_BOOL, 1));
        assert!(result.diagnostics.is_empty());
    }

    /// A missing delivery program is a cmd exit failure, matching PHP's successful popen.
    #[cfg(windows)]
    #[test]
    fn windows_missing_sendmail_program_reports_cmd_exit() {
        let state = State::with_ini_configuration(
            &[(b"sendmail_path".to_vec(), b"elephc-mail-command-does-not-exist-9f31".to_vec())],
            crate::state::CoreEncodingDefaults::default(),
            |_| Ok(()),
        ).0;
        let slots = [MbArgV1::string(b"to@example.test"), MbArgV1::string(b"subject"),
            MbArgV1::string(b"body")];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.unwrap();
        let result = send_prepared(prepare(&args, &state).unwrap(), &state, None);
        assert_eq!((result.kind, result.value), (super::super::RESULT_BOOL, 0));
        let status = command(&state, None).unwrap()
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .status().expect("run the native command-shell exit-code oracle");
        let code = status.code().expect("native shell reports an exit code");
        assert_ne!(code, 0, "the deliberately missing command must fail");
        assert_eq!(result.diagnostics,
            format!("Warning: mb_send_mail(): Sendmail exited with non-zero exit code {code}\n").into_bytes());
    }
