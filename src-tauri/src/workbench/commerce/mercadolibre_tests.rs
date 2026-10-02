use super::adapter::{ApiSession, HttpBody, HttpRequest, ScriptHttp, ScriptStep, SharedTransport, TransportFault};
use super::mercadolibre::{self, format_from, format_to, metric_bounds, site_offset_secs};
use super::types::{
    AttributeInput, CommerceError, DimensionsCm, ListingDraft, ListingSku, ListingStatus, ListingTarget, MetricKey,
    MetricRange,
};
use crate::workbench::shops::Platform;
use serde_json::{json, Value};
use std::sync::Arc;

const NOW: i64 = 1_790_899_200;

fn shop(region: &str) -> ApiSession {
    ApiSession {
        shop_id: "shop-1".into(),
        platform: Platform::Mercadolibre,
        remote_id: "789".into(),
        name: "demo".into(),
        region: Some(region.into()),
        app_id: "app".into(),
        app_secret: "secret".into(),
        access_token: "token-abc".into(),
        refresh_token: String::new(),
        open_key: String::new(),
        seller_secret: String::new(),
    }
}

fn scripted(script: &Arc<ScriptHttp>) -> SharedTransport {
    SharedTransport::script(script.clone())
}

fn ok(body: Value) -> ScriptStep {
    ScriptStep::Json { status: 200, body }
}

fn draft() -> ListingDraft {
    ListingDraft {
        title: "Item de test - No Ofertar".into(),
        description: Some("Descripcion con Texto Plano".into()),
        price: Some(350.0),
        currency: Some("ARS".into()),
        stock: Some(10),
        skus: vec![ListingSku {
            name: "Unico".into(),
            price: Some(350.0),
            stock: Some(10),
            code: Some("SKU-1".into()),
        }],
        images: vec![],
        weight_kg: Some(1.0),
        dimensions_cm: Some(DimensionsCm { l: 30.0, w: 20.0, h: 10.0 }),
        brand: Some("Marca del producto".into()),
    }
}

fn target(category: &str) -> ListingTarget {
    ListingTarget { shop_id: "shop-1".into(), category_id: category.into(), attributes: vec![] }
}

fn images() -> Vec<(String, Vec<u8>)> {
    vec![("cup.jpg".into(), b"\xff\xd8\xff\xd9".to_vec())]
}

fn user(site: &str, product_seller: bool) -> Value {
    let mut tags = vec!["normal"];
    if product_seller {
        tags.push("user_product_seller");
    }
    json!({"id": 789, "nickname": "TETE6838590", "site_id": site, "tags": tags})
}

fn category(id: &str, currencies: &[&str]) -> Value {
    json!({
        "id": id,
        "name": "Otros",
        "children_categories": [],
        "settings": {"listing_allowed": true, "status": "enabled", "currencies": currencies}
    })
}

fn color_attributes() -> Value {
    json!([
        {
            "id": "COLOR",
            "name": "Color",
            "tags": {"allow_variations": true, "defines_picture": true},
            "value_type": "list",
            "values": [{"id": "52049", "name": "Negro"}, {"id": "52005", "name": "Marron"}]
        },
        {"id": "SELLER_SKU", "name": "SKU", "tags": {"variation_attribute": true}, "value_type": "string"}
    ])
}

fn picture() -> Value {
    json!({"id": "959699-MLM43299127002_092020", "max_size": "994x1020", "variations": []})
}

fn item(id: &str, status: &str) -> Value {
    json!({
        "id": id,
        "title": "Item De Testeo",
        "status": status,
        "sub_status": [],
        "currency_id": "ARS",
        "price": 350,
        "tags": ["immediate_payment"]
    })
}

fn description() -> Value {
    json!({"plain_text": "Descripcion con Texto Plano"})
}

fn req_path(call: &HttpRequest) -> &str {
    call.url
        .split('?')
        .next()
        .and_then(|value| value.split("//").nth(1))
        .and_then(|value| value.find('/').map(|index| &value[index..]))
        .unwrap_or(call.url.as_str())
}

fn q<'a>(call: &'a HttpRequest, key: &str) -> Option<&'a str> {
    call.query.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str())
}

fn json_of(call: &HttpRequest) -> &Value {
    match &call.body {
        HttpBody::Json(value) => value,
        other => panic!("expected json body, got {other:?}"),
    }
}

fn push_local_reads(script: &ScriptHttp, site: &str, category_id: &str, currencies: &[&str], product_seller: bool) {
    script.push("/users/789", ok(user(site, product_seller)));
    script.push(&format!("/categories/{category_id}"), ok(category(category_id, currencies)));
}

#[test]
fn order_search_hours_follow_the_documented_minute_drop() {
    assert_eq!(format_from(0, 0), "1970-01-01T00:00:00.000+00:00");
    assert_eq!(format_to(1_000, 0), "1970-01-01T01:00:00.000+00:00");
    assert_eq!(format_to(3_600, 0), "1970-01-01T01:00:00.000+00:00");
    assert!(format_from(NOW, site_offset_secs("MLA")).ends_with("-03:00"));
    assert!(format_from(NOW, site_offset_secs("MLM")).ends_with("-06:00"));
}

#[test]
fn attribute_payload_maps_documented_types() {
    let mapped = mercadolibre::map_attributes(&json!([
        {"id": "BRAND", "name": "Marca", "tags": {"fixed": true}, "value_type": "string", "values": [{"id": "5601", "name": "BGH"}]},
        {"id": "COLOR", "name": "Color", "tags": {"allow_variations": true}, "value_type": "list", "values": [{"id": "52049", "name": "Negro"}]},
        {"id": "VOLUME_CAPACITY", "name": "Capacidad", "value_type": "number_unit", "default_unit": "l", "allowed_units": [{"id": "l", "name": "l"}]},
        {"id": "ITEM_CONDITION", "name": "Condicion", "tags": {"required": true}, "value_type": "list", "values": [{"id": "2230284", "name": "Nuevo"}, {"id": "2230581", "name": "Usado"}]},
        {"id": "GENDER", "name": "Genero", "value_type": "list", "tags": ["required", "multivalued"], "values": [{"id": "339665", "name": "Mujer"}]}
    ]));
    assert_eq!(mapped[0].input, AttributeInput::Text);
    assert!(!mapped[0].required);
    assert_eq!(mapped[1].input, AttributeInput::Select);
    assert_eq!(mapped[1].options[0].id, "52049");
    assert_eq!(mapped[2].input, AttributeInput::Number);
    assert_eq!(mapped[2].unit.as_deref(), Some("l"));
    assert!(mapped[3].required);
    assert_eq!(mapped[3].input, AttributeInput::Select);
    assert!(mapped[4].required);
    assert_eq!(mapped[4].input, AttributeInput::MultiSelect);
}

#[test]
fn item_status_keeps_unknown_and_closed_distinct() {
    assert_eq!(mercadolibre::map_item_status("active", &[], &[]).0, ListingStatus::Live);
    assert_eq!(mercadolibre::map_item_status("under_review", &[], &[]).0, ListingStatus::Reviewing);
    assert_eq!(mercadolibre::map_item_status("paused", &["forbidden".into()], &[]).0, ListingStatus::Banned);
    assert_eq!(mercadolibre::map_item_status("closed", &[], &[]).0, ListingStatus::Rejected);
    assert_eq!(mercadolibre::map_item_status("inactive", &["deleted".into()], &[]).0, ListingStatus::Rejected);
    let (status, reason) = mercadolibre::map_item_status("held_for_review", &[], &[]);
    assert_eq!(status, ListingStatus::Reviewing);
    assert!(reason.unwrap().contains("未识别"));
}

#[test]
fn capabilities_cover_listing_and_leave_refunds_out() {
    let caps = mercadolibre::capabilities();
    assert_eq!(caps.platform, Platform::Mercadolibre);
    assert!(caps.listing && caps.categories && caps.products && caps.orders);
    assert!(caps.metrics.contains(&MetricKey::Gmv));
    assert!(caps.metrics.contains(&MetricKey::ProductsLive));
    assert!(!caps.metrics.contains(&MetricKey::RefundAmount));
    assert!(!caps.metrics.contains(&MetricKey::RefundOrders));
}

#[tokio::test]
async fn categories_use_the_site_tree_and_child_documents() {
    let script = Arc::new(ScriptHttp::new());
    script.push(
        "/sites/MLA/categories",
        ok(json!([
            {"id": "MLA5725", "name": "Accesorios para Vehiculos"},
            {"id": "MLA1051", "name": "Celulares y Telefonos"}
        ])),
    );
    let roots = mercadolibre::categories(&scripted(&script), &shop("MLA"), None, NOW).await.unwrap();
    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0].parent_id, "0");
    assert!(!roots[0].leaf);
    let first = &script.calls()[0];
    assert!(first.headers.iter().any(|(key, value)| key == "Authorization" && value == "Bearer token-abc"));
    assert!(first.url.starts_with("https://api.mercadolibre.com/sites/MLA/categories"));

    let script = Arc::new(ScriptHttp::new());
    script.push(
        "/categories/MLA1051",
        ok(json!({
            "id": "MLA1051",
            "name": "Celulares y Telefonos",
            "children_categories": [{"id": "MLA1055", "name": "Celulares y Smartphones"}]
        })),
    );
    script.push(
        "/categories/MLA1055",
        ok(json!({
            "id": "MLA1055",
            "name": "Celulares y Smartphones",
            "children_categories": [],
            "settings": {"listing_allowed": true, "status": "enabled", "currencies": ["ARS"]}
        })),
    );
    let children = mercadolibre::categories(&scripted(&script), &shop("AR"), Some("MLA1051"), NOW).await.unwrap();
    assert_eq!(children.len(), 1);
    assert!(children[0].leaf);
    assert_eq!(children[0].parent_id, "MLA1051");
    assert_eq!(children[0].id, "MLA1055");
}

#[tokio::test]
async fn local_item_uploads_pictures_then_description_and_sku() {
    let script = Arc::new(ScriptHttp::new());
    push_local_reads(&script, "MLA", "MLA3530", &["ARS"], false);
    script.push("/pictures/items/upload", ok(picture()));
    script.push("/items", ok(item("MLA1136716168", "active")));
    script.push("/items/MLA1136716168/description", ok(description()));
    let outcome = mercadolibre::push_listing(&scripted(&script), &shop("MLA"), &draft(), &target("MLA3530"), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Live);
    assert_eq!(outcome.remote_id.as_deref(), Some("MLA1136716168"));
    let calls = script.calls();
    assert_eq!(req_path(&calls[2]), "/pictures/items/upload");
    assert!(!calls[2].mutating);
    match &calls[2].body {
        HttpBody::Multipart(file) => {
            assert_eq!(file.field, "file");
            assert_eq!(file.file_name, "cup.jpg");
            assert_eq!(file.bytes, b"\xff\xd8\xff\xd9");
        }
        other => panic!("expected multipart, got {other:?}"),
    }
    let item_call = calls.iter().find(|call| req_path(call) == "/items").unwrap();
    assert!(item_call.mutating);
    let body = json_of(item_call);
    assert!(body.get("description").is_none());
    assert!(body.get("shipping").is_none());
    assert!(body.get("sites_to_sell").is_none());
    assert_eq!(body["pictures"][0]["id"], "959699-MLM43299127002_092020");
    assert_eq!(body["currency_id"], "ARS");
    assert_eq!(body["listing_type_id"], "gold_special");
    assert!(body["attributes"].as_array().unwrap().iter().any(|attr| attr["id"] == "SELLER_SKU"));
    let description_call = calls.last().unwrap();
    assert_eq!(req_path(description_call), "/items/MLA1136716168/description");
    assert!(description_call.mutating);
    assert_eq!(json_of(description_call)["plain_text"], "Descripcion con Texto Plano");
}

#[tokio::test]
async fn legacy_variations_share_one_price_and_put_sku_on_each() {
    let script = Arc::new(ScriptHttp::new());
    push_local_reads(&script, "MLA", "MLA378496", &["ARS"], false);
    script.push("/categories/MLA378496/attributes", ok(color_attributes()));
    script.push("/pictures/items/upload", ok(picture()));
    script.push("/items", ok(item("MLA657381404", "active")));
    script.push("/items/MLA657381404/description", ok(description()));
    let mut draft = draft();
    draft.price = Some(100.0);
    draft.skus = vec![
        ListingSku { name: "Negro".into(), price: Some(100.0), stock: Some(4), code: Some("SC-1520".into()) },
        ListingSku { name: "Marron".into(), price: Some(100.0), stock: Some(4), code: Some("SC-1521".into()) },
    ];
    let outcome = mercadolibre::push_listing(&scripted(&script), &shop("MLA"), &draft, &target("MLA378496"), &images(), NOW, None).await;
    assert_eq!(outcome.remote_id.as_deref(), Some("MLA657381404"));
    let body = json_of(script.calls().iter().find(|call| req_path(call) == "/items").unwrap()).clone();
    let variations = body["variations"].as_array().unwrap();
    assert_eq!(variations.len(), 2);
    assert_eq!(variations[0]["price"], 100.0);
    assert_eq!(variations[1]["price"], 100.0);
    assert_eq!(variations[0]["attribute_combinations"][0]["id"], "COLOR");
    assert_eq!(variations[0]["attribute_combinations"][0]["value_name"], "Negro");
    assert_eq!(variations[0]["attributes"][0]["id"], "SELLER_SKU");
    assert_eq!(variations[0]["attributes"][0]["value_name"], "SC-1520");
    assert_eq!(variations[1]["attributes"][0]["value_name"], "SC-1521");
    assert!(body.get("family_name").is_none());
}

#[tokio::test]
async fn different_variation_prices_are_rejected_before_upload() {
    let script = Arc::new(ScriptHttp::new());
    push_local_reads(&script, "MLA", "MLA378496", &["ARS"], false);
    script.push("/categories/MLA378496/attributes", ok(color_attributes()));
    let mut draft = draft();
    draft.skus = vec![
        ListingSku { name: "Negro".into(), price: Some(100.0), stock: Some(1), code: Some("A".into()) },
        ListingSku { name: "Marron".into(), price: Some(120.0), stock: Some(1), code: Some("B".into()) },
    ];
    let outcome = mercadolibre::push_listing(&scripted(&script), &shop("MLA"), &draft, &target("MLA378496"), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.remote_id.is_none());
    assert!(script.calls().iter().all(|call| req_path(call) != "/pictures/items/upload" && req_path(call) != "/items"));
}

#[tokio::test]
async fn user_product_seller_creates_one_item_per_sku_without_variations() {
    let script = Arc::new(ScriptHttp::new());
    push_local_reads(&script, "MLM", "MLM1055", &["MXN"], true);
    script.push("/categories/MLM1055/attributes", ok(color_attributes()));
    script.push("/pictures/items/upload", ok(picture()));
    script.push("/items", ok(item("MLM2061397137", "active")));
    script.push("/items/MLM2061397137/description", ok(description()));
    script.push("/items", ok(item("MLM2061397138", "under_review")));
    script.push("/items/MLM2061397138/description", ok(description()));
    let mut draft = draft();
    draft.currency = Some("MXN".into());
    draft.title = "Apple iPhone 256GB".into();
    draft.skus = vec![
        ListingSku { name: "Blue".into(), price: Some(17616.0), stock: Some(6), code: Some("IP-BLUE".into()) },
        ListingSku { name: "Red".into(), price: Some(19800.0), stock: Some(8), code: Some("IP-RED".into()) },
    ];
    let outcome = mercadolibre::push_listing(&scripted(&script), &shop("MLM"), &draft, &target("MLM1055"), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Reviewing);
    assert_eq!(outcome.remote_id.as_deref(), Some("MLM2061397137,MLM2061397138"));
    let bodies: Vec<Value> = script.calls().iter().filter(|call| req_path(call) == "/items").map(json_of).cloned().collect();
    assert_eq!(bodies.len(), 2);
    assert!(bodies[0].get("title").is_none());
    assert!(bodies[0].get("variations").is_none());
    assert_eq!(bodies[0]["family_name"], "Apple iPhone 256GB");
    assert_eq!(bodies[0]["price"], 17616.0);
    assert_eq!(bodies[1]["price"], 19800.0);
    assert!(bodies[0]["attributes"].as_array().unwrap().iter().any(|attr| attr["value_name"] == "Blue" && attr["id"] == "COLOR"));
    assert!(bodies[1]["attributes"].as_array().unwrap().iter().any(|attr| attr["id"] == "SELLER_SKU" && attr["value_name"] == "IP-RED"));
}

#[tokio::test]
async fn global_selling_copies_marketplace_logistic_type_and_does_not_assume_fulfillment() {
    let script = Arc::new(ScriptHttp::new());
    script.push("/users/789", ok(user("CBT", false)));
    script.push(
        "/marketplace/users/789",
        ok(json!({
            "user_id": 789,
            "site_id": "CBT",
            "marketplaces": [
                {"user_id": 2560656535_i64, "site_id": "MLM", "logistic_type": "remote"},
                {"user_id": 2565546850_i64, "site_id": "MLB", "logistic_type": "remote"}
            ]
        })),
    );
    script.push("/categories/CBT1287", ok(category("CBT1287", &["USD"])));
    script.push("/pictures/items/upload", ok(picture()));
    script.push(
        "/global/items",
        ok(json!({
            "item_id": "CBT2796239245",
            "site_id": "CBT",
            "site_items": [
                {"item_id": "MLM1", "site_id": "MLM", "logistic_type": "remote"},
                {"item_id": "MLB1", "site_id": "MLB", "logistic_type": "remote"}
            ]
        })),
    );
    script.push("/marketplace/items/CBT2796239245", ok(item("CBT2796239245", "active")));
    let mut draft = draft();
    draft.currency = None;
    let outcome = mercadolibre::push_listing(&scripted(&script), &shop("CBT"), &draft, &target("CBT1287"), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Live);
    assert_eq!(outcome.remote_id.as_deref(), Some("CBT2796239245"));
    let body = json_of(script.calls().iter().find(|call| req_path(call) == "/global/items").unwrap()).clone();
    assert_eq!(body["currency_id"], "USD");
    assert_eq!(body["description"]["plain_text"], "Descripcion con Texto Plano");
    assert!(body.get("listing_type_id").is_none());
    let sites = body["sites_to_sell"].as_array().unwrap();
    assert!(sites.iter().all(|site| site["logistic_type"] == "remote" && site.get("net_proceeds").is_none()));
    assert_eq!(sites[0]["price"], 350.0);
    assert!(script.calls().iter().all(|call| !req_path(call).contains("/description")));
}

#[tokio::test]
async fn fulfillment_logistic_type_is_sent_only_when_the_account_has_it() {
    let script = Arc::new(ScriptHttp::new());
    script.push("/users/789", ok(user("CBT", false)));
    script.push(
        "/marketplace/users/789",
        ok(json!({
            "user_id": 789,
            "site_id": "CBT",
            "marketplaces": [{"user_id": 11, "site_id": "MLC", "logistic_type": "fulfillment"}]
        })),
    );
    script.push("/categories/CBT1287", ok(category("CBT1287", &["USD"])));
    script.push("/pictures/items/upload", ok(picture()));
    script.push("/global/items", ok(json!({"item_id": "CBT1", "site_id": "CBT"})));
    script.push("/marketplace/items/CBT1", ok(item("CBT1", "paused")));
    let mut draft = draft();
    draft.currency = Some("USD".into());
    let outcome = mercadolibre::push_listing(&scripted(&script), &shop("CBT"), &draft, &target("CBT1287"), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Reviewing);
    let body = json_of(script.calls().iter().find(|call| req_path(call) == "/global/items").unwrap()).clone();
    assert!(body.get("sites_to_sell").is_some());
    assert_eq!(body["sites_to_sell"][0]["logistic_type"], "fulfillment");
    assert_eq!(body["sites_to_sell"][0]["price"], 350.0);
    assert!(body["sites_to_sell"][0].get("net_proceeds").is_none());
    assert!(body.get("price").is_some());
}

#[tokio::test]
async fn remote_net_proceeds_omits_price_when_the_account_uses_that_model() {
    let script = Arc::new(ScriptHttp::new());
    script.push("/users/789", ok(user("CBT", false)));
    script.push(
        "/marketplace/users/789",
        ok(json!({
            "user_id": 789,
            "site_id": "CBT",
            "marketplaces": [{"user_id": 11, "site_id": "MLB", "logistic_type": "remote", "pricing_model": "net_proceeds"}]
        })),
    );
    script.push("/categories/CBT1287", ok(category("CBT1287", &["USD"])));
    script.push("/pictures/items/upload", ok(picture()));
    script.push("/global/items", ok(json!({"item_id": "CBT2", "site_id": "CBT"})));
    script.push("/marketplace/items/CBT2", ok(item("CBT2", "active")));
    let mut draft = draft();
    draft.currency = Some("USD".into());
    let outcome = mercadolibre::push_listing(&scripted(&script), &shop("CBT"), &draft, &target("CBT1287"), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Live);
    let body = json_of(script.calls().iter().find(|call| req_path(call) == "/global/items").unwrap()).clone();
    assert!(body.get("price").is_none());
    assert_eq!(body["sites_to_sell"][0]["net_proceeds"], 350.0);
    assert!(body["sites_to_sell"][0].get("price").is_none());
}

#[tokio::test]
async fn fully_managed_is_used_only_when_business_model_says_so() {
    let script = Arc::new(ScriptHttp::new());
    script.push("/users/789", ok(user("CBT", true)));
    script.push(
        "/marketplace/users/789",
        ok(json!({
            "user_id": 789,
            "site_id": "CBT",
            "marketplaces": [{
                "user_id": 3196646921_i64,
                "site_id": "MCO",
                "logistic_type": "fulfillment",
                "business_model": "CBT CN Fulfillment Managed",
                "pricing_model": "net_proceeds",
                "user_product": true
            }]
        })),
    );
    script.push("/categories/CBT7041", ok(category("CBT7041", &["USD"])));
    script.push("/pictures/items/upload", ok(picture()));
    script.push("/global/items", ok(json!({"item_id": "CBT9", "site_id": "CBT"})));
    script.push("/marketplace/items/CBT9", ok(item("CBT9", "active")));
    let mut draft = draft();
    draft.currency = None;
    draft.price = None;
    draft.stock = None;
    draft.skus.clear();
    let outcome = mercadolibre::push_listing(&scripted(&script), &shop("CBT"), &draft, &target("CBT7041"), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Live);
    let body = json_of(script.calls().iter().find(|call| req_path(call) == "/global/items").unwrap()).clone();
    assert!(body.get("sites_to_sell").is_none());
    assert!(body.get("price").is_none());
    assert!(body.get("currency_id").is_none());
    assert!(body.get("available_quantity").is_none());
    assert!(body.get("listing_type_id").is_none());
    assert_eq!(body["family_name"], "Item de test - No Ofertar");
    assert_eq!(body["description"]["plain_text"], "Descripcion con Texto Plano");
}

#[tokio::test]
async fn validation_and_transport_failures_keep_their_status() {
    let script = Arc::new(ScriptHttp::new());
    let missing = mercadolibre::push_listing(&scripted(&script), &shop("MLA"), &draft(), &target("MLA3530"), &[], NOW, None).await;
    assert_eq!(missing.status, ListingStatus::Rejected);
    assert!(script.calls().is_empty());

    let script = Arc::new(ScriptHttp::new());
    push_local_reads(&script, "MLA", "MLA3530", &["ARS"], false);
    let mut priced = draft();
    priced.currency = Some("USD".into());
    let currency = mercadolibre::push_listing(&scripted(&script), &shop("MLA"), &priced, &target("MLA3530"), &images(), NOW, None).await;
    assert_eq!(currency.status, ListingStatus::Rejected);
    assert!(currency.reason.unwrap().contains("币种"));
    assert!(script.calls().iter().all(|call| req_path(call) != "/items"));

    let script = Arc::new(ScriptHttp::new());
    push_local_reads(&script, "MLA", "MLA3530", &["ARS"], false);
    script.push("/pictures/items/upload", ScriptStep::Fault(TransportFault::BeforeDispatch("connect".into())));
    let before = mercadolibre::push_listing(&scripted(&script), &shop("MLA"), &draft(), &target("MLA3530"), &images(), NOW, None).await;
    assert_eq!(before.status, ListingStatus::Failed);
    assert!(before.remote_id.is_none());

    let script = Arc::new(ScriptHttp::new());
    push_local_reads(&script, "MLA", "MLA3530", &["ARS"], false);
    script.push("/pictures/items/upload", ok(picture()));
    script.push(
        "/items",
        ScriptStep::Json {
            status: 400,
            body: json!({
                "message": "Validation error",
                "error": "validation_error",
                "status": 400,
                "cause": [{
                    "code": "item.attributes.missing_required",
                    "message": "One or more required attributes are not present in the item."
                }]
            }),
        },
    );
    let rejected = mercadolibre::push_listing(&scripted(&script), &shop("MLA"), &draft(), &target("MLA3530"), &images(), NOW, None).await;
    assert_eq!(rejected.status, ListingStatus::Rejected);
    assert!(rejected.remote_id.is_none());
    assert!(rejected.reason.unwrap().contains("required attributes"));
    assert!(script.calls().iter().all(|call| !req_path(call).contains("/description")));

    let script = Arc::new(ScriptHttp::new());
    push_local_reads(&script, "MLA", "MLA3530", &["ARS"], false);
    script.push("/pictures/items/upload", ok(picture()));
    script.push("/items", ScriptStep::Fault(TransportFault::AfterDispatch("reset".into())));
    let uncertain = mercadolibre::push_listing(&scripted(&script), &shop("MLA"), &draft(), &target("MLA3530"), &images(), NOW, None).await;
    assert_eq!(uncertain.status, ListingStatus::Uncertain);

    let script = Arc::new(ScriptHttp::new());
    push_local_reads(&script, "MLA", "MLA3530", &["ARS"], false);
    script.push("/pictures/items/upload", ok(picture()));
    script.push("/items", ok(item("MLA1", "active")));
    script.push(
        "/items/MLA1/description",
        ScriptStep::Json { status: 500, body: json!({"message": "server", "error": "internal_error", "status": 500}) },
    );
    let described = mercadolibre::push_listing(&scripted(&script), &shop("MLA"), &draft(), &target("MLA3530"), &images(), NOW, None).await;
    assert_eq!(described.status, ListingStatus::Uncertain);
    assert_eq!(described.remote_id.as_deref(), Some("MLA1"));
}

#[tokio::test]
async fn refresh_reads_marketplace_items_for_cbt_and_local_items() {
    let script = Arc::new(ScriptHttp::new());
    script.push("/items/MLA1136716168", ok(json!({"id": "MLA1136716168", "status": "paused", "sub_status": ["out_of_stock"]})));
    let (status, reason) = mercadolibre::listing_status(&scripted(&script), &shop("MLA"), "MLA1136716168", NOW).await.unwrap();
    assert_eq!(status, ListingStatus::Reviewing);
    assert!(reason.unwrap().contains("out_of_stock"));

    let script = Arc::new(ScriptHttp::new());
    script.push("/marketplace/items/CBT2796239245", ok(json!({"id": "CBT2796239245", "status": "active", "sub_status": []})));
    let (status, _) = mercadolibre::listing_status(&scripted(&script), &shop("CBT"), "CBT2796239245", NOW).await.unwrap();
    assert_eq!(status, ListingStatus::Live);
}

#[tokio::test]
async fn products_page_through_search_and_keep_missing_prices_absent() {
    let script = Arc::new(ScriptHttp::new());
    script.push("/users/789", ok(user("MLA", false)));
    script.push(
        "/users/789/items/search",
        ok(json!({
            "seller_id": "789",
            "results": ["MLA599260060", "MLA594239600"],
            "paging": {"limit": 50, "offset": 0, "total": 80}
        })),
    );
    script.push(
        "/items",
        ok(json!([
            {"code": 200, "body": {
                "id": "MLA599260060",
                "title": "Item De Test - Por Favor No Ofertar",
                "price": 130,
                "currency_id": "ARS",
                "available_quantity": 1,
                "status": "active",
                "attributes": [{"id": "SELLER_SKU", "value_name": "SKU-A"}]
            }},
            {"code": 200, "body": {
                "id": "MLA594239600",
                "title": "Item De Test - Por Favor No Ofertar",
                "status": "paused",
                "currency_id": "ARS"
            }}
        ])),
    );
    let page = mercadolibre::products(&scripted(&script), &shop("MLA"), None, NOW).await.unwrap();
    assert_eq!(page.shop_id, "shop-1");
    assert_eq!(page.next_cursor.as_deref(), Some("2"));
    assert_eq!(page.items[0].price, Some(130.0));
    assert_eq!(page.items[0].sku.as_deref(), Some("SKU-A"));
    assert_eq!(page.items[0].stock, Some(1));
    assert!(page.items[1].price.is_none());
    assert!(page.items[1].stock.is_none());
    let calls = script.calls();
    let search = calls.iter().find(|call| req_path(call).ends_with("/items/search")).unwrap();
    assert_eq!(q(search, "offset"), Some("0"));
    assert_eq!(q(search, "limit"), Some("50"));
}

#[tokio::test]
async fn orders_page_uses_hour_dates_and_does_not_invent_a_next_page() {
    let script = Arc::new(ScriptHttp::new());
    script.push("/users/789", ok(user("MLA", false)));
    script.push(
        "/orders/search",
        ok(json!({
            "results": [{
                "id": 2000003508419013_i64,
                "status": "paid",
                "date_created": "2015-07-01T00:00:00.000-03:00",
                "total_amount": 10,
                "currency_id": "ARS",
                "buyer": {"id": 207040551},
                "tags": ["paid", "not_delivered"],
                "order_items": [{"quantity": 1, "item": {"id": "MLA1", "title": "Item De Test", "seller_sku": "SKU-1"}}]
            }],
            "paging": {"total": 1, "offset": 0, "limit": 50}
        })),
    );
    let page = mercadolibre::orders(&scripted(&script), &shop("MLA"), MetricRange::Today, None, NOW).await.unwrap();
    assert!(page.next_cursor.is_none());
    assert_eq!(page.items[0].id, "2000003508419013");
    assert_eq!(page.items[0].amount, Some(10.0));
    assert_eq!(page.items[0].lines[0].sku.as_deref(), Some("SKU-1"));
    assert_eq!(page.items[0].lines[0].quantity, 1);
    let (start, end) = metric_bounds(MetricRange::Today, NOW, site_offset_secs("MLA"));
    let calls = script.calls();
    let call = calls.iter().find(|call| req_path(call) == "/orders/search").unwrap();
    assert_eq!(q(call, "seller"), Some("789"));
    assert_eq!(q(call, "order.date_created.from"), Some(format_from(start, site_offset_secs("MLA")).as_str()));
    assert_eq!(q(call, "order.date_created.to"), Some(format_to(end, site_offset_secs("MLA")).as_str()));
    assert_eq!(q(call, "limit"), Some("50"));
}

#[tokio::test]
async fn cbt_order_paging_moves_to_the_next_marketplace_seller() {
    let script = Arc::new(ScriptHttp::new());
    script.push("/users/789", ok(user("CBT", false)));
    script.push(
        "/marketplace/users/789",
        ok(json!({
            "user_id": 789,
            "site_id": "CBT",
            "marketplaces": [
                {"user_id": 101, "site_id": "MLM", "logistic_type": "remote"},
                {"user_id": 202, "site_id": "MLB", "logistic_type": "remote"}
            ]
        })),
    );
    script.push(
        "/orders/search",
        ok(json!({
            "results": [{
                "id": 11,
                "status": "paid",
                "date_created": "2026-01-05T10:30:00.000-06:00",
                "total_amount": 20,
                "currency_id": "USD",
                "buyer": {"id": 7},
                "tags": ["paid", "delivered"]
            }],
            "paging": {"total": 1, "offset": 0, "limit": 50}
        })),
    );
    let page = mercadolibre::orders(&scripted(&script), &shop("CBT"), MetricRange::Last7, None, NOW).await.unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some("1:0"));
    let calls = script.calls();
    let call = calls.iter().find(|call| req_path(call) == "/orders/search").unwrap();
    assert_eq!(q(call, "seller"), Some("101"));
}

#[tokio::test]
async fn metrics_keep_zero_mixed_currency_missing_fields_and_refunds_truthful() {
    let (start, end) = metric_bounds(MetricRange::Yesterday, NOW, site_offset_secs("MLA"));
    let from = format_from(start, site_offset_secs("MLA"));
    let to = format_to(end, site_offset_secs("MLA"));

    let script = Arc::new(ScriptHttp::new());
    script.push("/users/789", ok(user("MLA", false)));
    script.push("/orders/search", ok(json!({"results": [], "paging": {"total": 0, "offset": 0, "limit": 50}})));
    script.push("/users/789/items/search", ok(json!({"results": [], "paging": {"total": 0, "limit": 1, "offset": 0}})));
    let metrics = mercadolibre::metrics(&scripted(&script), &shop("MLA"), MetricRange::Yesterday, NOW).await.unwrap();
    assert_eq!(metrics.shop_id, "shop-1");
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::Gmv), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::PendingShipment), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::ProductsLive), Some(&0.0));
    assert!(metrics.currency.is_none());
    assert!(metrics.error.is_none());
    assert!(metrics.unsupported.contains(&MetricKey::RefundAmount));
    assert!(metrics.unsupported.contains(&MetricKey::RefundOrders));
    assert!(!metrics.unsupported.contains(&MetricKey::Gmv));
    let calls = script.calls();
    let order_call = calls.iter().find(|call| req_path(call) == "/orders/search").unwrap();
    assert_eq!(q(order_call, "order.date_created.from"), Some(from.as_str()));
    assert_eq!(q(order_call, "order.date_created.to"), Some(to.as_str()));

    let script = Arc::new(ScriptHttp::new());
    script.push("/users/789", ok(user("MLA", false)));
    script.push(
        "/orders/search",
        ok(json!({
            "results": [
                {"id": 1, "status": "paid", "total_amount": 10, "currency_id": "ARS", "buyer": {"id": 4}, "tags": ["paid", "not_delivered"]},
                {"id": 2, "status": "confirmed", "total_amount": 99, "currency_id": "ARS", "buyer": {"id": 5}, "tags": ["not_delivered"]},
                {"id": 3, "status": "paid", "total_amount": 5, "currency_id": "USD", "buyer": {"id": 4}, "tags": ["delivered", "paid"]}
            ],
            "paging": {"total": 3, "offset": 0, "limit": 50}
        })),
    );
    script.push("/users/789/items/search", ok(json!({"paging": {"total": 6, "limit": 1, "offset": 0}, "results": ["MLA1"]})));
    let mixed = mercadolibre::metrics(&scripted(&script), &shop("MLA"), MetricRange::Last30, NOW).await.unwrap();
    assert_eq!(mixed.values.get(&MetricKey::Orders), Some(&2.0));
    assert_eq!(mixed.values.get(&MetricKey::Buyers), Some(&1.0));
    assert_eq!(mixed.values.get(&MetricKey::PendingShipment), Some(&1.0));
    assert_eq!(mixed.values.get(&MetricKey::ProductsLive), Some(&6.0));
    assert!(mixed.currency.is_none());
    assert!(mixed.unsupported.contains(&MetricKey::Gmv));
    assert!(!mixed.unsupported.contains(&MetricKey::Orders));
    assert!(mixed.unsupported.contains(&MetricKey::RefundAmount));

    let script = Arc::new(ScriptHttp::new());
    script.push("/users/789", ok(user("MLA", false)));
    script.push(
        "/orders/search",
        ok(json!({
            "results": [{"id": 9, "status": "paid", "currency_id": "ARS", "buyer": {"id": 1}}],
            "paging": {"total": 1, "offset": 0, "limit": 50}
        })),
    );
    script.push("/users/789/items/search", ok(json!({"results": [], "paging": {"limit": 1, "offset": 0}})));
    let missing = mercadolibre::metrics(&scripted(&script), &shop("MLA"), MetricRange::Today, NOW).await.unwrap();
    assert!(missing.unsupported.contains(&MetricKey::Gmv));
    assert!(missing.unsupported.contains(&MetricKey::PendingShipment));
    assert!(missing.unsupported.contains(&MetricKey::ProductsLive));
    assert_eq!(missing.values.get(&MetricKey::Orders), Some(&1.0));
    assert_eq!(missing.values.get(&MetricKey::Gmv), Some(&0.0));

    let script = Arc::new(ScriptHttp::new());
    script.push("/users/789", ok(user("MLA", false)));
    script.push(
        "/orders/search",
        ScriptStep::Json { status: 401, body: json!({"message": "unauthorized", "error": "unauthorized", "status": 401}) },
    );
    let auth = mercadolibre::metrics(&scripted(&script), &shop("MLA"), MetricRange::Today, NOW).await;
    assert!(matches!(auth, Err(CommerceError::Rejected(_))));
}

#[tokio::test]
async fn order_offset_advances_when_paging_total_says_more() {
    let script = Arc::new(ScriptHttp::new());
    script.push("/users/789", ok(user("MLA", false)));
    script.push(
        "/orders/search",
        ok(json!({
            "results": [{
                "id": 1,
                "status": "paid",
                "date_created": "2016-02-25T15:53:38.000-03:00",
                "total_amount": 10,
                "currency_id": "ARS",
                "buyer": {"id": 2},
                "tags": ["paid", "delivered"]
            }],
            "paging": {"total": 51, "offset": 0, "limit": 50}
        })),
    );
    let page = mercadolibre::orders(&scripted(&script), &shop("MLA"), MetricRange::Last7, None, NOW).await.unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some("1"));

    let script = Arc::new(ScriptHttp::new());
    script.push("/users/789", ok(user("MLA", false)));
    script.push("/orders/search", ok(json!({"results": [], "paging": {"total": 51, "offset": 1, "limit": 50}})));
    let next = mercadolibre::orders(&scripted(&script), &shop("MLA"), MetricRange::Last7, Some("1"), NOW).await.unwrap();
    assert!(next.items.is_empty());
    assert!(next.next_cursor.is_none());
    let calls = script.calls();
    let call = calls.iter().find(|call| req_path(call) == "/orders/search").unwrap();
    assert_eq!(q(call, "offset"), Some("1"));
}
