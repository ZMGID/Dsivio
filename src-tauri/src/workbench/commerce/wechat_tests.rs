//! 回放微信小店文档里的响应形状，不是真实店铺抓包。
//! 罗盘示例里的 `"0"` 是接口给出的零；分页用的 `total_count` 按本次回放的条数对齐，避免把文档样例里的总条数当成没拉完。

use super::super::adapter::{ApiSession, HttpBody, HttpRequest, ScriptHttp, ScriptStep, SharedTransport, TransportFault};
use super::super::types::{
    AttributeInput, CommerceError, ListingAttribute, ListingDraft, ListingSku, ListingStatus, ListingTarget, MetricKey,
    MetricRange,
};
use super::{attributes, capabilities, categories, listing_status, metrics, orders, products, push_listing};
use crate::workbench::shops::Platform;
use serde_json::{json, Value};
use std::sync::Arc;

/// 2024-05-21 02:00:00 UTC，对应中国时间 10:00。罗盘示例日期是前一天 20240520。
const NOW: i64 = 1_716_256_800;
const TOKEN: &str = "token-abc";

fn shop() -> ApiSession {
    ApiSession {
        shop_id: "shop-1".into(),
        platform: Platform::Wechat,
        remote_id: "wx_appid".into(),
        name: "微信小店".into(),
        region: Some("CN".into()),
        app_id: "wx_appid".into(),
        app_secret: "app-secret".into(),
        access_token: TOKEN.into(),
        refresh_token: String::new(),
        open_key: String::new(),
        seller_secret: String::new(),
    }
}

fn harness() -> (SharedTransport, Arc<ScriptHttp>) {
    let script = Arc::new(ScriptHttp::new());
    (SharedTransport::script(script.clone()), script)
}

fn step(body: Value) -> ScriptStep {
    ScriptStep::Json { status: 200, body }
}

fn push(script: &ScriptHttp, path: &str, body: Value) {
    script.push(path, step(body));
}

fn paths(script: &ScriptHttp) -> Vec<String> {
    script.calls().iter().map(|call| call.url.trim_start_matches("https://api.weixin.qq.com").to_string()).collect()
}

fn query<'a>(call: &'a HttpRequest, key: &str) -> &'a str {
    call.query.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str()).unwrap_or("")
}

fn json_body(call: &HttpRequest) -> &Value {
    match &call.body {
        HttpBody::Json(body) => body,
        _ => panic!("expected json body"),
    }
}

fn png() -> Vec<u8> {
    let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes
}

fn cat(id: i64, name: &str, parent: i64, level: i64, leaf: bool) -> Value {
    json!({"cat": {"cat_id": id, "name": name, "f_cat_id": parent, "level": level, "leaf": leaf}})
}

fn categories_body() -> Value {
    json!({
        "errcode": 0,
        "errmsg": "ok",
        "cats": [{"cat_and_qua": [cat(1, "旧类目", 0, 1, false)]}],
        "cats_v2": [{
            "cat_and_qua": [
                cat(545709, "鹅蛋", 10000131, 3, true),
                cat(10000131, "蛋类", 10000126, 2, false),
                cat(10000126, "食品", 0, 1, false)
            ]
        }]
    })
}

fn detail(seven_day: bool, limit_brand: bool) -> Value {
    json!({
        "errcode": 0,
        "errmsg": "ok",
        "attr": {"seven_day_return": seven_day, "is_limit_brand": limit_brand}
    })
}

fn rule_body() -> Value {
    json!({
        "errcode": 0,
        "errmsg": "ok",
        "product_attr_list": [
            {"name": "国产/进口", "type": "select_one", "type_v2": "select_one", "value": "国产;进口", "required_rule": {"rule_type": 1, "or_combinators": []}},
            {"name": "产地", "type": "select_one", "type_v2": "select_one", "value": "美国;韩国;日本", "required_rule": {"rule_type": 1, "or_combinators": []}}
        ],
        "sale_attr_list": [],
        "product_qua_list": [{"name": "质检报告", "required_rule": {"rule_type": 3}}]
    })
}

fn freight_body() -> Value {
    json!({"errcode": 0, "errmsg": "ok", "template_id_list": ["1061", "2008"]})
}

fn upload_body(url: &str) -> Value {
    json!({"errcode": 0, "errmsg": "ok", "pic_file": {"img_url": url}})
}

fn err(code: i64, message: &str) -> Value {
    json!({"errcode": code, "errmsg": message})
}

fn draft() -> ListingDraft {
    ListingDraft {
        title: "任天堂 Nintendo Switch 国行续航增强版".into(),
        description: Some("国行续航增强版".into()),
        price: Some(13.0),
        currency: Some("CNY".into()),
        stock: Some(100),
        skus: vec![ListingSku {
            name: "红蓝".into(),
            price: Some(13.0),
            stock: Some(100),
            code: Some("A24525252".into()),
        }],
        images: vec!["local.jpg".into()],
        weight_kg: Some(0.4),
        dimensions_cm: None,
        brand: None,
    }
}

fn target() -> ListingTarget {
    ListingTarget {
        shop_id: "shop-1".into(),
        category_id: "545709".into(),
        attributes: vec![
            ListingAttribute { id: "国产/进口".into(), value: "国产".into(), values: Vec::new() },
            ListingAttribute { id: "产地".into(), value: "美国".into(), values: Vec::new() },
        ],
    }
}

fn queue_listing_reads(script: &ScriptHttp) {
    push(script, "/shop/ec/category/all", categories_body());
    push(script, "/shop/ec/category/detail", detail(false, false));
    push(script, "/shop/ec/category/getcategoryproductrule", rule_body());
    push(script, "/channels/ec/merchant/getfreighttemplatelist", freight_body());
}

fn images() -> Vec<(String, Vec<u8>)> {
    vec![("a.png".into(), png()), ("b.png".into(), png()), ("c.png".into(), png())]
}

fn queue_uploads(script: &ScriptHttp) {
    push(script, "/shop/ec/basics/img/upload", upload_body("https://mmecimage.cn/p/wx1/a"));
    push(script, "/shop/ec/basics/img/upload", upload_body("https://mmecimage.cn/p/wx1/b"));
    push(script, "/shop/ec/basics/img/upload", upload_body("https://mmecimage.cn/p/wx1/c"));
}

fn empty_orders() -> Value {
    json!({"errcode": 0, "errmsg": "ok", "order_id_list": [], "next_key": "", "has_more": false})
}

fn order_list_page() -> Value {
    json!({
        "errcode": 0,
        "errmsg": "ok",
        "order_id_list": ["37423523451235145"],
        "next_key": "THE_NEXT_KEY_NEW",
        "has_more": true
    })
}

fn closed_order_list() -> Value {
    json!({
        "errcode": 0,
        "errmsg": "ok",
        "order_id_list": ["37423523451235145"],
        "next_key": "",
        "has_more": false
    })
}

fn order_detail(price: bool) -> Value {
    let mut order = json!({
        "order_id": "37423523451235145",
        "status": 20,
        "create_time": 1658505600,
        "openid": "OPENID",
        "order_detail": {
            "product_infos": [{"title": "商品标题", "sku_cnt": 1, "sku_code": "A1"}]
        }
    });
    if price {
        order["order_detail"]["price_info"] = json!({"order_price": 10500});
    }
    json!({"errcode": 0, "errmsg": "ok", "order": order})
}

fn compass_zero() -> Value {
    json!({
        "errcode": 0,
        "errmsg": "ok",
        "product_list": [{
            "product_id": "123",
            "data": {"pay_gmv": "0", "pay_refund_gmv": "0", "pay_cnt": "0", "refund_cnt": "0", "pay_uv": "0"}
        }],
        "total_count": 1
    })
}

fn live(total: i64) -> Value {
    json!({"errcode": 0, "errmsg": "ok", "product_ids": [], "next_key": "", "total_num": total})
}

fn product_get(stock: bool) -> Value {
    let mut sku = json!({"sku_id": "1", "sale_price": 1, "sku_code": "A24525252"});
    if stock {
        sku["stock_num"] = json!(5);
    }
    json!({
        "errcode": 0,
        "errmsg": "ok",
        "product": {
            "product_id": "10000000000001",
            "title": "任天堂 Nintendo Switch 国行续航增强版",
            "status": 5,
            "edit_status": 2,
            "min_price": 1,
            "skus": [sku]
        }
    })
}

#[test]
fn capabilities_cover_the_store_surface() {
    let caps = capabilities();
    assert_eq!(caps.platform, Platform::Wechat);
    assert_eq!(caps.metrics, MetricKey::ALL.to_vec());
    assert!(caps.listing && caps.categories && caps.products && caps.orders);
    assert!(caps.notes.iter().any(|note| note.contains("不补 0") || note.contains("也不记为 0")));
}

#[tokio::test]
async fn categories_prefer_cats_v2_and_filter_by_parent() {
    let (transport, script) = harness();
    push(&script, "/shop/ec/category/all", categories_body());
    let all = categories(&transport, &shop(), None, NOW).await.expect("categories");
    assert_eq!(all.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), ["545709", "10000131", "10000126"]);
    assert!(all[0].leaf);
    assert!(!all[1].leaf);
    assert_eq!(all[0].parent_id, "10000131");
    assert_eq!(all[2].parent_id, "0");
    let calls = script.calls();
    let call = &calls[0];
    assert_eq!(call.method, "POST");
    assert_eq!(query(call, "access_token"), TOKEN);
    assert!(query(call, "sign").is_empty());
    assert!(call.url.starts_with("https://api.weixin.qq.com/shop/ec/category/all"));
    assert!(!call.mutating);
    assert_eq!(json_body(call), &json!({}));

    let (transport, script) = harness();
    push(&script, "/shop/ec/category/all", categories_body());
    let leaf = categories(&transport, &shop(), Some("10000131"), NOW).await.expect("leaf");
    assert_eq!(leaf.len(), 1);
    assert_eq!(leaf[0].id, "545709");

    let (transport, script) = harness();
    push(&script, "/shop/ec/category/all", categories_body());
    let roots = categories(&transport, &shop(), Some("0"), NOW).await.expect("roots");
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].id, "10000126");
}

#[tokio::test]
async fn category_error_is_rejected_and_other_platform_makes_no_call() {
    let (transport, script) = harness();
    push(&script, "/shop/ec/category/all", err(9401020, "类目不存在"));
    let error = categories(&transport, &shop(), None, NOW).await.expect_err("rejected");
    assert!(matches!(error, CommerceError::Rejected(_)));
    assert!(error.message().contains("9401020"));

    let (transport, script) = harness();
    let mut other = shop();
    other.platform = Platform::Shopee;
    let error = categories(&transport, &other, None, NOW).await.expect_err("unsupported");
    assert!(matches!(error, CommerceError::Unsupported(_)));
    assert!(script.calls().is_empty());

    let (transport, script) = harness();
    let mut empty = shop();
    empty.access_token.clear();
    let error = categories(&transport, &empty, None, NOW).await.expect_err("token");
    assert!(matches!(error, CommerceError::Rejected(_)));
    assert!(script.calls().is_empty());
}

#[tokio::test]
async fn attributes_use_release_rules_and_skip_qualifications() {
    let (transport, script) = harness();
    push(&script, "/shop/ec/category/getcategoryproductrule", rule_body());
    let attrs = attributes(&transport, &shop(), "546776", NOW).await.expect("attrs");
    assert_eq!(attrs.len(), 2);
    assert_eq!(attrs[0].name, "国产/进口");
    assert!(attrs[0].required);
    assert_eq!(attrs[0].input, AttributeInput::Select);
    assert_eq!(attrs[0].options.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(), ["国产", "进口"]);
    assert_eq!(attrs[1].name, "产地");
    assert!(attrs.iter().all(|item| item.name != "质检报告"));
    let calls = script.calls();
    let call = &calls[0];
    assert_eq!(json_body(call)["cat_id"].as_i64(), Some(546776));
    assert_eq!(json_body(call)["release_mode"].as_i64(), Some(0));
    assert!(query(call, "sign").is_empty());

    let (transport, script) = harness();
    let error = attributes(&transport, &shop(), "abc", NOW).await.expect_err("invalid");
    assert!(matches!(error, CommerceError::Invalid(_)));
    assert!(script.calls().is_empty());
}

#[test]
fn documented_type_v2_maps_units_without_making_optional_required() {
    let body = json!({
        "product_attr_list": [{
            "name": "重量",
            "type": "string",
            "type_v2": "integer_unit",
            "value": "mg;g;kg",
            "required_rule": {"rule_type": 2}
        }]
    });
    let attrs = super::attribute_list(&body);
    assert_eq!(attrs.len(), 1);
    assert_eq!(attrs[0].input, AttributeInput::Number);
    assert!(!attrs[0].required);
    assert_eq!(attrs[0].options.len(), 3);
    assert!(attrs[0].unit.is_none());
}

#[tokio::test]
async fn push_submits_listing_with_sku_category_and_uploaded_images() {
    let (transport, script) = harness();
    queue_listing_reads(&script);
    queue_uploads(&script);
    push(&script, "/channels/ec/product/add", json!({
        "errcode": 0,
        "errmsg": "ok",
        "data": {"product_id": "10000000000001", "create_time": "2026-06-09 21:00:29"}
    }));
    let outcome = push_listing(&transport, &shop(), &draft(), &target(), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Reviewing);
    assert_eq!(outcome.remote_id.as_deref(), Some("10000000000001"));
    assert!(outcome.reason.is_none());
    let calls = script.calls();
    assert_eq!(
        paths(&script),
        vec![
            "/shop/ec/category/all",
            "/shop/ec/category/detail",
            "/shop/ec/category/getcategoryproductrule",
            "/channels/ec/merchant/getfreighttemplatelist",
            "/shop/ec/basics/img/upload",
            "/shop/ec/basics/img/upload",
            "/shop/ec/basics/img/upload",
            "/channels/ec/product/add",
        ]
    );
    assert!(calls.iter().all(|call| query(call, "access_token") == TOKEN && query(call, "sign").is_empty()));
    assert!(calls.iter().all(|call| call.method == "POST" && call.url.starts_with("https://api.weixin.qq.com")));
    assert!(paths(&script).iter().all(|path| !path.contains("cgi-bin") && !path.contains("/product/listing")));
    let upload = &calls[4];
    assert!(!upload.mutating);
    assert_eq!(query(upload, "upload_type"), "0");
    assert_eq!(query(upload, "resp_type"), "1");
    assert_eq!(query(upload, "width"), "1");
    assert_eq!(query(upload, "height"), "1");
    match &upload.body {
        HttpBody::Multipart(file) => {
            assert_eq!(file.field, "media");
            assert_eq!(file.file_name, "a.png");
            assert_eq!(file.bytes, png());
        }
        _ => panic!("upload is multipart"),
    }
    let add = &calls[7];
    assert!(add.mutating);
    let body = json_body(add);
    assert_eq!(body["listing"].as_i64(), Some(1));
    assert_eq!(body["title"], "任天堂 Nintendo Switch 国行续航增强版");
    assert_eq!(body["deliver_method"].as_i64(), Some(0));
    assert_eq!(body["brand_id"], "2100000000");
    assert_eq!(body["cats"], json!([]));
    assert_eq!(body["cats_v2"], json!([{"cat_id": 10000126}, {"cat_id": 10000131}, {"cat_id": 545709}]));
    assert_eq!(body["extra_service"]["seven_day_return"].as_i64(), Some(0));
    assert_eq!(body["extra_service"]["freight_insurance"].as_i64(), Some(0));
    assert_eq!(body["express_info"]["template_id"], "1061");
    assert_eq!(body["express_info"]["weight"].as_i64(), Some(400));
    assert_eq!(
        body["head_imgs"],
        json!(["https://mmecimage.cn/p/wx1/a", "https://mmecimage.cn/p/wx1/b", "https://mmecimage.cn/p/wx1/c"])
    );
    assert_eq!(body["desc_info"]["imgs"], body["head_imgs"]);
    assert_eq!(body["attrs"][0]["attr_key"], "国产/进口");
    assert_eq!(body["attrs"][0]["attr_value"], "国产");
    assert_eq!(body["attrs"][1]["attr_value"], "美国");
    assert_eq!(body["skus"][0]["sale_price"].as_i64(), Some(1300));
    assert_eq!(body["skus"][0]["stock_num"].as_i64(), Some(100));
    assert_eq!(body["skus"][0]["sku_code"], "A24525252");
    assert_eq!(body["skus"][0]["out_sku_id"], "A24525252");
    assert_eq!(body["skus"][0]["sku_deliver_info"]["stock_type"].as_i64(), Some(0));
    assert_eq!(body["skus"][0]["sku_attrs"][0]["attr_key"], "规格");
    assert_eq!(body["skus"][0]["sku_attrs"][0]["attr_value"], "红蓝");
    assert_eq!(body["skus"][0]["thumb_img"], "https://mmecimage.cn/p/wx1/a");
}

#[tokio::test]
async fn store_urls_skip_upload_and_update_keeps_the_existing_id() {
    let (transport, script) = harness();
    queue_listing_reads(&script);
    push(&script, "/channels/ec/product/update", json!({"errcode": 0, "errmsg": "ok", "data": {"update_time": "2026-06-09 21:00:29"}}));
    let mut draft = draft();
    draft.images = vec![
        "https://mmecimage.cn/p/wx1/a".into(),
        "https://mmecimage.cn/p/wx1/b".into(),
        "https://mmecimage.cn/p/wx1/c".into(),
    ];
    let outcome = push_listing(&transport, &shop(), &draft, &target(), &[], NOW, Some("10000000000001")).await;
    assert_eq!(outcome.status, ListingStatus::Reviewing);
    assert_eq!(outcome.remote_id.as_deref(), Some("10000000000001"));
    assert_eq!(paths(&script).last().map(String::as_str), Some("/channels/ec/product/update"));
    assert!(paths(&script).iter().all(|path| path != "/shop/ec/basics/img/upload"));
    let calls = script.calls();
    let body = json_body(calls.last().expect("update"));
    assert_eq!(body["product_id"], "10000000000001");
    assert_eq!(body["listing"].as_i64(), Some(1));
    assert_eq!(body["head_imgs"][0], "https://mmecimage.cn/p/wx1/a");
}

#[tokio::test]
async fn push_api_error_is_rejected_and_dispatch_fault_is_uncertain() {
    let (transport, script) = harness();
    queue_listing_reads(&script);
    queue_uploads(&script);
    push(&script, "/channels/ec/product/add", err(10020018, "没有该类目的权限"));
    let outcome = push_listing(&transport, &shop(), &draft(), &target(), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.remote_id.is_none());
    assert!(outcome.reason.unwrap().contains("10020018"));

    let (transport, script) = harness();
    queue_listing_reads(&script);
    queue_uploads(&script);
    script.push("/channels/ec/product/add", ScriptStep::Fault(TransportFault::AfterDispatch("reset".into())));
    let outcome = push_listing(&transport, &shop(), &draft(), &target(), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Uncertain);
    assert!(outcome.remote_id.is_none());
    assert_eq!(paths(&script).iter().filter(|path| path.ends_with("/product/add")).count(), 1);

    let (transport, script) = harness();
    queue_listing_reads(&script);
    script.push(
        "/channels/ec/product/update",
        ScriptStep::Fault(TransportFault::AfterDispatch("reset".into())),
    );
    let mut draft = draft();
    draft.images = vec![
        "https://mmecimage.cn/p/wx1/a".into(),
        "https://mmecimage.cn/p/wx1/b".into(),
        "https://mmecimage.cn/p/wx1/c".into(),
    ];
    let outcome = push_listing(&transport, &shop(), &draft, &target(), &[], NOW, Some("10000000000001")).await;
    assert_eq!(outcome.status, ListingStatus::Uncertain);
    assert_eq!(outcome.remote_id.as_deref(), Some("10000000000001"));
}

#[tokio::test]
async fn upload_error_stops_before_add_and_local_checks_make_no_call() {
    let (transport, script) = harness();
    queue_listing_reads(&script);
    push(&script, "/shop/ec/basics/img/upload", err(10020056, "图片格式不支持"));
    let outcome = push_listing(&transport, &shop(), &draft(), &target(), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.reason.unwrap().contains("10020056"));
    assert!(paths(&script).iter().all(|path| path != "/channels/ec/product/add"));

    let (transport, script) = harness();
    let outcome = push_listing(&transport, &shop(), &draft(), &target(), &images()[..2], NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(script.calls().is_empty());

    let (transport, script) = harness();
    let mut priced = draft();
    priced.currency = Some("USD".into());
    let outcome = push_listing(&transport, &shop(), &priced, &target(), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(script.calls().is_empty());

    let (transport, script) = harness();
    let mut empty = shop();
    empty.access_token.clear();
    let outcome = push_listing(&transport, &empty, &draft(), &target(), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(script.calls().is_empty());
}

#[tokio::test]
async fn missing_required_attr_stops_before_freight_and_upload() {
    let (transport, script) = harness();
    push(&script, "/shop/ec/category/all", categories_body());
    push(&script, "/shop/ec/category/detail", detail(false, false));
    push(&script, "/shop/ec/category/getcategoryproductrule", rule_body());
    let mut target = target();
    target.attributes.pop();
    let outcome = push_listing(&transport, &shop(), &draft(), &target, &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.reason.unwrap().contains("产地"));
    assert_eq!(
        paths(&script),
        vec!["/shop/ec/category/all", "/shop/ec/category/detail", "/shop/ec/category/getcategoryproductrule"]
    );
}

#[tokio::test]
async fn listing_status_maps_online_and_edit_state() {
    let cases = [
        (json!({"status": 5, "edit_status": 2}), ListingStatus::Reviewing, None),
        (json!({"status": 5, "edit_status": 4}), ListingStatus::Live, None),
        (json!({"status": 20, "edit_status": 4}), ListingStatus::Banned, Some("封禁下架")),
        (json!({"status": 11, "edit_status": 4}), ListingStatus::Live, Some("自主下架")),
        (json!({"edit_status": 8}), ListingStatus::Failed, Some("商品异步提交上传失败")),
        (json!({"status": 99, "edit_status": 4}), ListingStatus::Uncertain, Some("未识别的商品状态 99")),
    ];
    for (product, status, reason) in cases {
        let (transport, script) = harness();
        let body = json!({"errcode": 0, "errmsg": "ok", "product": product});
        push(&script, "/channels/ec/product/get", body);
        let (got, got_reason) = listing_status(&transport, &shop(), "10000000000001", NOW).await.expect("status");
        assert_eq!(got, status);
        assert_eq!(got_reason.as_deref(), reason);
        assert_eq!(json_body(&script.calls()[0])["data_type"].as_i64(), Some(3));
        assert!(!script.calls()[0].mutating);
    }

    let (transport, script) = harness();
    push(
        &script,
        "/channels/ec/product/get",
        json!({
            "errcode": 0,
            "errmsg": "ok",
            "product": {"status": 5, "edit_status": 3},
            "info_score": {"sub_score_list": [{"audit_remark": "标题不合规"}]}
        }),
    );
    let (status, reason) = listing_status(&transport, &shop(), "10000000000001", NOW).await.expect("rejected");
    assert_eq!(status, ListingStatus::Rejected);
    assert_eq!(reason.as_deref(), Some("标题不合规"));
}

#[tokio::test]
async fn listing_status_not_found_and_read_fault_is_failed() {
    let (transport, _) = harness();
    let error = listing_status(&transport, &shop(), "  ", NOW).await.expect_err("empty");
    assert!(matches!(error, CommerceError::Invalid(_)));

    let (transport, script) = harness();
    push(&script, "/channels/ec/product/get", err(10020052, "商品不存在"));
    let error = listing_status(&transport, &shop(), "10000000000001", NOW).await.expect_err("missing");
    assert!(matches!(error, CommerceError::NotFound(_)));

    let (transport, script) = harness();
    script.push("/channels/ec/product/get", ScriptStep::Fault(TransportFault::AfterDispatch("reset".into())));
    let error = listing_status(&transport, &shop(), "10000000000001", NOW).await.expect_err("fault");
    assert!(matches!(error, CommerceError::Failed(_)));
    assert!(!matches!(error, CommerceError::Uncertain(_)));
}

#[tokio::test]
async fn products_page_hydrates_price_and_leaves_missing_stock_empty() {
    let (transport, script) = harness();
    push(&script, "/channels/ec/product/list/get", json!({
        "errcode": 0,
        "errmsg": "ok",
        "product_ids": ["10000000000001"],
        "next_key": "NEXT",
        "total_num": 1
    }));
    push(&script, "/channels/ec/product/get", product_get(true));
    let page = products(&transport, &shop(), None, NOW).await.expect("page");
    assert_eq!(page.shop_id, "shop-1");
    assert_eq!(page.next_cursor.as_deref(), Some("NEXT"));
    assert_eq!(page.items[0].id, "10000000000001");
    assert_eq!(page.items[0].status, "5/2");
    assert_eq!(page.items[0].price, Some(1.0 / 100.0));
    assert_eq!(page.items[0].currency.as_deref(), Some("CNY"));
    assert_eq!(page.items[0].stock, Some(5));
    assert_eq!(page.items[0].sku.as_deref(), Some("A24525252"));
    assert_eq!(json_body(&script.calls()[0])["page_size"].as_i64(), Some(10));
    assert!(json_body(&script.calls()[0]).get("next_key").is_none());
    assert_eq!(json_body(&script.calls()[1])["data_type"].as_i64(), Some(3));

    let (transport, script) = harness();
    push(&script, "/channels/ec/product/list/get", json!({
        "errcode": 0, "errmsg": "ok", "product_ids": ["10000000000001"], "next_key": "NEXT", "total_num": 1
    }));
    push(&script, "/channels/ec/product/get", product_get(false));
    let page = products(&transport, &shop(), Some("NEXT"), NOW).await.expect("page");
    assert!(page.next_cursor.is_none());
    assert!(page.items[0].stock.is_none());
    assert_eq!(json_body(&script.calls()[0])["next_key"], "NEXT");
}

#[tokio::test]
async fn orders_page_uses_seven_day_windows_and_does_not_invent_amount() {
    let (transport, script) = harness();
    push(&script, "/channels/ec/order/list/get", order_list_page());
    push(&script, "/channels/ec/order/get", order_detail(true));
    let page = orders(&transport, &shop(), MetricRange::Yesterday, None, NOW).await.expect("orders");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, "37423523451235145");
    assert_eq!(page.items[0].status, "20");
    assert_eq!(page.items[0].amount, Some(105.0));
    assert_eq!(page.items[0].currency.as_deref(), Some("CNY"));
    assert_eq!(page.items[0].buyer.as_deref(), Some("OPENID"));
    let created = chrono::DateTime::from_timestamp(1_658_505_600, 0).unwrap().to_rfc3339();
    assert_eq!(page.items[0].created_at.as_deref(), Some(created.as_str()));
    assert_eq!(page.items[0].lines[0].title, "商品标题");
    assert_eq!(page.items[0].lines[0].quantity, 1);
    assert_eq!(page.next_cursor.as_deref(), Some("0:THE_NEXT_KEY_NEW"));
    let calls = script.calls();
    let listed = json_body(&calls[0]);
    assert!(listed.get("next_key").is_none());
    let (start, end) = super::metric_window(MetricRange::Yesterday, NOW, 8 * 3600);
    assert_eq!(listed["create_time_range"]["start_time"].as_i64(), Some(start));
    assert_eq!(listed["create_time_range"]["end_time"].as_i64(), Some(end));
    assert!(end - start < 7 * 86_400);
    assert_eq!(json_body(&script.calls()[1])["order_id"], "37423523451235145");

    let (transport, script) = harness();
    push(&script, "/channels/ec/order/list/get", order_list_page());
    push(&script, "/channels/ec/order/get", order_detail(false));
    let page = orders(&transport, &shop(), MetricRange::Yesterday, None, NOW).await.expect("orders");
    assert!(page.items[0].amount.is_none());
    assert!(page.items[0].currency.is_none());

    let (transport, script) = harness();
    push(&script, "/channels/ec/order/list/get", empty_orders());
    let windows = super::order_windows(MetricRange::Last30, NOW);
    assert!(windows.len() > 1);
    assert!(windows[0].1 - windows[0].0 <= 7 * 86_400 - 1);
    let page = orders(&transport, &shop(), MetricRange::Last30, None, NOW).await.expect("chunk");
    assert_eq!(page.next_cursor.as_deref(), Some("1:"));
    assert_eq!(json_body(&script.calls()[0])["create_time_range"]["start_time"].as_i64(), Some(windows[0].0));

    let (transport, _) = harness();
    let error = orders(&transport, &shop(), MetricRange::Yesterday, Some("nope"), NOW).await.expect_err("cursor");
    assert!(matches!(error, CommerceError::Invalid(_)));

    let (transport, script) = harness();
    push(&script, "/channels/ec/order/get", err(100002, "订单不存在"));
    push(&script, "/channels/ec/order/list/get", order_list_page());
    let error = orders(&transport, &shop(), MetricRange::Yesterday, None, NOW).await.expect_err("missing");
    assert!(matches!(error, CommerceError::NotFound(_)));
}

#[tokio::test]
async fn yesterday_metrics_keep_compass_zeros_and_ignore_pay_uv() {
    let (transport, script) = harness();
    push(&script, "/channels/ec/order/list/get", closed_order_list());
    push(&script, "/channels/ec/order/get", order_detail(true));
    push(&script, "/channels/ec/compass/shop/product/list/get", compass_zero());
    push(&script, "/channels/ec/product/list/get", live(8));
    let metrics = metrics(&transport, &shop(), MetricRange::Yesterday, NOW).await.expect("metrics");
    assert_eq!(metrics.shop_id, "shop-1");
    assert_eq!(metrics.currency.as_deref(), Some("CNY"));
    assert_eq!(metrics.values.get(&MetricKey::Gmv), Some(&105.0));
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&1.0));
    assert_eq!(metrics.values.get(&MetricKey::Buyers), Some(&1.0));
    assert_eq!(metrics.values.get(&MetricKey::PendingShipment), Some(&1.0));
    assert_eq!(metrics.values.get(&MetricKey::RefundAmount), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::RefundOrders), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::ProductsLive), Some(&8.0));
    assert!(metrics.unsupported.is_empty());
    assert_eq!(json_body(&script.calls()[2])["ds"], "20240520");
    assert!(paths(&script).iter().all(|path| !path.contains("cgi-bin")));
}

#[tokio::test]
async fn today_omits_refunds_and_keeps_a_real_zero_live_count() {
    let (transport, script) = harness();
    push(&script, "/channels/ec/order/list/get", empty_orders());
    push(&script, "/channels/ec/product/list/get", live(0));
    let metrics = metrics(&transport, &shop(), MetricRange::Today, NOW).await.expect("metrics");
    assert_eq!(metrics.values.get(&MetricKey::Gmv), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::Buyers), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::PendingShipment), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::ProductsLive), Some(&0.0));
    assert!(metrics.values.get(&MetricKey::RefundAmount).is_none());
    assert!(metrics.values.get(&MetricKey::RefundOrders).is_none());
    assert!(metrics.unsupported.contains(&MetricKey::RefundAmount));
    assert!(metrics.unsupported.contains(&MetricKey::RefundOrders));
    assert!(!metrics.unsupported.contains(&MetricKey::ProductsLive));
    assert!(!metrics.unsupported.contains(&MetricKey::Gmv));
    assert!(paths(&script).iter().all(|path| path != "/channels/ec/compass/shop/product/list/get"));
}

#[tokio::test]
async fn missing_refund_fields_and_incomplete_pages_are_not_zero() {
    let (transport, script) = harness();
    push(&script, "/channels/ec/order/list/get", empty_orders());
    push(&script, "/channels/ec/compass/shop/product/list/get", json!({
        "errcode": 0,
        "errmsg": "ok",
        "product_list": [{"product_id": "123", "data": {"refund_cnt": "0"}}],
        "total_count": 1
    }));
    push(&script, "/channels/ec/product/list/get", live(3));
    let report = metrics(&transport, &shop(), MetricRange::Yesterday, NOW).await.expect("metrics");
    assert!(report.values.get(&MetricKey::RefundAmount).is_none());
    assert!(report.unsupported.contains(&MetricKey::RefundAmount));
    assert_eq!(report.values.get(&MetricKey::RefundOrders), Some(&0.0));
    assert!(!report.unsupported.contains(&MetricKey::RefundOrders));

    let (transport, script) = harness();
    push(&script, "/channels/ec/order/list/get", empty_orders());
    push(&script, "/channels/ec/compass/shop/product/list/get", json!({
        "errcode": 0,
        "errmsg": "ok",
        "product_list": [{"product_id": "123", "data": {"pay_refund_gmv": "500", "refund_cnt": "1"}}],
        "total_count": 2
    }));
    push(&script, "/channels/ec/product/list/get", live(1));
    let metrics = metrics(&transport, &shop(), MetricRange::Yesterday, NOW).await.expect("metrics");
    assert!(metrics.values.get(&MetricKey::RefundAmount).is_none());
    assert!(metrics.values.get(&MetricKey::RefundOrders).is_none());
    assert_ne!(metrics.values.get(&MetricKey::RefundAmount), Some(&5.0));
    assert!(metrics.unsupported.contains(&MetricKey::RefundAmount));
    assert!(metrics.error.is_some());
    assert_eq!(metrics.values.get(&MetricKey::Gmv), Some(&0.0));
    let _ = script;
}

#[tokio::test]
async fn unknown_order_status_and_auth_do_not_become_zero_metrics() {
    let (transport, script) = harness();
    push(&script, "/channels/ec/order/list/get", closed_order_list());
    let mut detail = order_detail(true);
    detail["order"]["status"] = json!(999);
    push(&script, "/channels/ec/order/get", detail);
    push(&script, "/channels/ec/product/list/get", live(1));
    let report = metrics(&transport, &shop(), MetricRange::Today, NOW).await.expect("metrics");
    assert!(report.values.get(&MetricKey::Gmv).is_none());
    assert!(report.values.get(&MetricKey::Orders).is_none());
    assert!(report.unsupported.contains(&MetricKey::Gmv));
    assert!(report.unsupported.contains(&MetricKey::Orders));
    assert_eq!(report.values.get(&MetricKey::ProductsLive), Some(&1.0));
    assert!(report.error.as_deref().unwrap().contains("无法归类"));
    let _ = script;

    let (transport, script) = harness();
    push(&script, "/channels/ec/order/list/get", err(40001, "invalid credential"));
    let error = metrics(&transport, &shop(), MetricRange::Today, NOW).await.expect_err("auth");
    assert!(matches!(error, CommerceError::Rejected(_)));
    assert!(error.message().contains("40001"));
    assert_eq!(script.calls().len(), 1);
}

#[test]
fn status_fold_and_dates_follow_the_store_rules() {
    let empty = super::fold_orders(&[]).expect("empty");
    assert_eq!((empty.orders, empty.gmv_fen, empty.buyers, empty.pending), (0.0, Some(0), Some(0.0), 0.0));
    let missing = super::fold_orders(&[super::OrderView { status: 20, amount_fen: None, buyer: Some("a".into()) }]).expect("missing");
    assert_eq!(missing.orders, 1.0);
    assert_eq!(missing.gmv_fen, None);
    assert_eq!(missing.buyers, Some(1.0));
    assert_eq!(missing.pending, 1.0);
    let unpaid = super::fold_orders(&[super::OrderView { status: 10, amount_fen: Some(100), buyer: Some("a".into()) }]).expect("unpaid");
    assert_eq!(unpaid.orders, 0.0);
    let buyers = super::fold_orders(&[
        super::OrderView { status: 100, amount_fen: Some(100), buyer: Some("a".into()) },
        super::OrderView { status: 30, amount_fen: Some(50), buyer: Some("a".into()) },
    ])
    .expect("buyers");
    assert_eq!(buyers.buyers, Some(1.0));
    assert_eq!(buyers.gmv_fen, Some(150));
    assert!(super::fold_orders(&[super::OrderView { status: 999, amount_fen: Some(1), buyer: None }]).is_none());

    assert_eq!(super::map_product_status(Some(5), Some(2), None).unwrap().0, ListingStatus::Reviewing);
    assert_eq!(super::map_product_status(Some(5), Some(4), None).unwrap().0, ListingStatus::Live);
    assert_eq!(super::map_product_status(Some(13), Some(4), None).unwrap().0, ListingStatus::Banned);
    assert_eq!(super::map_product_status(Some(6), None, None).unwrap().1.as_deref(), Some("回收站"));
    assert_eq!(super::map_product_status(Some(0), None, None).unwrap().0, ListingStatus::Reviewing);
    assert_eq!(super::map_product_status(None, Some(1), None).unwrap().0, ListingStatus::Failed);
    assert_eq!(super::map_product_status(None, Some(4), None).unwrap().0, ListingStatus::Reviewing);
    assert_eq!(super::map_product_status(Some(99), None, None).unwrap().0, ListingStatus::Uncertain);
    assert!(super::map_product_status(None, None, None).is_none());

    let (yesterday, includes_today) = super::compass_dates(MetricRange::Yesterday, NOW).expect("yesterday");
    assert_eq!(yesterday, vec!["20240520".to_string()]);
    assert!(!includes_today);
    let (today, includes_today) = super::compass_dates(MetricRange::Today, NOW).expect("today");
    assert!(today.is_empty() && includes_today);
    let (last7, includes_today) = super::compass_dates(MetricRange::Last7, NOW).expect("last7");
    assert_eq!(
        last7,
        vec!["20240515", "20240516", "20240517", "20240518", "20240519", "20240520"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
    );
    assert!(includes_today);
    let (last30, _) = super::compass_dates(MetricRange::Last30, NOW).expect("last30");
    assert_eq!(last30.len(), 29);
    assert_eq!(last30.last().map(String::as_str), Some("20240520"));
    assert_eq!(super::image_size(&png()).expect("png"), (1, 1));
    assert!(super::image_size(&[0xFF, 0xD8, 0xFF, 0xD9]).is_err());
}
