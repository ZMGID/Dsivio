use std::{
    cmp::Ordering,
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn image(path: &Path) -> bool {
    path.extension().is_some_and(|e| {
        matches!(
            e.to_string_lossy().to_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "webp"
        )
    })
}

pub(super) fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut left = String::new();
                let mut right = String::new();
                while a.peek().is_some_and(|c| c.is_ascii_digit()) {
                    left.push(a.next().unwrap());
                }
                while b.peek().is_some_and(|c| c.is_ascii_digit()) {
                    right.push(b.next().unwrap());
                }
                let l = left.trim_start_matches('0');
                let r = right.trim_start_matches('0');
                let order = l
                    .len()
                    .cmp(&r.len())
                    .then(l.cmp(r))
                    .then(left.len().cmp(&right.len()));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                a.next();
                b.next();
                if x != y {
                    return x.cmp(&y);
                }
            }
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
        }
    }
}

fn scan(path: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if depth > 6 {
        return Err("商品目录超过 6 层，请选择更具体的文件夹".into());
    }
    if path.is_symlink() {
        return Ok(());
    }
    if path.is_dir() {
        for e in fs::read_dir(path).map_err(|e| e.to_string())? {
            scan(&e.map_err(|e| e.to_string())?.path(), depth + 1, out)?;
        }
    } else if image(path) {
        out.push(path.to_path_buf());
        if out.len() > 1000 {
            return Err("一次最多导入 1000 张素材".into());
        }
    }
    Ok(())
}

/// Multiple selected SKU folders keep their identity. A single parent folder with
/// no loose images treats its immediate subfolders as products; deeper folders are material.
pub(super) fn collect(
    paths: &[String],
    as_products: bool,
) -> Result<Vec<(String, Vec<PathBuf>)>, String> {
    let mut grouped = BTreeMap::<PathBuf, Vec<PathBuf>>::new();
    for path in paths {
        let path = Path::new(path);
        if path.is_symlink() {
            continue;
        }
        let path = path.canonicalize().map_err(|e| e.to_string())?;
        let roots = if as_products && paths.len() == 1 && path.is_dir() {
            let entries: Vec<_> = fs::read_dir(&path)
                .map_err(|e| e.to_string())?
                .map(|e| e.map(|e| e.path()).map_err(|e| e.to_string()))
                .collect::<Result<_, _>>()?;
            if entries.iter().any(|p| p.is_file() && image(p)) {
                vec![path.clone()]
            } else {
                entries
                    .into_iter()
                    .filter(|p| p.is_dir() && !p.is_symlink())
                    .collect()
            }
        } else {
            vec![path.clone()]
        };
        for root in roots {
            let key = if !as_products {
                PathBuf::new()
            } else if root.is_dir() {
                root.clone()
            } else {
                root.parent().unwrap_or(&root).to_path_buf()
            };
            scan(&root, 0, grouped.entry(key).or_default())?;
        }
    }
    if grouped.values().map(Vec::len).sum::<usize>() > 1000 {
        return Err("一次最多导入 1000 张素材".into());
    }
    let mut result = vec![];
    for (folder, mut files) in grouped {
        files.sort_by(|a, b| {
            natural_cmp(
                &a.file_name().unwrap_or_default().to_string_lossy(),
                &b.file_name().unwrap_or_default().to_string_lossy(),
            )
            .then(a.cmp(b))
        });
        files.dedup();
        if !files.is_empty() {
            result.push((
                if as_products {
                    folder
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into()
                } else {
                    "商品素材".into()
                },
                files,
            ));
        }
    }
    if result.is_empty() {
        return Err("未找到 PNG、JPEG 或 WebP 图片".into());
    }
    Ok(result)
}
