//! Isolated native tasks with bounded progress delivery and cancellation.
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{
    io::{BufRead, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
};

#[derive(Clone, Default)]
pub struct Progress {
    pub label: String,
    pub current: String,
    pub done: usize,
    pub total: usize,
    pub percent: f32,
}

impl Progress {
    pub fn from_event(event: &Value, fallback_label: &str) -> Self {
        Self {
            label: event["label"].as_str().unwrap_or(fallback_label).into(),
            current: event["current"].as_str().unwrap_or_default().into(),
            done: event["done"].as_u64().unwrap_or(0) as usize,
            total: event["total"].as_u64().unwrap_or(0) as usize,
            percent: event["percent"].as_f64().unwrap_or(0.) as f32,
        }
    }
}

pub struct Job {
    pub events: mpsc::Receiver<Value>,
    child: Option<Child>,
}
pub fn emit(value: Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}
fn terminal(value: &Value) -> bool {
    matches!(value["type"].as_str(), Some("complete" | "error"))
}
fn read_events(reader: impl BufRead, tx: mpsc::SyncSender<Value>) {
    let mut finished = false;
    for line in reader.lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value["type"] == "progress" {
            let _ = tx.try_send(value);
        } else {
            finished |= terminal(&value);
            if tx.send(value).is_err() {
                break;
            }
        }
    }
    if !finished {
        let _ = tx.send(
            json!({"type":"error", "message":"任务进程已退出，可重新尝试。详情见运行日志。"}),
        );
    }
}
impl Job {
    pub fn spawn(mut command: Command) -> Result<Self> {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().context("无法启动后台任务")?;
        let stdout = child.stdout.take().context("无法读取任务进度")?;
        let stderr = child.stderr.take().context("无法读取任务日志")?;
        let (tx, events) = mpsc::sync_channel(8);
        std::thread::spawn(move || {
            read_events(std::io::BufReader::new(stdout), tx);
        });
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stderr)
                .lines()
                .map_while(Result::ok)
            {
                crate::diagnostics::record(format!("task: {line}"));
            }
        });
        Ok(Self {
            events,
            child: Some(child),
        })
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn warning_followed_by_process_exit_reports_failure_and_completion_is_terminal() {
        for (stream, expected) in [
            ("{\"type\":\"warning\"}\n", vec!["warning", "error"]),
            (
                "invalid log line\n{\"type\":\"progress\"}\n{\"type\":\"complete\"}\n",
                vec!["progress", "complete"],
            ),
        ] {
            let (send, receive) = mpsc::sync_channel(8);
            read_events(std::io::Cursor::new(stream.as_bytes()), send);
            let types = receive
                .iter()
                .map(|v| v["type"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            assert_eq!(types, expected);
        }
    }
}
