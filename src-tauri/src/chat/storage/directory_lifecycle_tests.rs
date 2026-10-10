//! Main-thread Wry regression for persisted IM directories and project precedence.
use futures_util::{SinkExt, StreamExt};
use kivio::chat::{
    repository::ConversationRepository, storage, ChatProject, ChatProjectIndex, Conversation,
};
use kivio::im::{
    self,
    types::{CredentialInput, DmPolicy, ImPlatform},
    ImRuntime,
};
use kivio::mcp::{
    registry::{self, NativeToolContext},
    types::list_native_builtin_tool_defs,
};
use kivio::settings::Settings;
use kivio::state::AppState;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::Manager;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

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
    let identifier = format!("com.dsivio.im-directory-{}", uuid::Uuid::new_v4());
    let mut context = kivio::application_context();
    context.config_mut().identifier = identifier.clone();
    context.config_mut().app.windows.clear();
    let (socket_url, inbound) = runtime.block_on(local_socket());
    let mut settings = Settings::default();
    settings.providers.clear();
    settings.chat_memory.enabled = false;
    settings.chat_tools.native_tools.working_directory = scratch
        .path()
        .join("ordinary")
        .to_string_lossy()
        .into_owned();
    settings.im.wecom.enabled = true;
    settings.im.wecom.bot_id = "workspace-fixture".into();
    settings.im.wecom.websocket_url = socket_url;
    settings.im.wecom.access.dm_policy = DmPolicy::Open;
    settings.im.agent.provider_id = "absent-local-fixture".into();
    settings.im.agent.model = "offline".into();
    settings.im.agent.streaming = false;
    let app = tauri::Builder::default()
        .manage(AppState::new_headless(
            settings,
            scratch.path().join("usage"),
        ))
        .manage(ConversationRepository::default())
        .build(context)
        .unwrap();
    let data_dir = app.path().app_data_dir().unwrap();
    assert_eq!(
        data_dir.file_name().and_then(|name| name.to_str()),
        Some(identifier.as_str())
    );
    assert!(!data_dir.exists());
    app.manage(ImRuntime::load(data_dir.join("im")).unwrap());
    let _data = IsolatedData(data_dir);
    regressions(app.handle(), scratch.path());
    runtime
        .block_on(tokio::time::timeout(
            Duration::from_secs(20),
            platform_workspace_regression(app.handle(), scratch.path(), inbound),
        ))
        .expect("IM platform workspace regression timed out");
    drop(app);
}

fn regressions(app: &tauri::AppHandle, scratch: &Path) {
    let im_directory = scratch.join("im");
    let project_directory = scratch.join("project");
    let ordinary_directory = scratch.join("ordinary");
    std::fs::create_dir(&im_directory).unwrap();
    std::fs::create_dir(&project_directory).unwrap();
    std::fs::create_dir(&ordinary_directory).unwrap();
    let ordinary_root = ordinary_directory.to_str().unwrap();
    let mut conversation: Conversation = serde_json::from_value(serde_json::json!({
        "id": "conv_im_directory", "title": "IM directory regression",
        "provider_id": "", "model": "", "messages": [],
        "created_at": 1, "updated_at": 1,
    }))
    .unwrap();
    let project: ChatProject = serde_json::from_value(serde_json::json!({
        "id": "proj_im_directory", "name": "Project directory regression",
        "root_path": project_directory, "created_at": 1, "updated_at": 1,
    }))
    .unwrap();
    storage::save_project_index(
        app,
        &ChatProjectIndex {
            projects: vec![project.clone()],
        },
    )
    .unwrap();
    let mapping_path = storage::conversations_dir(app)
        .unwrap()
        .join("im-working-directories.json");
    std::fs::write(
        &mapping_path,
        serde_json::to_vec(&serde_json::json!({
            conversation.id.clone(): im_directory,
        }))
        .unwrap(),
    )
    .unwrap();

    // An ungrouped IM conversation retains its configured directory.
    assert_eq!(
        storage::resolve_conversation_working_directory(app, &conversation, ordinary_root).unwrap(),
        im_directory
    );

    // Moving the same persisted conversation into a project must take effect immediately.
    conversation.project_id = Some(project.id.clone());
    let directory =
        storage::resolve_conversation_working_directory(app, &conversation, ordinary_root).unwrap();
    assert_eq!(directory, project_directory);
    std::fs::write(directory.join("relative-output.txt"), "project output").unwrap();
    assert_eq!(
        std::fs::read_to_string(project_directory.join("relative-output.txt")).unwrap(),
        "project output"
    );
    assert!(!im_directory.join("relative-output.txt").exists());

    // Legacy project bindings obey the same precedence, without deleting the IM fallback.
    conversation.project_id = None;
    conversation.folder = Some(project.name.clone());
    assert_eq!(
        storage::resolve_conversation_working_directory(app, &conversation, ordinary_root).unwrap(),
        project_directory
    );
    conversation.folder = None;
    assert_eq!(
        storage::resolve_conversation_working_directory(app, &conversation, ordinary_root).unwrap(),
        im_directory
    );

    // A broken explicit binding must not silently redirect writes to the IM directory.
    conversation.project_id = Some("proj_missing".into());
    assert!(
        storage::resolve_conversation_working_directory(app, &conversation, ordinary_root).is_err()
    );
    let mut missing_root = project;
    missing_root.root_path = None;
    conversation.project_id = Some(missing_root.id.clone());
    storage::save_project_index(
        app,
        &ChatProjectIndex {
            projects: vec![missing_root],
        },
    )
    .unwrap();
    assert!(
        storage::resolve_conversation_working_directory(app, &conversation, ordinary_root).is_err()
    );

    // A stale mapping on an ungrouped conversation falls back to its ordinary workspace.
    conversation.project_id = None;
    std::fs::remove_dir(&im_directory).unwrap();
    assert_eq!(
        storage::resolve_conversation_working_directory(app, &conversation, ordinary_root).unwrap(),
        kivio::native_tools::conversation_workspace_directory(ordinary_root, &conversation.id)
            .unwrap(),
    );
    println!("IM directory lifecycle: project ID/name precedence, relative output, broken bindings, detach and stale mapping passed");
}

async fn platform_workspace_regression(
    app: &tauri::AppHandle,
    scratch: &Path,
    inbound: mpsc::Sender<Value>,
) {
    im::commands::im_save_credentials(
        app.clone(),
        ImPlatform::Wecom,
        "workspace-fixture".into(),
        CredentialInput {
            secret: "local-fixture-secret".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    im::start(app.clone());
    let root = std::fs::canonicalize(scratch.join("ordinary")).unwrap();
    let expected = root.join("WeCom");
    let mapping_path = storage::conversations_dir(app)
        .unwrap()
        .join("im-working-directories.json");
    inbound.send(callback("first")).await.unwrap();
    let id = wait_for_workspace(&mapping_path, &expected).await;
    let old_directory = scratch.join("old-im-directory");
    std::fs::create_dir(&old_directory).unwrap();
    let mut mappings: HashMap<String, String> =
        serde_json::from_slice(&std::fs::read(&mapping_path).unwrap()).unwrap();
    mappings.insert(id.clone(), old_directory.to_string_lossy().into_owned());
    std::fs::write(&mapping_path, serde_json::to_vec(&mappings).unwrap()).unwrap();

    // The next message must reuse the conversation but replace its stale directory binding.
    inbound.send(callback("second")).await.unwrap();
    assert_eq!(wait_for_workspace(&mapping_path, &expected).await, id);
    let conversation = storage::load_conversation(app, &id).unwrap();
    let directory =
        storage::resolve_conversation_working_directory(app, &conversation, root.to_str().unwrap())
            .unwrap();
    assert_eq!(directory, expected);
    let state = app.state::<AppState>();
    let tools =
        list_native_builtin_tool_defs(&state.settings_read().chat_tools.native_tools, false, false);
    let context = |depth| NativeToolContext {
        conversation_id: id.clone(),
        message_id: "workspace-tool-message".into(),
        tool_call_id: None,
        run_id: "workspace-tool-run".into(),
        generation: 0,
        round: 0,
        depth,
    };
    let write = tools
        .iter()
        .find(|tool| tool.id == "native__write_file")
        .unwrap();
    let written = registry::call_tool(
        app,
        &state,
        write,
        json!({"path":"relative-output.txt","content":"native file tool"}),
        None,
        Some(context(0)),
    )
    .await
    .unwrap();
    assert!(!written.is_error, "{}", written.content);
    let bash = tools
        .iter()
        .find(|tool| tool.id == "native__run_command")
        .unwrap();
    let output = registry::call_tool(
        app,
        &state,
        bash,
        json!({"command":"pwd; printf 'native command' > command-output.txt","timeout_ms":3000}),
        None,
        Some(context(1)),
    )
    .await
    .unwrap();
    assert!(!output.is_error, "{}", output.content);
    assert!(
        output.content.contains(expected.to_str().unwrap()),
        "{}",
        output.content
    );
    assert_eq!(
        std::fs::read_to_string(expected.join("relative-output.txt")).unwrap(),
        "native file tool"
    );
    assert_eq!(
        std::fs::read_to_string(expected.join("command-output.txt")).unwrap(),
        "native command"
    );
    assert!(!root.join("relative-output.txt").exists());
    assert!(!old_directory.join("command-output.txt").exists());
    app.state::<ImRuntime>().shutdown().await;
    println!("Native IM platform workspace: new + reused conversation, stale binding replacement, relative file and command execution passed ({})", expected.display());
}

async fn wait_for_workspace(path: &Path, expected: &Path) -> String {
    loop {
        if let Ok(content) = std::fs::read(path) {
            if let Ok(mappings) = serde_json::from_slice::<HashMap<String, String>>(&content) {
                if let Some((id, _)) = mappings
                    .iter()
                    .find(|(_, directory)| Path::new(directory) == expected)
                {
                    return id.clone();
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn callback(id: &str) -> Value {
    json!({
        "cmd":"aibot_msg_callback", "headers":{"req_id":id},
        "body":{"msgid":id,"chatid":"workspace-chat","chattype":"single",
            "from":{"userid":"workspace-user"},"msgtype":"text","text":{"content":"workspace message"}}
    })
}

async fn local_socket() -> (String, mpsc::Sender<Value>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let (sender, mut incoming) = mpsc::channel::<Value>(4);
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let Some(Ok(Message::Text(subscribe))) = socket.next().await else {
            return;
        };
        let subscribe: Value = serde_json::from_str(&subscribe).unwrap();
        assert_eq!(subscribe["cmd"], "aibot_subscribe");
        let ack = json!({"headers":{"req_id":subscribe["headers"]["req_id"]},"errcode":0});
        socket
            .send(Message::Text(ack.to_string().into()))
            .await
            .unwrap();
        loop {
            tokio::select! {
                frame = socket.next() => {
                    match frame {
                        Some(Ok(Message::Text(text))) => {
                            let payload: Value = serde_json::from_str(&text).unwrap();
                            if let Some(req_id) = payload.pointer("/headers/req_id") {
                                let ack = json!({"headers":{"req_id":req_id},"errcode":0});
                                if socket.send(Message::Text(ack.to_string().into())).await.is_err() { break; }
                            }
                        }
                        Some(Ok(Message::Ping(payload))) => {
                            if socket.send(Message::Pong(payload)).await.is_err() { break; }
                        }
                        Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                        _ => {}
                    }
                }
                message = incoming.recv() => {
                    let Some(message) = message else { break };
                    if socket.send(Message::Text(message.to_string().into())).await.is_err() { break; }
                }
            }
        }
    });
    (url, sender)
}
