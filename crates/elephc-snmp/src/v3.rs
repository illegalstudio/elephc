//! Purpose:
//! SNMPv3 USM request adapter over async-snmp.
//!
//! Called from:
//! - `crate::elephc_snmp_request` when the session version is 3.
//!
//! Key details:
//! - async-snmp handles engine discovery, USM authentication, time windows and
//!   DES/AES-128/192/256 privacy. This module only maps elephc's string
//!   protocol names and session fields onto `UsmConfig`/`Client`.
//! - A current-thread Tokio runtime is created per request; the compiler's
//!   bridge runs synchronously from PHP.

use std::str::FromStr;
use std::time::Duration;

use async_snmp::{
    oid::Oid, AuthProtocol, Client, PrivProtocol, Retry, UsmConfig, Value as AsyncValue,
};

use crate::client::{self, Response, Varbind, Value};
use crate::Session;

/// Maps an auth protocol name to async-snmp's enum.
fn auth_protocol(name: &str) -> Option<AuthProtocol> {
    match name.to_ascii_uppercase().as_str() {
        "MD5" => Some(AuthProtocol::Md5),
        "SHA" | "SHA1" | "SHA-1" => Some(AuthProtocol::Sha1),
        "SHA224" | "SHA-224" => Some(AuthProtocol::Sha224),
        "SHA256" | "SHA-256" => Some(AuthProtocol::Sha256),
        "SHA384" | "SHA-384" => Some(AuthProtocol::Sha384),
        "SHA512" | "SHA-512" => Some(AuthProtocol::Sha512),
        _ => None,
    }
}

/// Maps a privacy protocol name to async-snmp's enum.
fn priv_protocol(name: &str) -> Option<PrivProtocol> {
    match name.to_ascii_uppercase().as_str() {
        "DES" => Some(PrivProtocol::Des),
        "AES" | "AES128" | "AES-128" => Some(PrivProtocol::Aes128),
        "AES192" | "AES-192" => Some(PrivProtocol::Aes192Blumenthal),
        "AES256" | "AES-256" => Some(PrivProtocol::Aes256Blumenthal),
        "AES192C" | "AES-192-C" | "AES192-REEDER" => Some(PrivProtocol::Aes192Reeder),
        "AES256C" | "AES-256-C" | "AES256-REEDER" => Some(PrivProtocol::Aes256Reeder),
        _ => None,
    }
}

/// Converts one async-snmp value into the bridge's typed value.
fn convert_value(value: AsyncValue) -> Value {
    match value {
        AsyncValue::Integer(value) => Value::Int(i64::from(value)),
        AsyncValue::OctetString(value) | AsyncValue::Opaque(value) | AsyncValue::Nsap(value) => {
            Value::Bytes(value.to_vec())
        }
        AsyncValue::Null => Value::Null,
        AsyncValue::ObjectIdentifier(value) => Value::Oid(format!(".{value}")),
        AsyncValue::IpAddress(bytes) => Value::Ip(format!(
            "{}.{}.{}.{}",
            bytes[0], bytes[1], bytes[2], bytes[3]
        )),
        AsyncValue::Counter32(value)
        | AsyncValue::Gauge32(value)
        | AsyncValue::UInteger32(value)
        | AsyncValue::TimeTicks(value) => Value::Int(i64::from(value)),
        AsyncValue::Counter64(value) => {
            if value <= i64::MAX as u64 {
                Value::Int(value as i64)
            } else {
                Value::Bytes(value.to_string().into_bytes())
            }
        }
        AsyncValue::NoSuchObject => Value::NoSuchObject,
        AsyncValue::NoSuchInstance => Value::NoSuchInstance,
        AsyncValue::EndOfMibView => Value::EndOfMibView,
        _ => Value::Null,
    }
}

/// Builds the USM config from the session's security setters.
fn usm_config(session: &Session) -> Result<UsmConfig, String> {
    let mut config = UsmConfig::new(session.community.clone());
    if let Some((protocol, password)) = &session.auth {
        let protocol = auth_protocol(protocol)
            .ok_or_else(|| "unsupported SNMPv3 auth protocol".to_string())?;
        config = if let Some((privacy, privacy_password)) = &session.privacy {
            let privacy = priv_protocol(privacy)
                .ok_or_else(|| "unsupported SNMPv3 privacy protocol".to_string())?;
            config
                .auth_priv(protocol, password, privacy, privacy_password)
                .map_err(|error| error.to_string())?
        } else {
            config.auth(protocol, password).map_err(|error| error.to_string())?
        };
    } else if session.privacy.is_some() {
        return Err("SNMPv3 privacy requires authentication".to_string());
    }
    if !session.context_name.is_empty() {
        config = config.context_name(session.context_name.clone());
    }
    Ok(config)
}

/// Performs one v3 GET/GETNEXT request.
pub(crate) fn request(
    session: &Session,
    oid: &str,
    pdu: i64,
) -> Result<Response, String> {
    let (host, port) = client::endpoint(&session.host)?;
    let config = usm_config(session)?;
    let oid = Oid::from_str(oid).map_err(|error| error.to_string())?;
    let timeout = Duration::from_millis(session.timeout_ms.max(1));
    let retries = if session.retries < 0 { 5 } else { session.retries.max(0) };
    let retry = Retry::fixed(u32::try_from(retries).unwrap_or(5), Duration::ZERO)
        .map_err(|error| error.to_string())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("create SNMPv3 runtime: {error}"))?;
    runtime.block_on(async move {
        let client = Client::builder((host.as_str(), port), config)
            .request_timeout(timeout)
            .retry(retry)
            .connect()
            .await
            .map_err(|error| error.to_string())?;
        let response = if pdu == client::GET_NEXT_REQUEST {
            client.get_next(&oid).await.map_err(|error| error.to_string())?
        } else {
            client.get(&oid).await.map_err(|error| error.to_string())?
        };
        let mut varbinds = Vec::with_capacity(response.varbinds.len());
        for varbind in response.varbinds {
            varbinds.push(Varbind {
                oid: format!(".{}", varbind.oid),
                value: convert_value(varbind.value),
            });
        }
        Ok(Response {
            error_status: 0,
            error_index: 0,
            varbinds,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_snmp::SecurityLevel;

    /// PHP protocol names map to the expected USM algorithms.
    #[test]
    fn protocol_names_cover_php_and_cisco_spellings() {
        assert_eq!(auth_protocol("SHA-256"), Some(AuthProtocol::Sha256));
        assert_eq!(auth_protocol("sha512"), Some(AuthProtocol::Sha512));
        assert_eq!(
            priv_protocol("AES-256"),
            Some(PrivProtocol::Aes256Blumenthal)
        );
        assert_eq!(priv_protocol("AES256C"), Some(PrivProtocol::Aes256Reeder));
        assert_eq!(
            priv_protocol("AES-192"),
            Some(PrivProtocol::Aes192Blumenthal)
        );
        assert_eq!(priv_protocol("bogus"), None);
    }

    /// A session with auth and AES-256 privacy builds an authPriv USM config.
    #[test]
    fn session_builds_auth_priv_config() {
        let session = Session {
            version: 3,
            host: "127.0.0.1:161".to_string(),
            community: b"user".to_vec(),
            timeout_ms: 1_000,
            retries: 1,
            auth: Some(("SHA-256".to_string(), b"authpass".to_vec())),
            privacy: Some(("AES-256".to_string(), b"privpass".to_vec())),
            context_name: Vec::new(),
            context_engine: Vec::new(),
        };
        let config = usm_config(&session).expect("valid config");
        assert_eq!(config.security_level(), SecurityLevel::AuthPriv);
    }
}
