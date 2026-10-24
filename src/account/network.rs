//! A single background connection owns the API client and serialises GUI commands.
use anyhow::{Context, Result};
use majsoul_api::{Client, Policy, Snapshot, Zeroizing};
use std::sync::mpsc;
use tokio::sync::{mpsc as commands, watch};

pub fn prefetch() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let _ = std::thread::Builder::new()
            .name("account-routes".into())
            .spawn(|| {
                if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    let _ = runtime.block_on(majsoul_api::discover(&Policy::default()));
                }
            });
    });
}

pub enum Event {
    Progress(&'static str),
    Snapshot(Box<Snapshot>),
    Error(String),
    Disconnected(String),
}
enum Command {
    Sync,
}
pub struct Connection {
    pub events: mpsc::Receiver<Event>,
    commands: commands::Sender<Command>,
    cancel: watch::Sender<bool>,
}
impl Connection {
    pub fn login(
        username: String,
        password: Zeroizing<String>,
        device_id: String,
        remember: bool,
    ) -> Result<Self> {
        let (send, events) = mpsc::channel();
        let (command_tx, command_rx) = commands::channel(1);
        let (cancel, cancel_rx) = watch::channel(false);
        std::thread::Builder::new()
            .name("majsoul-account".into())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .context("无法创建账号网络任务")
                    .map(|runtime| {
                        runtime.block_on(run(
                            username, password, device_id, remember, command_rx, cancel_rx, &send,
                        ))
                    });
                if let Err(error) = result {
                    let _ = send.send(Event::Error(format!("{error:#}")));
                }
            })?;
        Ok(Self {
            events,
            commands: command_tx,
            cancel,
        })
    }
    pub fn sync(&self) -> Result<()> {
        self.commands
            .try_send(Command::Sync)
            .context("同步任务尚未完成或连接已关闭")
    }
    pub fn is_closed(&self) -> bool {
        self.commands.is_closed()
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
    }
}

async fn run(
    username: String,
    password: Zeroizing<String>,
    device_id: String,
    remember: bool,
    mut commands: commands::Receiver<Command>,
    mut cancel: watch::Receiver<bool>,
    events: &mpsc::Sender<Event>,
) {
    let initialise = async {
        let policy = Policy::default();
        let _ = events.send(Event::Progress("获取版本与线路…"));
        let discovery = majsoul_api::discover(&policy).await?;
        let _ = events.send(Event::Progress("连接账号服务器…"));
        let mut client = match Client::connect(discovery, policy).await {
            Ok(client) => client,
            Err(error) => {
                majsoul_api::invalidate_discovery().await;
                return Err(error);
            }
        };
        client.set_device_id(&device_id)?;
        let _ = events.send(Event::Progress("正在登录…"));
        client.login_with_hash(&username, password.clone()).await?;
        let _ = events.send(Event::Progress("读取角色、皮肤与库存…"));
        let snapshot = client.sync().await?;
        if remember
            && let Err(error) =
                super::credentials::save(&crate::model::state_dir(), &username, Some(&password))
        {
            crate::diagnostics::record(format!("remember credential: {error:#}"));
        }
        Ok::<_, anyhow::Error>((client, snapshot))
    };
    let initial = tokio::select! {
        _ = cancel.changed() => return,
        result = initialise => result,
    };
    let (mut client, snapshot) = match initial {
        Ok(result) => result,
        Err(error) => {
            crate::diagnostics::record(format!("account login/sync: {error:#}"));
            let _ = events.send(Event::Error(format!("{error:#}")));
            return;
        }
    };
    if events.send(Event::Snapshot(Box::new(snapshot))).is_err() {
        return;
    }
    let mut heartbeat = tokio::time::interval(client.heartbeat_interval());
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    heartbeat.tick().await;
    loop {
        tokio::select! {
            _ = cancel.changed() => { client.close().await; return; },
            command = commands.recv() => {
                let Some(Command::Sync) = command else { client.close().await; return; };
                let result = tokio::select! {
                    _ = cancel.changed() => { client.close().await; return; },
                    result = client.sync() => result,
                };
                if !client.is_connected() {
                    majsoul_api::invalidate_discovery().await;
                    let message = result.err().map(|error| format!("{error:#}")).unwrap_or_else(|| "连接已中断，请重新登录".into());
                    crate::diagnostics::record(format!("account connection: {message}"));
                    let _ = events.send(Event::Disconnected(message));
                    client.close().await;
                    return;
                }
                let event = match result {
                    Ok(snapshot) => Event::Snapshot(Box::new(snapshot)),
                    Err(error) => {
                        crate::diagnostics::record(format!("account sync: {error:#}"));
                        Event::Error(format!("{error:#}"))
                    },
                };
                if events.send(event).is_err() { return; }
            },
            _ = heartbeat.tick() => {
                let result = tokio::select! {
                    _ = cancel.changed() => { client.close().await; return; },
                    result = client.heartbeat() => result,
                };
                if let Err(error) = result {
                    majsoul_api::invalidate_discovery().await;
                    crate::diagnostics::record(format!("account connection: {error:#}"));
                    let _ = events.send(Event::Disconnected(format!("{error:#}")));
                    return;
                }
            }
        }
    }
}
