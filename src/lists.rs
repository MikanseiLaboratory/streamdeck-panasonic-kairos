use panasonic_kairos::http_async::Client;

use crate::settings::{ActionSettings, ListItem};

pub async fn datasource(
    client: &Client,
    event: &str,
    settings: &ActionSettings,
) -> Result<Vec<ListItem>, String> {
    match event {
        "kairos_macros" => {
            let macros = client.list_macros().await.map_err(err)?;
            Ok(macros
                .into_iter()
                .map(|m| {
                    let label = if m.path.is_empty() {
                        m.name.clone()
                    } else {
                        format!("{}{}", m.path, m.name)
                    };
                    ListItem {
                        label,
                        value: m.uuid,
                    }
                })
                .collect())
        }
        "kairos_scenes" => {
            let scenes = client.list_scenes().await.map_err(err)?;
            Ok(scenes
                .into_iter()
                .map(|s| {
                    let label = if s.path.is_empty() {
                        s.name.clone()
                    } else {
                        format!("{}{}", s.path, s.name)
                    };
                    ListItem {
                        label,
                        value: s.uuid,
                    }
                })
                .collect())
        }
        "kairos_snapshots" => {
            let Some(scene_id) = configured(&settings.scene_id) else {
                return Ok(Vec::new());
            };
            let scene = client.get_scene(scene_id).await.map_err(err)?;
            Ok(scene
                .snapshots
                .into_iter()
                .map(|s| ListItem {
                    label: s.name,
                    value: s.uuid,
                })
                .collect())
        }
        "kairos_actions" => {
            let Some(scene_id) = configured(&settings.scene_id) else {
                return Ok(Vec::new());
            };
            let scene = client.get_scene(scene_id).await.map_err(err)?;
            Ok(scene
                .actions
                .into_iter()
                .map(|a| ListItem {
                    label: a.name,
                    value: a.uuid,
                })
                .collect())
        }
        "kairos_aux" => {
            let auxes = client.list_aux().await.map_err(err)?;
            Ok(auxes
                .into_iter()
                .map(|a| ListItem {
                    label: format!("{} ({})", a.name.trim(), a.index),
                    value: a.uuid,
                })
                .collect())
        }
        "kairos_aux_sources" => {
            let Some(aux_id) = configured(&settings.aux_id) else {
                return Ok(Vec::new());
            };
            let aux = client.get_aux(aux_id).await.map_err(err)?;
            Ok(aux
                .sources
                .into_iter()
                .map(|s| ListItem {
                    label: s.clone(),
                    value: s,
                })
                .collect())
        }
        "kairos_layers" => {
            let Some(scene_id) = configured(&settings.scene_id) else {
                return Ok(Vec::new());
            };
            let scene = client.get_scene(scene_id).await.map_err(err)?;
            Ok(scene
                .layers
                .into_iter()
                .map(|l| ListItem {
                    label: l.name.clone(),
                    value: l.uuid.unwrap_or(l.name),
                })
                .collect())
        }
        "kairos_layer_sources" => {
            let Some(scene_id) = configured(&settings.scene_id) else {
                return Ok(Vec::new());
            };
            let Some(layer_id) = configured(&settings.layer_id) else {
                return Ok(Vec::new());
            };
            let scene = client.get_scene(scene_id).await.map_err(err)?;
            let layer = scene
                .layers
                .into_iter()
                .find(|l| l.name == layer_id || l.uuid.as_deref() == Some(layer_id))
                .ok_or_else(|| "layer not found".to_string())?;
            Ok(layer
                .sources
                .into_iter()
                .map(|s| ListItem {
                    label: s.clone(),
                    value: s,
                })
                .collect())
        }
        "kairos_multiviewers" => {
            let mvs = client.list_multiviewers().await.map_err(err)?;
            Ok(mvs
                .into_iter()
                .map(|m| ListItem {
                    label: m.name,
                    value: m.uuid,
                })
                .collect())
        }
        "kairos_presets" => {
            let Some(multiviewer_id) = configured(&settings.multiviewer_id) else {
                return Ok(Vec::new());
            };
            let mv = client.get_multiviewer(multiviewer_id).await.map_err(err)?;
            Ok(mv
                .presets
                .into_iter()
                .map(|p| ListItem {
                    label: format!("{} ({})", p.name, p.id),
                    value: p.id.to_string(),
                })
                .collect())
        }
        "kairos_scene_macros" => {
            let Some(scene_id) = configured(&settings.scene_id) else {
                return Ok(Vec::new());
            };
            let scene = client.get_scene(scene_id).await.map_err(err)?;
            Ok(scene
                .macros
                .into_iter()
                .map(|m| ListItem {
                    label: m.name,
                    value: m.uuid,
                })
                .collect())
        }
        "kairos_inputs" => {
            let inputs = client.list_inputs().await.map_err(err)?;
            Ok(inputs
                .into_iter()
                .map(|i| ListItem {
                    label: i.name.clone(),
                    value: i.uuid,
                })
                .collect())
        }
        "kairos_players" => Ok(player_items()),
        other => Err(format!("unknown datasource {other}")),
    }
}

pub fn player_items() -> Vec<ListItem> {
    const PLAYERS: &[&str] = &[
        "RR1", "RR2", "RR3", "RR4", "RR5", "RR6", "RR7", "RR8", "CP1", "CP2", "AP1", "AP2", "AP3",
        "AP4", "AP5", "AP6", "AP7", "AP8", "AP9", "AP10", "AP11", "AP12", "AP13", "AP14", "AP15",
        "AP16",
    ];
    PLAYERS
        .iter()
        .map(|name| ListItem {
            label: (*name).to_string(),
            value: (*name).to_string(),
        })
        .collect()
}

fn configured(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
