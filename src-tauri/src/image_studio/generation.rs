//! Bounded image execution with a single writer for the persisted task.
use super::{engine::Submission, storage, types::*};
use futures::{future::BoxFuture, stream::FuturesUnordered, FutureExt, StreamExt};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub(super) const CONCURRENCY: usize = 9;

pub(super) trait Backend: Sync {
    fn submit<'a>(&'a self, plan: &'a ImagePlan) -> BoxFuture<'a, Result<Submission, String>>;
    fn poll<'a>(
        &'a self,
        cfg: &'a StudioConfig,
        remote: &'a str,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, String>>;
    fn store(&self, result: &mut ImageResult, bytes: &[u8]) -> Result<(), String>;
}

pub(super) async fn resume(
    backend: &impl Backend,
    cfg: &StudioConfig,
    remote: &str,
    flag: &AtomicBool,
) -> Result<Vec<u8>, String> {
    for _ in 0..180 {
        if flag.load(Ordering::Relaxed) {
            return Err("已停止查询，远程任务编号已保存，可稍后恢复".into());
        }
        if let Some(bytes) = backend.poll(cfg, remote).await? {
            return Ok(bytes);
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    Err("等待超时，远程任务编号已保存。稍后点击恢复查询，不会重新下单。".into())
}

fn progress(
    task: &mut Task,
    total: usize,
    completed: usize,
    active: usize,
    failed: usize,
    flag: &AtomicBool,
) {
    task.progress = if flag.load(Ordering::Relaxed) {
        format!("正在停止，等待已提交的 {active} 张图片收尾；已有结果会保存")
    } else {
        format!(
            "并发生成中 · 已完成 {completed}/{total} 张 · 进行中 {active} 张 · 失败 {failed} 张"
        )
    };
}

pub(super) async fn run(
    task: &mut Task,
    cfg: &StudioConfig,
    plans: Vec<ImagePlan>,
    flag: &AtomicBool,
    backend: &impl Backend,
    mut persist: impl FnMut(&mut Task) -> Result<(), String>,
) -> Result<(), String> {
    let total = plans.len();
    let mut queued = plans.into_iter();
    let mut pending: FuturesUnordered<BoxFuture<'_, (usize, Result<Submission, String>)>> =
        FuturesUnordered::new();
    let mut completed = 0;
    let mut failed = 0;
    let mut first_error = None;
    let mut storage_error = None;

    loop {
        while pending.len() < CONCURRENCY
            && storage_error.is_none()
            && !flag.load(Ordering::Relaxed)
        {
            let Some(plan) = queued.next() else { break };
            let index = task.results.len();
            task.results.push(ImageResult {
                id: storage::id(),
                product_id: plan.product_id.clone(),
                slot_id: plan.slot_id.clone(),
                revision: task.revision,
                path: None,
                error: None,
                remote_id: None,
                prompt: plan.prompt.clone(),
                width: 0,
                height: 0,
                review: None,
                config: cfg.clone(),
            });
            progress(task, total, completed, pending.len() + 1, failed, flag);
            // Record the attempt before submitting. Only this coordinator writes task state.
            if let Err(error) = persist(task) {
                task.results.pop();
                storage_error = Some(error);
                break;
            }
            pending.push(async move { (index, backend.submit(&plan).await) }.boxed());
        }

        let Some((index, submission)) = pending.next().await else {
            break;
        };
        let outcome = match submission {
            Ok(Submission::Pending(remote)) => {
                task.results[index].remote_id = Some(remote.clone());
                // A remote ID must reach disk before polling it, including during cancellation.
                if let Err(error) = persist(task) {
                    task.results[index].error = Some(error.clone());
                    storage_error = Some(error.clone());
                    Err(error)
                } else {
                    let cfg = task.results[index].config.clone();
                    pending.push(
                        async move {
                            (
                                index,
                                resume(backend, &cfg, &remote, flag)
                                    .await
                                    .map(Submission::Image),
                            )
                        }
                        .boxed(),
                    );
                    continue;
                }
            }
            Ok(Submission::Image(bytes)) => backend.store(&mut task.results[index], &bytes),
            Err(error) => Err(error),
        };
        completed += 1;
        if let Err(error) = outcome {
            task.results[index].error = Some(error.clone());
            failed += 1;
            first_error.get_or_insert(error);
        }
        progress(task, total, completed, pending.len(), failed, flag);
        if let Err(error) = persist(task) {
            // Drain already submitted work even when saving fails; never drop billable requests.
            storage_error = Some(error);
        }
    }
    if let Some(error) = storage_error {
        return Err(error);
    }
    if flag.load(Ordering::Relaxed) {
        return Ok(());
    }
    if let Some(error) = first_error {
        return Err(if total == 1 {
            error
        } else {
            format!("本轮 {failed}/{total} 张图片失败，其余结果已保存；请查看对应图片的错误后单独重试。")
        });
    }
    Ok(())
}
