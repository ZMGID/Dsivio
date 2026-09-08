//! Original dsimage templates and assets, pinned to bc83321d19cf51f694b1aa06efbc4cdcc7effd34.
use super::types::Template;
use std::{fs, path::Path};

struct Builtin {
    id: &'static str,
    json: &'static str,
    assets: &'static [(&'static str, &'static [u8])],
}

macro_rules! asset {
    ($folder:literal, $file:literal) => {
        (
            $file,
            include_bytes!(concat!(
                "../../resources/image-studio/",
                $folder,
                "/",
                $file
            ))
            .as_slice(),
        )
    };
}

const CATALOG: &[Builtin] = &[
    Builtin {
        id: "builtin-default-v1",
        json: include_str!("../../resources/image-studio/default.json"),
        assets: &[],
    },
    Builtin {
        id: "builtin-kids-v1",
        json: include_str!("../../resources/image-studio/kids.json"),
        assets: &[],
    },
    Builtin {
        id: "builtin-mens-backpack-v1",
        json: include_str!("../../resources/image-studio/mens-backpack/template.json"),
        assets: &[
            asset!("mens-backpack", "h1.png"),
            asset!("mens-backpack", "h2.png"),
            asset!("mens-backpack", "h3.png"),
            asset!("mens-backpack", "h4.png"),
            asset!("mens-backpack", "h5.png"),
            asset!("mens-backpack", "h6.png"),
            asset!("mens-backpack", "h7.png"),
            asset!("mens-backpack", "h8.png"),
            asset!("mens-backpack", "h9.png"),
            asset!("mens-backpack", "assets/logo.png"),
            asset!("mens-backpack", "assets/back_template.png"),
        ],
    },
    Builtin {
        id: "builtin-womens-backpack-v1",
        json: include_str!("../../resources/image-studio/womens-backpack/template.json"),
        assets: &[
            asset!("womens-backpack", "h1.png"),
            asset!("womens-backpack", "h2.png"),
            asset!("womens-backpack", "h3.png"),
            asset!("womens-backpack", "h4.png"),
            asset!("womens-backpack", "h5.png"),
            asset!("womens-backpack", "h6.png"),
            asset!("womens-backpack", "h7.png"),
            asset!("womens-backpack", "h8.png"),
            asset!("womens-backpack", "h9.png"),
        ],
    },
];

pub fn install_assets(id: &str, destination: &Path) -> Result<(), String> {
    let builtin = CATALOG
        .iter()
        .find(|entry| entry.id == id)
        .ok_or("内置模板不存在")?;
    for (name, bytes) in builtin.assets {
        let path = destination.join(name);
        // Built-ins are read-only; edits are saved to a new template directory.
        if fs::read(&path).is_ok_and(|existing| existing == *bytes) {
            continue;
        }
        fs::create_dir_all(path.parent().ok_or("素材目录无效")?).map_err(|e| e.to_string())?;
        fs::write(path, bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn templates(root: &Path) -> Result<Vec<Template>, String> {
    CATALOG
        .iter()
        .map(|entry| {
            let directory = format!("templates/{}", entry.id);
            let destination = root.join(&directory);
            fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
            install_assets(entry.id, &destination)?;
            let data = serde_json::from_str(entry.json).map_err(|e| e.to_string())?;
            // Publish the standard dsimage format for the built-in Skill as well.
            let file = destination.join("template.json");
            if super::storage::read::<serde_json::Value>(&file)
                .ok()
                .as_ref()
                != Some(&data)
            {
                super::storage::write(&file, &data)?;
            }
            Ok(Template {
                id: entry.id.into(),
                directory,
                builtin: true,
                data,
            })
        })
        .collect()
}
