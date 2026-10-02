use super::super::transport::{Outbound, ResolvedShop, ScriptTransport, Step, Transport, TransportFault};
use super::super::types::{AttributeInput, CommerceError, ListingDraft, ListingStatus, ListingTarget, MetricKey, MetricRange};
use super::{fetch_attributes, fetch_categories, fetch_metrics, fetch_orders, fetch_products, push_listing, refresh_remote, sign_form};
use crate::workbench::shops::Platform;
use serde_json::{json, Value};
use std::sync::Arc;

const NOW: i64 = 1_790_899_200;

fn shop() -> ResolvedShop {
    ResolvedShop {
        platform: Platform::Kuaishou,
        partner_id: "ks-app".into(),
        partner_key: "ks-secret".into(),
        access_token: "token-abc".into(),
        remote_id: "2589256601".into(),
        region: None,
        name: "快手店".into(),
    }
}

fn other_shop() -> ResolvedShop {
    let mut shop = shop();
    shop.platform = Platform::Shopee;
    shop
}

fn harness() -> (Transport, Arc<ScriptTransport>) {
    let script = Arc::new(ScriptTransport::new());
    (Transport::script(script.clone()), script)
}

fn ok(data: Value) -> Step {
    Step::Json(json!({"result": 1, "error_msg": "success", "data": data}))
}

fn param(call: &Outbound) -> Value {
    call.query
        .iter()
        .find(|(key, _)| key == "param")
        .and_then(|(_, value)| serde_json::from_str(value).ok())
        .unwrap_or(Value::Null)
}

fn draft(price: f64) -> ListingDraft {
    serde_json::from_value(json!({
        "title": "美式复古醒酸美裙",
        "description": "新款夏季半身裙",
        "price": price,
        "currency": "CNY",
        "stock": 100,
        "images": ["cup.jpg"],
        "skus": [
            {"name": "白色", "price": 39.9, "stock": 100, "code": "WHITE"},
            {"name": "黑色", "price": 42.0, "stock": 8, "code": "BLACK"}
        ]
    }))
    .expect("draft")
}

fn target() -> ListingTarget {
    serde_json::from_value(json!({"shopId": "shop-1", "categoryId": "1609"})).expect("target")
}

fn queue_create(script: &ScriptTransport, item: Value) {
    script.push(
        "/open/logistics/express/template/list",
        ok(json!({"expressTemplateDetailVOS": [{"id": 9426102401_i64, "name": "默认非偏远地区包邮模板", "status": 1, "deleteTime": 0}], "total": 1})),
    );
    script.push(
        "/open/item/image/upload",
        ok(json!({"originImgUrl": "local", "kwaiImgUrl": "https://p2-ec.ecukwai.com/bs2/image-kwaishop-product/ITEM_IMAGE-1.jpg"})),
    );
    script.push(
        "/open/item/category/config",
        ok(json!({"categoryConfig": {"refundRuleList": [1]}, "propConfigs": []})),
    );
    script.push("/open/item/new", ok(item));
}

#[test]
fn sign_matches_the_shop_hmac_base64_pattern() {
    let pairs = vec![
        ("method".into(), "open.item.category".into()),
        ("appkey".into(), "ks-app".into()),
        ("access_token".into(), "token-abc".into()),
        ("version".into(), "1".into()),
        ("signMethod".into(), "HMAC-SHA256".into()),
        ("timestamp".into(), "1790899200000".into()),
    ];
    let sign = sign_form("ks-secret", &pairs).expect("sign");
    assert_eq!(sign, "9lT8m0NV1NiJRlNH4T8PwK1sEutLkIOt2fHP55PMFeo=");
}

#[tokio::test]
async fn categories_map_the_documented_tree_and_parent_filter() {
    let (transport, script) = harness();
    let body = json!({"result": 1, "error_msg": "success", "data": [
        {"categoryId": 1107, "categoryName": "家居生活", "categoryPid": 0},
        {"categoryId": 1124, "categoryName": "居家日用", "categoryPid": 1107},
        {"categoryId": 2988, "categoryName": "生活日用", "categoryPid": 1124}
    ]});
    script.push("/open/item/category", Step::Json(body.clone()));
    let all = fetch_categories(&transport, &shop(), NOW, None).await.expect("categories");
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].id, "1107");
    assert!(!all[0].leaf);
    assert!(all[2].leaf);
    assert_eq!(all[2].parent_id, "1124");
    let call = &script.calls()[0];
    assert_eq!(call.host, "https://openapi.kwaixiaodian.com");
    assert_eq!(call.path, "/open/item/category");
    assert!(call.query.iter().any(|(key, value)| key == "sign" && value == "9lT8m0NV1NiJRlNH4T8PwK1sEutLkIOt2fHP55PMFeo="));
    assert!(call.query.iter().any(|(key, _)| key == "param") == false);

    script.push("/open/item/category", Step::Json(body));
    let children = fetch_categories(&transport, &shop(), NOW, Some("1124")).await.expect("children");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "生活日用");
}

#[tokio::test]
async fn attributes_map_radio_checkbox_and_numeric_text() {
    let (transport, script) = harness();
    script.push(
        "/open/item/category/config",
        ok(json!({"categoryConfig": {"refundRuleList": [1]}, "propConfigs": [
            {
                "propId": 102,
                "propName": "品牌",
                "required": true,
                "propInputType": "RADIO",
                "prePropValues": [{"propValueId": 2654018, "propValue": "穆之宜"}],
                "unitProp": []
            },
            {"propId": 5544, "propName": "面料材质", "required": true, "propInputType": "CHECKBOX", "prePropValues": [], "unitProp": []},
            {
                "propId": 5032,
                "propName": "克重",
                "required": false,
                "propInputType": "TEXT",
                "propInputConfig": {"inputFormatConfig": {"patternList": ["([1-9][0-9]*(\\.[0-9]+)?|0\\.[0-9]*[1-9][0-9]*)"]}},
                "unitProp": [{"unitPropValueId": 339170972, "unitPropValueName": "g"}]
            },
            {"propId": 6484, "propName": "吊牌图", "required": false, "propInputType": "IMAGE", "unitProp": []}
        ]})),
    );
    let attributes = fetch_attributes(&transport, &shop(), NOW, "1609").await.expect("attributes");
    assert_eq!(attributes.len(), 4);
    assert_eq!(attributes[0].input, AttributeInput::Select);
    assert!(attributes[0].required);
    assert_eq!(attributes[0].options[0].name, "穆之宜");
    assert_eq!(attributes[1].input, AttributeInput::MultiSelect);
    assert_eq!(attributes[2].input, AttributeInput::Number);
    assert_eq!(attributes[2].unit.as_deref(), Some("g"));
    assert_eq!(attributes[3].input, AttributeInput::Text);
    assert_eq!(param(&script.calls()[0])["categoryId"], json!(1609));
}

#[tokio::test]
async fn push_uploads_image_and_creates_sku_variants() {
    let (transport, script) = harness();
    queue_create(
        &script,
        json!({"kwaiItemId": 24904424073914_i64, "relItemId": 1, "skuIdMapping": [
            {"kwaiSkuId": 158419190357914_i64, "relSkuId": 1},
            {"kwaiSkuId": 158419190357915_i64, "relSkuId": 2}
        ]}),
    );
    let outcome = push_listing(
        &transport,
        &shop(),
        &draft(39.9),
        &target(),
        &[("cup.jpg".into(), b"\xff\xd8\xff\xd9".to_vec())],
        NOW,
        None,
    )
    .await;
    assert_eq!(outcome.status, ListingStatus::Reviewing);
    assert_eq!(outcome.remote_id.as_deref(), Some("24904424073914"));
    assert!(outcome.reason.is_none());
    let calls = script.calls();
    assert_eq!(calls[0].path, "/open/logistics/express/template/list");
    assert!(!calls[0].mutating);
    assert_eq!(calls[1].path, "/open/item/image/upload");
    assert_eq!(calls[1].file_bytes.as_deref(), Some(&b"\xff\xd8\xff\xd9".to_vec()[..]));
    assert!(calls[1].headers.iter().any(|(key, value)| key == "x-dsivio-file-field" && value == "imgBytes"));
    assert_eq!(param(&calls[1])["uploadType"], json!(1));
    assert!(calls[3].mutating);
    let created = param(&calls[3]);
    assert_eq!(created["categoryId"], json!(1609));
    assert_eq!(created["expressTemplateId"], json!(9426102401_i64));
    assert_eq!(created["imageUrls"][0], "https://p2-ec.ecukwai.com/bs2/image-kwaishop-product/ITEM_IMAGE-1.jpg");
    assert_eq!(created["serviceRule"]["refundRule"], "1");
    assert_eq!(created["serviceRule"]["immediatelyOnOfflineFlag"], json!(0));
    assert_eq!(created["skuList"].as_array().map(Vec::len), Some(2));
    assert_eq!(created["skuList"][0]["skuSalePrice"], json!(3990));
    assert_eq!(created["skuList"][0]["skuStock"], json!(100));
    assert_eq!(created["skuList"][0]["skuNick"], "WHITE");
    assert_eq!(created["skuList"][0]["skuProps"][0]["propValueName"], "白色");
    assert_eq!(created["skuList"][0]["skuProps"][0]["propVersion"], json!(1));
    assert_eq!(created["skuList"][1]["skuSalePrice"], json!(4200));
    assert_eq!(created["skuList"][1]["skuProps"][0]["propValueName"], "黑色");
}

#[tokio::test]
async fn push_rejects_invalid_drafts_before_dispatch() {
    let (transport, script) = harness();
    let mut empty = draft(39.9);
    empty.title.clear();
    let outcome = push_listing(&transport, &shop(), &empty, &target(), &[("a.jpg".into(), vec![1])], NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(script.calls().is_empty());

    let bad_target = ListingTarget { shop_id: "shop-1".into(), category_id: "abc".into(), attributes: Vec::new() };
    let outcome = push_listing(&transport, &shop(), &draft(39.9), &bad_target, &[("a.jpg".into(), vec![1])], NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.reason.unwrap().contains("类目"));
    assert!(script.calls().is_empty());

    let outcome = push_listing(&transport, &shop(), &draft(39.9), &target(), &[], NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(script.calls().is_empty());
}

#[tokio::test]
async fn push_business_rejection_has_no_remote_id() {
    let (transport, script) = harness();
    script.push(
        "/open/logistics/express/template/list",
        ok(json!({"expressTemplateDetailVOS": [{"id": 9426102401_i64, "status": 1, "deleteTime": 0}], "total": 1})),
    );
    script.push(
        "/open/item/image/upload",
        ok(json!({"kwaiImgUrl": "https://p2-ec.ecukwai.com/bs2/image-kwaishop-product/ITEM_IMAGE-1.jpg"})),
    );
    script.push("/open/item/category/config", ok(json!({"categoryConfig": {"refundRuleList": [1]}, "propConfigs": []})));
    script.push("/open/item/new", Step::Json(json!({"result": 100, "error_msg": "类目不存在"})));
    let outcome = push_listing(&transport, &shop(), &draft(39.9), &target(), &[("a.jpg".into(), vec![1])], NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert_eq!(outcome.reason.as_deref(), Some("类目不存在"));
    assert!(outcome.remote_id.is_none());
}

#[tokio::test]
async fn push_after_dispatch_is_uncertain_and_existing_remote_only_queries() {
    let (transport, script) = harness();
    script.push(
        "/open/logistics/express/template/list",
        ok(json!({"expressTemplateDetailVOS": [{"id": 9426102401_i64, "status": 1, "deleteTime": 0}], "total": 1})),
    );
    script.push(
        "/open/item/image/upload",
        ok(json!({"kwaiImgUrl": "https://p2-ec.ecukwai.com/bs2/image-kwaishop-product/ITEM_IMAGE-1.jpg"})),
    );
    script.push("/open/item/category/config", ok(json!({"categoryConfig": {"refundRuleList": [1]}, "propConfigs": []})));
    script.push("/open/item/new", Step::Fault(TransportFault::AfterDispatch("连接中断".into())));
    let outcome = push_listing(&transport, &shop(), &draft(39.9), &target(), &[("a.jpg".into(), vec![1])], NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Uncertain);
    assert!(outcome.remote_id.is_none());
    assert!(script.calls().iter().any(|call| call.path == "/open/item/new" && call.mutating));

    script.push(
        "/open/item/get",
        ok(json!({"kwaiItemId": 24904424073914_i64, "auditStatus": 2, "onOfflineStatus": 1, "title": "美式复古醒酸美裙"})),
    );
    let before = script.calls().len();
    let outcome = push_listing(&transport, &shop(), &draft(39.9), &target(), &[("a.jpg".into(), vec![1])], NOW, Some("24904424073914")).await;
    assert_eq!(outcome.status, ListingStatus::Live);
    assert_eq!(outcome.remote_id.as_deref(), Some("24904424073914"));
    let extra: Vec<_> = script.calls().into_iter().skip(before).collect();
    assert_eq!(extra.len(), 1);
    assert_eq!(extra[0].path, "/open/item/get");
    assert!(!extra[0].mutating);
}

#[tokio::test]
async fn refresh_maps_documented_audit_states() {
    let (transport, script) = harness();
    script.push(
        "/open/item/get",
        ok(json!({"itemId": 3655213008601_i64, "auditStatus": 2, "onOfflineStatus": 1, "duplicationStatus": 2, "title": "专用的回形针"})),
    );
    let (status, reason) = refresh_remote(&transport, &shop(), NOW, "3655213008601").await.expect("live");
    assert_eq!(status, ListingStatus::Live);
    assert!(reason.is_none());

    script.push(
        "/open/item/get",
        ok(json!({"itemId": 1, "auditStatus": 0, "onOfflineStatus": 0, "duplicationStatus": 2})),
    );
    let (status, _) = refresh_remote(&transport, &shop(), NOW, "1").await.expect("reviewing");
    assert_eq!(status, ListingStatus::Reviewing);

    script.push(
        "/open/item/get",
        ok(json!({"itemId": 1, "auditStatus": 3, "auditReason": "图片不合规", "onOfflineStatus": 0, "duplicationStatus": 2})),
    );
    let (status, reason) = refresh_remote(&transport, &shop(), NOW, "1").await.expect("rejected");
    assert_eq!(status, ListingStatus::Rejected);
    assert_eq!(reason.as_deref(), Some("图片不合规"));
}

#[tokio::test]
async fn product_pages_follow_total_page() {
    let (transport, script) = harness();
    script.push(
        "/open/item/list",
        ok(json!({
            "totalItemCount": 2,
            "currentPageItemCount": 1,
            "currentPageNumber": 1,
            "totalPage": 2,
            "items": [{"kwaiItemId": 4193319911_i64, "title": "示例商品", "price": 9900, "itemStatus": 1, "auditStatus": 2, "onOfflineStatus": 1, "stock": 4}]
        })),
    );
    let page = fetch_products(&transport, &shop(), NOW, None).await.expect("page");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].remote_id, "4193319911");
    assert_eq!(page.items[0].status, ListingStatus::Live);
    assert_eq!(page.items[0].price, Some(99.0));
    assert_eq!(page.items[0].currency.as_deref(), Some("CNY"));
    assert_eq!(page.items[0].stock, Some(4));
    assert_eq!(page.cursor.as_deref(), Some("2"));
    assert_eq!(param(&script.calls()[0])["pageNumber"], json!(1));

    script.push(
        "/open/item/list",
        ok(json!({
            "totalPage": 2,
            "currentPageNumber": 2,
            "items": [{"kwaiItemId": 4193319912_i64, "title": "第二页", "price": 100, "auditStatus": 0, "onOfflineStatus": 0}]
        })),
    );
    let page = fetch_products(&transport, &shop(), NOW, Some("2")).await.expect("next");
    assert_eq!(page.items[0].status, ListingStatus::Reviewing);
    assert!(page.cursor.is_none());
}

#[tokio::test]
async fn order_pages_follow_cursor_then_nomore() {
    let (transport, script) = harness();
    script.push(
        "/open/order/cursor/list",
        ok(json!({
            "beginTime": 1790784000000_i64,
            "endTime": 1790870399000_i64,
            "cursor": "1687251104545_2307500074108220",
            "pageSize": 50,
            "orderList": [{
                "orderBaseInfo": {
                    "oid": 2307500074108220_i64,
                    "status": 30,
                    "totalFee": 3990,
                    "buyerOpenId": "f198f0846993046e14bf572410405c46",
                    "createTime": 1678975518244_i64
                }
            }]
        })),
    );
    let page = fetch_orders(&transport, &shop(), NOW, MetricRange::Yesterday, None).await.expect("orders");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].remote_id, "2307500074108220");
    assert_eq!(page.items[0].status, "30");
    assert_eq!(page.items[0].amount, Some(39.9));
    assert_eq!(page.items[0].currency.as_deref(), Some("CNY"));
    assert_eq!(page.items[0].buyer.as_deref(), Some("f198f0846993046e14bf572410405c46"));
    assert_eq!(page.cursor.as_deref(), Some("0:1687251104545_2307500074108220"));
    let query = param(&script.calls()[0]);
    assert_eq!(query["beginTime"], json!(1_790_784_000_000_i64));
    assert_eq!(query["endTime"], json!(1_790_870_399_000_i64));
    assert_eq!(query["cursor"], "");
    assert_eq!(query["orderViewStatus"], json!(1));

    script.push(
        "/open/order/cursor/list",
        ok(json!({"cursor": "nomore", "orderList": []})),
    );
    let page = fetch_orders(&transport, &shop(), NOW, MetricRange::Yesterday, Some("0:1687251104545_2307500074108220"))
        .await
        .expect("end");
    assert!(page.items.is_empty());
    assert!(page.cursor.is_none());
}

fn order_row(oid: i64, status: i64, fee: Option<i64>, buyer: Option<&str>) -> Value {
    let mut base = json!({"oid": oid, "status": status, "createTime": 1_790_800_000_000_i64});
    if let Some(fee) = fee {
        base["totalFee"] = json!(fee);
    }
    if let Some(buyer) = buyer {
        base["buyerOpenId"] = json!(buyer);
    }
    json!({"orderBaseInfo": base})
}

fn queue_yesterday(script: &ScriptTransport, orders: Value, refunds: Value, items: Value) {
    script.push(
        "/open/order/cursor/list",
        ok(json!({"cursor": "nomore", "orderList": orders, "beginTime": 1790784000000_i64, "endTime": 1790870399000_i64})),
    );
    script.push(
        "/open/seller/order/refund/pcursor/list",
        ok(json!({"pcursor": "nomore", "refundOrderInfoList": refunds})),
    );
    script.push(
        "/open/item/list",
        ok(json!({"totalPage": 1, "currentPageNumber": 1, "items": items})),
    );
}

#[tokio::test]
async fn metrics_sum_documented_orders_refunds_and_live_goods() {
    let (transport, script) = harness();
    queue_yesterday(
        &script,
        json!([
            order_row(1, 30, Some(1000), Some("buyer-a")),
            order_row(2, 10, Some(5000), Some("buyer-b")),
            order_row(3, 80, Some(700), Some("buyer-a")),
            order_row(4, 40, Some(250), Some("buyer-c"))
        ]),
        json!([
            {"refundId": 11, "status": 60, "refundFee": 200},
            {"refundId": 12, "status": 10, "refundFee": 900},
            {"refundId": 13, "status": 70, "refundFee": 100}
        ]),
        json!([
            {"kwaiItemId": 1, "onOfflineStatus": 1, "auditStatus": 2},
            {"kwaiItemId": 2, "onOfflineStatus": 0, "auditStatus": 2}
        ]),
    );
    let metrics = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Yesterday, NOW).await.expect("metrics");
    assert_eq!(metrics.currency.as_deref(), Some("CNY"));
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&2.0));
    assert_eq!(metrics.values.get(&MetricKey::Gmv), Some(&12.5));
    assert_eq!(metrics.values.get(&MetricKey::Buyers), Some(&2.0));
    assert_eq!(metrics.values.get(&MetricKey::PendingShipment), Some(&1.0));
    assert_eq!(metrics.values.get(&MetricKey::RefundOrders), Some(&1.0));
    assert_eq!(metrics.values.get(&MetricKey::RefundAmount), Some(&2.0));
    assert_eq!(metrics.values.get(&MetricKey::ProductsLive), Some(&1.0));
    assert!(metrics.unsupported.is_empty());
    assert!(metrics.error.is_none());
}

#[tokio::test]
async fn metrics_keep_real_zeros_and_mark_missing_fields_unsupported() {
    let (transport, script) = harness();
    queue_yesterday(&script, json!([]), json!([]), json!([]));
    let metrics = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Yesterday, NOW).await.expect("zeros");
    assert!(metrics.unsupported.is_empty());
    assert_eq!(metrics.values.get(&MetricKey::Gmv), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::RefundAmount), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::ProductsLive), Some(&0.0));
    assert_eq!(metrics.currency.as_deref(), Some("CNY"));

    queue_yesterday(
        &script,
        json!([order_row(1, 30, None, None)]),
        json!([{"refundId": 1, "status": 60}]),
        json!([{"kwaiItemId": 1, "auditStatus": 2}]),
    );
    let metrics = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Yesterday, NOW).await.expect("gaps");
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&1.0));
    assert_eq!(metrics.values.get(&MetricKey::Gmv), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::Buyers), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::RefundOrders), Some(&1.0));
    assert_eq!(metrics.values.get(&MetricKey::RefundAmount), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::ProductsLive), Some(&0.0));
    assert!(metrics.unsupported.contains(&MetricKey::Gmv));
    assert!(metrics.unsupported.contains(&MetricKey::Buyers));
    assert!(metrics.unsupported.contains(&MetricKey::RefundAmount));
    assert!(metrics.unsupported.contains(&MetricKey::ProductsLive));
    assert!(!metrics.unsupported.contains(&MetricKey::Orders));
}

#[tokio::test]
async fn metrics_auth_rejects_and_refund_failure_stays_unsupported() {
    let (transport, script) = harness();
    script.push(
        "/open/order/cursor/list",
        Step::Json(json!({"result": 0, "error_msg": "access_token已过期"})),
    );
    let error = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Yesterday, NOW).await.expect_err("auth");
    assert!(matches!(error, CommerceError::Rejected(_)));

    script.push(
        "/open/order/cursor/list",
        ok(json!({"cursor": "nomore", "orderList": [order_row(1, 70, Some(0), Some("buyer"))]})),
    );
    script.push(
        "/open/seller/order/refund/pcursor/list",
        Step::Fault(TransportFault::AfterDispatch("售后接口超时".into())),
    );
    script.push(
        "/open/item/list",
        ok(json!({"totalPage": 1, "items": [{"kwaiItemId": 1, "onOfflineStatus": 1}]})),
    );
    let metrics = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Today, NOW).await.expect("partial");
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&1.0));
    assert_eq!(metrics.values.get(&MetricKey::Gmv), Some(&0.0));
    assert!(!metrics.unsupported.contains(&MetricKey::Gmv));
    assert!(metrics.unsupported.contains(&MetricKey::RefundAmount));
    assert!(metrics.unsupported.contains(&MetricKey::RefundOrders));
    assert_eq!(metrics.values.get(&MetricKey::ProductsLive), Some(&1.0));
}

#[tokio::test]
async fn last30_splits_orders_by_seven_days_and_refunds_by_one_day() {
    let (transport, script) = harness();
    for _ in 0..5 {
        script.push("/open/order/cursor/list", ok(json!({"cursor": "nomore", "orderList": []})));
    }
    for _ in 0..30 {
        script.push("/open/seller/order/refund/pcursor/list", ok(json!({"pcursor": "nomore", "refundOrderInfoList": []})));
    }
    script.push("/open/item/list", ok(json!({"totalPage": 1, "items": []})));
    let metrics = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Last30, NOW).await.expect("last30");
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&0.0));
    let calls = script.calls();
    assert_eq!(calls.iter().filter(|call| call.path == "/open/order/cursor/list").count(), 5);
    assert_eq!(calls.iter().filter(|call| call.path == "/open/seller/order/refund/pcursor/list").count(), 30);
    let first = param(calls.iter().find(|call| call.path == "/open/order/cursor/list").expect("order"));
    let refund = param(calls.iter().find(|call| call.path == "/open/seller/order/refund/pcursor/list").expect("refund"));
    assert!(first["endTime"].as_i64().unwrap() - first["beginTime"].as_i64().unwrap() <= 7 * 86_400 * 1000);
    assert!(refund["endTime"].as_i64().unwrap() - refund["beginTime"].as_i64().unwrap() <= 86_400 * 1000);
}

#[tokio::test]
async fn predispatch_fault_is_failed_and_other_platforms_are_unsupported() {
    let (transport, script) = harness();
    script.push(
        "/open/logistics/express/template/list",
        Step::Fault(TransportFault::BeforeDispatch("测试脚本没有响应".into())),
    );
    let outcome = push_listing(&transport, &shop(), &draft(39.9), &target(), &[("a.jpg".into(), vec![1])], NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Failed);
    assert!(script.calls().iter().all(|call| call.path != "/open/item/new"));

    let error = fetch_categories(&transport, &other_shop(), NOW, None).await.expect_err("platform");
    assert!(matches!(error, CommerceError::Unsupported(_)));
    let outcome = push_listing(&transport, &other_shop(), &draft(39.9), &target(), &[("a.jpg".into(), vec![1])], NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
}

#[tokio::test]
async fn order_cursor_rejects_garbage_before_dispatch() {
    let (transport, script) = harness();
    let error = fetch_orders(&transport, &shop(), NOW, MetricRange::Today, Some("nope")).await.expect_err("cursor");
    assert!(matches!(error, CommerceError::Invalid(_)));
    assert!(script.calls().is_empty());
}
