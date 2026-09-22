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
    assert!(execute(&mut run, &root.0, &cancel, &executor)
        .await
        .unwrap_err()
        .contains("temporary failure"));
    let saved = read(&root.0, &run.id).unwrap();
    assert_eq!(saved.nodes[2].status, WorkflowStatus::Succeeded);
    assert_eq!(saved.nodes[1].status, WorkflowStatus::Failed);
    assert_eq!(saved.nodes[0].status, WorkflowStatus::Pending);
    run = saved;
    execute(&mut run, &root.0, &cancel, &executor)
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
    assert!(execute(&mut run, &root.0, &cancel, &executor)
        .await
        .is_err());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(run.nodes[2].status, WorkflowStatus::Succeeded);
    assert_eq!(run.nodes[1].status, WorkflowStatus::Pending);
    cancel.store(false, Ordering::SeqCst);
    execute(&mut run, &root.0, &cancel, &executor)
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
