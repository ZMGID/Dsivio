use super::super::transport::{Outbound, ResolvedShop, ScriptTransport, Step, Transport, TransportFault};
use super::super::types::{
    AttributeInput, CommerceError, ListingDraft, ListingStatus, ListingTarget, MetricKey, MetricRange,
};
use super::{
    fetch_categories, fetch_metrics, fetch_orders, fetch_products, map_attributes, map_categories, map_product_status,
    push_listing, refresh_remote, sign_request,
};
use crate::workbench::shops::Platform;
use serde_json::{json, Value};
use std::sync::Arc;

const NOW: i64 = 1_711_929_600;
const APP_KEY: &str = "38abcd";
const SECRET: &str = "app-secret";
const SHOP_ID: &str = "7000714532876273420";
const CIPHER: &str = "GCP_XF90igAAAABh00qsWgtvOiGFNqyubMt3";
const REQUEST_ID: &str = "202203070749000101890810281E8C70B7";

fn shop(region: &str) -> ResolvedShop {
    ResolvedShop {
        platform: Platform::Tiktok,
        partner_id: APP_KEY.into(),
        partner_key: SECRET.into(),
        access_token: "act.example_access_token".into(),
        remote_id: SHOP_ID.into(),
        region: Some(region.into()),
        name: "Maomao beauty shop".into(),
    }
}

fn scripted() -> (Arc<ScriptTransport>, Transport) {
    let script = Arc::new(ScriptTransport::new());
    let transport = Transport::script(script.clone());
    (script, transport)
}

fn query<'a>(call: &'a Outbound, key: &str) -> Option<&'a str> {
    call.query.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str())
}

fn paths(script: &ScriptTransport) -> Vec<String> {
    script.calls().iter().map(|call| call.path.clone()).collect()
}

fn ok(data: Value) -> Value {
    json!({"code": 0, "data": data, "message": "Success", "request_id": REQUEST_ID})
}

fn shops_body() -> Value {
    ok(json!({"shops": [{
        "id": SHOP_ID,
        "name": "Maomao beauty shop",
        "region": "GB",
        "seller_type": "CROSS_BORDER",
        "cipher": CIPHER,
        "code": "CNGBCBA4LLU8"
    }]}))
}

fn queue_shops(script: &ScriptTransport) {
    script.push("/authorization/202309/shops", Step::Json(shops_body()));
}

fn draft(skus: Value, dimensions: bool) -> ListingDraft {
    let mut value = json!({
        "title": "Cotton ankle socks",
        "description": "Soft cotton socks",
        "price": 12.5,
        "currency": "GBP",
        "stock": 5,
        "skus": skus,
        "images": ["sock.jpg"],
        "weightKg": 0.2
    });
    if dimensions {
        value["dimensionsCm"] = json!({"l": 10.0, "w": 8.0, "h": 3.0});
    }
    serde_json::from_value(value).expect("draft")
}

fn target() -> ListingTarget {
    serde_json::from_value(json!({"shopId": "shop-1", "categoryId": "600001", "attributes": []})).expect("target")
}

fn images() -> Vec<(String, Vec<u8>)> {
    vec![("sock.jpg".into(), vec![1, 2, 3])]
}

fn rules(dimensions_required: bool) -> Value {
    ok(json!({
        "cod": {"is_supported": true},
        "package_dimension": {"is_required": dimensions_required},
        "size_chart": {"is_required": false, "is_supported": true}
    }))
}

fn attributes_body(required: bool) -> Value {
    ok(json!({"attributes": [{
        "id": "100392",
        "name": "Occasion",
        "type": "PRODUCT_PROPERTY",
        "is_requried": required,
        "values": [{"id": "1001533", "name": "Birthday"}],
        "is_customizable": true,
        "is_multiple_selection": false
    }]}))
}

fn warehouses_body() -> Value {
    ok(json!({"warehouses": [{
        "id": "7000714532876273410",
        "name": "Guangzhou",
        "effect_status": "ENABLED",
        "type": "SALES_WAREHOUSE",
        "sub_type": "DOMESTIC_WAREHOUSE",
        "is_default": true
    }]}))
}

fn upload_body() -> Value {
    ok(json!({
        "uri": "tos-maliva-i-o3syd03w52-us/c668cdf70b7f483c94dbe",
        "url": "https://p-oec-va.ibyteimg.com/tos-maliva-i-o3syd03w52-us/c668cdf70b7f483c94dbe",
        "height": 720,
        "width": 720,
        "use_case": "MAIN_IMAGE"
    }))
}

fn created_body() -> Value {
    ok(json!({
        "product_id": "1729592969712207008",
        "skus": [{"id": "1729592969712207012", "seller_sku": "RED-1", "sales_attributes": [{"id": "100000", "value_id": "1729592969712207123"}]}]
    }))
}

fn queue_prefix(script: &ScriptTransport, dimensions_required: bool, attribute_required: bool) {
    queue_shops(script);
    script.push("/product/202309/categories/600001/rules", Step::Json(rules(dimensions_required)));
    script.push("/product/202309/categories/600001/attributes", Step::Json(attributes_body(attribute_required)));
    script.push("/logistics/202309/warehouses", Step::Json(warehouses_body()));
}

#[test]
fn sign_request_matches_official_hmac() {
    let shops = sign_request(
        SECRET,
        "/authorization/202309/shops",
        &[("app_key".into(), "app-key".into()), ("timestamp".into(), "1711929600".into())],
        None,
    )
    .expect("sign");
    assert_eq!(shops, "3c83382abff88dc68360eb1c7edfa19904878b28c1da5089fb1598d83a99b8a6");

    let body = r#"{"description":"<p>Cotton socks</p>","category_id":"600001"}"#;
    let posted = sign_request(
        SECRET,
        "/product/202309/products",
        &[
            ("timestamp".into(), "1711929600".into()),
            ("shop_cipher".into(), "GCP_cipher".into()),
            ("app_key".into(), "app-key".into()),
            ("sign".into(), "ignored".into()),
            ("access_token".into(), "ignored".into()),
        ],
        Some(body),
    )
    .expect("sign");
    assert_eq!(posted, "eeb004427981debaed3ca2e7deb6837085aafd58df26edc4bec077ee40578133");
}

#[tokio::test]
async fn categories_replay_official_sample_and_sign_shop_cipher() {
    let (script, transport) = scripted();
    queue_shops(&script);
    script.push(
        "/product/202309/categories",
        Step::Json(ok(json!({"categories": [{
            "id": "600002",
            "parent_id": "600001",
            "local_name": "Home Supplies",
            "is_leaf": false,
            "permission_statuses": ["INVITE_ONLY", "NON_MAIN_CATEGORY"]
        }]}))),
    );
    let categories = fetch_categories(&transport, &shop("GB"), NOW, Some("600001")).await.expect("categories");
    assert_eq!(categories.len(), 1);
    assert_eq!(categories[0].id, "600002");
    assert_eq!(categories[0].name, "Home Supplies");
    assert_eq!(categories[0].parent_id, "600001");
    assert!(!categories[0].leaf);

    let shops_call = script.calls().into_iter().find(|call| call.path == "/authorization/202309/shops").expect("shops");
    assert_eq!(shops_call.host, "https://open-api.tiktokglobalshop.com");
    assert_eq!(query(&shops_call, "sign"), Some("1008c5e7170346450a9622c9cfe18386d10e9a925b205d1509e820d3d1c8c6d4"));
    assert!(shops_call.headers.iter().any(|(name, value)| name == "x-tts-access-token" && value == "act.example_access_token"));
    assert!(query(&shops_call, "shop_cipher").is_none());

    let category_call = script.calls().into_iter().find(|call| call.path == "/product/202309/categories").expect("categories call");
    assert_eq!(query(&category_call, "shop_cipher"), Some(CIPHER));
    assert_eq!(query(&category_call, "category_version"), Some("v1"));
    assert_eq!(query(&category_call, "locale"), Some("en-US"));
}

#[tokio::test]
async fn us_categories_use_v2() {
    let (script, transport) = scripted();
    queue_shops(&script);
    script.push("/product/202309/categories", Step::Json(ok(json!({"categories": []}))));
    fetch_categories(&transport, &shop("US"), NOW, None).await.expect("categories");
    let call = script.calls().into_iter().find(|call| call.path == "/product/202309/categories").expect("call");
    assert_eq!(query(&call, "category_version"), Some("v2"));
}

#[test]
fn attributes_keep_documented_required_flag_and_inputs() {
    let mapped = map_attributes(&attributes_body(true));
    assert_eq!(mapped.len(), 1);
    assert!(mapped[0].required);
    assert_eq!(mapped[0].input, AttributeInput::Select);
    assert_eq!(mapped[0].options[0].id, "1001533");
    assert_eq!(mapped[0].options[0].name, "Birthday");

    let numeric = map_attributes(&ok(json!({"attributes": [{
        "id": "200001",
        "name": "Count",
        "type": "PRODUCT_PROPERTY",
        "is_requried": true,
        "value_data_format": "POSITIVE_INT_OR_DECIMAL",
        "is_customizable": true,
        "is_multiple_selection": false
    }]})));
    assert_eq!(numeric[0].input, AttributeInput::Number);

    let sample = map_categories(&ok(json!({"categories": [{
        "id": "600002", "parent_id": "600001", "local_name": "Home Supplies", "is_leaf": false
    }]})));
    assert_eq!(sample[0].name, "Home Supplies");
}

#[tokio::test]
async fn publish_uploads_image_then_creates_skus() {
    let (script, transport) = scripted();
    queue_prefix(&script, false, false);
    script.push("/product/202309/images/upload", Step::Json(upload_body()));
    script.push("/product/202309/products", Step::Json(created_body()));
    let skus = json!([
        {"name": "Red", "price": 12.5, "stock": 5, "code": "RED-1"},
        {"name": "Blue", "price": 13.0, "stock": 4, "code": "BLUE-1"}
    ]);
    let outcome = push_listing(&transport, &shop("GB"), &draft(skus, true), &target(), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Reviewing);
    assert_eq!(outcome.remote_id.as_deref(), Some("1729592969712207008"));
    assert!(outcome.reason.is_none());

    let upload = script.calls().into_iter().find(|call| call.path == "/product/202309/images/upload").expect("upload");
    assert!(query(&upload, "shop_cipher").is_none());
    assert_eq!(upload.file_bytes.as_deref(), Some(&[1, 2, 3][..]));
    assert_eq!(upload.json.as_ref().and_then(|body| body.get("use_case")).and_then(Value::as_str), Some("MAIN_IMAGE"));
    assert!(upload.headers.iter().any(|(name, value)| name == "content-type" && value == "multipart/form-data"));
    assert!(upload.headers.iter().any(|(name, value)| name == "x-dsivio-file-field" && value == "data"));

    let create = script.calls().into_iter().find(|call| call.path == "/product/202309/products").expect("create");
    assert_eq!(query(&create, "shop_cipher"), Some(CIPHER));
    assert!(create.mutating);
    let body = create.json.expect("body");
    assert_eq!(body["save_mode"], "LISTING");
    assert_eq!(body["category_version"], "v1");
    assert_eq!(body["description"], "<p>Soft cotton socks</p>");
    assert_eq!(body["main_images"][0]["uri"], "tos-maliva-i-o3syd03w52-us/c668cdf70b7f483c94dbe");
    assert_eq!(body["package_weight"]["unit"], "KILOGRAM");
    assert_eq!(body["package_dimensions"]["unit"], "CENTIMETER");
    assert_eq!(body["skus"].as_array().map(Vec::len), Some(2));
    assert_eq!(body["skus"][0]["seller_sku"], "RED-1");
    assert_eq!(body["skus"][0]["inventory"][0]["warehouse_id"], "7000714532876273410");
    assert_eq!(body["skus"][0]["inventory"][0]["quantity"], json!(5));
    assert_eq!(body["skus"][0]["sales_attributes"][0]["name"], "Specification");
    assert_eq!(body["skus"][0]["sales_attributes"][0]["value_name"], "Red");
    assert_eq!(body["skus"][1]["sales_attributes"][0]["value_name"], "Blue");
    assert_eq!(body["skus"][0]["price"]["currency"], "GBP");
}

#[tokio::test]
async fn rejects_invalid_draft_before_any_call() {
    let (script, transport) = scripted();
    let mut broken = draft(json!([]), true);
    broken.title.clear();
    let outcome = push_listing(&transport, &shop("GB"), &broken, &target(), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.remote_id.is_none());
    assert!(script.calls().is_empty());
}

#[tokio::test]
async fn rejects_required_attribute_before_upload() {
    let (script, transport) = scripted();
    queue_prefix(&script, false, true);
    let outcome = push_listing(&transport, &shop("GB"), &draft(json!([]), true), &target(), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.reason.unwrap_or_default().contains("Occasion"));
    let called = paths(&script);
    assert!(called.iter().any(|path| path.ends_with("/attributes")));
    assert!(!called.iter().any(|path| path.ends_with("/images/upload") || path == "/product/202309/products"));
}

#[tokio::test]
async fn rejects_missing_dimensions_before_creation() {
    let (script, transport) = scripted();
    queue_shops(&script);
    script.push("/product/202309/categories/600001/rules", Step::Json(rules(true)));
    let outcome = push_listing(&transport, &shop("GB"), &draft(json!([]), false), &target(), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.reason.unwrap_or_default().contains("包裹"));
    assert!(!paths(&script).iter().any(|path| path == "/product/202309/products" || path.ends_with("/images/upload")));
}

#[tokio::test]
async fn upload_failure_does_not_create_product() {
    let (script, transport) = scripted();
    queue_prefix(&script, false, false);
    script.push(
        "/product/202309/images/upload",
        Step::Json(json!({
            "code": 36009021,
            "message": "Invalid file size. The uploaded file size exceeds the maximum limit.",
            "request_id": REQUEST_ID
        })),
    );
    let outcome = push_listing(&transport, &shop("GB"), &draft(json!([]), true), &target(), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.remote_id.is_none());
    assert!(!paths(&script).iter().any(|path| path == "/product/202309/products"));
}

#[tokio::test]
async fn create_before_dispatch_is_rejected() {
    let (script, transport) = scripted();
    queue_prefix(&script, false, false);
    script.push("/product/202309/images/upload", Step::Json(upload_body()));
    script.push("/product/202309/products", Step::Fault(TransportFault::BeforeDispatch("dns".into())));
    let outcome = push_listing(&transport, &shop("GB"), &draft(json!([]), true), &target(), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert_ne!(outcome.status, ListingStatus::Uncertain);
    assert_eq!(paths(&script).iter().filter(|path| path.as_str() == "/product/202309/products").count(), 1);
}

#[tokio::test]
async fn create_after_dispatch_is_uncertain_and_not_retried() {
    let (script, transport) = scripted();
    queue_prefix(&script, false, false);
    script.push("/product/202309/images/upload", Step::Json(upload_body()));
    script.push("/product/202309/products", Step::Fault(TransportFault::AfterDispatch("connection reset".into())));
    let outcome = push_listing(&transport, &shop("GB"), &draft(json!([]), true), &target(), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Uncertain);
    assert!(outcome.remote_id.is_none());
    assert_eq!(paths(&script).iter().filter(|path| path.as_str() == "/product/202309/products").count(), 1);
}

#[tokio::test]
async fn refresh_maps_documented_product_statuses() {
    let cases = [
        ("ACTIVATE", None, ListingStatus::Live, None),
        ("PENDING", None, ListingStatus::Reviewing, None),
        ("FAILED", Some("violate listing rules"), ListingStatus::Rejected, Some("violate listing rules")),
        ("FREEZE", None, ListingStatus::Banned, Some("FREEZE")),
        ("PLATFORM_DEACTIVATED", None, ListingStatus::Banned, Some("PLATFORM_DEACTIVATED")),
        ("SELLER_DEACTIVATED", None, ListingStatus::Live, Some("SELLER_DEACTIVATED")),
    ];
    for (status, reason, expected, expected_reason) in cases {
        let (script, transport) = scripted();
        queue_shops(&script);
        let mut data = json!({"id": "1729592969712207008", "status": status});
        if let Some(reason) = reason {
            data["audit"] = json!({"status": "FAILED"});
            data["audit_failed_reasons"] = json!([{"reasons": [reason]}]);
        }
        script.push("/product/202309/products/1729592969712207008", Step::Json(ok(data)));
        let (mapped, message) = refresh_remote(&transport, &shop("GB"), NOW, "1729592969712207008").await.expect(status);
        assert_eq!(mapped, expected, "{status}");
        assert_eq!(message.as_deref(), expected_reason, "{status}");
    }
    assert_eq!(map_product_status("DELETED", None, None).0, ListingStatus::Rejected);
}

#[tokio::test]
async fn existing_remote_only_queries_status() {
    let (script, transport) = scripted();
    queue_shops(&script);
    script.push(
        "/product/202309/products/1729592969712207008",
        Step::Json(ok(json!({"id": "1729592969712207008", "status": "ACTIVATE"}))),
    );
    let outcome = push_listing(
        &transport,
        &shop("GB"),
        &draft(json!([]), true),
        &target(),
        &images(),
        NOW,
        Some("1729592969712207008"),
    )
    .await;
    assert_eq!(outcome.status, ListingStatus::Live);
    assert_eq!(outcome.remote_id.as_deref(), Some("1729592969712207008"));
    assert!(!paths(&script).iter().any(|path| path == "/product/202309/products" || path.ends_with("/images/upload")));
}

#[tokio::test]
async fn products_follow_the_next_page_token() {
    let (script, transport) = scripted();
    queue_shops(&script);
    script.push(
        "/product/202502/products/search",
        Step::Json(ok(json!({
            "total_count": 2,
            "next_page_token": "b2Zmc2V0PTAK",
            "products": [{
                "id": "1729592969712207008",
                "title": "Short Boat Invisible Socks",
                "status": "ACTIVATE",
                "skus": [{
                    "id": "1729592969712207012",
                    "seller_sku": "Color-Red-XM01",
                    "price": {"currency": "USD", "tax_exclusive_price": "111.01", "sale_price": "121.11"},
                    "inventory": [{"warehouse_id": "7068517275539719942", "quantity": 999}]
                }]
            }]
        }))),
    );
    script.push(
        "/product/202502/products/search",
        Step::Json(ok(json!({
            "total_count": 2,
            "products": [{"id": "1729592969712207009", "title": "Second", "status": "PENDING", "skus": []}]
        }))),
    );
    let first = fetch_products(&transport, &shop("GB"), "shop-1", None, NOW).await.expect("page");
    assert_eq!(first.shop_id, "shop-1");
    assert_eq!(first.next_cursor.as_deref(), Some("b2Zmc2V0PTAK"));
    assert_eq!(first.items[0].status, "ACTIVATE");
    assert_eq!(first.items[0].price, "121.11".parse().ok());
    assert_eq!(first.items[0].currency.as_deref(), Some("USD"));
    assert_eq!(first.items[0].stock, Some(999));
    assert_eq!(first.items[0].sku.as_deref(), Some("Color-Red-XM01"));

    queue_shops(&script);
    let second = fetch_products(&transport, &shop("GB"), "shop-1", first.next_cursor.as_deref(), NOW).await.expect("page 2");
    assert!(second.next_cursor.is_none());
    assert_eq!(second.items[0].status, "PENDING");
    assert!(second.items[0].price.is_none());
    assert!(second.items[0].stock.is_none());
    let searches: Vec<_> = script.calls().into_iter().filter(|call| call.path == "/product/202502/products/search").collect();
    assert!(query(&searches[0], "page_token").is_none());
    assert_eq!(query(&searches[1], "page_token"), Some("b2Zmc2V0PTAK"));
    assert_eq!(query(&searches[0], "page_size"), Some("20"));
}

#[tokio::test]
async fn orders_follow_the_next_page_token() {
    let (script, transport) = scripted();
    let token = "6AsPQsUMvH3RkchNUPPh22NROHkE0D8pmq/N5M1kHYcZmtRyv9aVrNv65W7Q6tFA+7D1ud64MPNz5OaT";
    queue_shops(&script);
    script.push(
        "/order/202309/orders/search",
        Step::Json(ok(json!({
            "next_page_token": token,
            "total_count": 22113,
            "orders": [{
                "id": "576461413038785752",
                "status": "UNPAID",
                "create_time": 1619611561,
                "user_id": "7213489962827123654",
                "payment": {"currency": "IDR", "total_amount": "5000"},
                "line_items": [{"product_name": "Women's Winter Crochet Clothes", "seller_sku": "red_iphone_case"}]
            }]
        }))),
    );
    script.push(
        "/order/202309/orders/search",
        Step::Json(ok(json!({
            "total_count": 22113,
            "orders": [{"id": "576461413038785753", "status": "AWAITING_SHIPMENT", "payment": {"currency": "IDR", "total_amount": "0"}}]
        }))),
    );
    let first = fetch_orders(&transport, &shop("GB"), "shop-1", MetricRange::Today, None, NOW).await.expect("orders");
    assert_eq!(first.shop_id, "shop-1");
    assert_eq!(first.range, MetricRange::Today);
    assert_eq!(first.items[0].id, "576461413038785752");
    assert_eq!(first.items[0].status, "UNPAID");
    assert_eq!(first.items[0].amount, "5000".parse().ok());
    assert_eq!(first.items[0].currency.as_deref(), Some("IDR"));
    assert_eq!(first.items[0].buyer.as_deref(), Some("7213489962827123654"));
    assert_eq!(first.items[0].created_at.as_deref(), Some("2021-04-28T12:06:01+00:00"));
    assert_eq!(first.items[0].lines[0].title, "Women's Winter Crochet Clothes");
    assert_eq!(first.items[0].lines[0].sku.as_deref(), Some("red_iphone_case"));
    assert_eq!(first.items[0].lines[0].quantity, 1);
    queue_shops(&script);
    let second = fetch_orders(&transport, &shop("GB"), "shop-1", MetricRange::Today, first.next_cursor.as_deref(), NOW)
        .await
        .expect("page 2");
    assert_eq!(second.items[0].amount, Some(0.0));
    let searches: Vec<_> = script.calls().into_iter().filter(|call| call.path == "/order/202309/orders/search").collect();
    assert_eq!(query(&searches[1], "page_token"), Some(token));
    assert_eq!(query(&searches[0], "page_size"), Some("20"));
    let body = searches[0].json.as_ref().expect("window");
    assert_eq!(body["create_time_ge"], json!(1_711_929_600_i64));
    assert_eq!(body["create_time_lt"], json!(1_712_016_000_i64));
}

fn queue_metric_tails(script: &ScriptTransport, live: i64, pending: i64) {
    script.push("/product/202502/products/search", Step::Json(ok(json!({"total_count": live, "products": []}))));
    script.push("/order/202309/orders/search", Step::Json(ok(json!({"total_count": pending, "orders": []}))));
}

#[tokio::test]
async fn metrics_use_shop_performance_windows_and_mark_refund_orders_unsupported() {
    let (script, transport) = scripted();
    queue_shops(&script);
    script.push(
        "/analytics/202609/shop/performance",
        Step::Json(ok(json!({"performance": {"intervals": [{"sales": {
            "gmv": {"amount": "29.71", "currency": "USD"},
            "orders_count": 13,
            "customers_count": 7,
            "refunds": {"amount": "0.00", "currency": "USD"},
            "cancellations_and_returns": 4
        }}]}}))),
    );
    queue_metric_tails(&script, 8, 2);
    let metrics = fetch_metrics(&transport, &shop("GB"), "shop-1", MetricRange::Today, NOW).await.expect("metrics");
    assert_eq!(metrics.values.get(&MetricKey::Gmv).copied(), "29.71".parse().ok());
    assert_eq!(metrics.values.get(&MetricKey::Orders).copied(), Some(13.0));
    assert_eq!(metrics.values.get(&MetricKey::RefundAmount).copied(), "0.00".parse().ok());
    assert_eq!(metrics.values.get(&MetricKey::Buyers).copied(), Some(7.0));
    assert_eq!(metrics.values.get(&MetricKey::ProductsLive).copied(), Some(8.0));
    assert_eq!(metrics.values.get(&MetricKey::PendingShipment).copied(), Some(2.0));
    assert_eq!(metrics.unsupported, vec![MetricKey::RefundOrders]);
    assert_eq!(metrics.currency.as_deref(), Some("USD"));
    assert!(metrics.error.is_none());
    let performance = script.calls().into_iter().find(|call| call.path == "/analytics/202609/shop/performance").expect("performance");
    assert_eq!(query(&performance, "start_date_ge"), Some("2024-04-01"));
    assert_eq!(query(&performance, "end_date_lt"), Some("2024-04-02"));
    assert_eq!(query(&performance, "granularity"), Some("ALL"));
    assert_eq!(query(&performance, "currency"), Some("LOCAL"));
}

#[tokio::test]
async fn last7_window_is_six_days_before_today_through_tomorrow() {
    let (script, transport) = scripted();
    queue_shops(&script);
    script.push(
        "/analytics/202609/shop/performance",
        Step::Json(ok(json!({"performance": {"intervals": [{"sales": {"orders_count": 0, "gmv": {"amount": "0.00", "currency": "GBP"}, "customers_count": 0, "refunds": {"amount": "0.00", "currency": "GBP"}}}]}}))),
    );
    queue_metric_tails(&script, 0, 0);
    let metrics = fetch_metrics(&transport, &shop("GB"), "shop-1", MetricRange::Last7, NOW).await.expect("metrics");
    assert_eq!(metrics.values.get(&MetricKey::Orders).copied(), Some(0.0));
    assert_eq!(metrics.values.get(&MetricKey::Gmv).copied(), Some(0.0));
    let performance = script.calls().into_iter().find(|call| call.path == "/analytics/202609/shop/performance").expect("performance");
    assert_eq!(query(&performance, "start_date_ge"), Some("2024-03-26"));
    assert_eq!(query(&performance, "end_date_lt"), Some("2024-04-02"));
}

#[tokio::test]
async fn absent_metric_fields_are_omitted_not_zero() {
    let (script, transport) = scripted();
    queue_shops(&script);
    script.push(
        "/analytics/202609/shop/performance",
        Step::Json(ok(json!({"performance": {"intervals": [{"sales": {"orders_count": 0, "gmv": {"amount": "29.71", "currency": "USD"}}}]}}))),
    );
    queue_metric_tails(&script, 1, 0);
    let metrics = fetch_metrics(&transport, &shop("GB"), "shop-1", MetricRange::Yesterday, NOW).await.expect("metrics");
    assert!(metrics.values.get(&MetricKey::Buyers).is_none());
    assert!(metrics.values.get(&MetricKey::RefundAmount).is_none());
    assert!(metrics.unsupported.contains(&MetricKey::Buyers));
    assert!(metrics.unsupported.contains(&MetricKey::RefundAmount));
    assert!(metrics.unsupported.contains(&MetricKey::RefundOrders));
    assert_eq!(metrics.values.get(&MetricKey::Orders).copied(), Some(0.0));
    let performance = script.calls().into_iter().find(|call| call.path == "/analytics/202609/shop/performance").expect("performance");
    assert_eq!(query(&performance, "start_date_ge"), Some("2024-03-31"));
    assert_eq!(query(&performance, "end_date_lt"), Some("2024-04-01"));
}

#[tokio::test]
async fn mixed_gmv_currencies_are_not_summed() {
    let (script, transport) = scripted();
    queue_shops(&script);
    script.push(
        "/analytics/202609/shop/performance",
        Step::Json(ok(json!({"performance": {"intervals": [
            {"sales": {"orders_count": 2, "gmv": {"amount": "10.00", "currency": "USD"}}},
            {"sales": {"orders_count": 3, "gmv": {"amount": "5.00", "currency": "EUR"}}}
        ]}}))),
    );
    queue_metric_tails(&script, 1, 1);
    let metrics = fetch_metrics(&transport, &shop("GB"), "shop-1", MetricRange::Last30, NOW).await.expect("metrics");
    assert_eq!(metrics.values.get(&MetricKey::Orders).copied(), Some(5.0));
    assert!(metrics.values.get(&MetricKey::Gmv).is_none());
    assert!(metrics.unsupported.contains(&MetricKey::Gmv));
    let performance = script.calls().into_iter().find(|call| call.path == "/analytics/202609/shop/performance").expect("performance");
    assert_eq!(query(&performance, "start_date_ge"), Some("2024-03-03"));
    assert_eq!(query(&performance, "end_date_lt"), Some("2024-04-02"));
}

#[tokio::test]
async fn expired_token_rejects_metrics_instead_of_zeros() {
    let (script, transport) = scripted();
    queue_shops(&script);
    script.push(
        "/analytics/202609/shop/performance",
        Step::Json(json!({
            "code": 105002,
            "message": "Expired credentials. The 'access_token' or 'x-tts-access-token' header has expired.",
            "request_id": REQUEST_ID
        })),
    );
    let error = fetch_metrics(&transport, &shop("GB"), "shop-1", MetricRange::Today, NOW).await.expect_err("auth");
    assert!(matches!(error, CommerceError::Rejected(_)));
    assert!(error.to_string().contains("105002"));
}

#[tokio::test]
async fn category_query_failure_is_returned() {
    let (script, transport) = scripted();
    queue_shops(&script);
    script.push(
        "/product/202309/categories",
        Step::Json(json!({"code": 12052023, "message": "Category does not exist", "request_id": REQUEST_ID})),
    );
    let error = fetch_categories(&transport, &shop("GB"), NOW, None).await.expect_err("query");
    assert!(matches!(error, CommerceError::Rejected(_)));
    assert!(error.to_string().contains("12052023"));
}

#[tokio::test]
async fn other_platforms_are_unsupported_without_calls() {
    let (script, transport) = scripted();
    let mut other = shop("GB");
    other.platform = Platform::Shopee;
    let error = fetch_products(&transport, &other, "shop-1", None, NOW).await.expect_err("platform");
    assert!(matches!(error, CommerceError::Unsupported(_)));
    let outcome = push_listing(&transport, &other, &draft(json!([]), true), &target(), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(script.calls().is_empty());
}

