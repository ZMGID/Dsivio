//! Local product archive. JSON records live under `{app_data}/workbench/products`.
//!
//! Image files are copies this module owns, in `images/{id}/`. They last only as
//! long as the product: `products_delete` removes the record and that directory
//! together. A path that already lives in the product's image directory is kept
//! (the managed copy from an earlier save). Any other filesystem path is copied
//! at save time, so the archive does not follow the source file. `blob:` URLs,
//! `data:` URLs, and other non-file addresses are rejected — they are not files
//! this process can keep.
//!
//! One lock covers list, save, and delete. The JSON file is replaced by rename.
//! A stale `revision` is refused so another window cannot overwrite a newer record.
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
const MAX_NAME_CHARS: usize = 200;
const MAX_DESCRIPTION_CHARS: usize = 8000;
const MAX_SKUS: usize = 50;
const MAX_SKU_CHARS: usize = 80;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductSku {
    pub code: String,
    pub name: String,
    pub price: Option<f64>,
    pub stock: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Product {
    pub id: String,
    pub revision: u32,
    pub name: String,
    pub description: String,
    pub currency: Option<String>,
    pub price: Option<f64>,
    pub stock: Option<i32>,
    pub skus: Vec<ProductSku>,
    pub images: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductInlineImage {
    pub name: String,
    pub base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductSaveRequest {
    pub id: Option<String>,
    pub revision: Option<u32>,
    pub name: String,
    pub description: String,
    pub currency: Option<String>,
    pub price: Option<f64>,
    pub stock: Option<i32>,
    pub skus: Vec<ProductSku>,
    pub images: Vec<String>,
    #[serde(default)]
    pub inline_images: Vec<ProductInlineImage>,
}

fn lock() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    static LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
    LOCK.lock().map_err(|err| err.to_string())
}

fn root() -> Result<PathBuf, String> {
    let dir = crate::app_data::app_data_dir()
        .ok_or("无法定位应用数据目录")?
        .join("workbench")
        .join("products");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

fn parse_id(id: &str) -> Result<String, String> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| "无效的商品 ID".to_string())?;
    Ok(parsed.to_string())
}

fn record_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}

fn image_dir(dir: &Path, id: &str) -> PathBuf {
    dir.join("images").join(id)
}

fn load_product(dir: &Path, id: &str) -> Result<Option<Product>, String> {
    let path = record_path(dir, id);
    if !path.is_file() {
        return Ok(None);
    }
    files::read(&path).map(Some)
}

fn list_locked(dir: &Path) -> Result<Vec<Product>, String> {
    fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    let entries = fs::read_dir(dir).map_err(|err| err.to_string())?;
    let mut products = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        products.push(files::read::<Product>(&path)?);
    }
    products.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
    Ok(products)
}

fn reject_unmanaged(source: &str) -> Result<(), String> {
    let lower = source.to_ascii_lowercase();
    if lower.starts_with("blob:") || lower.starts_with("data:") || lower.contains("://") {
        return Err("图片必须是本机文件，不能使用浏览器临时地址".into());
    }
    Ok(())
}

fn ensure_image(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("单张图片不能超过 50 MB".into());
    }
    decode(bytes)?;
    let format = image::guess_format(bytes).map_err(|err| err.to_string())?;
    match format {
        image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::WebP => Ok(()),
        _ => Err("仅支持 PNG、JPEG、WebP".into()),
    }
}

fn store_bytes(image_dir: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    ensure_image(bytes)?;
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
    path.canonicalize().map_err(|err| err.to_string())
}

fn copy_image(dir: &Path, product_id: &str, source: &Path) -> Result<PathBuf, String> {
    let dest_dir = image_dir(dir, product_id);
    if source.is_file() {
        if let (Ok(src), Ok(root)) = (source.canonicalize(), dest_dir.canonicalize()) {
            if src.starts_with(&root) {
                let bytes = fs::read(&src).map_err(|err| err.to_string())?;
                ensure_image(&bytes)?;
                return Ok(src);
            }
        }
    }
    let meta = fs::metadata(source).map_err(|_| format!("找不到图片：{}", source.display()))?;
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

fn nonneg_price(value: Option<f64>, label: &str) -> Result<Option<f64>, String> {
    match value {
        None => Ok(None),
        Some(number) if number.is_finite() && number >= 0.0 => Ok(Some(number)),
        Some(_) => Err(format!("{label}必须是非负数字")),
    }
}

fn nonneg_stock(value: Option<i32>, label: &str) -> Result<Option<i32>, String> {
    match value {
        None => Ok(None),
        Some(number) if number >= 0 => Ok(Some(number)),
        Some(_) => Err(format!("{label}不能为负")),
    }
}

fn normalize_currency(value: Option<String>) -> Result<Option<String>, String> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() != 3 || !trimmed.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return Err("货币须为 3 位字母代码".into());
    }
    Ok(Some(trimmed.to_ascii_uppercase()))
}

fn normalize_skus(skus: Vec<ProductSku>) -> Result<Vec<ProductSku>, String> {
    if skus.len() > MAX_SKUS {
        return Err(format!("一个商品最多 {MAX_SKUS} 个 SKU"));
    }
    let mut normalized = Vec::new();
    for sku in skus {
        let code = sku.code.trim();
        let name = sku.name.trim();
        let blank = code.is_empty() && name.is_empty() && sku.price.is_none() && sku.stock.is_none();
        if blank {
            continue;
        }
        if code.is_empty() && name.is_empty() {
            return Err("SKU 需要编号或名称".into());
        }
        if code.chars().count() > MAX_SKU_CHARS || name.chars().count() > MAX_SKU_CHARS {
            return Err("SKU 编号或名称不能超过 80 字".into());
        }
        normalized.push(ProductSku {
            code: code.to_string(),
            name: name.to_string(),
            price: nonneg_price(sku.price, "SKU 价格")?,
            stock: nonneg_stock(sku.stock, "SKU 库存")?,
        });
    }
    Ok(normalized)
}

fn import_images(dir: &Path, id: &str, dest_dir: &Path, request: &ProductSaveRequest) -> Result<Vec<PathBuf>, String> {
    let mut images = Vec::new();
    for source in &request.images {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            return Err("图片路径无效".into());
        }
        reject_unmanaged(trimmed)?;
        images.push(copy_image(dir, id, Path::new(trimmed))?);
    }
    for inline in &request.inline_images {
        let bytes = STANDARD
            .decode(inline.base64.trim())
            .map_err(|_| "图片数据无效".to_string())?;
        images.push(store_bytes(dest_dir, &bytes)?);
    }
    Ok(images)
}

fn save_locked(dir: &Path, request: ProductSaveRequest) -> Result<Product, String> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err("商品名称不能为空".into());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err("商品名称不能超过 200 字".into());
    }
    let description = request.description.trim();
    if description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err("卖点不能超过 8000 字".into());
    }
    if request.images.len() + request.inline_images.len() > MAX_IMAGES {
        return Err("一个商品最多 12 张图片".into());
    }
    let name = name.to_string();
    let description = description.to_string();
    let existing = if let Some(id) = request.id.as_deref().filter(|id| !id.is_empty()) {
        let id = parse_id(id)?;
        match load_product(dir, &id)? {
            Some(product) => {
                if request.revision != Some(product.revision) {
                    return Err("此商品已在其他窗口更新，请重新打开后操作".into());
                }
                Some(product)
            }
            None => return Err("商品不存在".into()),
        }
    } else {
        None
    };

    let id = existing
        .as_ref()
        .map(|product| product.id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let created_at = existing
        .as_ref()
        .map(|product| product.created_at.clone())
        .unwrap_or_else(files::now);
    let revision = existing.as_ref().map(|product| product.revision).unwrap_or(0).saturating_add(1);
    let currency = normalize_currency(request.currency.clone())?;
    let price = nonneg_price(request.price, "价格")?;
    let stock = nonneg_stock(request.stock, "库存")?;
    let skus = normalize_skus(request.skus.clone())?;
    let dest_dir = image_dir(dir, &id);
    let images = match import_images(dir, &id, &dest_dir, &request) {
        Ok(images) => images,
        Err(err) => {
            if existing.is_none() {
                let _ = fs::remove_dir_all(&dest_dir);
            }
            return Err(err);
        }
    };
    let product = Product {
        id,
        revision,
        name,
        description,
        currency,
        price,
        stock,
        skus,
        images: images.iter().map(|path| path.to_string_lossy().into_owned()).collect(),
        created_at,
        updated_at: files::now(),
    };
    if let Err(err) = files::write(&record_path(dir, &product.id), &product) {
        if existing.is_none() {
            let _ = fs::remove_dir_all(&dest_dir);
        }
        return Err(err);
    }
    cleanup_images(&dest_dir, &images)?;
    Ok(product)
}

fn delete_locked(dir: &Path, id: &str) -> Result<(), String> {
    let id = parse_id(id)?;
    let path = record_path(dir, &id);
    if !path.is_file() {
        return Err("商品不存在".into());
    }
    fs::remove_file(&path).map_err(|err| err.to_string())?;
    let images = image_dir(dir, &id);
    if images.is_dir() {
        fs::remove_dir_all(&images).map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn products_list() -> Result<Vec<Product>, String> {
    let _guard = lock()?;
    list_locked(&root()?)
}

#[tauri::command]
pub fn products_save(request: ProductSaveRequest) -> Result<Product, String> {
    let _guard = lock()?;
    save_locked(&root()?, request)
}

#[tauri::command]
pub fn products_delete(id: String) -> Result<(), String> {
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
        let dir = std::env::temp_dir().join(format!("dsivio-products-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn request(name: &str) -> ProductSaveRequest {
        ProductSaveRequest {
            id: None,
            revision: None,
            name: name.into(),
            description: "轻便，耐磨".into(),
            currency: Some("cny".into()),
            price: Some(19.5),
            stock: Some(8),
            skus: vec![ProductSku {
                code: " RED-1 ".into(),
                name: "红色".into(),
                price: Some(19.5),
                stock: Some(3),
            }],
            images: Vec::new(),
            inline_images: Vec::new(),
        }
    }

    #[test]
    fn save_edit_conflict_and_delete_round_trip() {
        let dir = temp_dir();
        let saved = save_locked(&dir, request("运动水杯")).unwrap();
        assert_eq!(saved.revision, 1);
        assert_eq!(saved.currency.as_deref(), Some("CNY"));
        assert_eq!(saved.price, Some(19.5));
        assert_eq!(saved.stock, Some(8));
        assert_eq!(saved.skus[0].code, "RED-1");
        assert_eq!(saved.description, "轻便，耐磨");

        let mut stale = request("新名字");
        stale.id = Some(saved.id.clone());
        stale.revision = Some(0);
        let err = save_locked(&dir, stale).unwrap_err();
        assert!(err.contains("重新打开"), "{err}");
        let listed = list_locked(&dir).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "运动水杯");
        assert_eq!(listed[0].revision, 1);

        let mut next = request("运动水杯");
        next.id = Some(saved.id.clone());
        next.revision = Some(1);
        next.description = "更新后的卖点".into();
        next.price = Some(20.0);
        next.stock = None;
        next.skus = vec![ProductSku {
            code: "".into(),
            name: "".into(),
            price: None,
            stock: None,
        }];
        let updated = save_locked(&dir, next).unwrap();
        assert_eq!(updated.revision, 2);
        assert_eq!(updated.description, "更新后的卖点");
        assert_eq!(updated.price, Some(20.0));
        assert_eq!(updated.stock, None);
        assert!(updated.skus.is_empty());
        assert_eq!(updated.created_at, saved.created_at);

        delete_locked(&dir, &saved.id).unwrap();
        assert!(list_locked(&dir).unwrap().is_empty());
        assert!(!record_path(&dir, &saved.id).exists());
        assert!(delete_locked(&dir, &saved.id).unwrap_err().contains("不存在"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_price_currency_and_sku_do_not_persist() {
        let dir = temp_dir();
        let mut negative = request("杯子");
        negative.price = Some(-1.0);
        assert!(save_locked(&dir, negative).unwrap_err().contains("价格"));
        assert!(list_locked(&dir).unwrap().is_empty());

        let mut currency = request("杯子");
        currency.currency = Some("元".into());
        assert!(save_locked(&dir, currency).unwrap_err().contains("货币"));

        let mut sku = request("杯子");
        sku.skus = vec![ProductSku {
            code: "".into(),
            name: "".into(),
            price: Some(1.0),
            stock: None,
        }];
        assert!(save_locked(&dir, sku).unwrap_err().contains("SKU"));
        assert!(list_locked(&dir).unwrap().is_empty());

        let mut stock = request("杯子");
        stock.stock = Some(-2);
        assert!(save_locked(&dir, stock).unwrap_err().contains("库存"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_import_copies_decodes_and_keeps_managed_files() {
        let dir = temp_dir();
        let mut blob = request("杯子");
        blob.images = vec!["blob:http://localhost/7c9e".into()];
        let err = save_locked(&dir, blob).unwrap_err();
        assert!(err.contains("浏览器临时地址"), "{err}");
        assert!(list_locked(&dir).unwrap().is_empty());

        let big = dir.join("big.png");
        let file = fs::File::create(&big).unwrap();
        file.set_len(MAX_IMAGE_BYTES + 1).unwrap();
        let mut oversize = request("杯子");
        oversize.images = vec![big.to_string_lossy().into_owned()];
        assert!(save_locked(&dir, oversize).unwrap_err().contains("50 MB"));

        let text = dir.join("note.txt");
        fs::write(&text, b"not an image").unwrap();
        let mut invalid = request("杯子");
        invalid.images = vec![text.to_string_lossy().into_owned()];
        assert!(save_locked(&dir, invalid).is_err());

        let source = dir.join("cup.png");
        fs::write(&source, TINY_PNG).unwrap();
        let mut ok = request("杯子");
        ok.images = vec![source.to_string_lossy().into_owned()];
        ok.inline_images = vec![ProductInlineImage {
            name: "extra.png".into(),
            base64: STANDARD.encode(TINY_PNG),
        }];
        let saved = save_locked(&dir, ok).unwrap();
        assert_eq!(saved.images.len(), 2);
        let root = image_dir(&dir, &saved.id).canonicalize().unwrap();
        for image in &saved.images {
            let path = PathBuf::from(image).canonicalize().unwrap();
            assert!(path.is_file(), "{image}");
            assert!(path.starts_with(&root), "{image}");
            assert_ne!(path, source.canonicalize().unwrap());
            decode(&fs::read(&path).unwrap()).unwrap();
        }

        let mut again = request("杯子");
        again.id = Some(saved.id.clone());
        again.revision = Some(saved.revision);
        again.images = vec![saved.images[0].clone()];
        again.inline_images.clear();
        let kept = save_locked(&dir, again).unwrap();
        assert_eq!(kept.images, vec![saved.images[0].clone()]);
        let files_after = fs::read_dir(image_dir(&dir, &saved.id)).unwrap().count();
        assert_eq!(files_after, 1);

        delete_locked(&dir, &saved.id).unwrap();
        assert!(!image_dir(&dir, &saved.id).exists());
        assert!(source.is_file());
        let _ = fs::remove_dir_all(&dir);
    }
}
