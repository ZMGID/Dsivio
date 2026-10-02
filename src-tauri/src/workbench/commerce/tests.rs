use super::cli::parse;
use super::adapter::{ScriptHttp, SharedTransport};
use super::service::{self, Runtime, ShopSource};
use super::shopee::{self, ResolvedShop, ScriptTransport, Step, Transport, TransportFault};
use super::types::{AttributeInput, ListingDraft, ListingStatus, MetricKey, MetricRange};
use crate::workbench::shops::{sign_shopee, Platform};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

const NOW: i64 = 1_790_899_200;

fn shop(platform: Platform) -> ResolvedShop {
    ResolvedShop {
        platform,
        partner_id: "123456".into(),
        partner_key: "partner-secret".into(),
        access_token: "token-abc".into(),
        remote_id: "789".into(),
        region: Some("SG".into()),
        name: "demo".into(),
    }
}

fn harness(platform: Platform) -> (tempfile::TempDir, Runtime, Arc<ScriptTransport>) {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = Arc::new(ScriptTransport::new());
    let mut shops = HashMap::new();
    shops.insert("shop-1".into(), shop(platform));
    let runtime = Runtime {
        db: dir.path().join("commerce.sqlite3"),
        transport: Transport::script(script.clone()),
        http: SharedTransport::script(Arc::new(ScriptHttp::new())),
        shops: ShopSource::Fixed(shops),
        now: NOW,
    };
    (dir, runtime, script)
}

fn draft(image: &str) -> ListingDraft {
    serde_json::from_value(json!({
        "title": "Ceramic Cup",
        "description": "A stoneware cup for tea.",
        "price": 12.0,
        "currency": "SGD",
        "stock": 4,
        "images": [image],
        "weightKg": 0.4,
        "dimensionsCm": { "l": 10.0, "w": 8.0, "h": 9.0 },
        "brand": "NoBrand"
    }))
    .expect("draft")
}

fn target() -> Value {
    json!({ "shopId": "shop-1", "categoryId": "100" })
}

fn image_file(dir: &std::path::Path) -> String {
    let path = dir.join("cup.jpg");
    std::fs::write(&path, b"\xff\xd8\xff\xd9").expect("image");
    path.to_string_lossy().into_owned()
}

fn push_happy(script: &ScriptTransport) {
    script.push(
        "/api/v2/logistics/get_channel_list",
        Step::Json(json!({"error": "", "response": {"logistics_channel_list": [
            {"logistics_channel_id": 80001, "enabled": false},
            {"logistics_channel_id": 80003, "enabled": true}
        ]}})),
    );
    script.push(
        "/api/v2/media_space/upload_image",
        Step::Json(json!({"error": "", "response": {"image_info": {"image_id": "img-1"}}})),
    );
    script.push(
        "/api/v2/product/add_item",
        Step::Json(json!({"error": "", "response": {"item_id": 99001}})),
    );
}

#[test]
fn shopee_sign_matches_the_documented_base_string() {
    let sign = sign_shopee(
        "123456",
        "partner-secret",
        "/api/v2/product/get_category",
        1_700_000_000,
        Some("token-abc"),
        Some("789"),
    )
    .expect("sign");
    assert_eq!(sign, "87ae1b8c09e365612e33ec07109fb2e6958aabd3084b68ea3ddfaa566839e4e7");
}

#[tokio::test]
async fn category_call_uses_the_shared_signer_and_maps_the_tree() {
    let (_dir, runtime, script) = harness(Platform::Shopee);
    script.push(
        "/api/v2/product/get_category",
        Step::Json(json!({"error": "", "response": {"category_list": [
            {"category_id": 1, "parent_category_id": 0, "display_category_name": "Home", "has_children": true},
            {"category_id": 100, "parent_category_id": 1, "original_category_name": "Cups", "has_children": false}
        ]}})),
    );
    let all = service::categories(&runtime, "shop-1", None).await.expect("categories");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, "1");
    assert!(!all[0].leaf);
    assert_eq!(all[1].parent_id, "1");
    assert!(all[1].leaf);
    assert_eq!(all[1].name, "Cups");
    script.push(
        "/api/v2/product/get_category",
        Step::Json(json!({"error": "", "response": {"category_list": [
            {"category_id": 1, "parent_category_id": 0, "display_category_name": "Home", "has_children": true},
            {"category_id": 100, "parent_category_id": 1, "display_category_name": "Cups", "has_children": false}
        ]}})),
    );
    let children = service::categories(&runtime, "shop-1", Some("1")).await.expect("children");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].id, "100");
    let call = script.calls().into_iter().find(|call| call.path.ends_with("get_category")).expect("call");
    let expected = sign_shopee("123456", "partner-secret", &call.path, NOW, Some("token-abc"), Some("789")).unwrap();
    assert_eq!(call.query_value("sign"), Some(expected.as_str()));
    assert_eq!(call.query_value("shop_id"), Some("789"));
}

#[tokio::test]
async fn attribute_tree_maps_input_types() {
    let (_dir, runtime, script) = harness(Platform::Shopee);
    script.push(
        "/api/v2/product/get_attribute_tree",
        Step::Json(json!({"error": "", "response": {"list": [{"category_id": 100, "attribute_tree": [
            {"attribute_id": 11, "name": "Color", "mandatory": true, "attribute_info": {"input_type": 1}, "attribute_value_list": [{"value_id": 7, "name": "Red"}]},
            {"attribute_id": 12, "name": "Material", "mandatory": false, "attribute_info": {"input_type": 3, "input_validation_type": 2, "attribute_unit_list": ["cm"]}},
            {"attribute_id": 13, "name": "Tags", "mandatory": false, "attribute_info": {"input_type": 4}}
        ]}]}})),
    );
    let attributes = service::attributes(&runtime, "shop-1", "100").await.expect("attributes");
    assert_eq!(attributes[0].input, AttributeInput::Select);
    assert!(attributes[0].required);
    assert_eq!(attributes[0].options[0].name, "Red");
    assert_eq!(attributes[1].input, AttributeInput::Number);
    assert_eq!(attributes[1].unit.as_deref(), Some("cm"));
    assert_eq!(attributes[2].input, AttributeInput::MultiSelect);
}

#[test]
fn legacy_attribute_list_and_item_status_map() {
    let mapped = shopee::map_attributes(&json!({"response": {"attribute_list": [
        {"attribute_id": 3, "display_attribute_name": "Note", "is_mandatory": false, "input_type": "TEXT_FILED"}
    ]}}));
    assert_eq!(mapped[0].input, AttributeInput::Text);
    assert_eq!(shopee::map_item_status("NORMAL").0, ListingStatus::Live);
    assert_eq!(shopee::map_item_status("REVIEWING").0, ListingStatus::Reviewing);
    assert_eq!(shopee::map_item_status("UNLIST").0, ListingStatus::Live);
    assert_eq!(shopee::map_item_status("UNLIST").1.as_deref(), Some("item unlisted (UNLIST)"));
    assert_eq!(shopee::map_item_status("BANNED").0, ListingStatus::Banned);
    assert_eq!(shopee::map_item_status("SELLER_DELETE").0, ListingStatus::Rejected);
}

#[tokio::test]
async fn metrics_aggregate_recorded_orders_and_mark_unsupported_keys() {
    let (_dir, runtime, script) = harness(Platform::Shopee);
    script.push(
        "/api/v2/order/get_order_list",
        Step::Json(json!({"error": "", "response": {"more": false, "next_cursor": "", "order_list": [
            {"order_sn": "A", "order_status": "READY_TO_SHIP"},
            {"order_sn": "B", "order_status": "COMPLETED"},
            {"order_sn": "C", "order_status": "CANCELLED"},
            {"order_sn": "D", "order_status": "UNPAID"},
            {"order_sn": "E", "order_status": "IN_CANCEL"},
            {"order_sn": "F", "order_status": "PROCESSED"},
            {"order_sn": "G", "order_status": "COMPLETED"}
        ]}})),
    );
    script.push(
        "/api/v2/order/get_order_detail",
        Step::Json(json!({"error": "", "response": {"order_list": [
            {"order_sn": "A", "order_status": "READY_TO_SHIP", "total_amount": 10, "currency": "SGD", "buyer_user_id": 7},
            {"order_sn": "B", "order_status": "COMPLETED", "total_amount": 4, "currency": "SGD", "buyer_user_id": 8},
            {"order_sn": "C", "order_status": "CANCELLED", "total_amount": 100, "currency": "SGD", "buyer_user_id": 9},
            {"order_sn": "D", "order_status": "UNPAID", "total_amount": 3, "buyer_user_id": 10},
            {"order_sn": "E", "order_status": "IN_CANCEL", "total_amount": 1, "buyer_user_id": 11},
            {"order_sn": "F", "order_status": "PROCESSED", "total_amount": 2, "currency": "SGD", "buyer_user_id": 7},
            {"order_sn": "G", "order_status": "COMPLETED", "total_amount": 1, "currency": "SGD"}
        ]}})),
    );
    script.push(
        "/api/v2/returns/get_return_list",
        Step::Json(json!({"error": "error_permission", "message": "no permission"})),
    );
    script.push(
        "/api/v2/product/get_item_list",
        Step::Json(json!({"error": "", "response": {"total_count": 4, "has_next_page": false, "item": [{}]}})),
    );
    let metrics = service::metrics(&runtime, "shop-1", MetricRange::Today).await.expect("metrics");
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&4.0));
    assert_eq!(metrics.values.get(&MetricKey::Gmv), Some(&17.0));
    assert_eq!(metrics.values.get(&MetricKey::PendingShipment), Some(&1.0));
    assert_eq!(metrics.values.get(&MetricKey::ProductsLive), Some(&4.0));
    assert_eq!(metrics.currency.as_deref(), Some("SGD"));
    assert!(metrics.error.is_none());
    assert!(metrics.unsupported.contains(&MetricKey::Buyers));
    assert!(metrics.unsupported.contains(&MetricKey::RefundAmount));
    assert!(metrics.unsupported.contains(&MetricKey::RefundOrders));
    assert_eq!(metrics.values.get(&MetricKey::Buyers), Some(&0.0));
    let detail = script.calls().into_iter().find(|call| call.path.ends_with("get_order_detail")).unwrap();
    assert_eq!(detail.query_value("response_optional_fields"), Some("buyer_user_id,total_amount,item_list"));
}

#[tokio::test]
async fn last30_splits_order_queries_on_the_fifteen_day_cap() {
    let (_dir, runtime, script) = harness(Platform::Shopee);
    for _ in 0..2 {
        script.push(
            "/api/v2/order/get_order_list",
            Step::Json(json!({"error": "", "response": {"more": false, "order_list": []}})),
        );
    }
    script.push(
        "/api/v2/returns/get_return_list",
        Step::Json(json!({"error": "", "response": {"more": false, "return": [
            {"return_sn": "R1", "status": "ACCEPTED", "refund_amount": 3},
            {"return_sn": "R2", "status": "CANCELLED", "refund_amount": 9},
            {"return_sn": "R3", "status": "CLOSED", "refund_amount": 8}
        ]}})),
    );
    script.push("/api/v2/product/get_item_list", Step::Json(json!({"error": "", "response": {"total_count": 0}})));
    let metrics = service::metrics(&runtime, "shop-1", MetricRange::Last30).await.expect("metrics");
    let order_calls = script.calls().iter().filter(|call| call.path.ends_with("get_order_list")).count();
    assert_eq!(order_calls, 2);
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::RefundAmount), Some(&3.0));
    assert_eq!(metrics.values.get(&MetricKey::RefundOrders), Some(&1.0));
    assert!(metrics.unsupported.is_empty());
}

#[tokio::test]
async fn order_auth_failure_is_an_error_not_zeros() {
    let (_dir, runtime, script) = harness(Platform::Shopee);
    script.push(
        "/api/v2/order/get_order_list",
        Step::Json(json!({"error": "error_auth", "message": "Invalid access_token"})),
    );
    let error = service::metrics(&runtime, "shop-1", MetricRange::Last7).await.expect_err("auth");
    assert_eq!(error.exit_code(), 3);
    assert!(error.to_string().contains("Invalid access_token"));
    assert_eq!(script.calls().len(), 1);
}

#[tokio::test]
async fn missing_order_amount_marks_gmv_unsupported() {
    let (_dir, runtime, script) = harness(Platform::Shopee);
    script.push(
        "/api/v2/order/get_order_list",
        Step::Json(json!({"error": "", "response": {"more": false, "order_list": [
            {"order_sn": "A", "order_status": "READY_TO_SHIP"}
        ]}})),
    );
    script.push(
        "/api/v2/order/get_order_detail",
        Step::Json(json!({"error": "", "response": {"order_list": []}})),
    );
    script.push("/api/v2/returns/get_return_list", Step::Json(json!({"error": "", "response": {"more": false, "return": []}})));
    script.push("/api/v2/product/get_item_list", Step::Json(json!({"error": "", "response": {"item": []}})));
    let metrics = service::metrics(&runtime, "shop-1", MetricRange::Today).await.expect("metrics");
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&1.0));
    assert!(metrics.unsupported.contains(&MetricKey::Gmv));
    assert!(metrics.unsupported.contains(&MetricKey::Buyers));
    assert_eq!(metrics.values.get(&MetricKey::ProductsLive), Some(&0.0));
}

#[tokio::test]
async fn add_item_happy_path_records_reviewing_and_the_payload() {
    let (dir, runtime, script) = harness(Platform::Shopee);
    let image = image_file(dir.path());
    push_happy(&script);
    let records = service::submit(&runtime, draft(&image), vec![serde_json::from_value(target()).unwrap()], Some("group-1".into()))
        .await
        .expect("submit");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].status, ListingStatus::Reviewing);
    assert_eq!(records[0].remote_id.as_deref(), Some("99001"));
    assert_eq!(records[0].attempts, 1);
    assert_eq!(records[0].group_id, "group-1");
    let add = script.calls().into_iter().find(|call| call.path.ends_with("add_item")).expect("add");
    let body = add.json.expect("body");
    assert_eq!(body["category_id"], json!(100));
    assert_eq!(body["image"]["image_id_list"][0], "img-1");
    assert_eq!(body["logistic_info"][0]["logistic_id"], json!(80003));
    assert_eq!(body["logistic_info"][0]["enabled"], true);
    assert_eq!(body["item_name"], "Ceramic Cup");
    assert_eq!(body["brand"]["original_brand_name"], "NoBrand");
    assert_eq!(body["seller_stock"][0]["stock"], json!(4));
    assert_eq!(body["dimension"]["package_length"], json!(10));
    assert!((body["weight"].as_f64().unwrap() - 0.4).abs() < 0.001);
    let upload = script.calls().into_iter().find(|call| call.path.ends_with("upload_image")).unwrap();
    assert_eq!(upload.query_value("scene"), Some("normal"));
    assert_eq!(upload.file_name.as_deref(), Some("cup.jpg"));
}

#[tokio::test]
async fn platform_rejection_stores_a_rejected_record() {
    let (dir, runtime, script) = harness(Platform::Shopee);
    let image = image_file(dir.path());
    script.push(
        "/api/v2/logistics/get_channel_list",
        Step::Json(json!({"error": "", "response": {"logistics_channel_list": [{"logistics_channel_id": 80003, "enabled": true}]}})),
    );
    script.push("/api/v2/media_space/upload_image", Step::Json(json!({"error": "", "response": {"image_info": {"image_id": "img-1"}}})));
    script.push("/api/v2/product/add_item", Step::Json(json!({"error": "error_param", "message": "title invalid"})));
    let records = service::submit(&runtime, draft(&image), vec![serde_json::from_value(target()).unwrap()], None).await.expect("submit");
    assert_eq!(records[0].status, ListingStatus::Rejected);
    assert_eq!(records[0].reason.as_deref(), Some("error_param: title invalid"));
    assert!(records[0].remote_id.is_none());
}

#[tokio::test]
async fn transport_failure_after_add_item_is_uncertain_and_blocks_resubmit() {
    let (dir, runtime, script) = harness(Platform::Shopee);
    let image = image_file(dir.path());
    script.push(
        "/api/v2/logistics/get_channel_list",
        Step::Json(json!({"error": "", "response": {"logistics_channel_list": [{"logistics_channel_id": 80003, "enabled": true}]}})),
    );
    script.push("/api/v2/media_space/upload_image", Step::Json(json!({"error": "", "response": {"image_info": {"image_id": "img-1"}}})));
    script.push("/api/v2/product/add_item", Step::Fault(TransportFault::AfterDispatch("timed out".into())));
    let records = service::submit(&runtime, draft(&image), vec![serde_json::from_value(target()).unwrap()], Some("group-1".into()))
        .await
        .expect("submit");
    assert_eq!(records[0].status, ListingStatus::Uncertain);
    assert!(records[0].reason.as_deref().unwrap_or("").contains("timed out"));
    let before = script.calls().len();
    let error = service::resubmit(&runtime, &records[0].id, None, None).await.expect_err("blocked");
    assert_eq!(error.exit_code(), 5);
    assert_eq!(script.calls().len(), before);
}

#[tokio::test]
async fn connect_failure_before_add_item_is_failed_and_can_resubmit() {
    let (dir, runtime, script) = harness(Platform::Shopee);
    let image = image_file(dir.path());
    script.push(
        "/api/v2/logistics/get_channel_list",
        Step::Json(json!({"error": "", "response": {"logistics_channel_list": [{"logistics_channel_id": 80003, "enabled": true}]}})),
    );
    script.push("/api/v2/media_space/upload_image", Step::Json(json!({"error": "", "response": {"image_info": {"image_id": "img-1"}}})));
    script.push("/api/v2/product/add_item", Step::Fault(TransportFault::BeforeDispatch("connection refused".into())));
    let records = service::submit(&runtime, draft(&image), vec![serde_json::from_value(target()).unwrap()], None).await.expect("submit");
    assert_eq!(records[0].status, ListingStatus::Failed);
    push_happy(&script);
    let again = service::resubmit(&runtime, &records[0].id, None, None).await.expect("resubmit");
    assert_eq!(again.status, ListingStatus::Reviewing);
    assert_eq!(again.attempts, 2);
    assert_eq!(again.id, records[0].id);
}

#[tokio::test]
async fn duplicate_guard_refuses_a_second_submit_for_the_same_shop() {
    let (dir, runtime, script) = harness(Platform::Shopee);
    let image = image_file(dir.path());
    push_happy(&script);
    let records = service::submit(&runtime, draft(&image), vec![serde_json::from_value(target()).unwrap()], Some("group-1".into()))
        .await
        .expect("first");
    assert_eq!(records[0].status, ListingStatus::Reviewing);
    let before = script.calls().len();
    let error = service::submit(&runtime, draft(&image), vec![serde_json::from_value(target()).unwrap()], Some("group-1".into()))
        .await
        .expect_err("duplicate");
    assert_eq!(error.exit_code(), 2);
    assert!(error.to_string().contains("已有"));
    assert_eq!(script.calls().len(), before);
    let live_dup = service::resubmit(&runtime, &records[0].id, None, None).await.expect_err("not rejected");
    assert_eq!(live_dup.exit_code(), 2);
}

#[tokio::test]
async fn rejected_record_resubmits_in_place_and_refresh_maps_normal_to_live() {
    let (dir, runtime, script) = harness(Platform::Shopee);
    let image = image_file(dir.path());
    script.push(
        "/api/v2/logistics/get_channel_list",
        Step::Json(json!({"error": "", "response": {"logistics_channel_list": [{"logistics_channel_id": 80003, "enabled": true}]}})),
    );
    script.push("/api/v2/media_space/upload_image", Step::Json(json!({"error": "", "response": {"image_info": {"image_id": "img-1"}}})));
    script.push("/api/v2/product/add_item", Step::Json(json!({"error": "error_param", "message": "category"})));
    let records = service::submit(&runtime, draft(&image), vec![serde_json::from_value(target()).unwrap()], Some("group-1".into()))
        .await
        .expect("submit");
    assert_eq!(records[0].status, ListingStatus::Rejected);
    let blocked = service::submit(&runtime, draft(&image), vec![serde_json::from_value(target()).unwrap()], Some("group-1".into()))
        .await
        .expect_err("use resubmit");
    assert_eq!(blocked.exit_code(), 2);
    push_happy(&script);
    let updated = service::resubmit(&runtime, &records[0].id, None, None).await.expect("resubmit");
    assert_eq!(updated.status, ListingStatus::Reviewing);
    script.push(
        "/api/v2/product/get_item_base_info",
        Step::Json(json!({"error": "", "response": {"item_list": [{"item_id": 99001, "item_status": "NORMAL"}]}})),
    );
    let live = service::refresh(&runtime, &updated.id).await.expect("refresh");
    assert_eq!(live.status, ListingStatus::Live);
    assert!(live.reason.is_none());
}

#[tokio::test]
async fn wired_tiktok_shop_does_not_call_shopee() {
    let (_dir, runtime, script) = harness(Platform::Tiktok);
    let error = service::metrics(&runtime, "shop-1", MetricRange::Today).await.expect_err("empty script");
    assert_ne!(error.exit_code(), 2);
    assert!(script.calls().iter().all(|call| !call.path.contains("/api/v2/")));
}

#[test]
fn cli_parses_the_contract_commands() {
    let metrics = parse("metrics", &["shop-1".into(), "--range".into(), "last7".into()]).unwrap();
    assert_eq!(metrics["action"], "metrics");
    assert_eq!(metrics["range"], "last7");
    let attributes = parse("attributes", &["shop-1".into(), "100".into()]).unwrap();
    assert_eq!(attributes["categoryId"], "100");
    assert!(parse("status", &[]).is_err());
    assert!(parse("shops", &["extra".into()]).is_err());
}

#[test]
fn approval_is_only_required_for_submit_and_resubmit() {
    assert!(super::action_requires_approval(&json!({"action": "submit"})));
    assert!(super::action_requires_approval(&json!({"action": "resubmit"})));
    assert!(!super::action_requires_approval(&json!({"action": "metrics"})));
    assert!(!super::action_requires_approval(&json!({"action": "status"})));
}

#[test]
fn exit_codes_follow_the_contract() {
    use super::CommerceError;
    assert_eq!(CommerceError::invalid("x").exit_code(), 2);
    assert_eq!(CommerceError::unsupported("x").exit_code(), 2);
    assert_eq!(CommerceError::duplicate("x").exit_code(), 2);
    assert_eq!(CommerceError::not_found("x").exit_code(), 2);
    assert_eq!(CommerceError::rejected("x").exit_code(), 3);
    assert_eq!(CommerceError::failed("x").exit_code(), 4);
    assert_eq!(CommerceError::uncertain("x").exit_code(), 5);
    assert_eq!(CommerceError::internal("x").exit_code(), 1);
    assert_eq!(CommerceError::rejected("title invalid").to_string(), "title invalid");
}
