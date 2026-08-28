//! Action settings as persisted by Stream Deck (SDPI `setting=` keys).

use panasonic_kairos::{Credentials, HttpConfig};
use serde::{Deserialize, Serialize};

fn default_host() -> String {
    "192.168.10.10".to_string()
}

fn default_port() -> String {
    "1234".to_string()
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
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
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
}

impl ActionSettings {
    pub fn host_trimmed(&self) -> &str {
        self.host.trim()
    }

    pub fn port_trimmed(&self) -> &str {
        self.port.trim()
    }

    pub fn http_config(&self) -> HttpConfig {
        let host = self.host_trimmed();
        let port = self.port_trimmed();
        let addr = if port.is_empty() {
            host.to_string()
        } else {
            format!("{host}:{port}")
        };
        HttpConfig::new(addr)
            .with_https(self.https)
            .with_credentials(Credentials::password(self.password.trim()))
            .with_timeout_ms(8_000)
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

#[derive(Debug, Serialize)]
pub struct PiOut {
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<ListItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected: Option<bool>,
}

impl PiOut {
    pub fn items(event: impl Into<String>, items: Vec<ListItem>) -> Self {
        Self {
            event: event.into(),
            items: Some(items),
            connected: None,
        }
    }

    pub fn state(connected: bool) -> Self {
        Self {
            event: "kairos_state".into(),
            items: None,
            connected: Some(connected),
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
        assert_eq!(s.host, "192.168.10.10");
        assert_eq!(s.port, "1234");
        assert!(!s.https);
    }

    #[test]
    fn pi_datasource_event() {
        let msg: PiMessage = serde_json::from_str(r#"{"event":"kairos_macros"}"#).unwrap();
        assert_eq!(msg.event.as_deref(), Some("kairos_macros"));
    }
}
