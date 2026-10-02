use super::super::adapter::{ApiSession, HttpBody, HttpRequest, ScriptHttp, ScriptStep, SharedTransport, TransportFault};
use super::super::types::{
    AttributeInput, CommerceError, ListingDraft, ListingStatus, ListingTarget, MetricKey, MetricRange,
};
use super::{
    attributes, capabilities, categories, gmt8_timestamp, listing_status, map_product_status, metrics, orders, products,
    push_listing, sign_base, sign_param,
};
use crate::workbench::shops::Platform;
use serde_json::{json, Value};
use std::sync::Arc;

const NOW: i64 = 1_790_899_200;
const TODAY_START: i64 = 1_790_870_400;
const SECRET: &str = "749698a6-fcb3-4358-b241-ec1d93cf9c1f";
const APP_KEY: &str = "6844048284663924231";

fn session() -> ApiSession {
    ApiSession {
        shop_id: "shop-1".into(),
        platform: Platform::Douyin,
        remote_id: "99".into(),
        name: "demo".into(),
        region: Some("US".into()),
        app_id: APP_KEY.into(),
        app_secret: SECRET.into(),
        access_token: "token-abc".into(),
        refresh_token: String::new(),
        open_key: String::new(),
        seller_secret: String::new(),
    }
}

fn http(script: &Arc<ScriptHttp>) -> SharedTransport {
    SharedTransport::script(script.clone())
}

fn push(script: &ScriptHttp, path: &str, body: Value) {
    script.push(path, ScriptStep::Json { status: 200, body });
}

fn query<'a>(call: &'a HttpRequest, key: &str) -> &'a str {
    call.query.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str()).expect(key)
}

fn json_body(call: &HttpRequest) -> &Value {
    match &call.body {
        HttpBody::Json(value) => value,
        _ => panic!("expected json body"),
    }
}

fn ok(data: Value) -> Value {
    json!({"code": 10000, "msg": "success", "sub_code": "", "sub_msg": "", "data": data})
}

fn draft(extra: Value) -> ListingDraft {
    let mut body = json!({
        "title": "测试商品标题",
        "price": 9.9,
        "stock": 3,
        "weightKg": 0.5,
        "images": ["https://cdn.example/a.jpg", "https://cdn.example/b.jpg"],
        "skus": [
            {"name": "红色", "price": 9.9, "stock": 2, "code": "A"},
            {"name": "蓝色", "price": 10, "stock": 0}
        ]
    });
    if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            body.insert(key.clone(), value.clone());
        }
    }
    serde_json::from_value(body).unwrap()
}

fn target(extra: Vec<Value>) -> ListingTarget {
    let mut attributes = vec![
        json!({"id": "mobile", "value": "40012345"}),
        json!({"id": "reduce_type", "value": "1"}),
        json!({"id": "405", "value": "27664", "values": ["复习资料"]}),
    ];
    attributes.extend(extra);
    serde_json::from_value(json!({"shopId": "shop-1", "categoryId": "20000", "attributes": attributes})).unwrap()
}

fn image_ready(url: &str) -> Value {
    ok(json!({"material_id": "m1", "folder_id": "0", "is_new": true, "audit_status": 3, "byte_url": url}))
}

fn created(product_id: i64) -> Value {
    ok(json!({
        "product_id": product_id,
        "out_product_id": 0,
        "outer_product_id": "",
        "create_time": "2026-10-02 08:00:00",
        "sku": [{"sku_id": 1, "code": "A", "outer_sku_id": "1"}]
    }))
}

fn push_images(script: &ScriptHttp) {
    push(script, "/material/uploadImageSync", image_ready("https://p3.douyin.com/a.jpg"));
    push(script, "/material/uploadImageSync", image_ready("https://p3.douyin.com/b.jpg"));
}

#[test]
fn sign_matches_the_call_guide_sample() {
    let param = r#"{"page":"0","size":"20"}"#;
    let base = sign_base(APP_KEY, "product.list", param, "2020-07-05 22:33:59");
    assert_eq!(
        base,
        r#"app_key6844048284663924231methodproduct.listparam_json{"page":"0","size":"20"}timestamp2020-07-05 22:33:59v2"#
    );
    assert!(!base.contains("access_token"));
    assert!(!base.contains("sign_method"));
    let sign = sign_param(SECRET, APP_KEY, "product.list", param, "2020-07-05 22:33:59").unwrap();
    assert_eq!(sign, "a84fc5747114e63196565192a19f80d9d78819b8c8f5940eeb282bf78260901d");
    assert_eq!(gmt8_timestamp(NOW), "2026-10-02 08:00:00");
    let caps = capabilities();
    assert!(caps.listing && caps.categories && caps.products && caps.orders);
    assert_eq!(caps.metrics.len(), 7);
}

#[test]
fn product_status_follows_the_state_machine() {
    assert_eq!(map_product_status(0, 3, 0), (ListingStatus::Live, None));
    assert_eq!(map_product_status(0, 3, 2).0, ListingStatus::Live);
    assert_eq!(map_product_status(0, 2, 2).0, ListingStatus::Reviewing);
    assert_eq!(map_product_status(1, 5, 0).0, ListingStatus::Banned);
    assert_eq!(map_product_status(0, 4, 0).0, ListingStatus::Rejected);
    assert_eq!(map_product_status(2, 1, 0).0, ListingStatus::Rejected);
    assert_eq!(map_product_status(1, 7, 0).1.as_deref(), Some("审核通过待上架"));
}

#[tokio::test]
async fn categories_skip_disabled_and_reject_bad_token() {
    let script = Arc::new(ScriptHttp::new());
    push(
        &script,
        "/shop/getShopCategory",
        ok(json!({"data": [
            {"id": 20005, "name": "女装", "level": 1, "parent_id": 0, "is_leaf": false, "enable": true},
            {"id": 20006, "name": "失效", "level": 1, "parent_id": 0, "is_leaf": true, "enable": false}
        ]})),
    );
    let listed = categories(&http(&script), &session(), None, NOW).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "20005");
    assert_eq!(listed[0].parent_id, "0");
    assert!(!listed[0].leaf);
    let call = &script.calls()[0];
    assert_eq!(call.url, "https://openapi-fxg.jinritemai.com/shop/getShopCategory");
    assert_eq!(query(call, "method"), "shop.getShopCategory");
    assert_eq!(query(call, "timestamp"), "2026-10-02 08:00:00");
    assert_eq!(query(call, "sign_method"), "hmac-sha256");
    assert_eq!(query(call, "access_token"), "token-abc");
    assert!(!call.mutating);
    assert_eq!(json_body(call)["cid"], json!(0));
    let param = serde_json::to_string(json_body(call)).unwrap();
    assert_eq!(query(call, "sign"), sign_param(SECRET, APP_KEY, "shop.getShopCategory", &param, query(call, "timestamp")).unwrap());
    assert!(!sign_base(APP_KEY, "shop.getShopCategory", &param, query(call, "timestamp")).contains("token-abc"));

    let script = Arc::new(ScriptHttp::new());
    push(
        &script,
        "/shop/getShopCategory",
        json!({"code": 40003, "msg": "非法的参数", "sub_code": "isv.access-token-expired", "sub_msg": "access_token已过期"}),
    );
    let error = categories(&http(&script), &session(), Some("20005"), NOW).await.unwrap_err();
    assert!(matches!(error, CommerceError::Rejected(_)));
    assert!(error.message().contains("access_token"));
    assert_eq!(json_body(&script.calls()[0])["cid"], json!(20005));

    let script = Arc::new(ScriptHttp::new());
    let error = categories(&http(&script), &session(), Some("abc"), NOW).await.unwrap_err();
    assert!(matches!(error, CommerceError::Invalid(_)));
    assert!(script.calls().is_empty());

    let mut other = session();
    other.platform = Platform::Shopee;
    let error = categories(&http(&script), &other, None, NOW).await.unwrap_err();
    assert!(matches!(error, CommerceError::Unsupported(_)));

    push(&script, "/shop/getShopCategory", json!({"code": 70000, "msg": "服务不可用", "sub_code": "isv.api-service-off", "sub_msg": "接口已下线"}));
    let off = categories(&http(&script), &session(), None, NOW).await.unwrap_err();
    assert!(matches!(off, CommerceError::Unsupported(_)));
}

#[tokio::test]
async fn attributes_map_select_multi_number_and_skip_disabled() {
    let script = Arc::new(ScriptHttp::new());
    push(
        &script,
        "/product/getCatePropertyV2",
        ok(json!({"data": [
            {"property_id": 405, "property_name": "材质", "required": 1, "status": 0, "type": "select", "options": [{"name": "棉", "value_id": 11, "value": "11"}]},
            {"property_id": 406, "property_name": "风格", "required": 0, "status": 0, "type": "multi_select", "options": [{"name": "简约", "value_id": 2}]},
            {"property_id": 407, "property_name": "净重", "required": 0, "status": 0, "type": "text", "measure_templates": [{"value_modules": [{"validate_rule": {"data_type": "float"}, "units": [{"unit_name": "g", "unit_id": 1}]}]}]},
            {"property_id": 408, "property_name": "失效属性", "required": 1, "status": 1, "type": "text"}
        ], "tpl_type": 0})),
    );
    let attributes = attributes(&http(&script), &session(), "20000", NOW).await.unwrap();
    assert_eq!(attributes.len(), 3);
    assert_eq!(attributes[0].id, "405");
    assert!(attributes[0].required);
    assert_eq!(attributes[0].input, AttributeInput::Select);
    assert_eq!(attributes[0].options[0].id, "11");
    assert_eq!(attributes[1].input, AttributeInput::MultiSelect);
    assert!(!attributes[1].required);
    assert_eq!(attributes[2].input, AttributeInput::Number);
    assert_eq!(attributes[2].unit.as_deref(), Some("g"));
}

#[tokio::test]
async fn push_creates_variants_from_documented_fields() {
    let script = Arc::new(ScriptHttp::new());
    push_images(&script);
    push(&script, "/product/addV2", created(3558192687276554));
    let outcome = push_listing(&http(&script), &session(), &draft(json!({})), &target(vec![]), &[], NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Reviewing);
    assert_eq!(outcome.remote_id.as_deref(), Some("3558192687276554"));
    assert!(outcome.reason.is_none());
    let calls = script.calls();
    assert_eq!(calls.len(), 3);
    assert!(!calls[0].mutating);
    assert_eq!(json_body(&calls[0])["folder_id"], json!("0"));
    assert_eq!(json_body(&calls[0])["url"], json!("https://cdn.example/a.jpg"));
    let add = &calls[2];
    assert!(add.url.ends_with("/product/addV2"));
    assert!(add.mutating);
    let body = json_body(add);
    assert_eq!(body["commit"], json!(true));
    assert_eq!(body["freight_id"], json!(0));
    assert_eq!(body["mobile"], json!("40012345"));
    assert_eq!(body["reduce_type"], json!(1));
    assert_eq!(body["product_type"], json!(0));
    assert_eq!(body["standard_brand_id"], json!(596120136));
    assert_eq!(body["category_leaf_id"], json!(20000));
    assert_eq!(body["weight_unit"], json!(0));
    assert_eq!(body["weight"], json!(0.5));
    assert_eq!(body["specs"], json!("规格|红色,蓝色"));
    assert_eq!(body["pic"], json!("https://p3.douyin.com/a.jpg|https://p3.douyin.com/b.jpg"));
    assert_eq!(body["description"], body["pic"]);
    let prices: Value = serde_json::from_str(body["spec_prices"].as_str().unwrap()).unwrap();
    assert_eq!(prices[0]["price"], json!(990));
    assert_eq!(prices[0]["stock_num"], json!(2));
    assert_eq!(prices[0]["code"], json!("A"));
    assert_eq!(prices[1]["price"], json!(1000));
    assert_eq!(prices[1]["stock_num"], json!(0));
    assert!(body["product_format_new"].as_str().unwrap().contains("复习资料"));
    assert!(!body["product_format_new"].as_str().unwrap().contains("40012345"));
    assert!(body.get("product_id").is_none());
    let param = serde_json::to_string(body).unwrap();
    assert_eq!(query(add, "sign"), sign_param(SECRET, APP_KEY, "product.addV2", &param, query(add, "timestamp")).unwrap());
}

#[tokio::test]
async fn push_rejects_before_dispatch_and_classifies_api_errors() {
    let script = Arc::new(ScriptHttp::new());
    let missing = push_listing(
        &http(&script),
        &session(),
        &draft(json!({})),
        &serde_json::from_value(json!({"shopId": "shop-1", "categoryId": "20000"})).unwrap(),
        &[],
        NOW,
        None,
    )
    .await;
    assert_eq!(missing.status, ListingStatus::Rejected);
    assert!(missing.reason.unwrap().contains("mobile"));
    assert!(script.calls().is_empty());

    let local = draft(json!({"images": ["/tmp/a.jpg"]}));
    let rejected = push_listing(&http(&script), &session(), &local, &target(vec![]), &[("a.jpg".into(), vec![1, 2, 3])], NOW, None).await;
    assert_eq!(rejected.status, ListingStatus::Rejected);
    assert!(rejected.reason.unwrap().contains("HTTPS"));
    assert!(script.calls().is_empty());

    let short = push_listing(&http(&script), &session(), &draft(json!({"title": "短"})), &target(vec![]), &[], NOW, None).await;
    assert_eq!(short.status, ListingStatus::Rejected);
    let emoji = push_listing(&http(&script), &session(), &draft(json!({"title": "测试商品标题😀"})), &target(vec![]), &[], NOW, None).await;
    assert_eq!(emoji.status, ListingStatus::Rejected);
    let currency = push_listing(&http(&script), &session(), &draft(json!({"currency": "USD"})), &target(vec![]), &[], NOW, None).await;
    assert_eq!(currency.status, ListingStatus::Rejected);
    assert!(script.calls().is_empty());

    push_images(&script);
    push(
        &script,
        "/product/addV2",
        json!({"code": 50002, "msg": "业务处理失败", "sub_code": "isv.business-failed:2010326", "sub_msg": "商品创建失败: 因为店铺受到处罚，限制发品"}),
    );
    let business = push_listing(&http(&script), &session(), &draft(json!({})), &target(vec![]), &[], NOW, None).await;
    assert_eq!(business.status, ListingStatus::Rejected);
    assert!(business.reason.unwrap().contains("限制发品"));

    let script = Arc::new(ScriptHttp::new());
    push_images(&script);
    script.push("/product/addV2", ScriptStep::Fault(TransportFault::AfterDispatch("连接中断".into())));
    let uncertain = push_listing(&http(&script), &session(), &draft(json!({})), &target(vec![]), &[], NOW, None).await;
    assert_eq!(uncertain.status, ListingStatus::Uncertain);
    assert!(uncertain.remote_id.is_none());

    let script = Arc::new(ScriptHttp::new());
    push_images(&script);
    script.push("/product/addV2", ScriptStep::Fault(TransportFault::BeforeDispatch("连接失败".into())));
    let before = push_listing(&http(&script), &session(), &draft(json!({})), &target(vec![]), &[], NOW, None).await;
    assert_eq!(before.status, ListingStatus::Failed);

    let script = Arc::new(ScriptHttp::new());
    push(&script, "/material/uploadImageSync", json!({"code": 20000, "msg": "服务不可用", "sub_code": "dop.service-error", "sub_msg": "服务繁忙"}));
    let upload = push_listing(&http(&script), &session(), &draft(json!({})), &target(vec![]), &[], NOW, None).await;
    assert_eq!(upload.status, ListingStatus::Failed);
    assert!(script.calls().iter().all(|call| !call.url.contains("/product/addV2")));

    let script = Arc::new(ScriptHttp::new());
    push_images(&script);
    push(&script, "/product/addV2", json!({"code": 20000, "msg": "服务不可用", "sub_code": "dop.service-error", "sub_msg": "服务繁忙"}));
    let create = push_listing(&http(&script), &session(), &draft(json!({})), &target(vec![]), &[], NOW, None).await;
    assert_eq!(create.status, ListingStatus::Uncertain);
}

#[tokio::test]
async fn material_audit_blocks_product_submit() {
    let script = Arc::new(ScriptHttp::new());
    push(&script, "/material/uploadImageSync", ok(json!({"material_id": "m1", "audit_status": 1, "byte_url": ""})));
    push(&script, "/material/queryMaterialDetail", ok(json!({"material_info": {"material_id": "m1", "audit_status": 2}})));
    let pending = push_listing(
        &http(&script),
        &session(),
        &draft(json!({"images": ["https://cdn.example/a.jpg"], "skus": []})),
        &target(vec![]),
        &[],
        NOW,
        None,
    )
    .await;
    assert_eq!(pending.status, ListingStatus::Failed);
    assert!(pending.reason.unwrap().contains("审核"));
    assert!(script.calls().iter().all(|call| !call.url.contains("/product/addV2")));

    let script = Arc::new(ScriptHttp::new());
    push(&script, "/material/uploadImageSync", ok(json!({"material_id": "m9", "audit_status": 1})));
    push(&script, "/material/queryMaterialDetail", ok(json!({"material_info": {"audit_status": 3, "byte_url": "https://p3.douyin.com/a.jpg"}})));
    push(&script, "/product/addV2", created(88));
    let ready = push_listing(
        &http(&script),
        &session(),
        &draft(json!({"images": ["https://cdn.example/a.jpg"], "skus": []})),
        &target(vec![]),
        &[],
        NOW,
        None,
    )
    .await;
    assert_eq!(ready.status, ListingStatus::Reviewing);
    assert_eq!(ready.remote_id.as_deref(), Some("88"));
    let calls = script.calls();
    let add = calls.iter().find(|call| call.url.ends_with("/product/addV2")).unwrap();
    assert_eq!(json_body(add)["specs"], json!("规格|默认"));
    let prices: Value = serde_json::from_str(json_body(add)["spec_prices"].as_str().unwrap()).unwrap();
    assert_eq!(prices[0]["price"], json!(990));
    assert_eq!(prices[0]["stock_num"], json!(3));

    let script = Arc::new(ScriptHttp::new());
    push(&script, "/material/uploadImageSync", ok(json!({"material_id": "m4", "audit_status": 1})));
    push(&script, "/material/queryMaterialDetail", ok(json!({"material_info": {"audit_status": 4, "audit_reject_desc": "图片不合规"}})));
    let denied = push_listing(
        &http(&script),
        &session(),
        &draft(json!({"images": ["https://cdn.example/a.jpg"], "skus": []})),
        &target(vec![]),
        &[],
        NOW,
        None,
    )
    .await;
    assert_eq!(denied.status, ListingStatus::Rejected);
    assert_eq!(denied.reason.as_deref(), Some("图片不合规"));
}

#[tokio::test]
async fn edit_and_listing_status_follow_product_detail() {
    let script = Arc::new(ScriptHttp::new());
    push_images(&script);
    push(&script, "/product/editV2", created(77));
    let edited = push_listing(&http(&script), &session(), &draft(json!({})), &target(vec![]), &[], NOW, Some("77")).await;
    assert_eq!(edited.status, ListingStatus::Reviewing);
    assert_eq!(edited.remote_id.as_deref(), Some("77"));
    let calls = script.calls();
    let edit = calls.iter().find(|call| call.url.ends_with("/product/editV2")).unwrap();
    assert_eq!(json_body(edit)["product_id"], json!(77));
    assert!(edit.mutating);

    let script = Arc::new(ScriptHttp::new());
    push(&script, "/product/detail", ok(json!({"product_id": 77, "status": 0, "check_status": 3, "draft_status": 0})));
    let (status, reason) = listing_status(&http(&script), &session(), "77", NOW).await.unwrap();
    assert_eq!(status, ListingStatus::Live);
    assert!(reason.is_none());

    let script = Arc::new(ScriptHttp::new());
    push(&script, "/product/detail", ok(json!({"status": 1, "check_status": 5, "draft_status": 0})));
    assert_eq!(listing_status(&http(&script), &session(), "77", NOW).await.unwrap().0, ListingStatus::Banned);

    let script = Arc::new(ScriptHttp::new());
    push(&script, "/product/detail", ok(json!({"status": 0, "check_status": 4, "draft_status": 4})));
    let rejected = listing_status(&http(&script), &session(), "77", NOW).await.unwrap();
    assert_eq!(rejected.0, ListingStatus::Rejected);
    assert_eq!(rejected.1.as_deref(), Some("审核未通过"));

    let script = Arc::new(ScriptHttp::new());
    script.push("/product/detail", ScriptStep::Fault(TransportFault::AfterDispatch("读取中断".into())));
    let error = listing_status(&http(&script), &session(), "77", NOW).await.unwrap_err();
    assert!(matches!(error, CommerceError::Failed(_)));
}

#[tokio::test]
async fn metrics_sum_dated_orders_and_mark_unsupported_fields() {
    let script = Arc::new(ScriptHttp::new());
    push(
        &script,
        "/order/searchList",
        ok(json!({"page": 0, "size": 100, "total": 2, "shop_order_list": [
            {"order_id": "1", "order_status": 2, "pay_amount": 100, "doudian_open_id": "buyer-a"}
        ]})),
    );
    push(
        &script,
        "/order/searchList",
        ok(json!({"page": 1, "size": 100, "total": 2, "shop_order_list": [
            {"order_id": "2", "order_status": 3, "pay_amount": 250, "doudian_open_id": "buyer-a"},
            {"order_id": "3", "order_status": 4, "pay_amount": 900, "doudian_open_id": "buyer-b"},
            {"order_id": "4", "order_status": 1, "pay_amount": 50, "doudian_open_id": "buyer-c"}
        ]})),
    );
    push(
        &script,
        "/afterSale/List",
        ok(json!({
            "has_more": false, "page": 0, "size": 100, "total": 3,
            "items": [
                {"aftersale_info": {"aftersale_id": "a", "aftersale_type": 0, "aftersale_status": 12, "refund_status": 3, "refund_amount": 150}},
                {"aftersale_info": {"aftersale_id": "b", "aftersale_type": 3, "aftersale_status": 14, "refund_status": 0, "refund_amount": 999}},
                {"aftersale_info": {"aftersale_id": "c", "aftersale_type": 1, "aftersale_status": 6, "refund_status": 1, "refund_amount": 10}}
            ]
        })),
    );
    push(&script, "/product/listV2", ok(json!({"data": [], "total": 4, "page": 1, "size": 1})));
    let today = metrics(&http(&script), &session(), MetricRange::Today, NOW).await.unwrap();
    assert_eq!(today.values.get(&MetricKey::Orders), Some(&2.0));
    assert_eq!(today.values.get(&MetricKey::PendingShipment), Some(&1.0));
    assert_eq!(today.values.get(&MetricKey::Gmv), Some(&3.5));
    assert_eq!(today.values.get(&MetricKey::Buyers), Some(&1.0));
    assert_eq!(today.values.get(&MetricKey::RefundOrders), Some(&1.0));
    assert_eq!(today.values.get(&MetricKey::RefundAmount), Some(&1.5));
    assert_eq!(today.values.get(&MetricKey::ProductsLive), Some(&4.0));
    assert_eq!(today.currency.as_deref(), Some("CNY"));
    assert!(today.unsupported.is_empty());
    let calls = script.calls();
    let order_call = &calls[0];
    assert_eq!(json_body(order_call)["create_time_start"], json!(TODAY_START));
    assert_eq!(json_body(order_call)["create_time_end"], json!(NOW));
    assert_eq!(json_body(order_call)["page"], json!(0));
    let refund_call = calls.iter().find(|call| call.url.contains("/afterSale/List")).unwrap();
    assert_eq!(json_body(refund_call)["start_time"], json!(TODAY_START));
    assert_eq!(json_body(refund_call)["end_time"], json!(NOW + 1));
    let live = calls.iter().find(|call| call.url.contains("/product/listV2")).unwrap();
    assert_eq!(json_body(live)["status"], json!(0));
    assert_eq!(json_body(live)["check_status"], json!(3));

    let script = Arc::new(ScriptHttp::new());
    push(
        &script,
        "/order/searchList",
        ok(json!({"page": 0, "size": 100, "total": 1, "shop_order_list": [
            {"order_id": "9", "order_status": 5, "pay_amount": 100}
        ]})),
    );
    push(&script, "/afterSale/List", json!({"code": 50002, "msg": "业务处理失败", "sub_code": "isv.business-failed:1", "sub_msg": "售后查询失败"}));
    push(&script, "/product/listV2", json!({"code": 20000, "msg": "服务不可用", "sub_code": "dop.service-error", "sub_msg": "服务繁忙"}));
    let partial = metrics(&http(&script), &session(), MetricRange::Yesterday, NOW).await.unwrap();
    assert_eq!(partial.values.get(&MetricKey::Orders), Some(&1.0));
    assert_eq!(partial.values.get(&MetricKey::Gmv), Some(&1.0));
    assert_eq!(partial.values.get(&MetricKey::Buyers), Some(&0.0));
    assert_eq!(partial.values.get(&MetricKey::RefundAmount), Some(&0.0));
    assert_eq!(partial.values.get(&MetricKey::RefundOrders), Some(&0.0));
    assert_eq!(partial.values.get(&MetricKey::ProductsLive), Some(&0.0));
    assert!(partial.unsupported.contains(&MetricKey::Buyers));
    assert!(partial.unsupported.contains(&MetricKey::RefundAmount));
    assert!(partial.unsupported.contains(&MetricKey::RefundOrders));
    assert!(partial.unsupported.contains(&MetricKey::ProductsLive));
    assert!(!partial.unsupported.contains(&MetricKey::Orders));
    assert!(!partial.unsupported.contains(&MetricKey::Gmv));
    assert_eq!(json_body(&script.calls()[0])["create_time_start"], json!(1_790_784_000));
    assert_eq!(json_body(&script.calls()[0])["create_time_end"], json!(1_790_870_399));

    let script = Arc::new(ScriptHttp::new());
    push(&script, "/order/searchList", ok(json!({"shop_order_list": [], "total": 0, "page": 0, "size": 100})));
    push(&script, "/afterSale/List", ok(json!({"items": [], "has_more": false, "total": 0})));
    push(&script, "/product/listV2", ok(json!({"data": [], "total": 0})));
    let empty = metrics(&http(&script), &session(), MetricRange::Last7, NOW).await.unwrap();
    assert_eq!(empty.values.get(&MetricKey::Orders), Some(&0.0));
    assert_eq!(empty.values.get(&MetricKey::Gmv), Some(&0.0));
    assert_eq!(empty.values.get(&MetricKey::Buyers), Some(&0.0));
    assert_eq!(empty.values.get(&MetricKey::RefundAmount), Some(&0.0));
    assert_eq!(empty.values.get(&MetricKey::ProductsLive), Some(&0.0));
    assert!(empty.currency.is_none());
    assert!(empty.unsupported.is_empty());
    assert_eq!(json_body(&script.calls()[0])["create_time_start"], json!(TODAY_START - 6 * 86_400));

    let script = Arc::new(ScriptHttp::new());
    push(&script, "/order/searchList", json!({"code": 40003, "msg": "非法的参数", "sub_code": "isv.access-token-expired", "sub_msg": "access_token已过期"}));
    let auth = metrics(&http(&script), &session(), MetricRange::Today, NOW).await.unwrap_err();
    assert!(matches!(auth, CommerceError::Rejected(_)));
    assert_eq!(script.calls().len(), 1);

    let script = Arc::new(ScriptHttp::new());
    push(&script, "/order/searchList", ok(json!({"shop_order_list": [], "total": 0})));
    push(&script, "/afterSale/List", ok(json!({"items": [], "has_more": false})));
    push(&script, "/product/listV2", json!({"code": 40003, "msg": "非法的参数", "sub_code": "isv.access-token-expired", "sub_msg": "access_token已过期"}));
    let live_auth = metrics(&http(&script), &session(), MetricRange::Today, NOW).await.unwrap_err();
    assert!(matches!(live_auth, CommerceError::Rejected(_)));
}

#[tokio::test]
async fn products_and_orders_page_with_documented_indexes() {
    let script = Arc::new(ScriptHttp::new());
    let rows: Vec<Value> = (0..100)
        .map(|id| json!({"product_id": id, "name": "杯子", "status": 0, "check_status": 3, "discount_price": 1990}))
        .collect();
    push(&script, "/product/listV2", ok(json!({"data": rows, "total": 140, "page": 1, "size": 100})));
    let page = products(&http(&script), &session(), None, NOW).await.unwrap();
    assert_eq!(page.shop_id, "shop-1");
    assert_eq!(page.items.len(), 100);
    assert_eq!(page.items[0].id, "0");
    assert_eq!(page.items[0].status, "0/3");
    assert_eq!(page.items[0].price, Some(19.9));
    assert_eq!(page.items[0].currency.as_deref(), Some("CNY"));
    assert_eq!(page.next_cursor.as_deref(), Some("2"));
    assert_eq!(json_body(&script.calls()[0])["page"], json!(1));
    assert_eq!(json_body(&script.calls()[0])["size"], json!(100));

    let script = Arc::new(ScriptHttp::new());
    let invalid = products(&http(&script), &session(), Some("101"), NOW).await.unwrap_err();
    assert!(matches!(invalid, CommerceError::Invalid(_)));
    assert!(matches!(products(&http(&script), &session(), Some("abc"), NOW).await.unwrap_err(), CommerceError::Invalid(_)));
    assert!(script.calls().is_empty());

    let script = Arc::new(ScriptHttp::new());
    push(
        &script,
        "/order/searchList",
        ok(json!({
            "page": 0, "size": 100, "total": 1,
            "shop_order_list": [{"order_id": "o1", "order_status": 2, "pay_amount": 990, "doudian_open_id": "u1", "create_time": TODAY_START}]
        })),
    );
    let page = orders(&http(&script), &session(), MetricRange::Today, None, NOW).await.unwrap();
    assert_eq!(page.shop_id, "shop-1");
    assert_eq!(page.range, MetricRange::Today);
    assert!(page.next_cursor.is_none());
    assert_eq!(page.items[0].id, "o1");
    assert_eq!(page.items[0].status, "2");
    assert_eq!(page.items[0].amount, Some(9.9));
    assert_eq!(page.items[0].buyer.as_deref(), Some("u1"));
    assert_eq!(page.items[0].created_at.as_deref(), Some(chrono::DateTime::from_timestamp(TODAY_START, 0).unwrap().to_rfc3339().as_str()));
    let call = &script.calls()[0];
    assert_eq!(json_body(call)["page"], json!(0));
    assert_eq!(json_body(call)["create_time_start"], json!(TODAY_START));
    assert_eq!(json_body(call)["create_time_end"], json!(NOW));

    let script = Arc::new(ScriptHttp::new());
    assert!(matches!(orders(&http(&script), &session(), MetricRange::Today, Some("-1"), NOW).await.unwrap_err(), CommerceError::Invalid(_)));
    assert!(script.calls().is_empty());
}

#[tokio::test]
async fn ampersand_in_title_is_escaped_in_the_signed_param() {
    let script = Arc::new(ScriptHttp::new());
    push(&script, "/material/uploadImageSync", image_ready("https://p3.douyin.com/a.jpg"));
    push(&script, "/product/addV2", created(5));
    let outcome = push_listing(
        &http(&script),
        &session(),
        &draft(json!({"title": "标题&测试商品", "images": ["https://cdn.example/a.jpg"], "skus": []})),
        &target(vec![]),
        &[],
        NOW,
        None,
    )
    .await;
    assert_eq!(outcome.status, ListingStatus::Reviewing);
    let calls = script.calls();
    let add = calls.iter().find(|call| call.url.ends_with("/product/addV2")).unwrap();
    assert!(matches!(add.body, HttpBody::Empty));
    let param = query(add, "param_json");
    assert!(param.contains("\\u0026"));
    assert!(!param.contains('&'));
    assert_eq!(query(add, "sign"), sign_param(SECRET, APP_KEY, "product.addV2", param, query(add, "timestamp")).unwrap());
}
