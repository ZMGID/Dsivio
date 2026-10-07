use super::*;
use std::sync::atomic::AtomicUsize;

fn workflow() -> GenerationWorkflow {
    serde_json::from_value(serde_json::json!({
        "id":"flow", "name":"文案流程", "createdAt":"now", "updatedAt":"now",
        "nodes":[
            {"id":"output","kind":"text.preview","title":"结果","position":{"x":640,"y":0},"config":{"type":"output"}},
            {"id":"join","kind":"text.join","title":"组合","position":{"x":320,"y":0},"config":{"type":"text","instruction":"商品","text":"本地备用","model":null}},
            {"id":"input","kind":"prompt.input","title":"输入","position":{"x":0,"y":0},"config":{"type":"prompt","text":"骑行头盔"}}
        ],
        "edges":[
            {"id":"a","source":"input","target":"join","sourceHandle":"text","targetHandle":"text"},
            {"id":"b","source":"join","target":"output","sourceHandle":"text","targetHandle":"text"}
        ]
    })).unwrap()
}
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("workflow-test-{}", uuid::Uuid::new_v4())))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct TestExecutor {
    calls: AtomicUsize,
    fail: AtomicBool,
    stop_after_input: bool,
}
impl TestExecutor {
    fn new(fail: bool) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            fail: AtomicBool::new(fail),
            stop_after_input: false,
        }
    }
}
impl Executor for TestExecutor {
    fn run<'a>(
        &'a self,
        node: &'a WorkflowNode,
        run: &'a mut WorkflowRun,
        root: &'a Path,
        index: usize,
        cancelled: &'a AtomicBool,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<BTreeMap<String, WorkflowValue>, String>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let durable = read(root, &run.id)?;
            assert_eq!(durable.nodes[index].status, WorkflowStatus::Running);
            if node.id == "join" && self.fail.swap(false, Ordering::SeqCst) {
                return Err("temporary failure".into());
            }
            let text = if node.id == "input" {
                "骑行头盔".into()
            } else {
                let parent = input(run, node, "text")
                    .and_then(|v| v.text)
                    .ok_or("missing upstream")?;
                if node.id == "join" {
                    format!("商品\n\n{parent}")
                } else {
                    parent
                }
            };
            if self.stop_after_input && node.id == "input" {
                cancelled.store(true, Ordering::SeqCst);
            }
            Ok(output(
                "text",
                WorkflowValue {
                    text: Some(text),
                    files: vec![],
                },
            ))
        })
    }
}
#[test]
fn rejects_cycles_invalid_ports_and_duplicate_inputs_before_submission() {
    let flow = workflow();
    assert_eq!(graph::validate(&flow).unwrap(), ["input", "join", "output"]);
    let mut bad = flow.clone();
    bad.edges[0].source_handle = Some("image".into());
    assert!(graph::validate(&bad).is_err());
    let mut bad = flow.clone();
    let mut edge = bad.edges[0].clone();
    edge.id = "c".into();
    bad.edges.push(edge);
    assert!(graph::validate(&bad).is_err());
    let mut bad = flow.clone();
    bad.nodes[2].kind = "text.join".into();
    bad.nodes[2].config = bad.nodes[1].config.clone();
    bad.edges.push(WorkflowEdge {
        id: "cycle".into(),
        source: "join".into(),
        target: "input".into(),
        source_handle: Some("text".into()),
        target_handle: Some("text".into()),
    });
    assert!(graph::validate(&bad).unwrap_err().contains("循环"));
    let mut bad = flow;
    bad.nodes[1].config = Some(WorkflowConfig::Placeholder);
    assert!(graph::validate(&bad).is_err());
}
#[tokio::test]
async fn checkpoints_topological_outputs_and_resumes_only_failed_nodes() {
    let root = Temp::new();
    let mut run = new_run(workflow());
    let executor = TestExecutor::new(true);
    let cancel = AtomicBool::new(false);
    assert!(execute(&mut run, &root.0, &cancel, &executor, None)
        .await
        .unwrap_err()
        .contains("temporary failure"));
    let saved = read(&root.0, &run.id).unwrap();
    assert_eq!(saved.nodes[2].status, WorkflowStatus::Succeeded);
    assert_eq!(saved.nodes[1].status, WorkflowStatus::Failed);
    assert_eq!(saved.nodes[0].status, WorkflowStatus::Pending);
    run = saved;
    execute(&mut run, &root.0, &cancel, &executor, None)
        .await
        .unwrap();
    assert_eq!(run.status, WorkflowStatus::Succeeded);
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        4,
        "completed input must not execute twice"
    );
    assert_eq!(
        run.nodes[0].outputs["text"].text.as_deref(),
        Some("商品\n\n骑行头盔")
    );
    assert_eq!(
        read(&root.0, &run.id).unwrap().nodes[0].outputs["text"].text,
        run.nodes[0].outputs["text"].text
    );
}
#[tokio::test]
async fn cancellation_prevents_downstream_and_keeps_completed_outputs() {
    let root = Temp::new();
    let mut run = new_run(workflow());
    let cancel = AtomicBool::new(false);
    let executor = TestExecutor {
        stop_after_input: true,
        ..TestExecutor::new(false)
    };
    assert!(execute(&mut run, &root.0, &cancel, &executor, None)
        .await
        .is_err());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(run.nodes[2].status, WorkflowStatus::Succeeded);
    assert_eq!(run.nodes[1].status, WorkflowStatus::Pending);
    cancel.store(false, Ordering::SeqCst);
    execute(&mut run, &root.0, &cancel, &executor, None)
        .await
        .unwrap();
    assert_eq!(executor.calls.load(Ordering::SeqCst), 3);
}
#[test]
fn exclusive_run_claim_and_restart_recovery_preserve_receipts() {
    let root = Temp::new();
    let mut run = new_run(workflow());
    run.workflow.id = uuid::Uuid::new_v4().to_string();
    let flag = claim(&run, &root.0).unwrap();
    let guard = Claim(run.workflow.id.clone());
    assert!(claim(&new_run(run.workflow.clone()), &root.0).is_err());
    cancel_workflow_run(run.id.clone()).unwrap();
    assert!(flag.load(Ordering::SeqCst));
    drop(guard);
    run.nodes[1].status = WorkflowStatus::Running;
    run.nodes[1].media_task_id = Some("existing-receipt".into());
    recover(&mut run);
    assert_eq!(run.status, WorkflowStatus::Interrupted);
    assert_eq!(
        run.nodes[1].media_task_id.as_deref(),
        Some("existing-receipt")
    );
    assert_eq!(run.nodes[1].status, WorkflowStatus::Interrupted);
}
#[test]
fn zip_output_is_real_and_collision_safe() {
    let root = Temp::new();
    std::fs::create_dir_all(&root.0).unwrap();
    let path = root.0.join("image.png");
    std::fs::write(&path, b"fixture").unwrap();
    let archive = root.0.join("result.zip");
    zip_files(
        &[
            path.to_string_lossy().into_owned(),
            path.to_string_lossy().into_owned(),
        ],
        &archive,
    )
    .unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive).unwrap()).unwrap();
    assert_eq!(zip.len(), 2);
    assert_eq!(zip.by_index(0).unwrap().name(), "1-image.png");
    assert!(directory(&root.0, "../../escape").is_err());
}
#[test]
fn video_inputs_are_bound_explicitly_without_forwarding_image_fields() {
    let mut images = vec!["/product.png".into()];
    let mut options = BTreeMap::new();
    bind_video_images(
        "video.generate",
        false,
        &BTreeMap::new(),
        &mut images,
        &mut options,
    );
    assert!(images.is_empty());
    assert_eq!(options["firstFrame"], "/product.png");
    let mut images = vec!["/product.png".into()];
    let mut options = BTreeMap::new();
    bind_video_images(
        "video.generate",
        false,
        &[("imageMode".into(), "reference".into())].into(),
        &mut images,
        &mut options,
    );
    assert_eq!(
        options["referenceImages"],
        serde_json::json!(["/product.png"])
    );
    let mut images = vec!["/product.png".into()];
    let mut options = BTreeMap::new();
    bind_video_images(
        "video.generate",
        true,
        &BTreeMap::new(),
        &mut images,
        &mut options,
    );
    assert_eq!(images.len(), 1);
    assert!(options.is_empty());
}
fn media_workflow() -> GenerationWorkflow {
    serde_json::from_value(serde_json::json!({
        "id":"media-flow", "name":"出图", "createdAt":"now", "updatedAt":"now",
        "nodes":[{
            "id":"gen","kind":"image.generate","title":"生成","position":{"x":0,"y":0},
            "config":{"type":"generate","prompt":"头盔","assets":[],"model":{"providerId":"p","model":"m"},"options":{}}
        }],
        "edges":[]
    }))
    .unwrap()
}
struct ReceiptExecutor {
    starts: AtomicUsize,
}
impl Executor for ReceiptExecutor {
    fn run<'a>(
        &'a self,
        _node: &'a WorkflowNode,
        run: &'a mut WorkflowRun,
        _root: &'a Path,
        index: usize,
        _cancelled: &'a AtomicBool,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<BTreeMap<String, WorkflowValue>, String>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            match receipt_action(&run.nodes[index].media_task_id) {
                Receipt::Existing(id) => assert_eq!(id, "receipt-1"),
                Receipt::NeedsSubmit => {
                    self.starts.fetch_add(1, Ordering::SeqCst);
                }
            }
            Ok(output(
                "image",
                WorkflowValue {
                    text: None,
                    files: vec!["/out.png".into()],
                },
            ))
        })
    }
}
struct TimeoutExecutor;
impl Executor for TimeoutExecutor {
    fn run<'a>(
        &'a self,
        _node: &'a WorkflowNode,
        run: &'a mut WorkflowRun,
        _root: &'a Path,
        index: usize,
        _cancelled: &'a AtomicBool,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<BTreeMap<String, WorkflowValue>, String>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            run.nodes[index].media_task_id = Some("receipt-timeout".into());
            run.nodes[index].status = WorkflowStatus::Interrupted;
            Err(MEDIA_WAIT_TIMEOUT.into())
        })
    }
}
#[test]
fn resume_fails_while_worker_holds_claim_before_final_save() {
    let root = Temp::new();
    let mut run = new_run(workflow());
    run.workflow.id = uuid::Uuid::new_v4().to_string();
    let _flag = claim(&run, &root.0).unwrap();
    let _guard = Claim(run.workflow.id.clone());
    assert_eq!(
        read(&root.0, &run.id).unwrap().status,
        WorkflowStatus::Running
    );
    let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    let error = resume_claimed(&root.0, &run.id, &mut active, None).unwrap_err();
    assert!(error.contains("正在运行"), "{error}");
}
#[tokio::test]
async fn cancel_persists_cancelled_run_and_resume_skips_succeeded_nodes() {
    let root = Temp::new();
    let mut run = new_run(workflow());
    let cancel = AtomicBool::new(false);
    let executor = TestExecutor {
        stop_after_input: true,
        ..TestExecutor::new(false)
    };
    assert!(execute(&mut run, &root.0, &cancel, &executor, None)
        .await
        .is_err());
    let saved = read(&root.0, &run.id).unwrap();
    assert_eq!(saved.status, WorkflowStatus::Cancelled);
    assert_eq!(
        saved
            .nodes
            .iter()
            .filter(|node| node.status == WorkflowStatus::Succeeded)
            .count(),
        1
    );
    cancel.store(false, Ordering::SeqCst);
    run = saved;
    run.status = WorkflowStatus::Running;
    run.error = None;
    execute(&mut run, &root.0, &cancel, &executor, None)
        .await
        .unwrap();
    assert_eq!(executor.calls.load(Ordering::SeqCst), 3);
    assert_eq!(run.status, WorkflowStatus::Succeeded);
}
#[tokio::test]
async fn failed_media_receipt_is_not_resubmitted_on_resume() {
    let root = Temp::new();
    let mut run = new_run(media_workflow());
    run.nodes[0].status = WorkflowStatus::Failed;
    run.nodes[0].error = Some("媒体生成失败".into());
    run.nodes[0].media_task_id = Some("receipt-1".into());
    let executor = ReceiptExecutor {
        starts: AtomicUsize::new(0),
    };
    let cancel = AtomicBool::new(false);
    execute(&mut run, &root.0, &cancel, &executor, None)
        .await
        .unwrap();
    assert_eq!(
        executor.starts.load(Ordering::SeqCst),
        0,
        "start_media_generation is the NeedsSubmit arm and must not run for an existing receipt"
    );
    assert_eq!(run.nodes[0].media_task_id.as_deref(), Some("receipt-1"));
    assert_eq!(run.nodes[0].status, WorkflowStatus::Succeeded);
}
#[tokio::test]
async fn node_failure_persists_run_status_in_the_same_save() {
    let root = Temp::new();
    let mut run = new_run(workflow());
    let executor = TestExecutor::new(true);
    let cancel = AtomicBool::new(false);
    let error = execute(&mut run, &root.0, &cancel, &executor, None)
        .await
        .unwrap_err();
    assert!(error.contains("temporary failure"), "{error}");
    let saved = read(&root.0, &run.id).unwrap();
    assert_eq!(saved.status, WorkflowStatus::Failed);
    assert!(saved.error.unwrap().contains("temporary failure"));
    let failed = saved
        .nodes
        .iter()
        .find(|node| node.status == WorkflowStatus::Failed)
        .unwrap();
    assert_eq!(failed.error.as_deref(), Some("temporary failure"));
    assert_eq!(saved.updated_at, failed.finished_at.clone().unwrap());
}
#[test]
fn check_workflow_reports_unsupported_kind_and_accepts_text_sample() {
    let ready = check_workflow_issues(None, &workflow());
    assert!(ready.is_empty(), "{ready:?}");
    let mut unsupported = workflow();
    unsupported.nodes.push(WorkflowNode {
        id: "extra".into(),
        kind: "audio.missing".into(),
        title: "未开放".into(),
        position: WorkflowPosition { x: 0.0, y: 0.0 },
        config: Some(WorkflowConfig::Placeholder),
        note: None,
    });
    let issues = check_workflow_issues(None, &unsupported);
    assert!(
        issues.iter().any(|issue| issue.contains("尚不支持")),
        "{issues:?}"
    );
}
#[test]
fn retention_deletes_oldest_terminal_run_past_the_limit() {
    let root = Temp::new();
    std::fs::create_dir_all(&root.0).unwrap();
    let mut flow = workflow();
    flow.id = uuid::Uuid::new_v4().to_string();
    let mut running = new_run(flow.clone());
    running.status = WorkflowStatus::Running;
    running.created_at = "2020-01-01T00:00:00+00:00".into();
    let running_id = running.id.clone();
    save(&root.0, &running, None).unwrap();
    let mut oldest = String::new();
    for index in 0..=MAX_RUNS_PER_WORKFLOW {
        let mut run = new_run(flow.clone());
        run.status = WorkflowStatus::Succeeded;
        run.created_at = format!("2026-01-01T00:{index:02}:00+00:00");
        if index == 0 {
            oldest = run.id.clone();
        }
        save(&root.0, &run, None).unwrap();
    }
    prune_workflow_runs(&root.0, &flow.id).unwrap();
    assert!(
        !root.0.join(&oldest).exists(),
        "oldest terminal run should be deleted"
    );
    assert!(
        root.0.join(&running_id).exists(),
        "running run must be kept"
    );
    let remaining = std::fs::read_dir(&root.0).unwrap().count();
    assert_eq!(remaining, MAX_RUNS_PER_WORKFLOW);
    let broken = root.0.join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("run.json"), b"{").unwrap();
    prune_workflow_runs(&root.0, &flow.id).unwrap();
    assert!(broken.exists());
}
#[test]
fn recover_keeps_recorded_failure_and_only_interrupts_running_nodes() {
    let mut run = new_run(workflow());
    run.error = Some("already recorded".into());
    run.nodes[1].status = WorkflowStatus::Failed;
    run.nodes[1].error = Some("temporary failure".into());
    run.nodes[2].status = WorkflowStatus::Running;
    run.nodes[2].media_task_id = Some("existing-receipt".into());
    recover(&mut run);
    assert_eq!(run.status, WorkflowStatus::Failed);
    assert_eq!(run.error.as_deref(), Some("already recorded"));
    assert_eq!(run.nodes[1].error.as_deref(), Some("temporary failure"));
    assert_eq!(run.nodes[2].status, WorkflowStatus::Interrupted);
    assert_eq!(
        run.nodes[2].media_task_id.as_deref(),
        Some("existing-receipt")
    );
    assert_eq!(run.nodes[0].status, WorkflowStatus::Pending);
}
#[test]
fn cancel_without_active_run_is_an_error() {
    let error = cancel_workflow_run(uuid::Uuid::new_v4().to_string()).unwrap_err();
    assert!(error.contains("没有正在运行的记录"), "{error}");
}
#[tokio::test]
async fn media_wait_timeout_is_terminal_and_keeps_receipt() {
    let root = Temp::new();
    let mut run = new_run(media_workflow());
    let cancel = AtomicBool::new(false);
    let error = execute(&mut run, &root.0, &cancel, &TimeoutExecutor, None)
        .await
        .unwrap_err();
    assert!(error.contains(MEDIA_WAIT_TIMEOUT), "{error}");
    let saved = read(&root.0, &run.id).unwrap();
    assert_eq!(saved.status, WorkflowStatus::Interrupted);
    assert_eq!(saved.nodes[0].status, WorkflowStatus::Interrupted);
    assert_eq!(
        saved.nodes[0].media_task_id.as_deref(),
        Some("receipt-timeout")
    );
}
