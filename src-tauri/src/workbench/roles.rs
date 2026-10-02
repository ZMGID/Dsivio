//! Saved character roles for video pages. JSON under `{app_data}/workbench/roles`,
//! reference images copied into that tree. One writer, revision checked on save.
use crate::content_templates::files::{self, decode};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};
use ts_rs::TS;

const MAX_IMAGES: usize = 12;
const MAX_IMAGE_BYTES: u64 = 50 * 1024 * 1024;
const MAX_NAME_CHARS: usize = 80;
const MAX_DESCRIPTION_CHARS: usize = 4000;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Role {
    pub id: String,
    pub revision: u32,
    pub name: String,
    pub description: String,
    pub images: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoleInlineImage {
    pub name: String,
    pub base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoleSaveRequest {
    pub id: Option<String>,
    pub revision: Option<u32>,
    pub name: String,
    pub description: String,
    pub images: Vec<String>,
    #[serde(default)]
    pub inline_images: Vec<RoleInlineImage>,
}

fn lock() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    static LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
    LOCK.lock().map_err(|err| err.to_string())
}

fn root() -> Result<PathBuf, String> {
    let dir = crate::app_data::app_data_dir()
        .ok_or("无法定位应用数据目录")?
        .join("workbench")
        .join("roles");
    fs::create_dir_all(dir.join("images")).map_err(|err| err.to_string())?;
    Ok(dir)
}

fn parse_id(id: &str) -> Result<String, String> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| "无效的角色 ID".to_string())?;
    Ok(parsed.to_string())
}

fn record_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}

fn image_dir(dir: &Path, id: &str) -> PathBuf {
    dir.join("images").join(id)
}

fn load_role(dir: &Path, id: &str) -> Result<Option<Role>, String> {
    let path = record_path(dir, id);
    if !path.is_file() {
        return Ok(None);
    }
    files::read(&path).map(Some)
}

fn list_locked(dir: &Path) -> Result<Vec<Role>, String> {
    fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    let entries = fs::read_dir(dir).map_err(|err| err.to_string())?;
    let mut roles = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        roles.push(files::read::<Role>(&path)?);
    }
    roles.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
    Ok(roles)
}

fn store_bytes(image_dir: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("单张图片不能超过 50 MB".into());
    }
    decode(bytes)?;
    let format = image::guess_format(bytes).map_err(|err| err.to_string())?;
    let extension = match format {
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Png => "png",
        image::ImageFormat::WebP => "webp",
        _ => return Err("仅支持 PNG、JPEG、WebP".into()),
    };
    fs::create_dir_all(image_dir).map_err(|err| err.to_string())?;
    let path = image_dir.join(format!("{}.{}", files::id(), extension));
    fs::write(&path, bytes).map_err(|err| err.to_string())?;
    Ok(path)
}

fn copy_image(dir: &Path, role_id: &str, source: &Path) -> Result<PathBuf, String> {
    let dest_dir = image_dir(dir, role_id);
    fs::create_dir_all(&dest_dir).map_err(|err| err.to_string())?;
    if source.is_file() {
        if let (Ok(src), Ok(root)) = (source.canonicalize(), dest_dir.canonicalize()) {
            if src.starts_with(&root) {
                return Ok(src);
            }
        }
    }
    let meta = fs::metadata(source).map_err(|_| format!("找不到参考图：{}", source.display()))?;
    if meta.len() > MAX_IMAGE_BYTES {
        return Err("单张图片不能超过 50 MB".into());
    }
    let bytes = fs::read(source).map_err(|err| err.to_string())?;
    store_bytes(&dest_dir, &bytes)
}

fn cleanup_images(image_dir: &Path, keep: &[PathBuf]) -> Result<(), String> {
    if !image_dir.is_dir() {
        return Ok(());
    }
    let keep: HashSet<PathBuf> = keep
        .iter()
        .filter_map(|path| path.canonicalize().ok())
        .collect();
    for entry in fs::read_dir(image_dir).map_err(|err| err.to_string())?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let canonical = path.canonicalize().unwrap_or(path.clone());
        if !keep.contains(&canonical) {
            fs::remove_file(&path).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn save_locked(dir: &Path, request: RoleSaveRequest) -> Result<Role, String> {
    fs::create_dir_all(dir.join("images")).map_err(|err| err.to_string())?;
    let name = request.name.trim();
    if name.is_empty() {
        return Err("角色名字不能为空".into());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err("角色名字不能超过 80 字".into());
    }
    let description = request.description.trim();
    if description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err("角色描述不能超过 4000 字".into());
    }
    if request.images.len() + request.inline_images.len() > MAX_IMAGES {
        return Err("一个角色最多 12 张参考图".into());
    }
    if request.images.iter().any(|path| path.trim().is_empty()) {
        return Err("参考图路径无效".into());
    }

    let existing = if let Some(id) = request.id.as_deref().filter(|id| !id.is_empty()) {
        let id = parse_id(id)?;
        match load_role(dir, &id)? {
            Some(role) => {
                if request.revision != Some(role.revision) {
                    return Err("此角色已在其他窗口更新，请重新打开后操作".into());
                }
                Some(role)
            }
            None => return Err("角色不存在".into()),
        }
    } else {
        None
    };

    let id = existing
        .as_ref()
        .map(|role| role.id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let created_at = existing
        .as_ref()
        .map(|role| role.created_at.clone())
        .unwrap_or_else(files::now);
    let revision = existing.as_ref().map(|role| role.revision).unwrap_or(0).saturating_add(1);
    let dest_dir = image_dir(dir, &id);
    let mut images = Vec::new();
    for source in &request.images {
        images.push(copy_image(dir, &id, Path::new(source.trim()))?);
    }
    for inline in &request.inline_images {
        let bytes = STANDARD
            .decode(inline.base64.trim())
            .map_err(|_| "参考图数据无效".to_string())?;
        images.push(store_bytes(&dest_dir, &bytes)?);
    }
    let role = Role {
        id,
        revision,
        name: name.to_string(),
        description: description.to_string(),
        images: images.iter().map(|path| path.to_string_lossy().into_owned()).collect(),
        created_at,
        updated_at: files::now(),
    };
    files::write(&record_path(dir, &role.id), &role)?;
    cleanup_images(&dest_dir, &images)?;
    Ok(role)
}

fn delete_locked(dir: &Path, id: &str) -> Result<(), String> {
    let id = parse_id(id)?;
    let path = record_path(dir, &id);
    if !path.is_file() {
        return Err("角色不存在".into());
    }
    fs::remove_file(&path).map_err(|err| err.to_string())?;
    let images = image_dir(dir, &id);
    if images.is_dir() {
        fs::remove_dir_all(&images).map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn roles_list() -> Result<Vec<Role>, String> {
    let _guard = lock()?;
    list_locked(&root()?)
}

#[tauri::command]
pub fn roles_save(request: RoleSaveRequest) -> Result<Role, String> {
    let _guard = lock()?;
    save_locked(&root()?, request)
}

#[tauri::command]
pub fn roles_delete(id: String) -> Result<(), String> {
    let _guard = lock()?;
    delete_locked(&root()?, &id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dsivio-roles-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn request(name: &str) -> RoleSaveRequest {
        RoleSaveRequest {
            id: None,
            revision: None,
            name: name.into(),
            description: "一位出镜的人".into(),
            images: Vec::new(),
            inline_images: Vec::new(),
        }
    }

    #[test]
    fn revision_conflict_keeps_the_saved_role() {
        let dir = temp_dir();
        let saved = save_locked(&dir, request("小美")).unwrap();
        assert_eq!(saved.revision, 1);
        let mut stale = request("新名字");
        stale.id = Some(saved.id.clone());
        stale.revision = Some(0);
        let err = save_locked(&dir, stale).unwrap_err();
        assert!(err.contains("重新打开"), "{err}");
        let listed = list_locked(&dir).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "小美");
        assert_eq!(listed[0].revision, 1);

        let mut next = request("小美");
        next.id = Some(saved.id.clone());
        next.revision = Some(1);
        next.description = "更新后的描述".into();
        let updated = save_locked(&dir, next).unwrap();
        assert_eq!(updated.revision, 2);
        assert_eq!(updated.description, "更新后的描述");
        assert_eq!(updated.created_at, saved.created_at);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_import_rejects_oversize_and_non_images_and_copies_png() {
        let dir = temp_dir();
        let big = dir.join("big.png");
        let file = fs::File::create(&big).unwrap();
        file.set_len(MAX_IMAGE_BYTES + 1).unwrap();
        let mut oversize = request("小美");
        oversize.images = vec![big.to_string_lossy().into_owned()];
        assert!(save_locked(&dir, oversize).unwrap_err().contains("50 MB"));

        let text = dir.join("note.txt");
        fs::write(&text, b"not an image").unwrap();
        let mut invalid = request("小美");
        invalid.images = vec![text.to_string_lossy().into_owned()];
        assert!(save_locked(&dir, invalid).is_err());

        let source = dir.join("portrait.png");
        fs::write(&source, TINY_PNG).unwrap();
        let mut ok = request("小美");
        ok.images = vec![source.to_string_lossy().into_owned()];
        ok.inline_images = vec![RoleInlineImage {
            name: "preset.webp".into(),
            base64: STANDARD.encode(TINY_PNG),
        }];
        let saved = save_locked(&dir, ok).unwrap();
        assert_eq!(saved.images.len(), 2);
        for image in &saved.images {
            let path = PathBuf::from(image);
            assert!(path.is_file(), "{image}");
            assert!(path.starts_with(image_dir(&dir, &saved.id)));
            assert_ne!(path, source);
        }

        let mut again = request("小美");
        again.id = Some(saved.id.clone());
        again.revision = Some(saved.revision);
        again.images = saved.images.clone();
        let kept = save_locked(&dir, again).unwrap();
        assert_eq!(kept.images.len(), 2);
        let files_after = fs::read_dir(image_dir(&dir, &saved.id)).unwrap().count();
        assert_eq!(files_after, 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_removes_the_record_and_copied_images() {
        let dir = temp_dir();
        let source = dir.join("portrait.png");
        fs::write(&source, TINY_PNG).unwrap();
        let mut create = request("小美");
        create.images = vec![source.to_string_lossy().into_owned()];
        let saved = save_locked(&dir, create).unwrap();
        let images = image_dir(&dir, &saved.id);
        assert!(images.is_dir());
        delete_locked(&dir, &saved.id).unwrap();
        assert!(list_locked(&dir).unwrap().is_empty());
        assert!(!record_path(&dir, &saved.id).exists());
        assert!(!images.exists());
        assert!(delete_locked(&dir, &saved.id).unwrap_err().contains("不存在"));
        let _ = fs::remove_dir_all(&dir);
    }
}
