//! Bounded, event-driven waits. Dropping a wait never cancels a paid media job.
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::AppHandle;
use tokio::{sync::broadcast, task::JoinHandle, time::Instant};

pub const MAX_WAIT_MS: u64 = 60_000;
fn events() -> &'static broadcast::Sender<(String, String)> {
    static EVENTS: OnceLock<broadcast::Sender<(String, String)>> = OnceLock::new();
    EVENTS.get_or_init(|| broadcast::channel(128).0)
}
pub(crate) fn changed(domain: &str, id: &str) {
    let _ = events().send((domain.into(), id.into()));
}
pub fn timeout_ms(input: &Value) -> Result<u64, String> {
    match input.get("timeoutMs") {
        None => Ok(MAX_WAIT_MS),
        Some(value) => value
            .as_u64()
            .map(|v| v.min(MAX_WAIT_MS))
            .ok_or_else(|| "timeoutMs 必须为非负整数".into()),
    }
}
fn settled(domain: &str, status: &str) -> Option<&'static str> {
    match (domain, status) {
        (_, "running") | ("video", "submitting") => None,
        ("image", "ready") | ("video", "succeeded") => Some("finished"),
        (_, "error" | "failed") => Some("failed"),
        (_, "stopped") => Some("stopped"),
        (_, "interrupted" | "uncertain") => Some("needs_attention"),
        (_, "draft" | "planned" | "approved") => Some("idle"),
        _ => Some("needs_attention"),
    }
}
fn response(mut task: Value, state: &str, error: Option<String>) -> Value {
    task["wait"] = json!({"state":state,"error":error,
        "note":if state == "timeout" { "仍在处理，可再次 wait；不要 sleep 或重新提交。" }
        else if state == "finished" { "当前步骤已结束，请检查任务结果和下一步要求。" }
        else { "请按实际任务状态处理；等待不会提交或重试生成。" }});
    task
}

pub(crate) fn with_next_action(mut task: Value, domain: &str) -> Value {
    let wait_state = task["wait"]["state"].as_str();
    if matches!(task["status"].as_str(), Some("running" | "submitting"))
        && (wait_state.is_none() || wait_state == Some("timeout"))
    {
        task["nextAction"] = json!({"domain":domain,"action":"wait","input":{"id":task["id"]}});
        task["waitingHint"] =
            json!("调用 wait，完成会立即返回。不要用 sleep 定时，不要重复提交生成。");
    }
    task
}

type Poll = JoinHandle<Result<Value, String>>;
async fn wait_for(
    domain: &str,
    id: &str,
    timeout: Duration,
    mut snapshot: impl FnMut() -> Result<Value, String>,
    mut start_poll: impl FnMut() -> Poll,
) -> Result<Value, String> {
    // Subscribe BEFORE reading: a completion during the read must not be lost.
    let mut receiver = events().subscribe();
    let deadline = Instant::now() + timeout;
    let mut next_poll = Instant::now();
    let mut poll: Option<Poll> = None;
    loop {
        let task = snapshot()?;
        let status = task["status"].as_str().unwrap_or("");
        if let Some(state) = settled(domain, status) {
            return Ok(response(task, state, None));
        }
        if Instant::now() >= deadline {
            return Ok(response(task, "timeout", None));
        }
        if domain == "video" && status == "running" && poll.is_none() && Instant::now() >= next_poll
        {
            if task["remote"]["id"].as_str().is_none_or(|id| id.is_empty()) {
                return Ok(response(
                    task,
                    "needs_attention",
                    Some("缺少远程任务编号，请核查提交结果，不要重新生成。".into()),
                ));
            }
            poll = Some(start_poll());
        }
        tokio::select! {
            result = async { poll.as_mut().unwrap().await }, if poll.is_some() => {
                poll = None;
                next_poll = Instant::now() + crate::video_studio::POLL_INTERVAL;
                let error = match result {
                    Ok(Ok(_)) => None,
                    Ok(Err(error)) => Some(error),
                    Err(error) => Some(format!("查询任务异常：{error}")),
                };
                if let Some(error) = error {
                    let current = snapshot()?;
                    if let Some(state) = settled(domain, current["status"].as_str().unwrap_or("")) {
                        return Ok(response(current, state, None));
                    }
                    return Ok(response(current, "poll_error", Some(error)));
                }
            }
            event = receiver.recv() => {
                if let Ok((changed_domain, changed_id)) = event {
                    if changed_domain != domain || changed_id != id { continue; }
                }
            },
            // Recover external worker writes without native notifications too.
            _ = tokio::time::sleep_until(deadline.min(Instant::now() + Duration::from_secs(1))) => {},
        }
    }
}

pub async fn task(app: AppHandle, domain: &str, input: &Value) -> Result<Value, String> {
    if !matches!(domain, "image" | "video") {
        return Err("domain 必须为 image 或 video".into());
    }
    let id = input["id"].as_str().ok_or("需要任务 id")?;
    uuid::Uuid::parse_str(id).map_err(|_| "无效任务编号")?;
    let timeout = Duration::from_millis(timeout_ms(input)?);
    wait_for(
        domain,
        id,
        timeout,
        || {
            if domain == "image" {
                serde_json::to_value(crate::image_studio::image_studio_get(id.into())?)
                    .map_err(|e| e.to_string())
            } else {
                crate::video_studio::task_snapshot(id)
            }
        },
        || {
            let app = app.clone();
            let id = id.to_owned();
            // Once a read/download starts, let it finish and persist even if the
            // chat wait is cancelled or times out. This never starts a generation.
            tokio::spawn(async move { crate::video_studio::poll_task(app, &id).await })
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    fn unused_poll() -> Poll {
        panic!("must not poll this task")
    }
    #[tokio::test(start_paused = true)]
    async fn completion_notifies_all_waiters_before_fallback_or_timeout() {
        let task = Arc::new(Mutex::new(json!({"status":"running"})));
        let mut handles = vec![];
        for _ in 0..2 {
            let value = task.clone();
            handles.push(tokio::spawn(async move {
                wait_for(
                    "image",
                    "test-events",
                    Duration::from_secs(60),
                    || Ok(value.lock().unwrap().clone()),
                    unused_poll,
                )
                .await
                .unwrap()
            }));
        }
        tokio::task::yield_now().await;
        *task.lock().unwrap() = json!({"status":"ready","results":[{"path":"result.png"}]});
        changed("image", "test-events");
        tokio::task::yield_now().await;
        assert!(handles.iter().all(|h| h.is_finished()));
        for h in handles {
            assert_eq!(h.await.unwrap()["wait"]["state"], "finished");
        }
    }
    #[tokio::test(start_paused = true)]
    async fn timeout_is_bounded_and_terminal_states_return_without_polling() {
        let result = wait_for(
            "image",
            "timeout",
            Duration::from_secs(2),
            || Ok(json!({"status":"running"})),
            unused_poll,
        )
        .await
        .unwrap();
        assert_eq!(result["wait"]["state"], "timeout");
        for (domain, status, state) in [
            ("video", "uncertain", "needs_attention"),
            ("video", "failed", "failed"),
            ("image", "stopped", "stopped"),
            ("image", "draft", "idle"),
        ] {
            let result = wait_for(
                domain,
                "terminal",
                Duration::from_secs(60),
                || Ok(json!({"status":status})),
                unused_poll,
            )
            .await
            .unwrap();
            assert_eq!(result["wait"]["state"], state);
        }
        assert_eq!(
            timeout_ms(&json!({"timeoutMs":999999})).unwrap(),
            MAX_WAIT_MS
        );
        assert!(timeout_ms(&json!({"timeoutMs":-1})).is_err());
    }
    #[tokio::test(start_paused = true)]
    async fn video_wait_finishes_on_saved_result_and_reports_poll_errors() {
        let shared = Arc::new(Mutex::new(
            json!({"status":"running","remote":{"id":"job"}}),
        ));
        let result = wait_for(
            "video",
            "video",
            Duration::from_secs(60),
            || Ok(shared.lock().unwrap().clone()),
            || {
                let shared = shared.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    let task = json!({"status":"succeeded","output":"video.mp4"});
                    *shared.lock().unwrap() = task.clone();
                    Ok(task)
                })
            },
        )
        .await
        .unwrap();
        assert_eq!(result["output"], "video.mp4");
        assert_eq!(result["wait"]["state"], "finished");
        let result = wait_for(
            "video",
            "error",
            Duration::from_secs(60),
            || Ok(json!({"status":"running","remote":{"id":"job"}})),
            || tokio::spawn(async { Err("network error".into()) }),
        )
        .await
        .unwrap();
        assert_eq!(result["wait"]["state"], "poll_error");
    }
    #[tokio::test(start_paused = true)]
    async fn cancelling_wait_does_not_abort_inflight_download() {
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let marker = done.clone();
        let wait = tokio::spawn(async move {
            wait_for(
                "video",
                "cancel-test",
                Duration::from_secs(60),
                || Ok(json!({"status":"running","remote":{"id":"job"}})),
                || {
                    let marker = marker.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        marker.store(true, std::sync::atomic::Ordering::SeqCst);
                        Ok(json!({"status":"succeeded"}))
                    })
                },
            )
            .await
        });
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        wait.abort();
        let _ = wait.await;
        tokio::time::advance(Duration::from_secs(6)).await;
        tokio::task::yield_now().await;
        assert!(done.load(std::sync::atomic::Ordering::SeqCst));
    }
    #[test]
    fn waiting_hint_does_not_restart_failed_queries_or_finished_steps() {
        let timeout = with_next_action(
            json!({"id":"job","status":"running","wait":{"state":"timeout"}}),
            "video",
        );
        assert_eq!(timeout["nextAction"]["action"], "wait");
        for state in ["poll_error", "needs_attention"] {
            let result =
                with_next_action(json!({"status":"running","wait":{"state":state}}), "video");
            assert!(result.get("nextAction").is_none());
        }
        assert!(with_next_action(json!({"status":"ready"}), "image")
            .get("nextAction")
            .is_none());
    }
}
