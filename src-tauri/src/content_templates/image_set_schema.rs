use super::image::validate_template;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn read_json(path: &Path) -> Value {
    let text =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("json {}: {error}", path.display()))
}

fn collect(dir: &Path, file_name: &str, out: &mut Vec<PathBuf>) {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, file_name, out);
        } else if path.file_name().and_then(|name| name.to_str()) == Some(file_name) {
            out.push(path);
        }
    }
}

#[test]
fn examples_builtins_and_skill_templates_match_the_shared_schema() {
    let root = repo();
    let mut templates = Vec::new();
    collect(
        &root.join("docs/schemas/examples/image-set-v1"),
        "template.json",
        &mut templates,
    );
    collect(
        &root.join("src-tauri/resources/skills/dsimage/templates"),
        "template.json",
        &mut templates,
    );
    let studio = root.join("src-tauri/resources/image-studio");
    templates.push(studio.join("default.json"));
    templates.push(studio.join("kids.json"));
    templates.push(studio.join("mens-backpack/template.json"));
    templates.push(studio.join("womens-backpack/template.json"));
    assert!(
        templates.len() > 4,
        "shared examples and builtins must be on disk"
    );
    for path in &templates {
        let data = read_json(path);
        validate_template(&data).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }

    let client_schema = read_json(&root.join("docs/schemas/image-set-client.v1.schema.json"));
    let mut clients = Vec::new();
    collect(
        &root.join("docs/schemas/examples/image-set-v1"),
        "要求.json",
        &mut clients,
    );
    collect(
        &root.join("src-tauri/resources/skills/dsimage/templates"),
        "要求.json",
        &mut clients,
    );
    assert!(!clients.is_empty());
    for path in &clients {
        let data = read_json(path);
        jsonschema::validate(&client_schema, &data)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
}

#[test]
fn validate_template_rejects_schema_invalid_documents_and_duplicate_ids() {
    let missing_prompt = json!({
        "name": "x",
        "mode": "replace",
        "slots": [{ "id": "h1", "example": "h1.png" }]
    });
    assert!(validate_template(&missing_prompt).is_err());

    let duplicate = json!({
        "name": "x",
        "mode": "smart",
        "slots": [
            { "id": "h1", "brief": "第一页" },
            { "id": "h1", "brief": "又是第一页" }
        ]
    });
    let schema = read_json(&repo().join("docs/schemas/image-set-template.v1.schema.json"));
    assert!(
        jsonschema::is_valid(&schema, &duplicate),
        "duplicate slot ids are schema-valid; Rust must still reject them"
    );
    assert!(validate_template(&duplicate).is_err());
    let mut case_collision = duplicate.clone();
    case_collision["slots"][1]["id"] = json!("H1");
    assert!(validate_template(&case_collision).is_err());
}
