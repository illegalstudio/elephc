//! Purpose:
//! Windows PE integration coverage for native SMTP delivery and timezone-aware mail dates.
//!
//! Called from:
//! - The parent Windows PE codegen test module.
//!
//! Key details:
//! - Native Windows uses a bounded in-process SMTP peer; non-Windows hosts remain compile-only.
//! - Two deliveries distinguish AOT PHP timezone state from eval-context timezone state.

use super::*;

const TIMEZONE_MAIL_SOURCE: &str = r#"<?php
date_default_timezone_set("Etc/GMT-2");
$a = mb_send_mail("to@example.test", "AOT", "body", [
    "Bcc" => "hidden@example.test",
]);
$fragment = $argc > 0 ? 'date_default_timezone_set("Etc/GMT-3"); return mb_send_mail("to@example.test", "Eval", "body", ["Bcc" => "hidden@example.test"]);' : '';
$b = eval($fragment);
echo $a ? "1" : "0", $b ? "1" : "0";
"#;

/// Executes the V8 mail-Date callback through both AOT and eval SMTP deliveries.
#[test]
fn test_windows_smtp_dates_follow_aot_and_eval_timezones() {
    #[cfg(not(windows))]
    {
        compile_windows_pe(TIMEZONE_MAIL_SOURCE);
    }

    #[cfg(windows)]
    {
        use std::io::{BufRead, BufReader, Write};
        use std::net::{TcpListener, TcpStream};
        use std::time::{Duration, Instant};

        fn accept_bounded(listener: &TcpListener) -> TcpStream {
            let deadline = Instant::now() + Duration::from_secs(120);
            loop {
                match listener.accept() {
                    Ok((stream, _)) => return stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                    Err(error) => panic!("accept Windows SMTP client: {error}"),
                }
            }
        }

        fn receive_message(mut socket: TcpStream) -> Vec<u8> {
            socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            socket.set_write_timeout(Some(Duration::from_secs(10))).unwrap();
            socket.write_all(b"220 fixture ready\r\n").unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut data = Vec::new();
            let mut in_data = false;
            loop {
                let mut line = Vec::new();
                assert_ne!(reader.read_until(b'\n', &mut line).unwrap(), 0, "SMTP client closed early");
                if in_data {
                    if line == b".\r\n" {
                        socket.write_all(b"250 queued\r\n").unwrap();
                        in_data = false;
                    } else {
                        data.extend_from_slice(&line);
                    }
                } else if line == b"DATA\r\n" {
                    socket.write_all(b"354 continue\r\n").unwrap();
                    in_data = true;
                } else if line == b"QUIT\r\n" {
                    socket.write_all(b"221 bye\r\n").unwrap();
                    return data;
                } else if line.starts_with(b"HELO ") || line.starts_with(b"MAIL FROM:")
                    || line.starts_with(b"RCPT TO:") {
                    socket.write_all(b"250 ok\r\n").unwrap();
                } else {
                    panic!("unexpected SMTP command: {:?}", String::from_utf8_lossy(&line));
                }
            }
        }

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind Windows SMTP fixture");
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            [receive_message(accept_bounded(&listener)), receive_message(accept_bounded(&listener))]
        });
        let port_setting = format!("smtp_port={port}");
        compile_and_run_windows_expect_code_with_options(
            TIMEZONE_MAIL_SOURCE,
            "11",
            0,
            &[],
            &["--ini", "SMTP=127.0.0.1", "--ini", port_setting.as_str(),
                "--ini", "sendmail_from=sender@example.test"],
            &[],
            None,
        );
        let messages = server.join().expect("join Windows SMTP fixture");
        for (message, offset) in messages.iter().zip([b"+0200".as_slice(), b"+0300".as_slice()]) {
            let dates: Vec<_> = message.split(|byte| *byte == b'\n')
                .filter(|line| line.starts_with(b"Date: ")).collect();
            assert_eq!(dates.len(), 1, "expected one Date header: {}", String::from_utf8_lossy(message));
            assert!(dates[0].strip_suffix(b"\r").unwrap_or(dates[0]).ends_with(offset),
                "wrong mail timezone: {}", String::from_utf8_lossy(dates[0]));
            assert!(!message.windows(5).any(|part| part.eq_ignore_ascii_case(b"Bcc: ")),
                "Bcc leaked into SMTP DATA");
        }
    }
}
