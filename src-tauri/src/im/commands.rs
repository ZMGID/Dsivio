use super::{credentials, now, types::*, ImRuntime};
use tauri::{AppHandle, Emitter, Manager};

#[tauri::command]
pub async fn im_get_status(app: AppHandle) -> Result<Vec<ImStatus>, String> {
    let runtime = app.try_state::<ImRuntime>().ok_or("IM 运行时不可用")?;
    let statuses = runtime.state.lock().status.values().cloned().collect();
    Ok(statuses)
}
#[tauri::command]
pub async fn im_save_credentials(
    app: AppHandle,
    platform: ImPlatform,
    credentials: CredentialInput,
) -> Result<(), String> {
    let identity = app
        .state::<crate::state::AppState>()
        .settings_read()
        .im
        .identity(platform)
        .to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        super::credentials::save(platform, &identity, credentials)
    })
    .await
    .map_err(|_| "保存 IM 凭证失败")??;
    app.state::<ImRuntime>().reconnect(platform);
    Ok(())
}
#[tauri::command]
pub async fn im_clear_credentials(app: AppHandle, platform: ImPlatform) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || credentials::clear(platform))
        .await
        .map_err(|_| "删除 IM 凭证失败")??;
    app.state::<ImRuntime>().reconnect(platform);
    Ok(())
}
#[tauri::command]
pub fn im_reconnect(app: AppHandle, platform: ImPlatform) -> Result<(), String> {
    app.state::<ImRuntime>().reconnect(platform);
    Ok(())
}
#[tauri::command]
pub fn im_list_pairing_requests(app: AppHandle) -> Result<Vec<ImPairingRequest>, String> {
    let config = app
        .state::<crate::state::AppState>()
        .settings_read()
        .im
        .clone();
    let runtime = app.state::<ImRuntime>();
    let state = runtime.state.lock();
    Ok(state
        .store
        .pending
        .iter()
        .filter(|p| {
            p.request.expires_at > now() && p.identity == config.identity(p.request.platform)
        })
        .map(|p| p.request.clone())
        .collect())
}
#[tauri::command]
pub fn im_approve_pairing(
    app: AppHandle,
    platform: ImPlatform,
    code: String,
) -> Result<(), String> {
    let config = app
        .state::<crate::state::AppState>()
        .settings_read()
        .im
        .clone();
    let identity = config.identity(platform);
    app.state::<ImRuntime>().transact(|store| {
        let index = store
            .pending
            .iter()
            .position(|p| {
                p.request.platform == platform
                    && p.request.code.eq_ignore_ascii_case(code.trim())
                    && p.identity == identity
                    && p.request.expires_at > now()
            })
            .ok_or("配对码不存在、已过期或机器人 ID 已改变")?;
        let request = store.pending.remove(index).request;
        store.approved.retain(|a| {
            !(a.platform == platform && a.identity == identity && a.user_id == request.user_id)
        });
        store.approved.push(ImApprovedUser {
            platform,
            identity: identity.to_owned(),
            user_id: request.user_id,
            user_name: request.user_name,
            approved_at: now(),
        });
        Ok(())
    })?;
    let _ = app.emit("im-pairing-changed", ());
    Ok(())
}
#[tauri::command]
pub fn im_deny_pairing(app: AppHandle, platform: ImPlatform, code: String) -> Result<(), String> {
    app.state::<ImRuntime>().transact(|s| {
        s.pending.retain(|p| {
            !(p.request.platform == platform && p.request.code.eq_ignore_ascii_case(code.trim()))
        });
        Ok(())
    })?;
    let _ = app.emit("im-pairing-changed", ());
    Ok(())
}
#[tauri::command]
pub fn im_list_approved_users(app: AppHandle) -> Result<Vec<ImApprovedUser>, String> {
    let config = app
        .state::<crate::state::AppState>()
        .settings_read()
        .im
        .clone();
    let runtime = app.state::<ImRuntime>();
    let state = runtime.state.lock();
    Ok(state
        .store
        .approved
        .iter()
        .filter(|u| u.identity == config.identity(u.platform))
        .cloned()
        .collect())
}
#[tauri::command]
pub fn im_revoke_user(app: AppHandle, platform: ImPlatform, user_id: String) -> Result<(), String> {
    let identity = app
        .state::<crate::state::AppState>()
        .settings_read()
        .im
        .identity(platform)
        .to_owned();
    let runtime = app.state::<ImRuntime>();
    runtime.transact(|s| {
        s.approved.retain(|u| {
            !(u.platform == platform && u.identity == identity && u.user_id == user_id)
        });
        Ok(())
    })?;
    let conversations: Vec<_> = runtime
        .state
        .lock()
        .store
        .sessions
        .iter()
        .filter_map(|(route, id)| {
            let parts: Vec<String> = serde_json::from_str(route).ok()?;
            (parts.first().is_some_and(|p| p == platform.key())
                && parts.get(1) == Some(&identity)
                && (parts.get(4) == Some(&user_id) || parts.get(4).is_some_and(|p| p == "*")))
            .then(|| id.clone())
        })
        .collect();
    for id in conversations {
        app.state::<crate::state::AppState>()
            .cancel_chat_generation(&id);
    }
    let _ = app.emit("im-pairing-changed", ());
    Ok(())
}
#[tauri::command]
pub async fn im_begin_setup(
    app: AppHandle,
    platform: ImPlatform,
    domain: Option<FeishuDomain>,
) -> Result<ImSetupSession, String> {
    app.state::<ImRuntime>()
        .setup
        .begin(platform, domain.unwrap_or_default())
        .await
}
#[tauri::command]
pub async fn im_poll_setup(app: AppHandle, id: String) -> Result<ImSetupSession, String> {
    let result = app.state::<ImRuntime>().setup.poll(&id).await?;
    if result.status == ImSetupState::Completed {
        app.state::<ImRuntime>().reconnect(result.platform);
    }
    Ok(result)
}
#[tauri::command]
pub fn im_cancel_setup(app: AppHandle, id: String) -> Result<(), String> {
    app.state::<ImRuntime>().setup.cancel(&id)
}
