//! Panasonic KAIROS Simple Control Protocol (TCP port 3005).

use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::settings::ListItem;

const IO_TIMEOUT: Duration = Duration::from_secs(8);

pub struct TcpLink {
    stream: BufReader<TcpStream>,
}

impl TcpLink {
    pub async fn connect(host: &str, port: u16) -> Result<Self, String> {
        let addr = format!("{host}:{port}");
        let stream = timeout(IO_TIMEOUT, TcpStream::connect(&addr))
            .await
            .map_err(|_| format!("TCP connect timeout {addr}"))?
            .map_err(|e| format!("TCP connect {addr}: {e}"))?;
        stream
            .set_nodelay(true)
            .map_err(|e| format!("TCP nodelay: {e}"))?;
        Ok(Self {
            stream: BufReader::new(stream),
        })
    }

    pub async fn keep_alive(&mut self) -> Result<(), String> {
        self.write(b"\r\n").await
    }

    pub async fn exec(&mut self, command: &str) -> Result<(), String> {
        self.write(format!("{command}\r\n").as_bytes()).await?;
        let line = self.read_line().await?;
        if line.eq_ignore_ascii_case("ok") {
            Ok(())
        } else {
            Err(if line.is_empty() {
                "empty TCP response".into()
            } else {
                line
            })
        }
    }

    pub async fn list(&mut self, path: &str) -> Result<Vec<String>, String> {
        let command = if path.is_empty() {
            "list:\r\n".to_string()
        } else {
            format!("list:{path}\r\n")
        };
        self.write(command.as_bytes()).await?;
        let mut items = Vec::new();
        loop {
            let line = self.read_line().await?;
            if line.is_empty() {
                break;
            }
            if line.eq_ignore_ascii_case("error") {
                return Err(format!("TCP list {path} failed"));
            }
            items.push(line);
        }
        Ok(items)
    }

    pub async fn list_media_stills(&mut self) -> Result<Vec<ListItem>, String> {
        let mut stills = Vec::new();
        let mut queue = vec!["MEDIA.stills".to_string()];
        while let Some(path) = queue.pop() {
            let entries = self.list(&path).await?;
            for entry in entries {
                if entry.eq_ignore_ascii_case("ok") {
                    continue;
                }
                if entry.contains(".rr") {
                    stills.push(entry);
                } else {
                    queue.push(entry);
                }
            }
        }
        stills.sort();
        Ok(stills
            .into_iter()
            .map(|value| {
                let label = value
                    .strip_prefix("MEDIA.stills.")
                    .unwrap_or(value.as_str())
                    .trim_end_matches(".rr")
                    .to_string();
                ListItem { label, value }
            })
            .collect())
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        timeout(IO_TIMEOUT, async {
            self.stream.get_mut().write_all(bytes).await?;
            self.stream.get_mut().flush().await
        })
        .await
        .map_err(|_| "TCP write timeout".to_string())?
        .map_err(|e| format!("TCP write: {e}"))
    }

    async fn read_line(&mut self) -> Result<String, String> {
        let mut buf = String::new();
        let n = timeout(IO_TIMEOUT, self.stream.read_line(&mut buf))
            .await
            .map_err(|_| "TCP read timeout".to_string())?
            .map_err(|e| format!("TCP read: {e}"))?;
        if n == 0 {
            return Err("TCP closed".into());
        }
        Ok(buf.trim_end_matches(['\r', '\n']).to_string())
    }
}

pub fn force_source_cmd(scene: &str, layer: &str, bus: &str, source: &str) -> String {
    format!("SCENES.{scene}.Layers.{layer}.{bus}={source}")
}

pub fn media_still_cmd(scene: &str, layer: &str, bus: &str, still: &str) -> String {
    format!("SCENES.{scene}.Layers.{layer}.{bus}={still}")
}

pub fn layer_transition_cmd(scene: &str, layer: &str, auto: bool) -> String {
    let kind = if auto {
        "transition_auto"
    } else {
        "transition_cut"
    };
    format!("SCENES.{scene}.{layer}.{kind}")
}

pub fn player_cmd(player: &str, op: &str) -> String {
    format!("{player}.{op}")
}

pub fn audio_mute_cmd(channel: Option<&str>, mute: u8) -> String {
    match channel {
        None | Some("") | Some("master") => format!("AUDIOMIXER.mute={mute}"),
        Some(ch) => format!("AUDIOMIXER.{ch}.mute={mute}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_source_matches_companion() {
        assert_eq!(
            force_source_cmd("Main", "Background", "sourceA", "IP1"),
            "SCENES.Main.Layers.Background.sourceA=IP1"
        );
    }

    #[test]
    fn layer_auto_matches_companion() {
        assert_eq!(
            layer_transition_cmd("Main", "Background", true),
            "SCENES.Main.Background.transition_auto"
        );
    }

    #[test]
    fn audio_master_and_channel() {
        assert_eq!(audio_mute_cmd(None, 1), "AUDIOMIXER.mute=1");
        assert_eq!(
            audio_mute_cmd(Some("Channel 1"), 0),
            "AUDIOMIXER.Channel 1.mute=0"
        );
    }

    #[test]
    fn player_repeat_assignment() {
        assert_eq!(player_cmd("RR1", "repeat=1"), "RR1.repeat=1");
    }
}
