use std::sync::mpsc::{self, Receiver, Sender};

use diamond_plugin::{PluginHost, PluginManifest, PluginOutput};

pub enum PluginRequest {
    Invoke { command: String, text: Option<String> },
    Stop,
}

pub enum PluginEvent {
    Ready(Vec<(String, String)>),
    Output(PluginOutput),
    Failed(String),
    Stopped,
}

pub struct PluginWorker {
    pub sender: Sender<PluginRequest>,
    pub receiver: Receiver<PluginEvent>,
}

impl PluginWorker {
    pub fn launch(manifest: PluginManifest, ctx: egui::Context) -> Self {
        let (sender, requests) = mpsc::channel();
        let (events, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let notify = |event| {
                let _ = events.send(event);
                ctx.request_repaint();
            };
            let mut host = match PluginHost::start(manifest) {
                Ok(host) => host,
                Err(error) => {
                    notify(PluginEvent::Failed(error.to_string()));
                    notify(PluginEvent::Stopped);
                    return;
                }
            };
            let mut failure = None;
            notify(PluginEvent::Ready(host.commands().iter()
                .map(|command| (command.id.clone(), command.title.clone())).collect()));
            while let Ok(request) = requests.recv() {
                match request {
                    PluginRequest::Invoke { command, text } => {
                        match host.invoke(&command, text.as_deref()) {
                            Ok(output) => notify(PluginEvent::Output(output)),
                            Err(error) => { failure = Some(error.to_string()); break; }
                        }
                    }
                    PluginRequest::Stop => break,
                }
            }
            if let Err(error) = host.stop() { failure = Some(error.to_string()); }
            drop(host);
            if let Some(error) = failure { notify(PluginEvent::Failed(error)); }
            notify(PluginEvent::Stopped);
        });
        Self { sender, receiver }
    }
}

impl Drop for PluginWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(PluginRequest::Stop);
    }
}
