use super::*;

#[test]
fn bundled_backpack_templates_materialize_all_reference_images_and_copy_assets() {
    let base = std::env::temp_dir().join(format!("dsivio-builtin-test-{}", storage::id()));
    let templates = builtins::templates(&base).unwrap();
    assert_eq!(templates.len(), 4);
    for template in &templates {
        storage::validate_template(&template.data).unwrap();
        if template.data["mode"] == "replace" {
            let slots = template.data["slots"].as_array().unwrap();
            assert_eq!(slots.len(), 9);
            for slot in slots {
                let path = base.join(&template.directory).join(slot["example"].as_str().unwrap());
                let bytes = std::fs::read(path).unwrap();
                assert!(image::load_from_memory(&bytes).is_ok());
            }
        }
    }
    let copy = base.join("copy");
    builtins::install_assets("builtin-mens-backpack-v1", &copy).unwrap();
    assert!(copy.join("assets/logo.png").is_file());
    assert!(copy.join("assets/back_template.png").is_file());
    assert!(copy.join("h9.png").is_file());
    std::fs::remove_dir_all(base).unwrap();
}

fn fixture() -> Task {
    serde_json::from_value(json!({
        "id":"test-task","revision":3,"createdAt":"2026-09-08","updatedAt":"2026-09-08",
        "brief":{"feature":"smart","name":"test","requirement":"商品图","language":"pt-BR","platform":"通用电商","ratio":"1:1","resolution":"1k","count":1,"style":"","templateId":null,
          "products":[
            {"id":"a","name":"A","category":"背包","kind":"","facts":"","front":null,"back":null,"assets":[],"templateId":null},
            {"id":"b","name":"B","category":"背包","kind":"","facts":"","front":null,"back":null,"assets":[],"templateId":null},
            {"id":"c","name":"C","category":"背包","kind":"","facts":"","front":null,"back":null,"assets":[],"templateId":null},
            {"id":"d","name":"D","category":"童装","kind":"","facts":"","front":null,"back":null,"assets":[],"templateId":null}]},
        "plans":[{"productId":"a","slotId":"h1","purpose":"主图","copy":"","prompt":"a","refs":[]},{"productId":"b","slotId":"h1","purpose":"主图","copy":"","prompt":"b","refs":[]}],
        "results":[],"approvedGroups":[],"status":"planned","progress":"","error":null,"templates":[]
    })).unwrap()
}
fn result(product: &str, rev: u64, path: Option<&str>) -> ImageResult {
    serde_json::from_value(json!({"id":storage::id(),"productId":product,"slotId":"h1","revision":rev,"path":path,"error":null,"remoteId":null,"prompt":"","width":800,"height":800,"review":null,"config":{}})).unwrap()
}
#[test]
fn sample_gate_is_per_category_and_requires_both_complete_products() {
    let mut t = fixture();
    assert_eq!(sample_ids(&t, "背包"), vec!["a", "b"]);
    t.results.push(result("a", 3, Some("a.png")));
    assert!(!samples_complete(&t, "背包"));
    t.results.push(result("b", 3, Some("b.png")));
    assert!(samples_complete(&t, "背包"));
    assert!(!samples_complete(&t, "童装"));
    assert!(!samples_complete(&t, "missing"));
}
#[test]
fn failed_latest_revision_or_version_cannot_reuse_old_approval_evidence() {
    let mut t = fixture();
    t.results
        .extend([result("a", 3, Some("a.png")), result("b", 2, Some("b.png"))]);
    assert!(!samples_complete(&t, "背包"));
    t.results.push(result("b", 3, Some("b-new.png")));
    assert!(samples_complete(&t, "背包"));
    t.results.push(result("a", 3, None));
    assert!(!samples_complete(&t, "背包"));
}
#[test]
fn singleton_category_requires_all_its_planned_pages() {
    let mut t = fixture();
    t.brief.products.truncate(1);
    t.plans.truncate(1);
    t.results.push(result("a", 3, Some("a.png")));
    assert!(samples_complete(&t, "背包"));
    let mut extra = t.plans[0].clone();
    extra.slot_id = "h2".into();
    t.plans.push(extra);
    assert!(!samples_complete(&t, "背包"));
}
#[test]
fn original_bundled_templates_validate_with_their_actual_page_counts() {
    let default: Value =
        serde_json::from_str(include_str!("../../resources/image-studio/default.json")).unwrap();
    let kids: Value =
        serde_json::from_str(include_str!("../../resources/image-studio/kids.json")).unwrap();
    storage::validate_template(&default).unwrap();
    storage::validate_template(&kids).unwrap();
    assert_eq!(default["slots"].as_array().unwrap().len(), 9);
    assert_eq!(kids["slots"].as_array().unwrap().len(), 7);
}
#[test]
fn invalid_templates_and_traversal_ids_fail_before_io() {
    assert!(storage::validate_template(
        &json!({"name":"x","mode":"replace","slots":[{"id":"h1"}]})
    )
    .is_err());
    assert!(storage::validate_template(
        &json!({"name":"x","mode":"smart","slots":[{"id":"h1"},{"id":"h1"}]})
    )
    .is_err());
    for id in ["../outside", "x/y", "", "C:\\test"] {
        assert!(storage::safe_id(id).is_err());
    }
    assert!(storage::resolve("../outside.png").is_err());
}
#[test]
fn missing_back_ref_is_a_user_fix_not_an_invented_structure() {
    let t = fixture();
    let template = Template {
        id: "x".into(),
        data: json!({"mode":"smart"}),
        directory: "templates/x".into(),
        builtin: false,
    };
    let error = agent::template_refs(
        &template,
        &t.brief.products[0],
        &json!({"id":"h2","refs":["@product.back"]}),
    )
    .unwrap_err();
    assert!(error.contains("真实背面"));
}
#[test]
fn provider_capabilities_are_enforced_instead_of_silently_downgrading() {
    let mut b = fixture().brief;
    let mut c = StudioConfig {
        provider_id: "p".into(),
        model: "image".into(),
        protocol: "openai".into(),
        ..Default::default()
    };
    assert!(engine::validate(&c, &b).is_ok());
    b.resolution = "4k".into();
    assert!(engine::validate(&c, &b).is_err());
    c.protocol = "grok".into();
    assert!(engine::validate(&c, &b).is_err());
    c.protocol = "gemini".into();
    assert!(engine::validate(&c, &b).is_ok());
}
#[test]
fn malformed_agent_output_does_not_become_a_generation_prompt() {
    assert!(agent::parse_json("I generated your images").is_err());
    assert_eq!(
        agent::parse_json("```json\n{\"plans\":[]}\n```").unwrap(),
        json!({"plans":[]})
    );
}

#[test]
fn grok_multi_image_edits_identify_the_target_and_product_refs() {
    assert_eq!(
        engine::grok_prompt("Edit the FIRST image using the second image", 2),
        "Edit <IMAGE_0> using <IMAGE_1>"
    );
    assert!(
        engine::grok_prompt("Preserve the product", 3).contains("<IMAGE_0> <IMAGE_1> <IMAGE_2>")
    );
    assert_eq!(
        engine::grok_prompt("Single reference", 1),
        "Single reference"
    );
}
