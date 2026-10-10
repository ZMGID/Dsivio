//! Main-thread Wry regression: real task saves and sends, isolated app data.
use kivio::chat::{
    protocol::{self, ChatRunSync, ChatSyncRequest},
    repository::ConversationRepository,
    storage, ChatProject, Conversation, ModelRef,
};
use kivio::scheduled_tasks::{
    self,
    types::{RunStatus, RunTrigger, ScheduleRule, ScheduledTaskInput, TaskSource, TaskTarget},
    ScheduledTasks,
};
use kivio::settings::{ModelProvider, Settings};
use kivio::state::AppState;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::Manager;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

struct IsolatedData(PathBuf);
impl Drop for IsolatedData {
    fn drop(&mut self) {
        if self.0.exists() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tauri::async_runtime::set(runtime.handle().clone());
    let _entered = runtime.enter();
    let scratch = tempfile::tempdir().unwrap();
    let tasks_dir = scratch.path().join("tasks");
    let workspace = scratch.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let mut settings = Settings::default();
    let (model_url, _hold_model_streams) = runtime.block_on(local_model());
    settings.retry_enabled = false;
    settings.chat_memory.enabled = false;
    settings.providers = vec![ModelProvider {
        id: "fixture".into(),
        name: "Local fixture".into(),
        api_keys: vec!["local-only".into()],
        api_key_legacy: None,
        base_url: model_url,
        available_models: vec![],
        enabled_models: vec!["model-a".into(), "model-b".into()],
        api_format: "openai".into(),
        enabled: true,
        model_overrides: Default::default(),
        compress_request_body: false,
        request: Default::default(),
        active_key_index: 0,
    }];
    settings.providers[0].request.use_system_proxy = false;
    settings.chat_tools.native_tools.working_directory = workspace.to_string_lossy().into_owned();
    let identifier = format!("com.dsivio.schedule-lifecycle-{}", uuid::Uuid::new_v4());
    let mut context = kivio::application_context();
    context.config_mut().identifier = identifier.clone();
    context.config_mut().app.windows.clear();
    let app = tauri::Builder::default()
        .manage(AppState::new_headless(
            settings,
            scratch.path().join("usage"),
        ))
        .manage(ConversationRepository::default())
        .manage(ScheduledTasks::load(tasks_dir.clone()))
        .build(context)
        .unwrap();
    let data_dir = app.path().app_data_dir().unwrap();
    assert_eq!(
        data_dir.file_name().and_then(|name| name.to_str()),
        Some(identifier.as_str())
    );
    assert!(!data_dir.exists());
    let _data = IsolatedData(data_dir);
    runtime.block_on(regressions(&app, tasks_dir, &scratch));
    drop(app);
}

async fn regressions(
    app: &tauri::App<tauri::Wry>,
    tasks_dir: PathBuf,
    scratch: &tempfile::TempDir,
) {
    let handle = app.handle();
    let existing: Conversation = serde_json::from_value(serde_json::json!({
        "id": "conv_existing", "title": "Keep this conversation", "provider_id": "", "model": "",
        "messages": [], "created_at": 1, "updated_at": 1,
    }))
    .unwrap();
    app.state::<ConversationRepository>()
        .create(handle, existing)
        .await
        .unwrap();
    let before = index_ids(handle);
    let existing_path = storage::conversations_dir(handle)
        .unwrap()
        .join("conv_existing.json");
    let original_bytes = std::fs::read(&existing_path).unwrap();

    block_store(&tasks_dir);
    for _ in 0..2 {
        assert!(
            scheduled_tasks::save_task(handle, input(new_target(None)), TaskSource::User)
                .await
                .is_err()
        );
        assert_eq!(index_ids(handle), before);
        assert_eq!(std::fs::read(&existing_path).unwrap(), original_bytes);
        assert!(scheduled_tasks::service(handle).list().is_empty());
    }
    unblock_store(&tasks_dir);
    let saved = scheduled_tasks::save_task(handle, input(new_target(None)), TaskSource::User)
        .await
        .unwrap();
    let mut expected = vec!["conv_existing".to_string(), saved.conversation_id.clone()];
    expected.sort();
    assert_eq!(index_ids(handle), expected);
    assert!(storage::load_conversation(handle, &saved.conversation_id).is_ok());
    assert_eq!(
        ScheduledTasks::load(tasks_dir.clone())
            .get(&saved.id)
            .unwrap()
            .conversation_id,
        saved.conversation_id
    );
    println!("new conversation failure/retry: passed");

    block_store(&tasks_dir);
    assert!(scheduled_tasks::save_task(
        handle,
        input(TaskTarget::Conversation {
            conversation_id: "conv_existing".into()
        }),
        TaskSource::User
    )
    .await
    .is_err());
    assert_eq!(std::fs::read(&existing_path).unwrap(), original_bytes);
    assert_eq!(index_ids(handle), expected);
    unblock_store(&tasks_dir);
    println!("existing conversation conservation: passed");

    let project_root = scratch.path().join("project");
    std::fs::create_dir(&project_root).unwrap();
    let marker = project_root.join("keep.txt");
    std::fs::write(&marker, "project-owned work").unwrap();
    let project = storage::create_project(
        handle,
        ChatProject {
            id: "proj_rollback".into(),
            name: "Rollback boundary".into(),
            description: None,
            color: None,
            root_path: Some(project_root.to_string_lossy().into_owned()),
            created_at: 1,
            updated_at: 1,
        },
    )
    .unwrap();
    block_store(&tasks_dir);
    assert!(scheduled_tasks::save_task(
        handle,
        input(new_target(Some(project.id.clone()))),
        TaskSource::User
    )
    .await
    .is_err());
    assert_eq!(index_ids(handle), expected);
    assert_eq!(std::fs::read(&marker).unwrap(), b"project-owned work");
    assert_eq!(
        storage::find_project_by_id(handle, &project.id)
            .unwrap()
            .root_path,
        project.root_path
    );
    unblock_store(&tasks_dir);
    println!("project ownership boundary: passed");

    block_store(&tasks_dir);
    let (left, right) = tokio::join!(
        scheduled_tasks::save_task(handle, input(new_target(None)), TaskSource::User),
        scheduled_tasks::save_task(handle, input(new_target(None)), TaskSource::User),
    );
    assert!(left.is_err() && right.is_err());
    assert_eq!(index_ids(handle), expected);
    assert_eq!(std::fs::read(&existing_path).unwrap(), original_bytes);
    assert_eq!(
        scheduled_tasks::service(handle)
            .list()
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>(),
        vec![saved.id.as_str()]
    );
    unblock_store(&tasks_dir);
    println!("concurrent failed saves: passed");

    for arm_count in [1, 2] {
        let mut request = input(TaskTarget::NewConversation {
            provider_id: Some("fixture".into()),
            model: Some("model-a".into()),
            project_id: None,
            thinking_level: None,
        });
        request.schedule = ScheduleRule::Daily { hour: 9, minute: 0 };
        let task = scheduled_tasks::save_task(handle, request, TaskSource::User)
            .await
            .unwrap();
        if arm_count == 2 {
            app.state::<ConversationRepository>()
                .mutate(handle, &task.conversation_id, |conversation| {
                    conversation.reply_models = vec![
                        ModelRef {
                            provider_id: "fixture".into(),
                            model: "model-a".into(),
                        },
                        ModelRef {
                            provider_id: "fixture".into(),
                            model: "model-b".into(),
                        },
                    ];
                    Ok(())
                })
                .await
                .unwrap();
        }
        let run = scheduled_tasks::start_run(handle, task.clone(), RunTrigger::Manual, None);
        // Observe real decoded output before stopping, not just an HTTP request.
        tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let sync = protocol::chat_sync_state(handle.clone(), app.state(), ChatSyncRequest {
                        protocol_version: protocol::CHAT_PROTOCOL_VERSION,
                        conversation_id: task.conversation_id.clone(), cursors: vec![],
                    }).unwrap();
                    if sync.runs.len() == arm_count && sync.runs.iter().all(|run| {
                        matches!(run, ChatRunSync::Snapshot { snapshot } if snapshot.content == "partial response")
                    }) { break; }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await.unwrap();
        app.state::<AppState>()
            .cancel_chat_generation(&task.conversation_id);
        let terminal = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let latest = scheduled_tasks::service(handle)
                    .runs(&task.id)
                    .into_iter()
                    .find(|entry| entry.id == run.id)
                    .unwrap();
                if latest.status.is_terminal() {
                    break latest;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(terminal.status, RunStatus::Interrupted);
        let reloaded = ScheduledTasks::load(tasks_dir.clone());
        assert_eq!(
            reloaded
                .runs(&task.id)
                .iter()
                .find(|entry| entry.id == run.id)
                .unwrap()
                .status,
            RunStatus::Interrupted
        );
        let saved_task = reloaded.get(&task.id).unwrap();
        assert!(saved_task.enabled);
        assert_eq!(saved_task.last_error, None);
        let conversation = storage::load_conversation(handle, &task.conversation_id).unwrap();
        let mut kept = 0;
        for message in conversation
            .messages
            .iter()
            .filter(|message| message.role == "assistant")
        {
            assert_eq!(message.stream_outcome.as_deref(), Some("cancelled"));
            assert!(message.content.contains("partial response"));
            kept += 1;
        }
        assert_eq!(
            kept, arm_count,
            "cancelled model columns must retain their partial output"
        );
        println!("cancelled real send ({arm_count} model arms): passed");
    }
}

fn input(target: TaskTarget) -> ScheduledTaskInput {
    ScheduledTaskInput {
        id: None,
        name: "Rollback regression".into(),
        prompt: "Do this later".into(),
        schedule: ScheduleRule::Once {
            at: scheduled_tasks::now_secs() + 3600,
        },
        target,
        enabled: Some(true),
    }
}

fn new_target(project_id: Option<String>) -> TaskTarget {
    TaskTarget::NewConversation {
        provider_id: None,
        model: None,
        project_id,
        thinking_level: None,
    }
}

fn index_ids(app: &tauri::AppHandle) -> Vec<String> {
    let mut ids = storage::load_index(app)
        .unwrap()
        .conversations
        .into_iter()
        .map(|item| item.id)
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn block_store(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let file = dir.join("tasks.json");
    if file.is_file() {
        std::fs::rename(&file, dir.join("committed.json")).unwrap();
    }
    std::fs::create_dir(file).unwrap();
}

fn unblock_store(dir: &Path) {
    let file = dir.join("tasks.json");
    std::fs::remove_dir(&file).unwrap();
    let backup = dir.join("committed.json");
    if backup.is_file() {
        std::fs::rename(backup, file).unwrap();
    }
}

async fn local_model() -> (String, tokio::sync::watch::Sender<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let (hold, mut closed) = tokio::sync::watch::channel(());
    tokio::spawn(async move {
        loop {
            let stream = tokio::select! {
                accepted = listener.accept() => accepted.unwrap().0,
                _ = closed.changed() => break,
            };
            let mut closed = closed.clone();
            tokio::spawn(async move {
                let mut reader = tokio::io::BufReader::new(stream);
                let mut line = String::new();
                let mut content_length = 0;
                loop {
                    line.clear();
                    let read = reader.read_line(&mut line).await.unwrap();
                    if read == 0 {
                        return;
                    }
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':') {
                        if name.eq_ignore_ascii_case("content-length") {
                            content_length = value.trim().parse::<u64>().unwrap();
                        }
                    }
                }
                tokio::io::copy(
                    &mut (&mut reader).take(content_length),
                    &mut tokio::io::sink(),
                )
                .await
                .unwrap();
                reader.get_mut().write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"id\":\"fixture\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial response\"},\"finish_reason\":null}]}\n\n").await.unwrap();
                // Never send [DONE]; only the public stop command can end generation.
                let _ = closed.changed().await;
            });
        }
    });
    (base_url, hold)
}
