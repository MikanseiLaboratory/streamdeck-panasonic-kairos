use std::collections::HashMap;
use std::time::Duration;

use panasonic_kairos::http_async::Client;
use panasonic_kairos::{Aux, Scene};
use tokio::sync::{mpsc, oneshot};

use crate::actions::{Job, LayerBus};
use crate::lists;
use crate::settings::{ActionSettings, ListItem};

const IDLE_SECS: u64 = 30;
const MAX_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EndpointKey {
    pub host: String,
    pub port: String,
    pub password: String,
    pub https: bool,
}

impl EndpointKey {
    pub fn from_settings(settings: &ActionSettings) -> Option<Self> {
        let host = settings.host_trimmed();
        if host.is_empty() {
            return None;
        }
        Some(Self {
            host: host.to_string(),
            port: settings.port_trimmed().to_string(),
            password: settings.password.trim().to_string(),
            https: settings.https,
        })
    }
}

#[derive(Debug, Clone)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Retrying { backoff_secs: u64, error: String },
}

impl ConnectionStatus {
    pub fn label(&self) -> String {
        match self {
            Self::Connecting => "Connecting…".to_string(),
            Self::Connected => "Connected".to_string(),
            Self::Retrying {
                backoff_secs,
                error,
            } => format!("Retrying in {backoff_secs}s ({error})"),
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }
}

pub struct TallySnapshot {
    pub scenes: Vec<Scene>,
    pub auxes: Vec<Aux>,
}

pub enum Work {
    Exec {
        job: Job,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Lists {
        event: String,
        settings: Box<ActionSettings>,
        reply: oneshot::Sender<Result<Vec<ListItem>, String>>,
    },
    Tally {
        reply: oneshot::Sender<Result<TallySnapshot, String>>,
    },
    Stop,
}

struct Slot {
    tx: mpsc::UnboundedSender<Work>,
    refs: usize,
    generation: u64,
}

pub struct Pool {
    visible: HashMap<String, EndpointKey>,
    endpoints: HashMap<EndpointKey, Slot>,
    statuses: HashMap<EndpointKey, ConnectionStatus>,
    status_tx: mpsc::UnboundedSender<(EndpointKey, ConnectionStatus)>,
    idle_tx: mpsc::UnboundedSender<(EndpointKey, u64)>,
}

impl Pool {
    pub fn new(
        status_tx: mpsc::UnboundedSender<(EndpointKey, ConnectionStatus)>,
        idle_tx: mpsc::UnboundedSender<(EndpointKey, u64)>,
    ) -> Self {
        Self {
            visible: HashMap::new(),
            endpoints: HashMap::new(),
            statuses: HashMap::new(),
            status_tx,
            idle_tx,
        }
    }

    pub fn status_for_context(&self, context: &str) -> Option<&ConnectionStatus> {
        let key = self.visible.get(context)?;
        self.statuses.get(key)
    }

    pub fn set_status(&mut self, key: EndpointKey, status: ConnectionStatus) {
        self.statuses.insert(key, status);
    }

    pub fn contexts_for(&self, key: &EndpointKey) -> Vec<String> {
        self.visible
            .iter()
            .filter_map(|(ctx, k)| if k == key { Some(ctx.clone()) } else { None })
            .collect()
    }

    pub fn pin(&mut self, context: &str, key: Option<EndpointKey>) {
        let previous = self.visible.remove(context);
        if previous.as_ref() == key.as_ref() {
            if let Some(existing) = key {
                self.visible.insert(context.to_string(), existing);
            }
            return;
        }
        if let Some(old) = previous {
            self.release_key(old);
        }
        if let Some(key) = key {
            self.ensure(key.clone());
            self.visible.insert(context.to_string(), key);
        }
    }

    pub fn unpin(&mut self, context: &str) {
        if let Some(key) = self.visible.remove(context) {
            self.release_key(key);
        }
    }

    fn ensure(&mut self, key: EndpointKey) {
        if let Some(slot) = self.endpoints.get_mut(&key) {
            slot.refs += 1;
            slot.generation = slot.generation.wrapping_add(1);
            return;
        }
        let (tx, rx) = mpsc::unbounded_channel();
        let status_tx = self.status_tx.clone();
        let task_key = key.clone();
        tokio::spawn(run_endpoint(task_key, rx, status_tx));
        self.endpoints.insert(
            key,
            Slot {
                tx,
                refs: 1,
                generation: 0,
            },
        );
    }

    pub fn sender(&mut self, key: &EndpointKey) -> mpsc::UnboundedSender<Work> {
        if let Some(slot) = self.endpoints.get(key) {
            return slot.tx.clone();
        }
        self.ensure(key.clone());
        self.endpoints
            .get(key)
            .expect("endpoint spawned by ensure")
            .tx
            .clone()
    }

    pub fn apply_idle(&mut self, key: EndpointKey, generation: u64) {
        let stop = self
            .endpoints
            .get(&key)
            .is_some_and(|slot| slot.refs == 0 && slot.generation == generation);
        if stop {
            if let Some(slot) = self.endpoints.remove(&key) {
                let _ = slot.tx.send(Work::Stop);
            }
            self.statuses.remove(&key);
        }
    }

    fn release_key(&mut self, key: EndpointKey) {
        let Some(slot) = self.endpoints.get_mut(&key) else {
            return;
        };
        slot.refs = slot.refs.saturating_sub(1);
        if slot.refs == 0 {
            slot.generation = slot.generation.wrapping_add(1);
            let generation = slot.generation;
            let idle_tx = self.idle_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(IDLE_SECS)).await;
                let _ = idle_tx.send((key, generation));
            });
        }
    }
}

async fn run_endpoint(
    key: EndpointKey,
    mut rx: mpsc::UnboundedReceiver<Work>,
    status_tx: mpsc::UnboundedSender<(EndpointKey, ConnectionStatus)>,
) {
    let mut client: Option<Client> = None;
    let mut backoff = Duration::from_secs(1);

    loop {
        if client.is_none() {
            let _ = status_tx.send((key.clone(), ConnectionStatus::Connecting));
            match connect(&key).await {
                Ok(connected) => {
                    client = Some(connected);
                    backoff = Duration::from_secs(1);
                    let _ = status_tx.send((key.clone(), ConnectionStatus::Connected));
                }
                Err(e) => {
                    let status = ConnectionStatus::Retrying {
                        backoff_secs: backoff.as_secs().max(1),
                        error: e,
                    };
                    let _ = status_tx.send((key.clone(), status.clone()));
                    if wait_backoff(&mut rx, backoff, &status).await {
                        return;
                    }
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                    continue;
                }
            }
        }

        match rx.recv().await {
            None | Some(Work::Stop) => return,
            Some(Work::Exec { job, reply }) => {
                let Some(c) = client.as_ref() else { continue };
                let result = execute(c, job).await;
                if result.is_err() {
                    client = None;
                }
                let _ = reply.send(result);
            }
            Some(Work::Lists {
                event,
                settings,
                reply,
            }) => {
                let Some(c) = client.as_ref() else { continue };
                let result = lists::datasource(c, &event, &settings).await;
                if result.is_err() {
                    client = None;
                }
                let _ = reply.send(result);
            }
            Some(Work::Tally { reply }) => {
                let Some(c) = client.as_ref() else { continue };
                let result = tally_snapshot(c).await;
                if result.is_err() {
                    client = None;
                }
                let _ = reply.send(result);
            }
        }
    }
}

async fn connect(key: &EndpointKey) -> Result<Client, String> {
    let settings = ActionSettings {
        host: key.host.clone(),
        port: key.port.clone(),
        password: key.password.clone(),
        https: key.https,
        ..ActionSettings::default()
    };
    let client = Client::connect(settings.http_config()).map_err(|e| e.to_string())?;
    client.list_inputs().await.map_err(|e| e.to_string())?;
    Ok(client)
}

async fn tally_snapshot(client: &Client) -> Result<TallySnapshot, String> {
    let scenes = client.list_scenes().await.map_err(|e| e.to_string())?;
    let auxes = client.list_aux().await.map_err(|e| e.to_string())?;
    Ok(TallySnapshot { scenes, auxes })
}

async fn wait_backoff(
    rx: &mut mpsc::UnboundedReceiver<Work>,
    backoff: Duration,
    status: &ConnectionStatus,
) -> bool {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(backoff) => return false,
            work = rx.recv() => match work {
                None | Some(Work::Stop) => return true,
                Some(Work::Exec { reply, .. }) => {
                    let _ = reply.send(Err(status.label()));
                }
                Some(Work::Lists { reply, .. }) => {
                    let _ = reply.send(Err(status.label()));
                }
                Some(Work::Tally { reply }) => {
                    let _ = reply.send(Err(status.label()));
                }
            }
        }
    }
}

async fn execute(client: &Client, job: Job) -> Result<(), String> {
    match job {
        Job::PlayMacro { id } => client.play_macro(id).await.map_err(err),
        Job::RecallSnapshot { scene, snapshot } => {
            client.recall_snapshot(scene, snapshot).await.map_err(err)
        }
        Job::PlayAction { scene, action } => client.play_action(scene, action).await.map_err(err),
        Job::PlayNamedAction { scene, suffix } => {
            let sc = client.get_scene(scene.clone()).await.map_err(err)?;
            let preferred = format!("{}{suffix}", sc.name);
            let action = sc
                .actions
                .iter()
                .find(|a| a.name == preferred)
                .or_else(|| sc.actions.iter().find(|a| a.name.ends_with(suffix)))
                .ok_or_else(|| format!("no action ending with {suffix} on scene {}", sc.name))?;
            client
                .play_action(scene, action.uuid.clone())
                .await
                .map_err(err)
        }
        Job::SetAux { aux, source } => client.set_aux_source(aux, source).await.map_err(err),
        Job::SetLayer {
            scene,
            layer,
            bus,
            source,
        } => match bus {
            LayerBus::SourceA => client
                .set_layer_source_a(scene, layer, source)
                .await
                .map_err(err),
            LayerBus::SourceB => client
                .set_layer_source_b(scene, layer, source)
                .await
                .map_err(err),
        },
        Job::SetMultiviewerPreset {
            multiviewer,
            preset,
        } => client
            .set_multiviewer_preset(multiviewer, preset)
            .await
            .map_err(err),
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
