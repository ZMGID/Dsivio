//! Main-thread Wry regression for persisted IM directories and project precedence.
use kivio::chat::{storage, ChatProject, ChatProjectIndex, Conversation};
use std::path::{Path, PathBuf};
use tauri::Manager;

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
    let app = tauri::Builder::default().build(context).unwrap();
    let data_dir = app.path().app_data_dir().unwrap();
    assert_eq!(
        data_dir.file_name().and_then(|name| name.to_str()),
        Some(identifier.as_str())
    );
    assert!(!data_dir.exists());
    let _data = IsolatedData(data_dir);
    regressions(app.handle(), scratch.path());
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
