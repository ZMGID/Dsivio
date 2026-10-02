use super::super::transport::{ScriptTransport, Step, Transport, TransportFault};
use super::super::types::{
    AttributeInput, ListingDraft, ListingStatus, ListingTarget, MetricKey, MetricRange,
};
use super::{sign_pinduoduo, RemoteOrder, RemoteProduct};
use crate::workbench::shops::Platform;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

const NOW: i64 = 1_790_899_200;
const IMAGE: &[u8] = b"\xff\xd8\xff\xd9";
const IMAGE_URL: &str = "https://img.pddpic.com/open-gw/2021-10-11/ffc9a1e35840c3cf7f7af66f56c004c1.jpeg";

fn shop() -> super::super::transport::ResolvedShop {
    super::super::transport::ResolvedShop {
        platform: Platform::Pinduoduo,
        partner_id: "123456".into(),
        partner_key: "partner-secret".into(),
        access_token: "token-abc".into(),
        remote_id: "9001".into(),
        region: Some("CN".into()),
        name: "demo".into(),
    }
}

fn transport() -> (Arc<ScriptTransport>, Transport) {
    let script = Arc::new(ScriptTransport::new());
    let transport = Transport::script(script.clone());
    (script, transport)
}

fn push(script: &ScriptTransport, body: Value) {
    script.push("/api/router", Step::Json(body));
}

fn query<'a>(calls: &'a [super::super::transport::Outbound], index: usize, key: &str) -> &'a str {
    calls[index]
        .query
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
        .unwrap_or("")
}

fn api_types(calls: &[super::super::transport::Outbound]) -> Vec<&str> {
    calls
        .iter()
        .map(|call| call.query.iter().find(|(name, _)| name == "type").map(|(_, value)| value.as_str()).unwrap_or(""))
        .collect()
}

fn draft() -> ListingDraft {
    serde_json::from_value(json!({
        "title": "新疆特产红满疆枣夹核桃500g",
        "description": "新包装，保证产品的口感和新鲜度。单颗独立小包装，双重营养，1斤家庭分享装。",
        "price": 22.0,
        "currency": "CNY",
        "stock": 20,
        "skus": [{"name": "大号", "price": 22.0, "stock": 20, "code": "L"}],
        "weightKg": 0.4
    }))
    .expect("draft")
}

fn target() -> ListingTarget {
    serde_json::from_value(json!({
        "shopId": "shop-1",
        "categoryId": "8464",
        "attributes": [
            {"id": "multi_price", "value": "19"},
            {"id": "market_price", "value": "25"},
            {"id": "cost_template_id", "value": "163280639235619"},
            {"id": "shipment_limit_second", "value": "172800"},
            {"id": "goods_type", "value": "1"},
            {"id": "country_id", "value": "0"},
            {"id": "is_folt", "value": "false"},
            {"id": "is_pre_sale", "value": "false"},
            {"id": "is_refundable", "value": "true"},
            {"id": "second_hand", "value": "false"},
            {"id": "310", "value": "12345"}
        ]
    }))
    .expect("target")
}

fn image_upload() -> Value {
    json!({"goods_image_upload_response": {"image_url": IMAGE_URL}})
}

fn spec_list() -> Value {
    json!({"goods_spec_get_response": {"goods_spec_list": [{
        "cat_id": 8464,
        "parent_spec_id": 1216,
        "parent_spec_name": "尺寸"
    }]}})
}

fn spec_created() -> Value {
    json!({"goods_spec_id_get_response": {
        "parent_spec_id": 1216,
        "spec_name": "大号",
        "spec_id": 767
    }})
}

fn draft_saved() -> Value {
    json!({"goods_update_response": {
        "goods_commit_id": 164489173095_i64,
        "goods_id": 794029426838_i64,
        "matched_spu_id": 0
    }})
}

fn permission_denied() -> Value {
    json!({"error_response": {
        "error_msg": "调用权限不足",
        "sub_msg": "应用未获得该接口权限",
        "sub_code": "50001",
        "error_code": 50001,
        "request_id": "15440104776643887"
    }})
}

fn commit_page(status: i64, submit_time: i64, comment: &str, include: bool) -> Value {
    let list = if include {
        json!([{
            "check_status": status,
            "checked_time": submit_time,
            "commit_id": 72948403362_i64,
            "goods_id": 286996055566_i64,
            "goods_name": "这是通过API发布的商品不要拍",
            "is_shop": 0,
            "outer_goods_id": "",
            "reject_comment": comment,
            "submit_time": submit_time
        }])
    } else {
        json!([])
    };
    json!({"goods_commit_list_get_response": {"list": list, "total": if include { 1 } else { 0 }}})
}

fn order(sn: &str, status: i64, refund: i64, pay: f64) -> Value {
    json!({
        "order_sn": sn,
        "order_status": status,
        "refund_status": refund,
        "pay_amount": pay,
        "confirm_time": "2021-08-14 03:23:00"
    })
}

#[test]
fn sign_matches_the_official_md5_recipe() {
    let mut params = BTreeMap::new();
    params.insert("client_id".into(), "client".into());
    params.insert("data_type".into(), "JSON".into());
    params.insert("parent_cat_id".into(), "0".into());
    params.insert("timestamp".into(), "1600000000".into());
    params.insert("type".into(), "pdd.goods.cats.get".into());
    assert_eq!(sign_pinduoduo("secret", &params), "CF9A0385FA0B135E459C649F3514D8C4");
}

#[tokio::test]
async fn categories_and_rules_follow_the_official_documents() {
    let (script, transport) = transport();
    push(&script, json!({"goods_auth_cats_get_response": {"goods_cats_list": [
        {"cat_id": 14933, "cat_name": "童鞋/婴儿鞋/亲子鞋", "leaf": false},
        {"cat_id": 8464, "cat_name": "女装", "leaf": true}
    ]}}));
    let categories = super::fetch_categories(&transport, &shop(), NOW, Some("0")).await.expect("categories");
    assert_eq!(categories.len(), 2);
    assert_eq!(categories[0].id, "14933");
    assert_eq!(categories[0].parent_id, "0");
    assert!(!categories[0].leaf);
    assert!(categories[1].leaf);
    let calls = script.calls();
    let call = &calls[0];
    assert_eq!(call.host, "https://gw-api.pinduoduo.com");
    assert_eq!(call.path, "/api/router");
    assert_eq!(call.method, "POST");
    assert!(call.headers.iter().any(|(key, value)| key == "Content-Type" && value.starts_with("application/x-www-form-urlencoded")));
    assert_eq!(query(std::slice::from_ref(call), 0, "type"), "pdd.goods.authorization.cats");
    assert_eq!(query(std::slice::from_ref(call), 0, "parent_cat_id"), "0");
    assert!(!call.mutating);

    push(&script, json!({"cat_rule_get_response": {"goods_properties_rule": {"properties": [
        {
            "ref_pid": 310,
            "name": "品牌",
            "required": true,
            "required_rule_type": 0,
            "choose_max_num": 1,
            "property_value_type": 0,
            "value_unit": [],
            "values": [{"vid": 12345, "value": "其他"}]
        },
        {
            "ref_pid": 318,
            "name": "重量",
            "required": true,
            "required_rule_type": 1,
            "choose_max_num": 0,
            "property_value_type": 1,
            "value_unit": ["g"],
            "values": []
        }
    ]}}}));
    let attributes = super::fetch_attributes(&transport, &shop(), NOW, "8464").await.expect("attributes");
    assert_eq!(attributes[0].id, "310");
    assert!(attributes[0].required);
    assert_eq!(attributes[0].input, AttributeInput::Select);
    assert_eq!(attributes[0].options[0].id, "12345");
    assert!(!attributes[1].required);
    assert_eq!(attributes[1].input, AttributeInput::Number);
    assert_eq!(attributes[1].unit.as_deref(), Some("g"));
}

#[tokio::test]
async fn submit_maps_sku_price_stock_and_spec_from_the_draft() {
    let (script, transport) = transport();
    push(&script, image_upload());
    push(&script, spec_list());
    push(&script, spec_created());
    push(&script, draft_saved());
    push(&script, draft_saved());
    let outcome = super::push_listing(
        &transport,
        &shop(),
        &draft(),
        &target(),
        &[("cover.jpg".into(), IMAGE.to_vec())],
        NOW,
        None,
    )
    .await;
    assert_eq!(outcome.status, ListingStatus::Reviewing);
    assert_eq!(outcome.remote_id.as_deref(), Some("794029426838"));
    let calls = script.calls();
    assert_eq!(
        api_types(&calls),
        vec![
            "pdd.goods.image.upload",
            "pdd.goods.spec.get",
            "pdd.goods.spec.id.get",
            "pdd.goods.edit.goods.commit",
            "pdd.goods.submit.goods.commit",
        ]
    );
    assert!(calls[0].mutating);
    assert!(!calls[1].mutating);
    assert!(calls[2].mutating && calls[3].mutating && calls[4].mutating);
    assert_eq!(query(&calls, 0, "image"), STANDARD.encode(IMAGE));
    assert_eq!(query(&calls, 2, "parent_spec_id"), "1216");
    assert_eq!(query(&calls, 2, "spec_name"), "大号");
    let edit = &calls[3];
    assert_eq!(query(&calls, 3, "cat_id"), "8464");
    assert_eq!(query(&calls, 3, "market_price"), "2500");
    assert_eq!(query(&calls, 3, "cost_template_id"), "163280639235619");
    assert_eq!(query(&calls, 3, "country_id"), "0");
    assert_eq!(query(&calls, 3, "goods_type"), "1");
    assert_eq!(query(&calls, 3, "is_folt"), "false");
    assert_eq!(query(&calls, 3, "shipment_limit_second"), "172800");
    assert!(!edit.query.iter().any(|(key, _)| key == "goods_id" || key == "limit_quantity"));
    let skus: Vec<Value> = serde_json::from_str(query(&calls, 3, "sku_list")).expect("sku json");
    assert_eq!(skus[0]["price"], json!(2200));
    assert_eq!(skus[0]["multi_price"], json!(1900));
    assert_eq!(skus[0]["quantity"], json!(20));
    assert_eq!(skus[0]["is_onsale"], json!(1));
    assert_eq!(skus[0]["spec_id_list"], json!("[767]"));
    assert_eq!(skus[0]["out_sku_sn"], json!("L"));
    assert_eq!(skus[0]["weight"], json!(400));
    assert_eq!(skus[0]["thumb_url"], json!(IMAGE_URL));
    let properties: Vec<Value> = serde_json::from_str(query(&calls, 3, "goods_properties")).expect("properties");
    assert_eq!(properties[0]["ref_pid"], json!(310));
    assert_eq!(properties[0]["vid"], json!(12345));
    assert!(properties[0].get("value").is_none());
    assert_eq!(query(&calls, 4, "goods_id"), "794029426838");
    assert_eq!(query(&calls, 4, "goods_commit_id"), "164489173095");
    assert_eq!(query(&calls, 4, "operate_type"), "0");
}

#[tokio::test]
async fn missing_required_attribute_is_rejected_before_dispatch() {
    let (script, transport) = transport();
    let mut target = target();
    target.attributes.retain(|attribute| attribute.id != "cost_template_id");
    let outcome = super::push_listing(&transport, &shop(), &draft(), &target, &[( "a.jpg".into(), IMAGE.to_vec())], NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.reason.unwrap_or_default().contains("cost_template_id"));
    assert!(script.calls().is_empty());
}

#[tokio::test]
async fn permission_rejection_on_commit_stays_rejected() {
    let (script, transport) = transport();
    push(&script, image_upload());
    push(&script, spec_list());
    push(&script, spec_created());
    push(&script, permission_denied());
    let outcome = super::push_listing(
        &transport,
        &shop(),
        &draft(),
        &target(),
        &[("cover.jpg".into(), IMAGE.to_vec())],
        NOW,
        None,
    )
    .await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.remote_id.is_none());
    assert!(outcome.reason.unwrap_or_default().contains("权限"));
    assert_eq!(api_types(&script.calls()).len(), 4);
    assert_ne!(api_types(&script.calls()).last().copied(), Some("pdd.goods.submit.goods.commit"));
}

#[tokio::test]
async fn fault_after_submit_is_uncertain_and_keeps_goods_id() {
    let (script, transport) = transport();
    push(&script, image_upload());
    push(&script, spec_list());
    push(&script, spec_created());
    push(&script, draft_saved());
    script.push("/api/router", Step::Fault(TransportFault::AfterDispatch("connection reset".into())));
    let outcome = super::push_listing(
        &transport,
        &shop(),
        &draft(),
        &target(),
        &[("cover.jpg".into(), IMAGE.to_vec())],
        NOW,
        None,
    )
    .await;
    assert_eq!(outcome.status, ListingStatus::Uncertain);
    assert_eq!(outcome.remote_id.as_deref(), Some("794029426838"));
}

#[tokio::test]
async fn commit_status_uses_the_latest_draft_row() {
    let (script, transport) = transport();
    let goods = "286996055566";
    for (status, time, comment, include) in [
        (0, 1, "", false),
        (1, 1, "", false),
        (2, 10, "", true),
        (3, 20, "请重新上传第1张轮播图", true),
    ] {
        push(&script, commit_page(status, time, comment, include));
    }
    let (status, reason) = super::refresh_remote(&transport, &shop(), NOW, goods).await.expect("status");
    assert_eq!(status, ListingStatus::Rejected);
    assert!(reason.unwrap_or_default().contains("轮播图"));

    for (status, time, include) in [(0, 1, false), (1, 30, true), (2, 10, true), (3, 20, false)] {
        push(&script, commit_page(status, time, "", include));
    }
    let (status, reason) = super::refresh_remote(&transport, &shop(), NOW, goods).await.expect("reviewing");
    assert_eq!(status, ListingStatus::Reviewing);
    assert!(reason.is_none());

    for (status, include) in [(0, false), (1, false), (2, true), (3, false)] {
        push(&script, commit_page(status, 5, "", include));
    }
    let (status, _) = super::refresh_remote(&transport, &shop(), NOW, goods).await.expect("live");
    assert_eq!(status, ListingStatus::Live);
}

#[tokio::test]
async fn products_and_orders_page_with_the_official_totals() {
    let (script, transport) = transport();
    push(&script, json!({"goods_list_get_response": {
        "total_count": 150,
        "goods_list": [{
            "goods_id": 794029426838_i64,
            "goods_name": "新疆特产红满疆枣夹核桃500g",
            "image_url": IMAGE_URL,
            "is_onsale": 1,
            "goods_quantity": 20
        }]
    }}));
    let page = super::fetch_products(&transport, &shop(), NOW, 1).await.expect("products");
    assert!(page.has_next);
    assert_eq!(page.total, Some(150));
    assert_eq!(page.page_size, 100);
    assert_eq!(page.items[0], RemoteProduct {
        id: "794029426838".into(),
        title: "新疆特产红满疆枣夹核桃500g".into(),
        status: "onsale".into(),
        price: None,
        stock: Some(20),
        image_url: Some(IMAGE_URL.into()),
    });
    push(&script, json!({"goods_list_get_response": {"total_count": 150, "goods_list": []}}));
    let next = super::fetch_products(&transport, &shop(), NOW, 2).await.expect("page 2");
    assert!(!next.has_next);

    push(&script, json!({"order_list_get_response": {
        "has_next": true,
        "order_list": [order("210814-1", 1, 1, 10.5)]
    }}));
    let orders = super::fetch_orders(&transport, &shop(), NOW, 1_790_870_400, 1_790_899_200, 1).await.expect("orders");
    assert!(orders.has_next);
    assert_eq!(orders.items[0], RemoteOrder {
        id: "210814-1".into(),
        status: "1".into(),
        amount: Some(10.5),
        currency: Some("CNY".into()),
        buyer: None,
        created_at: None,
    });
    let rejected = super::fetch_orders(&transport, &shop(), NOW, 0, 86_400, 1).await;
    assert!(rejected.is_err());
    assert_eq!(script.calls().len(), 3);
}

#[tokio::test]
async fn metrics_keep_real_zeros_and_omit_unsupported_money() {
    let (script, transport) = transport();
    push(&script, json!({"order_list_get_response": {
        "has_next": true,
        "order_list": [order("210814-1", 1, 1, 10.5)]
    }}));
    push(&script, json!({"order_list_get_response": {
        "has_next": false,
        "order_list": [order("210814-2", 2, 4, 1.5)]
    }}));
    push(&script, json!({"goods_list_get_response": {"total_count": 7, "goods_list": []}}));
    let metrics = super::fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Today, NOW).await.expect("metrics");
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&2.0));
    assert_eq!(metrics.values.get(&MetricKey::Gmv), Some(&12.0));
    assert_eq!(metrics.values.get(&MetricKey::PendingShipment), Some(&1.0));
    assert_eq!(metrics.values.get(&MetricKey::RefundOrders), Some(&1.0));
    assert_eq!(metrics.values.get(&MetricKey::ProductsLive), Some(&7.0));
    assert_eq!(metrics.currency.as_deref(), Some("CNY"));
    assert!(metrics.unsupported.contains(&MetricKey::Buyers));
    assert!(metrics.unsupported.contains(&MetricKey::RefundAmount));
    assert!(!metrics.unsupported.contains(&MetricKey::Gmv));
    assert_eq!(query(&script.calls(), 0, "order_status"), "5");
    assert_eq!(query(&script.calls(), 0, "refund_status"), "5");
    assert_eq!(query(&script.calls(), 0, "use_has_next"), "true");
    assert_eq!(query(&script.calls(), 2, "is_onsale"), "1");

    push(&script, json!({"order_list_get_response": {"has_next": false, "order_list": []}}));
    push(&script, json!({"goods_list_get_response": {"total_count": 0, "goods_list": []}}));
    let empty = super::fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Today, NOW).await.expect("empty");
    assert_eq!(empty.values.get(&MetricKey::Orders), Some(&0.0));
    assert_eq!(empty.values.get(&MetricKey::Gmv), Some(&0.0));
    assert_eq!(empty.values.get(&MetricKey::RefundOrders), Some(&0.0));
    assert!(empty.unsupported.contains(&MetricKey::Buyers));
    assert!(empty.unsupported.contains(&MetricKey::RefundAmount));

    push(&script, json!({"order_list_get_response": {"has_next": false, "order_list": [{
        "order_sn": "210814-3",
        "order_status": 3,
        "refund_status": 1
    }]}}));
    push(&script, json!({"goods_list_get_response": {"goods_list": []}}));
    let missing = super::fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Today, NOW).await.expect("missing amount");
    assert_eq!(missing.values.get(&MetricKey::Orders), Some(&1.0));
    assert!(missing.unsupported.contains(&MetricKey::Gmv));
    assert!(missing.unsupported.contains(&MetricKey::ProductsLive));
    assert!(missing.currency.is_none());
}

#[tokio::test]
async fn order_permission_error_is_not_a_zero_dashboard() {
    let (script, transport) = transport();
    push(&script, permission_denied());
    let error = super::fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Today, NOW).await.expect_err("permission");
    assert!(error.to_string().contains("权限"));
    assert_eq!(script.calls().len(), 1);
}

#[tokio::test]
async fn other_platforms_are_unsupported() {
    let (_script, transport) = transport();
    let mut foreign = shop();
    foreign.platform = Platform::Shopee;
    let error = super::fetch_categories(&transport, &foreign, NOW, None).await.expect_err("platform");
    assert!(matches!(error, super::super::types::CommerceError::Unsupported(_)));
}
