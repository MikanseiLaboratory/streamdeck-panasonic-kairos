use crate::settings::ActionSettings;
use panasonic_kairos::MacroState;

pub const MACRO: &str = "dev.mikanseilaboratory.kairos.macro";
pub const SCENE_MACRO: &str = "dev.mikanseilaboratory.kairos.scene-macro";
pub const SNAPSHOT: &str = "dev.mikanseilaboratory.kairos.snapshot";
pub const ACTION: &str = "dev.mikanseilaboratory.kairos.action";
pub const CUT: &str = "dev.mikanseilaboratory.kairos.cut";
pub const AUTO: &str = "dev.mikanseilaboratory.kairos.auto";
pub const AUX: &str = "dev.mikanseilaboratory.kairos.aux";
pub const LAYER: &str = "dev.mikanseilaboratory.kairos.layer";
pub const FORCE_SOURCE: &str = "dev.mikanseilaboratory.kairos.force-source";
pub const MEDIA_STILL: &str = "dev.mikanseilaboratory.kairos.media-still";
pub const LAYER_CUT: &str = "dev.mikanseilaboratory.kairos.layer-cut";
pub const LAYER_AUTO: &str = "dev.mikanseilaboratory.kairos.layer-auto";
pub const PLAYER: &str = "dev.mikanseilaboratory.kairos.player";
pub const AUDIO_MUTE: &str = "dev.mikanseilaboratory.kairos.audio-mute";
pub const INPUT_TALLY: &str = "dev.mikanseilaboratory.kairos.input-tally";
pub const MULTIVIEWER: &str = "dev.mikanseilaboratory.kairos.multiviewer";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerBus {
    SourceA,
    SourceB,
}

impl LayerBus {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "sourceA" | "source_a" | "a" => Ok(Self::SourceA),
            "sourceB" | "source_b" | "b" => Ok(Self::SourceB),
            other => Err(format!("unknown layer bus {other}")),
        }
    }

    pub fn as_tcp(self) -> &'static str {
        match self {
            Self::SourceA => "sourceA",
            Self::SourceB => "sourceB",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Job {
    PlayMacro {
        id: String,
        state: MacroState,
    },
    PlaySceneMacro {
        scene: String,
        id: String,
        state: MacroState,
    },
    RecallSnapshot {
        scene: String,
        snapshot: String,
    },
    PlayAction {
        scene: String,
        action: String,
    },
    PlayNamedAction {
        scene: String,
        suffix: &'static str,
    },
    SetAux {
        aux: String,
        source: String,
    },
    SetLayer {
        scene: String,
        layer: String,
        bus: LayerBus,
        source: String,
    },
    SetMultiviewerPreset {
        multiviewer: String,
        preset: u32,
    },
    ForceSource {
        scene: String,
        layer: String,
        bus: LayerBus,
        source: String,
    },
    SetMediaStill {
        scene: String,
        layer: String,
        bus: LayerBus,
        still: String,
    },
    LayerTransition {
        scene: String,
        layer: String,
        auto: bool,
    },
    Player {
        player: String,
        op: String,
    },
    AudioMute {
        channel: Option<String>,
        mute: u8,
    },
}

impl Job {
    pub fn uses_tcp(&self) -> bool {
        matches!(
            self,
            Self::ForceSource { .. }
                | Self::SetMediaStill { .. }
                | Self::LayerTransition { .. }
                | Self::Player { .. }
                | Self::AudioMute { .. }
        )
    }
}

pub fn skip_ok(action: &str) -> bool {
    matches!(
        action,
        AUX | LAYER | FORCE_SOURCE | MEDIA_STILL | INPUT_TALLY
    )
}

pub fn build_job(action: &str, settings: &ActionSettings) -> Result<Job, String> {
    match action {
        MACRO => Ok(Job::PlayMacro {
            id: required(&settings.macro_id, "macro")?,
            state: parse_macro_state(&settings.macro_state),
        }),
        SCENE_MACRO => Ok(Job::PlaySceneMacro {
            scene: required(&settings.scene_id, "scene")?,
            id: required(&settings.macro_id, "macro")?,
            state: parse_macro_state(&settings.macro_state),
        }),
        SNAPSHOT => Ok(Job::RecallSnapshot {
            scene: required(&settings.scene_id, "scene")?,
            snapshot: required(&settings.snapshot_id, "snapshot")?,
        }),
        ACTION => Ok(Job::PlayAction {
            scene: required(&settings.scene_id, "scene")?,
            action: required(&settings.action_id, "action")?,
        }),
        CUT => Ok(Job::PlayNamedAction {
            scene: required(&settings.scene_id, "scene")?,
            suffix: ":cut",
        }),
        AUTO => Ok(Job::PlayNamedAction {
            scene: required(&settings.scene_id, "scene")?,
            suffix: ":auto",
        }),
        AUX => Ok(Job::SetAux {
            aux: required(&settings.aux_id, "AUX")?,
            source: required(&settings.source, "source")?,
        }),
        LAYER | FORCE_SOURCE => {
            let job_layer = || -> Result<_, String> {
                Ok((
                    required(&settings.scene_id, "scene")?,
                    required(&settings.layer_id, "layer")?,
                    parse_bus(&settings.bus)?,
                    required(&settings.source, "source")?,
                ))
            };
            let (scene, layer, bus, source) = job_layer()?;
            if action == FORCE_SOURCE {
                Ok(Job::ForceSource {
                    scene,
                    layer,
                    bus,
                    source,
                })
            } else {
                Ok(Job::SetLayer {
                    scene,
                    layer,
                    bus,
                    source,
                })
            }
        }
        MEDIA_STILL => Ok(Job::SetMediaStill {
            scene: required(&settings.scene_id, "scene")?,
            layer: required(&settings.layer_id, "layer")?,
            bus: parse_bus(&settings.bus)?,
            still: required(&settings.still_id, "still")?,
        }),
        LAYER_CUT => Ok(Job::LayerTransition {
            scene: required(&settings.scene_id, "scene")?,
            layer: required(&settings.layer_id, "layer")?,
            auto: false,
        }),
        LAYER_AUTO => Ok(Job::LayerTransition {
            scene: required(&settings.scene_id, "scene")?,
            layer: required(&settings.layer_id, "layer")?,
            auto: true,
        }),
        PLAYER => Ok(Job::Player {
            player: required(&settings.player_id, "player")?,
            op: required(&settings.player_op, "player action")?,
        }),
        AUDIO_MUTE => {
            let target = settings.audio_target.trim();
            if target.is_empty() {
                return Err("audio target is not set".into());
            }
            let channel = if target.eq_ignore_ascii_case("master") {
                None
            } else {
                Some(target.to_string())
            };
            Ok(Job::AudioMute {
                channel,
                mute: parse_mute(&settings.audio_mute)?,
            })
        }
        INPUT_TALLY => Err("Input Tally has no press action".into()),
        MULTIVIEWER => {
            let preset = required(&settings.preset_id, "preset")?
                .parse::<u32>()
                .map_err(|_| "preset must be a number".to_string())?;
            Ok(Job::SetMultiviewerPreset {
                multiviewer: required(&settings.multiviewer_id, "multiviewer")?,
                preset,
            })
        }
        other => Err(format!("unknown action {other}")),
    }
}

fn parse_bus(value: &str) -> Result<LayerBus, String> {
    LayerBus::parse(if value.trim().is_empty() {
        "sourceA"
    } else {
        value.trim()
    })
}

pub fn parse_macro_state(value: &str) -> MacroState {
    match value.trim() {
        "stop" => MacroState::Stop,
        "record" => MacroState::Record,
        "stop_record" => MacroState::StopRecord,
        _ => MacroState::Play,
    }
}

fn parse_mute(value: &str) -> Result<u8, String> {
    match value.trim() {
        "" | "0" | "unmute" | "false" => Ok(0),
        "1" | "mute" | "true" => Ok(1),
        other => Err(format!("unknown mute value {other}")),
    }
}

fn required(value: &str, name: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{name} is not set"))
    } else {
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> ActionSettings {
        ActionSettings {
            macro_id: "m1".into(),
            scene_id: "Main".into(),
            snapshot_id: "s1".into(),
            action_id: "a1".into(),
            aux_id: "0".into(),
            source: "IP1".into(),
            layer_id: "Background".into(),
            bus: "sourceA".into(),
            still_id: "MEDIA.stills.clip.rr".into(),
            player_id: "RR1".into(),
            player_op: "play".into(),
            audio_target: "master".into(),
            audio_mute: "1".into(),
            macro_state: "stop".into(),
            multiviewer_id: "0".into(),
            preset_id: "1".into(),
            ..ActionSettings::default()
        }
    }

    #[test]
    fn cut_and_auto_use_name_suffix() {
        let s = settings();
        match build_job(CUT, &s).unwrap() {
            Job::PlayNamedAction { suffix, .. } => assert_eq!(suffix, ":cut"),
            other => panic!("{other:?}"),
        }
        match build_job(AUTO, &s).unwrap() {
            Job::PlayNamedAction { suffix, .. } => assert_eq!(suffix, ":auto"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn missing_macro_errors() {
        let s = ActionSettings::default();
        assert!(build_job(MACRO, &s).is_err());
    }

    #[test]
    fn layer_defaults_to_source_a() {
        let mut s = settings();
        s.bus.clear();
        match build_job(LAYER, &s).unwrap() {
            Job::SetLayer { bus, .. } => assert_eq!(bus, LayerBus::SourceA),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn macro_stop_and_force_source() {
        let s = settings();
        match build_job(MACRO, &s).unwrap() {
            Job::PlayMacro { state, .. } => assert_eq!(state, MacroState::Stop),
            other => panic!("{other:?}"),
        }
        match build_job(FORCE_SOURCE, &s).unwrap() {
            Job::ForceSource { .. } => {}
            other => panic!("{other:?}"),
        }
        match build_job(LAYER_AUTO, &s).unwrap() {
            Job::LayerTransition { auto, .. } => assert!(auto),
            other => panic!("{other:?}"),
        }
        match build_job(AUDIO_MUTE, &s).unwrap() {
            Job::AudioMute { channel, mute } => {
                assert!(channel.is_none());
                assert_eq!(mute, 1);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn input_tally_is_not_a_press() {
        assert!(build_job(INPUT_TALLY, &settings()).is_err());
    }
}
