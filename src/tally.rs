use crate::actions::{
    ACTION, AUTO, CUT, FORCE_SOURCE, INPUT_TALLY, LAYER, MEDIA_STILL, SCENE_MACRO, SNAPSHOT,
};
use crate::settings::ActionSettings;
use panasonic_kairos::{Aux, Input, Scene};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TallyLight {
    Program,
    Preview,
}

#[derive(Debug, Clone)]
pub struct TallyBinding {
    pub action: String,
    pub settings: ActionSettings,
}

impl TallyBinding {
    pub fn watches(&self) -> bool {
        match self.action.as_str() {
            crate::actions::AUX => {
                !self.settings.aux_id.trim().is_empty() && !self.settings.source.trim().is_empty()
            }
            LAYER | FORCE_SOURCE => {
                !self.settings.scene_id.trim().is_empty()
                    && !self.settings.layer_id.trim().is_empty()
                    && !self.settings.source.trim().is_empty()
            }
            MEDIA_STILL => {
                !self.settings.scene_id.trim().is_empty()
                    && !self.settings.layer_id.trim().is_empty()
                    && !self.settings.still_id.trim().is_empty()
            }
            CUT | AUTO | SNAPSHOT | ACTION | SCENE_MACRO => {
                !self.settings.scene_id.trim().is_empty()
            }
            INPUT_TALLY => !self.settings.input_id.trim().is_empty(),
            _ => false,
        }
    }
}

pub fn light_for(
    binding: &TallyBinding,
    scenes: &[Scene],
    auxes: &[Aux],
    inputs: &[Input],
) -> Option<TallyLight> {
    if binding.action == crate::actions::AUX {
        let aux = find_aux(auxes, binding.settings.aux_id.trim())?;
        return (aux.source == binding.settings.source.trim()).then_some(TallyLight::Program);
    }
    if matches!(binding.action.as_str(), LAYER | FORCE_SOURCE | MEDIA_STILL) {
        let scene = find_scene(scenes, binding.settings.scene_id.trim())?;
        let layer = find_layer(scene, binding.settings.layer_id.trim())?;
        let want = if binding.action == MEDIA_STILL {
            binding.settings.still_id.trim()
        } else {
            binding.settings.source.trim()
        };
        if layer.source_a.as_deref() == Some(want) {
            return Some(TallyLight::Program);
        }
        if layer.source_b.as_deref() == Some(want) {
            return Some(TallyLight::Preview);
        }
        return None;
    }
    if matches!(
        binding.action.as_str(),
        CUT | AUTO | SNAPSHOT | ACTION | SCENE_MACRO
    ) {
        let scene = find_scene(scenes, binding.settings.scene_id.trim())?;
        return from_tally(scene.tally);
    }
    if binding.action == INPUT_TALLY {
        let input = find_input(inputs, binding.settings.input_id.trim())?;
        return from_tally(input.tally);
    }
    None
}

fn from_tally(value: u32) -> Option<TallyLight> {
    if value & 1 != 0 {
        Some(TallyLight::Program)
    } else if value & 2 != 0 {
        Some(TallyLight::Preview)
    } else if value > 0 {
        Some(TallyLight::Program)
    } else {
        None
    }
}

fn find_aux<'a>(auxes: &'a [Aux], id: &str) -> Option<&'a Aux> {
    auxes
        .iter()
        .find(|a| a.uuid == id || a.name == id || a.index.to_string() == id)
}

fn find_scene<'a>(scenes: &'a [Scene], id: &str) -> Option<&'a Scene> {
    scenes.iter().find(|s| s.uuid == id || s.name == id)
}

fn find_layer<'a>(scene: &'a Scene, id: &str) -> Option<&'a panasonic_kairos::Layer> {
    scene
        .layers
        .iter()
        .find(|l| l.name == id || l.uuid.as_deref() == Some(id))
}

fn find_input<'a>(inputs: &'a [Input], id: &str) -> Option<&'a Input> {
    inputs
        .iter()
        .find(|i| i.uuid == id || i.name == id || i.index.to_string() == id)
}

pub fn image_data_uri(light: TallyLight) -> String {
    let svg = match light {
        TallyLight::Program => solid("#E10600"),
        TallyLight::Preview => solid("#00A651"),
    };
    format!("data:image/svg+xml;charset=utf8,{svg}")
}

fn solid(fill: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144"><rect width="144" height="144" fill="{fill}"/></svg>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use panasonic_kairos::{Aux, Layer, Scene};

    fn scene_with_layer(source_a: &str) -> Scene {
        Scene {
            actions: vec![],
            layers: vec![Layer {
                name: "Background".into(),
                path: String::new(),
                source_a: Some(source_a.into()),
                source_b: Some("IP2".into()),
                sources: vec![],
                uuid: None,
            }],
            macros: vec![],
            name: "Main".into(),
            path: String::new(),
            snapshots: vec![],
            tally: 1,
            uuid: "scene-1".into(),
        }
    }

    #[test]
    fn layer_source_a_match_is_program() {
        let binding = TallyBinding {
            action: LAYER.into(),
            settings: ActionSettings {
                scene_id: "Main".into(),
                layer_id: "Background".into(),
                source: "IP1".into(),
                bus: "sourceA".into(),
                ..ActionSettings::default()
            },
        };
        let light = light_for(&binding, &[scene_with_layer("IP1")], &[], &[]);
        assert_eq!(light, Some(TallyLight::Program));
    }

    #[test]
    fn layer_source_b_match_is_preview() {
        let binding = TallyBinding {
            action: LAYER.into(),
            settings: ActionSettings {
                scene_id: "Main".into(),
                layer_id: "Background".into(),
                source: "IP2".into(),
                bus: "sourceB".into(),
                ..ActionSettings::default()
            },
        };
        let light = light_for(&binding, &[scene_with_layer("IP1")], &[], &[]);
        assert_eq!(light, Some(TallyLight::Preview));
    }

    #[test]
    fn aux_match_is_program() {
        let binding = TallyBinding {
            action: crate::actions::AUX.into(),
            settings: ActionSettings {
                aux_id: "0".into(),
                source: "Black".into(),
                ..ActionSettings::default()
            },
        };
        let aux = Aux {
            index: 0,
            name: "IP-AUX1".into(),
            source: "Black".into(),
            sources: vec![],
            uuid: "aux-1".into(),
        };
        assert_eq!(
            light_for(&binding, &[], &[aux], &[]),
            Some(TallyLight::Program)
        );
    }

    #[test]
    fn scene_tally_lights_cut() {
        let binding = TallyBinding {
            action: CUT.into(),
            settings: ActionSettings {
                scene_id: "scene-1".into(),
                ..ActionSettings::default()
            },
        };
        assert_eq!(
            light_for(&binding, &[scene_with_layer("IP1")], &[], &[]),
            Some(TallyLight::Program)
        );
    }

    #[test]
    fn input_tally_preview_bit() {
        let binding = TallyBinding {
            action: INPUT_TALLY.into(),
            settings: ActionSettings {
                input_id: "in-1".into(),
                ..ActionSettings::default()
            },
        };
        let input = Input {
            index: 0,
            name: "IP1".into(),
            tally: 2,
            uuid: "in-1".into(),
        };
        assert_eq!(
            light_for(&binding, &[], &[], &[input]),
            Some(TallyLight::Preview)
        );
    }
}
