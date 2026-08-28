use crate::settings::ActionSettings;

pub const MACRO: &str = "dev.mikanseilaboratory.kairos.macro";
pub const SNAPSHOT: &str = "dev.mikanseilaboratory.kairos.snapshot";
pub const ACTION: &str = "dev.mikanseilaboratory.kairos.action";
pub const CUT: &str = "dev.mikanseilaboratory.kairos.cut";
pub const AUTO: &str = "dev.mikanseilaboratory.kairos.auto";
pub const AUX: &str = "dev.mikanseilaboratory.kairos.aux";
pub const LAYER: &str = "dev.mikanseilaboratory.kairos.layer";
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
}

#[derive(Debug, Clone)]
pub enum Job {
    PlayMacro {
        id: String,
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
}

pub fn needs_tally(action: &str) -> bool {
    action == AUX || action == LAYER
}

pub fn skip_ok(action: &str) -> bool {
    needs_tally(action)
}

pub fn build_job(action: &str, settings: &ActionSettings) -> Result<Job, String> {
    match action {
        MACRO => Ok(Job::PlayMacro {
            id: required(&settings.macro_id, "macro")?,
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
        LAYER => Ok(Job::SetLayer {
            scene: required(&settings.scene_id, "scene")?,
            layer: required(&settings.layer_id, "layer")?,
            bus: LayerBus::parse(if settings.bus.trim().is_empty() {
                "sourceA"
            } else {
                settings.bus.trim()
            })?,
            source: required(&settings.source, "source")?,
        }),
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
}
