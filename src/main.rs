mod actions;
mod lists;
mod pool;
mod settings;
mod tally;
mod tcp;

use std::collections::HashMap;
use std::env;
use std::time::Duration;

use actions::{build_job, skip_ok};
use futures::{SinkExt, StreamExt};
use pool::{ConnectionStatus, EndpointKey, Pool, Work};
use settings::{ActionSettings, EndpointInfo, ListItem, PiMessage, PiOut};
use streamdeck_rs::registration::RegistrationParams;
use streamdeck_rs::{ImagePayload, Message, MessageOut, StreamDeckSocket, Target};
use tally::{image_data_uri, TallyBinding, TallyLight};
use tokio::sync::{mpsc, oneshot};
use tokio::time::interval;

type SdSocket = StreamDeckSocket<(), ActionSettings, PiMessage, PiOut>;

enum Outgoing {
    ShowOk {
        context: String,
    },
    ShowAlert {
        context: String,
    },
    ToPi {
        action: String,
        context: String,
        payload: PiOut,
    },
    SetImage {
        context: String,
        image: Option<String>,
    },
    GetSettings {
        context: String,
    },
    Log {
        message: String,
    },
}

struct KeyWatch {
    endpoint: Option<EndpointKey>,
    binding: TallyBinding,
}

struct Plugin {
    pool: Pool,
    open_pi: HashMap<String, String>,
    watches: HashMap<String, KeyWatch>,
    last_light: HashMap<String, Option<TallyLight>>,
    outgoing: mpsc::UnboundedSender<Outgoing>,
}

#[tokio::main]
async fn main() {
    let params = RegistrationParams::from_args(env::args()).expect("Stream Deck registration args");
    let mut socket: SdSocket = StreamDeckSocket::connect(params.port, params.event, params.uuid)
        .await
        .expect("connect to Stream Deck");

    let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel();
    let (status_tx, mut status_rx) = mpsc::unbounded_channel();
    let (idle_tx, mut idle_rx) = mpsc::unbounded_channel();

    let mut plugin = Plugin {
        pool: Pool::new(status_tx, idle_tx),
        open_pi: HashMap::new(),
        watches: HashMap::new(),
        last_light: HashMap::new(),
        outgoing: outgoing_tx,
    };

    let mut tally_tick = interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            msg = socket.next() => {
                match msg {
                    Some(Ok(message)) => plugin.handle(message),
                    Some(Err(e)) => eprintln!("streamdeck read error: {e:?}"),
                    None => break,
                }
            }
            out = outgoing_rx.recv() => {
                let Some(out) = out else { break };
                if let Err(e) = send_out(&mut socket, out).await {
                    eprintln!("streamdeck write error: {e:?}");
                }
            }
            status = status_rx.recv() => {
                let Some((key, status)) = status else { break };
                plugin.on_status(key, status);
            }
            idle = idle_rx.recv() => {
                let Some((key, generation)) = idle else { break };
                plugin.pool.apply_idle(key, generation);
            }
            _ = tally_tick.tick() => {
                plugin.poll_tally();
            }
        }
    }
}

async fn send_out(socket: &mut SdSocket, out: Outgoing) -> Result<(), String> {
    let result = match out {
        Outgoing::ShowOk { context } => socket.send(MessageOut::ShowOk { context }).await,
        Outgoing::ShowAlert { context } => socket.send(MessageOut::ShowAlert { context }).await,
        Outgoing::ToPi {
            action,
            context,
            payload,
        } => {
            socket
                .send(MessageOut::SendToPropertyInspector {
                    action,
                    context,
                    payload,
                })
                .await
        }
        Outgoing::SetImage { context, image } => {
            socket
                .send(MessageOut::SetImage {
                    context,
                    payload: ImagePayload {
                        image,
                        target: Target::Both,
                        state: None,
                    },
                })
                .await
        }
        Outgoing::GetSettings { context } => socket.send(MessageOut::GetSettings { context }).await,
        Outgoing::Log { message } => {
            socket
                .send(MessageOut::LogMessage {
                    payload: streamdeck_rs::LogMessagePayload { message },
                })
                .await
        }
    };
    result.map_err(|e| format!("{e:?}"))
}

impl Plugin {
    fn handle(&mut self, message: Message<(), ActionSettings, PiMessage>) {
        match message {
            Message::WillAppear {
                action,
                context,
                payload,
                ..
            } => {
                self.watch_key(action, context, payload.settings);
            }
            Message::DidReceiveSettings {
                action,
                context,
                payload,
                ..
            } => {
                self.watch_key(action, context.clone(), payload.settings);
                self.push_status(&context);
            }
            Message::WillDisappear { context, .. } => {
                self.unwatch_key(&context);
                self.pool.unpin(&context);
            }
            Message::KeyDown {
                action,
                context,
                payload,
                ..
            } => self.run_action(action, context, payload.settings),
            Message::PropertyInspectorDidAppear {
                action, context, ..
            } => {
                self.open_pi.insert(context.clone(), action.clone());
                if self.pool.status_for_context(&context).is_none() {
                    let _ = self.outgoing.send(Outgoing::GetSettings {
                        context: context.clone(),
                    });
                }
                self.push_status(&context);
                self.refresh_open_lists(&action, &context);
            }
            Message::PropertyInspectorDidDisappear { context, .. } => {
                self.open_pi.remove(&context);
            }
            Message::SendToPlugin {
                action,
                context,
                payload,
            } => self.on_pi_message(action, context, payload),
            _ => {}
        }
    }

    fn on_pi_message(&mut self, action: String, context: String, payload: PiMessage) {
        let Some(event) = payload.event.filter(|e| e.starts_with("kairos_")) else {
            return;
        };
        if event == "kairos_state" {
            self.push_status(&context);
            return;
        }
        if event == "kairos_connections" {
            self.push_connections(&action, &context);
            return;
        }
        let Some(watch) = self.watches.get(&context) else {
            let _ = self.outgoing.send(Outgoing::ToPi {
                action,
                context,
                payload: PiOut::items(event, Vec::new()),
            });
            return;
        };
        let Some(key) = watch.endpoint.clone() else {
            let _ = self.outgoing.send(Outgoing::ToPi {
                action: action.clone(),
                context: context.clone(),
                payload: PiOut::items(event, Vec::new()),
            });
            self.push_status(&context);
            return;
        };
        let settings = watch.binding.settings.clone();
        let tx = self.pool.sender(&key);
        let outgoing = self.outgoing.clone();
        tokio::spawn(async move {
            let (reply_tx, reply_rx) = oneshot::channel();
            if tx
                .send(Work::Lists {
                    event: event.clone(),
                    settings: Box::new(settings),
                    reply: reply_tx,
                })
                .is_err()
            {
                let _ = outgoing.send(Outgoing::ToPi {
                    action,
                    context,
                    payload: PiOut::items(event, Vec::new()),
                });
                return;
            }
            let items = match reply_rx.await {
                Ok(Ok(items)) => items,
                _ => Vec::new(),
            };
            let _ = outgoing.send(Outgoing::ToPi {
                action,
                context,
                payload: PiOut::items(event, items),
            });
        });
    }

    fn datasource_events(action: &str) -> &'static [&'static str] {
        match action {
            actions::MACRO => &["kairos_macros"],
            actions::SCENE_MACRO => &["kairos_scenes", "kairos_scene_macros"],
            actions::SNAPSHOT => &["kairos_scenes", "kairos_snapshots"],
            actions::ACTION => &["kairos_scenes", "kairos_actions"],
            actions::CUT | actions::AUTO => &["kairos_scenes"],
            actions::AUX => &["kairos_aux", "kairos_aux_sources"],
            actions::LAYER | actions::FORCE_SOURCE => {
                &["kairos_scenes", "kairos_layers", "kairos_layer_sources"]
            }
            actions::MEDIA_STILL => &["kairos_scenes", "kairos_layers", "kairos_stills"],
            actions::LAYER_CUT | actions::LAYER_AUTO => &["kairos_scenes", "kairos_layers"],
            actions::PLAYER => &["kairos_players"],
            actions::INPUT_TALLY => &["kairos_inputs"],
            actions::MULTIVIEWER => &["kairos_multiviewers", "kairos_presets"],
            _ => &[],
        }
    }

    fn refresh_open_lists(&mut self, action: &str, context: &str) {
        if !self.open_pi.contains_key(context) {
            return;
        }
        self.push_connections(action, context);
        for event in Self::datasource_events(action) {
            self.on_pi_message(
                action.to_string(),
                context.to_string(),
                PiMessage {
                    event: Some((*event).to_string()),
                },
            );
        }
    }

    fn connection_items(&self) -> Vec<ListItem> {
        let list = self.pool.endpoint_list();
        let mut host_counts: HashMap<String, usize> = HashMap::new();
        for (key, _) in &list {
            *host_counts
                .entry(format!("{}:{}", key.host, key.port))
                .or_default() += 1;
        }
        let mut items = vec![ListItem {
            label: "Enter host".into(),
            value: "manual".into(),
        }];
        for (key, status) in list {
            let mut addr = if key.port.is_empty() {
                key.host.clone()
            } else {
                format!("{}:{}", key.host, key.port)
            };
            if key.https {
                addr.push_str(" (https)");
            }
            let count_key = format!("{}:{}", key.host, key.port);
            let label = if host_counts.get(&count_key).copied().unwrap_or(0) > 1 {
                format!("{} ({}) · {status}", addr, key.password)
            } else {
                format!("{addr} · {status}")
            };
            let https = if key.https { "1" } else { "0" };
            items.push(ListItem {
                label,
                value: format!(
                    "{}\t{}\t{}\t{https}\t{}",
                    key.host, key.port, key.password, key.tcp_port
                ),
            });
        }
        items
    }

    fn push_connections(&self, action: &str, context: &str) {
        let _ = self.outgoing.send(Outgoing::ToPi {
            action: action.to_string(),
            context: context.to_string(),
            payload: PiOut::items("kairos_connections", self.connection_items()),
        });
    }

    fn watch_key(&mut self, action: String, context: String, settings: ActionSettings) {
        let endpoint = EndpointKey::from_settings(&settings);
        self.pool.pin(&context, endpoint.clone());
        self.watches.insert(
            context.clone(),
            KeyWatch {
                endpoint,
                binding: TallyBinding {
                    action: action.clone(),
                    settings,
                },
            },
        );
        self.refresh_tally_image(&context, None);
        self.refresh_open_lists(&action, &context);
        self.broadcast_status();
    }

    fn unwatch_key(&mut self, context: &str) {
        self.watches.remove(context);
        self.last_light.remove(context);
        let _ = self.outgoing.send(Outgoing::SetImage {
            context: context.to_string(),
            image: None,
        });
    }

    fn on_status(&mut self, key: EndpointKey, status: ConnectionStatus) {
        let connected = status.is_connected();
        self.pool.set_status(key.clone(), status);
        if !connected {
            for context in self.pool.contexts_for(&key) {
                self.refresh_tally_image(&context, Some(None));
            }
        }
        self.broadcast_status();
    }

    fn broadcast_status(&self) {
        for context in self.open_pi.keys().cloned().collect::<Vec<_>>() {
            self.push_status(&context);
        }
    }

    fn push_status(&self, context: &str) {
        let Some(action) = self.open_pi.get(context) else {
            return;
        };
        let connected = self
            .pool
            .status_for_context(context)
            .is_some_and(ConnectionStatus::is_connected);
        let label = self
            .pool
            .status_for_context(context)
            .map(ConnectionStatus::label)
            .unwrap_or_else(|| "Not connected".to_string());
        let endpoints = self
            .pool
            .endpoint_list()
            .into_iter()
            .map(|(key, status)| EndpointInfo {
                host: key.host,
                port: key.port,
                password: key.password,
                https: key.https,
                status,
            })
            .collect();
        let _ = self.outgoing.send(Outgoing::ToPi {
            action: action.to_string(),
            context: context.to_string(),
            payload: PiOut::state(connected, label, endpoints),
        });
        self.push_connections(action, context);
    }

    fn poll_tally(&mut self) {
        let contexts: Vec<String> = self
            .watches
            .iter()
            .filter(|(_, w)| w.binding.watches())
            .map(|(c, _)| c.clone())
            .collect();
        if contexts.is_empty() {
            return;
        }
        let mut keys: Vec<EndpointKey> = Vec::new();
        for context in &contexts {
            if let Some(key) = self.watches.get(context).and_then(|w| w.endpoint.clone()) {
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
        for key in keys {
            let tx = self.pool.sender(&key);
            let outgoing = self.outgoing.clone();
            let watches: Vec<(String, TallyBinding)> = self
                .watches
                .iter()
                .filter(|(_, w)| w.endpoint.as_ref() == Some(&key) && w.binding.watches())
                .map(|(c, w)| (c.clone(), w.binding.clone()))
                .collect();
            tokio::spawn(async move {
                let (reply_tx, reply_rx) = oneshot::channel();
                if tx.send(Work::Tally { reply: reply_tx }).is_err() {
                    return;
                }
                let Ok(Ok(snap)) = reply_rx.await else {
                    return;
                };
                for (context, binding) in watches {
                    let light = tally::light_for(&binding, &snap.scenes, &snap.auxes, &snap.inputs);
                    let image = light.map(image_data_uri);
                    let _ = outgoing.send(Outgoing::SetImage { context, image });
                }
            });
        }
    }

    fn refresh_tally_image(&mut self, context: &str, forced: Option<Option<TallyLight>>) {
        let light = forced.unwrap_or(None);
        if self.last_light.get(context) == Some(&light) {
            return;
        }
        self.last_light.insert(context.to_string(), light);
        let image = light.map(image_data_uri);
        let _ = self.outgoing.send(Outgoing::SetImage {
            context: context.to_string(),
            image,
        });
    }

    fn run_action(&mut self, action: String, context: String, settings: ActionSettings) {
        if action == actions::INPUT_TALLY {
            return;
        }
        let job = match build_job(&action, &settings) {
            Ok(job) => job,
            Err(e) => {
                let _ = self.outgoing.send(Outgoing::Log { message: e });
                let _ = self.outgoing.send(Outgoing::ShowAlert { context });
                return;
            }
        };
        let Some(key) = EndpointKey::from_settings(&settings) else {
            let _ = self.outgoing.send(Outgoing::ShowAlert { context });
            return;
        };
        let tx = self.pool.sender(&key);
        let outgoing = self.outgoing.clone();
        let skip = skip_ok(&action);
        tokio::spawn(async move {
            let (reply_tx, reply_rx) = oneshot::channel();
            if tx
                .send(Work::Exec {
                    job,
                    reply: reply_tx,
                })
                .is_err()
            {
                let _ = outgoing.send(Outgoing::ShowAlert { context });
                return;
            }
            match reply_rx.await {
                Ok(Ok(())) => {
                    if !skip {
                        let _ = outgoing.send(Outgoing::ShowOk { context });
                    }
                }
                Ok(Err(e)) => {
                    let _ = outgoing.send(Outgoing::Log { message: e });
                    let _ = outgoing.send(Outgoing::ShowAlert { context });
                }
                Err(_) => {
                    let _ = outgoing.send(Outgoing::ShowAlert { context });
                }
            }
        });
    }
}
