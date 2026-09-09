use super::*;

#[test]
fn shared_templates_discover_chat_files_and_prefer_current_standard_over_stale_record() {
    let base = std::env::temp_dir().join(format!("dsivio-shared-test-{}", storage::id()));
    let directory = base.join("templates/chat-created");
    std::fs::create_dir_all(&directory).unwrap();
    let data = json!({"name":"聊天模板","mode":"smart","slots":[{"id":"h1","brief":"内容"}],"custom":{"keep":true}});
    storage::write(&directory.join("template.json"), &data).unwrap();
    let mut templates = vec![];
    storage::scan_templates(&base, &base.join("templates"), 0, &mut templates).unwrap();
    assert_eq!(templates.len(), 1);
    let original_id = templates[0].id.clone();
    storage::write(&directory.join("record.json"), &templates[0]).unwrap();
    let mut updated = data.clone();
    updated["name"] = json!("聊天中改名");
    storage::write(&directory.join("template.json"), &updated).unwrap();
    templates.clear();
    storage::scan_templates(&base, &base.join("templates"), 0, &mut templates).unwrap();
    assert_eq!(templates[0].id, original_id);
    assert_eq!(templates[0].data, updated);
    // A half-written file must not take the entire library offline.
    std::fs::write(directory.join("template.json"), "{").unwrap();
    templates.clear();
    storage::scan_templates(&base, &base.join("templates"), 0, &mut templates).unwrap();
    assert!(templates.is_empty());
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn bundled_backpack_templates_materialize_all_reference_images_and_copy_assets() {
    let base = std::env::temp_dir().join(format!("dsivio-builtin-test-{}", storage::id()));
    let templates = builtins::templates(&base).unwrap();
    assert_eq!(templates.len(), 4);
    for template in &templates {
        storage::validate_template(&template.data).unwrap();
        if template.data["mode"] == "replace" {
            let slots = template.data["slots"].as_array().unwrap();
            assert_eq!(slots.len(), 9);
            for slot in slots {
                let path = base
                    .join(&template.directory)
                    .join(slot["example"].as_str().unwrap());
                let bytes = std::fs::read(path).unwrap();
                assert!(image::load_from_memory(&bytes).is_ok());
            }
        }
    }
    let copy = base.join("copy");
    builtins::install_assets("builtin-mens-backpack-v1", &copy).unwrap();
    assert!(copy.join("assets/logo.png").is_file());
    assert!(copy.join("assets/back_template.png").is_file());
    assert!(copy.join("h9.png").is_file());
    std::fs::remove_dir_all(base).unwrap();
}

fn fixture() -> Task {
    serde_json::from_value(json!({
        "id":"test-task","revision":3,"createdAt":"2026-09-08","updatedAt":"2026-09-08",
        "brief":{"feature":"smart","name":"test","requirement":"商品图","language":"pt-BR","platform":"通用电商","ratio":"1:1","resolution":"1k","count":1,"style":"","templateId":null,
          "products":[
            {"id":"a","name":"A","category":"背包","kind":"","facts":"","front":null,"back":null,"assets":[],"templateId":null},
            {"id":"b","name":"B","category":"背包","kind":"","facts":"","front":null,"back":null,"assets":[],"templateId":null},
            {"id":"c","name":"C","category":"背包","kind":"","facts":"","front":null,"back":null,"assets":[],"templateId":null},
            {"id":"d","name":"D","category":"童装","kind":"","facts":"","front":null,"back":null,"assets":[],"templateId":null}]},
        "plans":[{"productId":"a","slotId":"h1","purpose":"主图","copy":"","prompt":"a","refs":[]},{"productId":"b","slotId":"h1","purpose":"主图","copy":"","prompt":"b","refs":[]}],
        "results":[],"approvedGroups":[],"status":"planned","progress":"","error":null,"templates":[]
    })).unwrap()
}
fn result(product: &str, rev: u64, path: Option<&str>) -> ImageResult {
    serde_json::from_value(json!({"id":storage::id(),"productId":product,"slotId":"h1","revision":rev,"path":path,"error":null,"remoteId":null,"prompt":"","width":800,"height":800,"review":null,"config":{}})).unwrap()
}

fn workflow_fixture() -> Task {
    let mut task = fixture();
    task.brief.feature = "workflow".into();
    task.brief.workflow_input = Some(WorkflowInput {
        mode: "smart".into(),
        sources: vec![Asset {
            id: "source".into(),
            name: "原始商品.png".into(),
            path: "assets/source.png".into(),
        }],
    });
    let template = Template {
        id: "rules".into(),
        directory: "templates/rules".into(),
        builtin: false,
        data: json!({"name":"背包系列","mode":"smart","style":"white background","text_policy":"真实卖点",
            "output":{"ratio":"1:1","resolution":"1k"},"slots":[{"id":"h1","purpose":"主图","brief":"主体居中","refs":["@product.front"]}]}),
    };
    workflow::install_rules(&mut task, template, "初次制作".into(), "共用版式".into());
    task.workflow.as_mut().unwrap().sample_ids = vec!["a".into(), "b".into()];
    task
}

fn workflow_action(kind: &str) -> Action {
    Action {
        kind: kind.into(),
        ..Default::default()
    }
}

#[test]
fn workflow_append_preserves_approval_and_only_new_products_need_generation() {
    let mut task = workflow_fixture();
    task.results.extend([
        result("a", task.revision, Some("a.png")),
        result("b", task.revision, Some("b.png")),
    ]);
    task.workflow.as_mut().unwrap().approved_version = Some(1);
    let mut brief = task.brief.clone();
    let mut product = brief.products[0].clone();
    product.id = "next-product".into();
    product.name = "下一款商品".into();
    brief.products.push(product);
    workflow::save_brief(&mut task, brief).unwrap();
    assert!(workflow::sample_complete(&task));
    assert_eq!(task.workflow.as_ref().unwrap().approved_version, Some(1));
    workflow::validate_action(&task, &workflow_action("workflow_produce")).unwrap();
    task.plans = ["a", "b", "next-product"]
        .iter()
        .map(|id| ImagePlan {
            product_id: id.to_string(),
            slot_id: "h1".into(),
            purpose: "主图".into(),
            copy: "".into(),
            prompt: "product".into(),
            refs: vec![],
        })
        .collect();
    let ids = task
        .brief
        .products
        .iter()
        .map(|p| p.id.clone())
        .collect::<Vec<_>>();
    let pending = workflow::pending_plans(&task, &ids);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].product_id, "next-product");
    assert_eq!(task.results.len(), 2); // Saving does not duplicate unchanged image records.
}

#[test]
fn workflow_feedback_versions_rules_and_requires_fresh_trial_evidence() {
    let mut task = workflow_fixture();
    task.results.extend([
        result("a", task.revision, Some("a.png")),
        result("b", task.revision, Some("b.png")),
    ]);
    task.workflow.as_mut().unwrap().approved_version = Some(1);
    let mut rules = task.templates[0].clone();
    rules.data["slots"][0]["brief"] = json!("主体放大至画面宽度 70%");
    workflow::validate_rules(&rules.data, Some(&task.templates[0])).unwrap();
    workflow::install_rules(
        &mut task,
        rules,
        "产品太小".into(),
        "后续商品统一放大主体".into(),
    );
    let workflow = task.workflow.as_ref().unwrap();
    assert_eq!(workflow.rule_version, 2);
    assert_eq!(workflow.sample_ids, vec!["a", "b"]);
    assert_eq!(
        workflow.changes[0].template.data["slots"][0]["brief"],
        "主体居中"
    );
    assert_eq!(
        workflow.changes[1].template.data["slots"][0]["brief"],
        "主体放大至画面宽度 70%"
    );
    assert!(workflow.approved_version.is_none());
    assert!(task.plans.is_empty());
    assert_eq!(task.results.len(), 2);
    assert!(workflow::validate_action(&task, &workflow_action("workflow_approve")).is_err());
    assert!(workflow::validate_action(&task, &workflow_action("workflow_produce")).is_err());
    task.results.extend([
        result("a", task.revision, Some("a-v2.png")),
        result("b", task.revision, Some("b-v2.png")),
    ]);
    workflow::validate_action(&task, &workflow_action("workflow_approve")).unwrap();
    let reopened: Task = serde_json::from_value(serde_json::to_value(&task).unwrap()).unwrap();
    assert!(workflow::sample_complete(&reopened));
    assert_eq!(reopened.workflow.unwrap().changes[1].note, "产品太小");
}

#[test]
fn workflow_changed_source_requirements_or_trial_product_invalidate_the_right_state() {
    let mut task = workflow_fixture();
    task.results.extend([
        result("a", task.revision, Some("a.png")),
        result("b", task.revision, Some("b.png")),
    ]);
    task.workflow.as_mut().unwrap().approved_version = Some(1);
    let mut brief = task.brief.clone();
    brief.products[0].facts = "改为真实容量 20L".into();
    workflow::save_brief(&mut task, brief).unwrap();
    assert!(task.workflow.as_ref().unwrap().rules_current);
    assert!(task.workflow.as_ref().unwrap().approved_version.is_none());
    assert!(!workflow::sample_complete(&task));
    assert_eq!(
        task.results
            .iter()
            .filter(|r| r.revision == task.revision)
            .count(),
        1
    );
    let mut brief = task.brief.clone();
    brief.requirement = "更换整体版式".into();
    workflow::save_brief(&mut task, brief).unwrap();
    assert!(!task.workflow.as_ref().unwrap().rules_current);
    assert!(workflow::validate_action(&task, &workflow_action("workflow_trial")).is_err());
    workflow::validate_action(&task, &workflow_action("workflow_build")).unwrap();
}

#[test]
fn workflow_latest_failed_edit_cannot_be_hidden_by_saving_or_older_success() {
    let mut task = workflow_fixture();
    task.results.extend([
        result("a", task.revision, Some("a.png")),
        result("b", task.revision, Some("b.png")),
        result("a", task.revision, None),
    ]);
    let mut brief = task.brief.clone();
    brief.name = "重命名任务".into();
    workflow::save_brief(&mut task, brief).unwrap();
    assert!(!workflow::sample_complete(&task));
    assert!(workflow::validate_action(&task, &workflow_action("workflow_approve")).is_err());
    assert!(task
        .results
        .iter()
        .rev()
        .find(|r| r.revision == task.revision && r.product_id == "a")
        .unwrap()
        .path
        .is_none());
}

#[test]
fn workflow_remote_attempt_survives_append_and_blocks_rule_or_source_replacement() {
    let mut task = workflow_fixture();
    let mut remote = result("a", task.revision, None);
    remote.remote_id = Some("remote-id".into());
    remote.error = Some("已停止查询".into());
    task.results.push(remote);
    let old = result("b", task.revision - 1, Some("old.png"));
    let old_id = old.id.clone();
    task.results.insert(0, old);
    let mut brief = task.brief.clone();
    brief.name = "继续这个流程".into();
    workflow::save_brief(&mut task, brief).unwrap();
    assert_eq!(
        task.results.last().unwrap().remote_id.as_deref(),
        Some("remote-id")
    );
    let mut refine = workflow_action("workflow_refine");
    refine.note = "更换背景".into();
    assert!(workflow::validate_action(&task, &refine)
        .unwrap_err()
        .contains("恢复查询"));
    let mut changed = task.brief.clone();
    changed.products.remove(0);
    assert!(workflow::save_brief(&mut task, changed)
        .unwrap_err()
        .contains("恢复查询"));
    let mut resume = workflow_action("resume");
    resume.result_id = task.results.last().unwrap().id.clone();
    workflow::validate_action(&task, &resume).unwrap();
    resume.result_id = old_id;
    assert!(workflow::validate_action(&task, &resume).is_err());
}

#[test]
fn workflow_rejects_missing_sources_invalid_trial_selection_and_legacy_shortcuts() {
    let mut task = workflow_fixture();
    task.brief.workflow_input.as_mut().unwrap().sources.clear();
    assert!(workflow::validate_action(&task, &workflow_action("workflow_build")).is_err());
    for kind in ["bulk", "generate", "approve", "plan", "template"] {
        assert!(workflow::validate_action(&task, &workflow_action(kind)).is_err());
    }
    let mut trial = workflow_action("workflow_trial");
    for ids in [vec!["a", "a"], vec!["a", "b", "c"], vec!["missing"]] {
        trial.sample_ids = ids.into_iter().map(String::from).collect();
        assert!(workflow::validate_action(&task, &trial).is_err());
    }
    trial.sample_ids = vec!["b".into()];
    workflow::validate_action(&task, &trial).unwrap();
    task.brief.feature = "gen".into();
    assert!(workflow::validate_action(&task, &trial).is_err());
}

#[test]
fn workflow_rule_edits_cannot_rebind_assets_or_silently_change_page_identity_and_output() {
    let task = workflow_fixture();
    let template = &task.templates[0];
    for path in ["refs", "id", "brief"] {
        let mut data = template.data.clone();
        data["slots"][0][path] = if path == "refs" {
            json!(["../untrusted.png"])
        } else {
            json!("")
        };
        assert!(workflow::validate_rules(&data, Some(template)).is_err());
    }
    let mut data = template.data.clone();
    data["output"]["resolution"] = json!("4k");
    assert!(workflow::validate_rules(&data, Some(template)).is_err());
    let mut data = template.data.clone();
    data["style"] = json!("new shared visual style");
    workflow::validate_rules(&data, Some(template)).unwrap();
}
#[test]
fn sample_gate_is_per_category_and_requires_both_complete_products() {
    let mut t = fixture();
    assert_eq!(sample_ids(&t, "背包"), vec!["a", "b"]);
    t.results.push(result("a", 3, Some("a.png")));
    assert!(!samples_complete(&t, "背包"));
    t.results.push(result("b", 3, Some("b.png")));
    assert!(samples_complete(&t, "背包"));
    assert!(!samples_complete(&t, "童装"));
    assert!(!samples_complete(&t, "missing"));
}
#[test]
fn failed_latest_revision_or_version_cannot_reuse_old_approval_evidence() {
    let mut t = fixture();
    t.results
        .extend([result("a", 3, Some("a.png")), result("b", 2, Some("b.png"))]);
    assert!(!samples_complete(&t, "背包"));
    t.results.push(result("b", 3, Some("b-new.png")));
    assert!(samples_complete(&t, "背包"));
    t.results.push(result("a", 3, None));
    assert!(!samples_complete(&t, "背包"));
}
#[test]
fn singleton_category_requires_all_its_planned_pages() {
    let mut t = fixture();
    t.brief.products.truncate(1);
    t.plans.truncate(1);
    t.results.push(result("a", 3, Some("a.png")));
    assert!(samples_complete(&t, "背包"));
    let mut extra = t.plans[0].clone();
    extra.slot_id = "h2".into();
    t.plans.push(extra);
    assert!(!samples_complete(&t, "背包"));
}
#[test]
fn original_bundled_templates_validate_with_their_actual_page_counts() {
    let default: Value =
        serde_json::from_str(include_str!("../../resources/image-studio/default.json")).unwrap();
    let kids: Value =
        serde_json::from_str(include_str!("../../resources/image-studio/kids.json")).unwrap();
    storage::validate_template(&default).unwrap();
    storage::validate_template(&kids).unwrap();
    assert_eq!(default["slots"].as_array().unwrap().len(), 9);
    assert_eq!(kids["slots"].as_array().unwrap().len(), 7);
}
#[test]
fn invalid_templates_and_traversal_ids_fail_before_io() {
    assert!(storage::validate_template(
        &json!({"name":"x","mode":"replace","slots":[{"id":"h1"}]})
    )
    .is_err());
    assert!(storage::validate_template(
        &json!({"name":"x","mode":"smart","slots":[{"id":"h1"},{"id":"h1"}]})
    )
    .is_err());
    for id in ["../outside", "x/y", "", "C:\\test"] {
        assert!(storage::safe_id(id).is_err());
    }
    assert!(storage::resolve("../outside.png").is_err());
}
#[test]
fn missing_back_ref_is_a_user_fix_not_an_invented_structure() {
    let t = fixture();
    let template = Template {
        id: "x".into(),
        data: json!({"mode":"smart"}),
        directory: "templates/x".into(),
        builtin: false,
    };
    let error = agent::template_refs(
        &template,
        &t.brief.products[0],
        &json!({"id":"h2","refs":["@product.back"]}),
    )
    .unwrap_err();
    assert!(error.contains("真实背面"));
}
#[test]
fn provider_capabilities_are_enforced_instead_of_silently_downgrading() {
    let mut b = fixture().brief;
    let mut c = StudioConfig {
        provider_id: "p".into(),
        model: "image".into(),
        protocol: "openai".into(),
        ..Default::default()
    };
    assert!(engine::validate(&c, &b).is_ok());
    b.resolution = "4k".into();
    assert!(engine::validate(&c, &b).is_err());
    c.protocol = "grok".into();
    assert!(engine::validate(&c, &b).is_err());
    c.protocol = "gemini".into();
    assert!(engine::validate(&c, &b).is_ok());
}
#[test]
fn malformed_agent_output_does_not_become_a_generation_prompt() {
    assert!(agent::parse_json("I generated your images").is_err());
    assert_eq!(
        agent::parse_json("```json\n{\"plans\":[]}\n```").unwrap(),
        json!({"plans":[]})
    );
}

#[test]
fn grok_multi_image_edits_identify_the_target_and_product_refs() {
    assert_eq!(
        engine::grok_prompt("Edit the FIRST image using the second image", 2),
        "Edit <IMAGE_0> using <IMAGE_1>"
    );
    assert!(
        engine::grok_prompt("Preserve the product", 3).contains("<IMAGE_0> <IMAGE_1> <IMAGE_2>")
    );
    assert_eq!(
        engine::grok_prompt("Single reference", 1),
        "Single reference"
    );
}

struct GenerationBackend {
    snapshots: Mutex<Vec<Task>>,
    submitted: Mutex<Vec<String>>,
    polled: Mutex<Vec<String>>,
    active: std::sync::atomic::AtomicUsize,
    peak: std::sync::atomic::AtomicUsize,
    remote: bool,
    failures: Vec<String>,
}

impl GenerationBackend {
    fn new(remote: bool) -> Self {
        Self {
            snapshots: Mutex::new(vec![]),
            submitted: Mutex::new(vec![]),
            polled: Mutex::new(vec![]),
            active: 0.into(),
            peak: 0.into(),
            remote,
            failures: vec![],
        }
    }

    fn persist(&self, task: &mut Task) -> Result<(), String> {
        self.snapshots.lock().unwrap().push(task.clone());
        Ok(())
    }
}

impl generation::Backend for GenerationBackend {
    fn submit<'a>(
        &'a self,
        plan: &'a ImagePlan,
    ) -> futures::future::BoxFuture<'a, Result<engine::Submission, String>> {
        Box::pin(async move {
            assert!(
                self.snapshots
                    .lock()
                    .unwrap()
                    .last()
                    .unwrap()
                    .results
                    .iter()
                    .any(|r| {
                        r.product_id == plan.product_id
                            && r.slot_id == plan.slot_id
                            && r.path.is_none()
                    }),
                "an attempt must be saved before submission"
            );
            self.submitted.lock().unwrap().push(plan.slot_id.clone());
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(active, Ordering::SeqCst);
            if self.remote {
                return Ok(engine::Submission::Pending(plan.slot_id.clone()));
            }
            // A slow first page must not block starting or saving later pages.
            let delay = if plan.slot_id == "h0" { 30 } else { 1 };
            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            if self.failures.contains(&plan.slot_id) {
                return Err("模拟图片接口失败".into());
            }
            Ok(engine::Submission::Image(plan.slot_id.as_bytes().to_vec()))
        })
    }

    fn poll<'a>(
        &'a self,
        cfg: &'a StudioConfig,
        remote: &'a str,
    ) -> futures::future::BoxFuture<'a, Result<Option<Vec<u8>>, String>> {
        Box::pin(async move {
            assert!(
                self.snapshots
                    .lock()
                    .unwrap()
                    .last()
                    .unwrap()
                    .results
                    .iter()
                    .any(|r| { r.remote_id.as_deref() == Some(remote) && r.config == *cfg }),
                "remote IDs and provider snapshots must be saved before polling"
            );
            self.polled.lock().unwrap().push(remote.into());
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(Some(remote.as_bytes().to_vec()))
        })
    }

    fn store(&self, result: &mut ImageResult, bytes: &[u8]) -> Result<(), String> {
        assert_eq!(
            result.slot_id.as_bytes(),
            bytes,
            "out-of-order results must keep their identity"
        );
        result.path = Some(format!("results/{}.png", result.id));
        result.width = 1024;
        result.height = 1024;
        result.error = None;
        Ok(())
    }
}

fn generation_plans(task: &Task, count: usize) -> Vec<ImagePlan> {
    (0..count)
        .map(|i| ImagePlan {
            slot_id: format!("h{i}"),
            ..task.plans[0].clone()
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn image_generation_overlaps_nine_requests_and_saves_out_of_order_results_in_plan_order() {
    let mut task = fixture();
    task.results.push(result("a", 2, Some("previous.png")));
    let plans = generation_plans(&task, 14);
    let backend = GenerationBackend::new(false);
    let cfg = StudioConfig {
        provider_id: "chosen-provider".into(),
        ..Default::default()
    };
    generation::run(
        &mut task,
        &cfg,
        plans,
        &AtomicBool::new(false),
        &backend,
        |t| backend.persist(t),
    )
    .await
    .unwrap();
    assert_eq!(backend.peak.load(Ordering::SeqCst), 9);
    assert_eq!(backend.active.load(Ordering::SeqCst), 0);
    assert_eq!(backend.submitted.lock().unwrap().len(), 14);
    assert_eq!(task.results[0].path.as_deref(), Some("previous.png"));
    for (i, r) in task.results.iter().skip(1).enumerate() {
        assert_eq!(r.slot_id, format!("h{i}"));
        assert_eq!(r.revision, task.revision);
        assert!(r.config == cfg && r.path.is_some() && r.error.is_none());
    }
    assert!(backend.snapshots.lock().unwrap().iter().any(|t| {
        t.results[1].path.is_none() && t.results.iter().skip(2).any(|r| r.path.is_some())
    }));
    assert!(task.progress.contains("14/14"));
}

#[tokio::test(start_paused = true)]
async fn failed_pages_do_not_abort_other_images_or_automatically_resubmit() {
    let mut task = fixture();
    let plans = generation_plans(&task, 14);
    let mut backend = GenerationBackend::new(false);
    backend.failures = vec!["h1".into(), "h11".into()];
    let error = generation::run(
        &mut task,
        &StudioConfig::default(),
        plans,
        &AtomicBool::new(false),
        &backend,
        |t| backend.persist(t),
    )
    .await
    .unwrap_err();
    assert!(error.contains("2/14"));
    assert_eq!(backend.submitted.lock().unwrap().len(), 14);
    assert_eq!(task.results.iter().filter(|r| r.path.is_some()).count(), 12);
    assert_eq!(task.results.iter().filter(|r| r.error.is_some()).count(), 2);
}

#[tokio::test(start_paused = true)]
async fn stopping_drains_submitted_images_and_leaves_queued_pages_unsubmitted() {
    let mut task = fixture();
    let plans = generation_plans(&task, 14);
    let backend = GenerationBackend::new(false);
    let flag = AtomicBool::new(false);
    generation::run(
        &mut task,
        &StudioConfig::default(),
        plans,
        &flag,
        &backend,
        |t| {
            backend.persist(t)?;
            if t.results.iter().any(|r| r.path.is_some()) {
                flag.store(true, Ordering::Relaxed);
            }
            Ok(())
        },
    )
    .await
    .unwrap();
    assert_eq!(backend.submitted.lock().unwrap().len(), 9);
    assert_eq!(backend.active.load(Ordering::SeqCst), 0);
    assert_eq!(task.results.len(), 9);
    assert!(task.results.iter().all(|r| r.path.is_some()));
}

#[tokio::test(start_paused = true)]
async fn remote_images_share_the_concurrency_limit_and_persist_ids_before_polling() {
    let mut task = fixture();
    let plans = generation_plans(&task, 14);
    let backend = GenerationBackend::new(true);
    generation::run(
        &mut task,
        &StudioConfig::default(),
        plans,
        &AtomicBool::new(false),
        &backend,
        |t| backend.persist(t),
    )
    .await
    .unwrap();
    assert_eq!(backend.peak.load(Ordering::SeqCst), 9);
    assert_eq!(backend.submitted.lock().unwrap().len(), 14);
    assert_eq!(backend.polled.lock().unwrap().len(), 14);
    assert!(task
        .results
        .iter()
        .all(|r| r.path.is_some() && r.remote_id.is_some()));
}

#[tokio::test(start_paused = true)]
async fn stopping_preserves_all_submitted_remote_ids_without_polling_or_resubmitting() {
    let mut task = fixture();
    let plans = generation_plans(&task, 14);
    let backend = GenerationBackend::new(true);
    let flag = AtomicBool::new(false);
    generation::run(
        &mut task,
        &StudioConfig::default(),
        plans,
        &flag,
        &backend,
        |t| {
            backend.persist(t)?;
            if t.results.iter().any(|r| r.remote_id.is_some()) {
                flag.store(true, Ordering::Relaxed);
            }
            Ok(())
        },
    )
    .await
    .unwrap();
    assert_eq!(backend.submitted.lock().unwrap().len(), 9);
    assert!(backend.polled.lock().unwrap().is_empty());
    assert_eq!(task.results.len(), 9);
    assert!(task
        .results
        .iter()
        .all(|r| r.remote_id.is_some() && r.path.is_none()));
    assert!(task
        .results
        .iter()
        .all(|r| r.error.as_deref().unwrap().contains("可稍后恢复")));
}

#[tokio::test(start_paused = true)]
async fn persistence_failure_stops_new_submissions_but_drains_in_flight_results() {
    let mut task = fixture();
    let plans = generation_plans(&task, 14);
    let backend = GenerationBackend::new(false);
    let error = generation::run(
        &mut task,
        &StudioConfig::default(),
        plans,
        &AtomicBool::new(false),
        &backend,
        |t| {
            if t.results.iter().any(|r| r.path.is_some()) {
                return Err("模拟磁盘写入失败".into());
            }
            backend.persist(t)
        },
    )
    .await
    .unwrap_err();
    assert!(error.contains("磁盘"));
    assert_eq!(backend.submitted.lock().unwrap().len(), 9);
    assert_eq!(backend.active.load(Ordering::SeqCst), 0);
    assert!(task.results.iter().all(|r| r.path.is_some()));
}

#[tokio::test]
async fn stopped_batch_does_not_create_attempts_or_submit_requests() {
    let mut task = fixture();
    let plans = generation_plans(&task, 14);
    let backend = GenerationBackend::new(false);
    generation::run(
        &mut task,
        &StudioConfig::default(),
        plans,
        &AtomicBool::new(true),
        &backend,
        |t| backend.persist(t),
    )
    .await
    .unwrap();
    assert!(task.results.is_empty());
    assert!(backend.submitted.lock().unwrap().is_empty());
}
