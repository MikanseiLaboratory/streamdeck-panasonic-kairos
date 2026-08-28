//! Action settings as persisted by Stream Deck (SDPI `setting=` keys).

use panasonic_kairos::{Credentials, HttpConfig};
use serde::{Deserialize, Serialize};

fn default_connection_pick() -> String {
    "manual".to_string()
}

fn de_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    Ok(match v {
        None => false,
        Some(serde_json::Value::Bool(b)) => b,
        Some(serde_json::Value::String(s)) => matches!(s.as_str(), "true" | "1"),
        Some(serde_json::Value::Number(n)) => n.as_u64() == Some(1),
        Some(_) => false,
    })
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ActionSettings {
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: String,
    #[serde(default)]
    pub password: String,
    #[serde(default, deserialize_with = "de_bool")]
    pub https: bool,
    #[serde(default)]
    pub macro_id: String,
    #[serde(default)]
    pub scene_id: String,
    #[serde(default)]
    pub snapshot_id: String,
    #[serde(default)]
    pub action_id: String,
    #[serde(default)]
    pub aux_id: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub layer_id: String,
    /// `sourceA` or `sourceB`.
    #[serde(default)]
    pub bus: String,
    #[serde(default)]
    pub multiviewer_id: String,
    #[serde(default)]
    pub preset_id: String,
    /// `play`, `stop`, `record`, or `stop_record`.
    #[serde(default)]
    pub macro_state: String,
    #[serde(default)]
    pub still_id: String,
    #[serde(default)]
    pub player_id: String,
    #[serde(default)]
    pub player_op: String,
    /// `master` or `Channel 1` … `Channel 16`.
    #[serde(default)]
    pub audio_target: String,
    /// `0` unmute / `1` mute.
    #[serde(default)]
    pub audio_mute: String,
    #[serde(default)]
    pub input_id: String,
    /// Empty uses 3005.
    #[serde(default)]
    pub tcp_port: String,
    /// `manual` or `host\\tport\\tpassword\\thttps[\\ttcp_port]`.
    #[serde(default = "default_connection_pick")]
    pub connection_pick: String,
}

impl ActionSettings {
    pub fn connection(&self) -> (String, String, String, bool) {
        let (host, port, password, https, _) = self.connection_full();
        (host, port, password, https)
    }

    pub fn connection_full(&self) -> (String, String, String, bool, String) {
        let pick = self.connection_pick.trim();
        if pick != "manual" && !pick.is_empty() {
            let mut parts = pick.split('\t');
            let host = parts.next().unwrap_or("").trim().to_string();
            let port = parts.next().unwrap_or("").trim().to_string();
            let password = parts.next().unwrap_or("").to_string();
            let https = matches!(parts.next(), Some("1") | Some("true"));
            let tcp_port = parts.next().unwrap_or("").trim().to_string();
            if !host.is_empty() {
                return (host, port, password, https, tcp_port);
            }
        }
        (
            self.host.trim().to_string(),
            self.port.trim().to_string(),
            self.password.trim().to_string(),
            self.https,
            self.tcp_port.trim().to_string(),
        )
    }

    pub fn tcp_port(&self) -> u16 {
        let (_, _, _, _, tcp) = self.connection_full();
        parse_tcp_port(&tcp)
    }

    pub fn http_config(&self) -> HttpConfig {
        let (host, port, password, https) = self.connection();
        let addr = if port.is_empty() {
            host
        } else {
            format!("{host}:{port}")
        };
        HttpConfig::new(addr)
            .with_https(https)
            .with_credentials(Credentials::password(password))
            .with_timeout_ms(8_000)
    }
}

pub fn parse_tcp_port(value: &str) -> u16 {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        3005
    } else {
        trimmed.parse().unwrap_or(3005)
    }
}

#[derive(Debug, Deserialize)]
pub struct PiMessage {
    #[serde(default)]
    pub event: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListItem {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EndpointInfo {
    pub host: String,
    pub port: String,
    pub password: String,
    pub https: bool,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct PiOut {
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<ListItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoints: Option<Vec<EndpointInfo>>,
}

impl PiOut {
    pub fn items(event: impl Into<String>, items: Vec<ListItem>) -> Self {
        Self {
            event: event.into(),
            items: Some(items),
            connected: None,
            status: None,
            endpoints: None,
        }
    }

    pub fn state(connected: bool, status: impl Into<String>, endpoints: Vec<EndpointInfo>) -> Self {
        Self {
            event: "kairos_state".into(),
            items: None,
            connected: Some(connected),
            status: Some(status.into()),
            endpoints: Some(endpoints),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_accepts_bool_or_string() {
        let a: ActionSettings = serde_json::from_str(r#"{"https":true}"#).unwrap();
        assert!(a.https);
        let b: ActionSettings = serde_json::from_str(r#"{"https":"true"}"#).unwrap();
        assert!(b.https);
    }

    #[test]
    fn defaults_host_and_port() {
        let s: ActionSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.host, "");
        assert_eq!(s.port, "");
        assert!(!s.https);
        assert_eq!(s.connection_pick, "manual");
        let (host, port, _, _) = s.connection();
        assert!(host.is_empty());
        assert!(port.is_empty());
    }

    #[test]
    fn connection_pick_overrides_host_fields() {
        let s: ActionSettings = serde_json::from_str(
            r#"{"host":"1.1.1.1","port":"1","connection_pick":"10.0.0.5\t1234\tsecret\t1"}"#,
        )
        .unwrap();
        let (host, port, password, https) = s.connection();
        assert_eq!(host, "10.0.0.5");
        assert_eq!(port, "1234");
        assert_eq!(password, "secret");
        assert!(https);
    }

    #[test]
    fn pi_datasource_event() {
        let msg: PiMessage = serde_json::from_str(r#"{"event":"kairos_macros"}"#).unwrap();
        assert_eq!(msg.event.as_deref(), Some("kairos_macros"));
    }

    #[test]
    fn state_includes_endpoints() {
        let json = serde_json::to_value(PiOut::state(
            true,
            "Connected",
            vec![EndpointInfo {
                host: "192.168.10.10".into(),
                port: "1234".into(),
                password: String::new(),
                https: false,
                status: "Connected".into(),
            }],
        ))
        .unwrap();
        assert_eq!(json["event"], "kairos_state");
        assert_eq!(json["connected"], true);
        assert_eq!(json["endpoints"][0]["host"], "192.168.10.10");
        assert_eq!(json["endpoints"][0]["port"], "1234");
    }
}
