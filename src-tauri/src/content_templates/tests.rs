use super::*;
use serde_json::json;
use std::fs;

fn temp() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("dsivio-template-test-{}", files::id()));
    fs::create_dir_all(&root).unwrap();
    root
}
#[test]
fn image_round_trip_preserves_identity_slots_and_all_reference_assets() {
    let root = temp();
    fs::create_dir_all(root.join("tasks")).unwrap();
    fs::create_dir_all(root.join("results")).unwrap();
    fs::write(root.join("tasks/history.json"), b"history sentinel").unwrap();
    fs::write(root.join("results/output.png"), b"output sentinel").unwrap();
    let source = root.join("source");
    fs::create_dir_all(source.join("assets")).unwrap();
    for file in ["example.png", "assets/ref.png", "assets/kind.png"] {
        ::image::RgbaImage::new(2, 2)
            .save(source.join(file))
            .unwrap();
    }
    let data = json!({"name":"旧模板", "mode":"replace", "custom":{"keep":true}, "slots":[
        {"id":"second", "example":"example.png", "refs":["@example","assets/ref.png"], "refs_by_kind":{"bag":["assets/kind.png","@product.front"]}},
        {"id":"first", "example":"example.png"}
    ]});
    files::write(&source.join("template.json"), &data).unwrap();
    let mut imported = image::import_at(&root, source.to_str().unwrap()).unwrap();
    let id = imported.id.clone();
    let directory = imported.directory.clone();
    imported.data["name"] = json!("编辑后的模板");
    let saved = image::save_at(&root, imported).unwrap();
    assert_eq!(saved.id, id);
    assert_eq!(saved.directory, directory);
    let exported = image::export_at(&root, &id, root.to_str().unwrap()).unwrap();
    let again = image::import_at(&root, &exported).unwrap();
    assert_eq!(again.data, saved.data);
    for file in ["example.png", "assets/ref.png", "assets/kind.png"] {
        assert_eq!(
            fs::read(root.join(&again.directory).join(file)).unwrap(),
            fs::read(source.join(file)).unwrap()
        );
    }
    // Invalid reference failures cannot silently publish a save.
    let mut invalid = again.clone();
    invalid.data["slots"][0]["refs"] = json!(["../escape.png"]);
    assert!(image::save_at(&root, invalid).is_err());
    let disk: serde_json::Value =
        files::read(&root.join(&again.directory).join("template.json")).unwrap();
    assert_eq!(disk, again.data);
    assert_eq!(
        fs::read(root.join("tasks/history.json")).unwrap(),
        b"history sentinel"
    );
    assert_eq!(
        fs::read(root.join("results/output.png")).unwrap(),
        b"output sentinel"
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn image_builtin_edits_always_create_user_copy_even_with_forged_flag() {
    let root = temp();
    let mut builtin = image::list_at(&root)
        .unwrap()
        .into_iter()
        .find(|t| t.id == "builtin-mens-backpack-v1")
        .unwrap();
    let original = builtin.data.clone();
    builtin.builtin = false;
    builtin.data["name"] = json!("我的模板");
    let saved = image::save_at(&root, builtin).unwrap();
    assert!(!saved.builtin);
    assert_ne!(saved.id, "builtin-mens-backpack-v1");
    assert!(root.join(saved.directory).join("assets/logo.png").is_file());
    assert_eq!(
        image::list_at(&root)
            .unwrap()
            .iter()
            .find(|t| t.id == "builtin-mens-backpack-v1")
            .unwrap()
            .data,
        original
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn image_record_only_and_nested_skill_formats_keep_ids_and_order() {
    let root = temp();
    let dir = root.join("templates/legacy");
    fs::create_dir_all(&dir).unwrap();
    let old = image::Template {
        id: "legacy-id".into(),
        directory: "templates/legacy".into(),
        builtin: false,
        data: json!({"name":"旧格式", "mode":"smart", "slots":[{"id":"z"},{"id":"a"}]}),
    };
    files::write(&dir.join("record.json"), &old).unwrap();
    let list = image::list_at(&root).unwrap();
    let found = list.iter().find(|t| t.id == old.id).unwrap();
    assert_eq!(found.data, old.data);
    assert_eq!(found.directory, old.directory);
    let nested = root.join("templates/bag/variant");
    fs::create_dir_all(&nested).unwrap();
    files::write(
        &root.join("templates/bag/要求.json"),
        &json!({"templates":["variant"],"language":"pt-BR"}),
    )
    .unwrap();
    files::write(&nested.join("template.json"), &old.data).unwrap();
    let list = image::list_at(&root).unwrap();
    assert_eq!(
        list.iter()
            .find(|t| t.directory == "templates/bag/variant")
            .unwrap()
            .data["language"],
        "pt-BR"
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn video_legacy_edits_preserve_raw_fields_and_reference_provenance() {
    let root = temp();
    let raw = json!({"id":"internal-id", "name":"旧视频", "full_video_prompt":"旧剧本", "prompt_pattern":"保留", "reference_template":true,
        "kind":"generation", "validated_from":{"user_approved":false}, "source":{"duration_seconds":15,"aspect_ratio":"9:16","url":"source"},
        "shot_breakdown":[{"time":"0-3", "subject":"商品", "composition_camera":"近景", "unknown":42}],"extra":{"keep":true}});
    files::write(&root.join("different-filename.json"), &raw).unwrap();
    let mut template = video::get_at(&root, "internal-id").unwrap();
    assert_eq!(template.kind, "reference");
    assert_eq!(template.script, "旧剧本");
    assert_eq!(template.shots[0]["camera"], "近景");
    assert_eq!(template.spec["duration_seconds"], 15);
    template.name = "新名称".into();
    template.kind = "generation".into();
    template.data = json!({});
    let saved = video::save_at(&root, template).unwrap();
    let mut expected = raw.clone();
    expected["name"] = json!("新名称");
    assert_eq!(saved.data, expected);
    assert_eq!(saved.kind, "reference");
    let mut edited = saved;
    edited.script = "新剧本".into();
    edited.shots[0]["action"] = json!("转动商品");
    edited.spec["aspect_ratio"] = json!("1:1");
    let saved = video::save_at(&root, edited).unwrap();
    assert_eq!(saved.data["shot_breakdown"], raw["shot_breakdown"]);
    assert_eq!(saved.data["source"], raw["source"]);
    assert_eq!(saved.data["extra"], raw["extra"]);
    assert_eq!(saved.data["full_video_prompt"], raw["full_video_prompt"]);
    let export_dir = root.join("export");
    fs::create_dir(&export_dir).unwrap();
    let exported = video::export_at(&root, &saved.id, &export_dir).unwrap();
    let imported = video::import_at(&root, Path::new(&exported)).unwrap();
    assert_eq!(imported.script, saved.script);
    assert_eq!(imported.shots, saved.shots);
    assert_eq!(imported.spec, saved.spec);
    assert_eq!(imported.kind, "reference");
    assert!(!root.join("internal-id.json").exists());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn video_builtin_visibility_never_overwrites_user_same_name_file() {
    let root = temp();
    let mut builtin = video::list_at(&root).unwrap().remove(0);
    assert_eq!(builtin.kind, "generation");
    builtin.name = "我的修改".into();
    let saved = video::save_at(&root, builtin).unwrap();
    assert_eq!(video::list_at(&root).unwrap().len(), 1);
    assert_eq!(video::get_at(&root, &saved.id).unwrap().name, "我的修改");
    let path = root.join("broken.json");
    fs::write(path, "{").unwrap();
    assert!(video::list_at(&root).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn video_bom_and_prompt_pattern_are_supported_without_promoting_reference() {
    let root = temp();
    let path = root.join("source.json");
    fs::write(&path, "\u{feff}{\"name\":\"BOM\",\"prompt_pattern\":\"script\",\"kind\":\"generation\",\"reference_template\":\"reference\"}").unwrap();
    let imported = video::import_at(&root, &path).unwrap();
    assert_eq!(imported.script, "script");
    assert_eq!(imported.kind, "reference");
    assert!(video::insert_at(&root, json!([])).is_err());
    fs::remove_dir_all(root).unwrap();
}

fn legacy_video() -> serde_json::Value {
    json!({"id":"legacy", "name":"旧模板", "full_video_prompt":"旧剧本", "prompt_pattern":"备用剧本",
        "shot_breakdown":[{"subject":"商品", "custom":"镜头扩展"}],
        "source":{"duration_seconds":15,"aspect_ratio":"9:16","url":"source"},
        "reference_template":true,"validated_from":{"user_approved":false},"extra":{"keep":true}})
}

fn assert_video_edit_round_trip(
    edit: impl FnOnce(&mut video::VideoTemplate),
    check: impl Fn(&video::VideoTemplate),
) {
    let root = tempfile::tempdir().unwrap();
    let raw = legacy_video();
    files::write(&root.path().join("legacy.json"), &raw).unwrap();
    let mut edited = video::get_at(root.path(), "legacy").unwrap();
    edit(&mut edited);
    let saved = video::save_at(root.path(), edited).unwrap();
    let reread = video::get_at(root.path(), "legacy").unwrap();
    let export_dir = root.path().join("export");
    fs::create_dir(&export_dir).unwrap();
    let exported = video::export_at(root.path(), "legacy", &export_dir).unwrap();
    let imported = video::import_at(root.path(), Path::new(&exported)).unwrap();
    for template in [&saved, &reread, &imported] {
        check(template);
        assert_eq!(template.kind, "reference");
        for key in ["full_video_prompt", "prompt_pattern", "shot_breakdown", "source", "extra", "reference_template", "validated_from"] {
            assert_eq!(template.data[key], raw[key], "original field {key}");
        }
        assert_eq!(video::for_studio(template.clone())["script"], template.script);
        assert_eq!(video::for_studio(template.clone())["shots"], json!(template.shots));
    }
}

#[test]
fn video_explicit_empty_script_survives_save_read_export_import() {
    assert_video_edit_round_trip(
        |template| template.script.clear(),
        |template| {
            assert_eq!(template.script, "");
            assert_eq!(template.shots.len(), 1);
        },
    );
}

#[test]
fn video_explicit_empty_shots_survive_save_read_export_import() {
    assert_video_edit_round_trip(
        |template| template.shots.clear(),
        |template| {
            assert!(template.shots.is_empty());
            assert_eq!(template.script, "旧剧本");
        },
    );
}

#[test]
fn video_explicit_empty_spec_survives_save_read_export_import() {
    assert_video_edit_round_trip(
        |template| {
            template.spec["duration_seconds"] = serde_json::Value::Null;
            template.spec["aspect_ratio"] = json!("");
        },
        |template| {
            assert!(template.spec["duration_seconds"].is_null());
            assert_eq!(template.spec["aspect_ratio"], "");
        },
    );
}

#[test]
fn video_clearing_both_script_and_shots_fails_without_changing_disk() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("legacy.json");
    let raw = legacy_video();
    files::write(&path, &raw).unwrap();
    let mut edited = video::get_at(root.path(), "legacy").unwrap();
    edited.script.clear();
    edited.shots.clear();
    assert!(video::save_at(root.path(), edited).is_err());
    assert_eq!(files::read::<serde_json::Value>(&path).unwrap(), raw);
}

#[test]
fn video_missing_fields_still_use_legacy_aliases() {
    let mut raw = legacy_video();
    let template = video::normalize(raw.clone(), "legacy").unwrap();
    assert_eq!(template.script, "旧剧本");
    assert_eq!(template.shots[0]["action"], "商品");
    assert_eq!(template.spec["duration_seconds"], 15);
    assert_eq!(template.spec["aspect_ratio"], "9:16");
    raw.as_object_mut().unwrap().remove("full_video_prompt");
    assert_eq!(video::normalize(raw, "legacy").unwrap().script, "备用剧本");
}
