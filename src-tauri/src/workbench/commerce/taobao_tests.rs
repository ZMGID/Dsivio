use super::super::transport::{
    Inbound, Outbound, ResolvedShop, ScriptTransport, Step, Transport, TransportFault,
};
use super::super::types::{
    AttributeInput, CommerceError, ListingAttribute, ListingDraft, ListingSku, ListingStatus,
    ListingTarget, MetricKey, MetricRange,
};
use super::{
    capabilities, fetch_attributes, fetch_categories, fetch_metrics, fetch_orders, fetch_products,
    fill_schema, map_attributes, map_categories, map_item_status, map_seller_skus, metric_window,
    push_listing, refresh_remote, sign_top, top_timestamp,
};
use crate::workbench::shops::Platform;
use serde_json::{json, Value};
use std::sync::Arc;

const NOW: i64 = 1_790_899_200;
const PATH: &str = "/router/rest";

fn shop() -> ResolvedShop {
    ResolvedShop {
        platform: Platform::Taobao,
        partner_id: "12345678".into(),
        partner_key: "helloworld".into(),
        access_token: "session-key".into(),
        remote_id: "tb-user".into(),
        region: Some("CN".into()),
        name: "demo".into(),
    }
}

fn recorded(steps: Vec<Step>) -> (Arc<ScriptTransport>, Transport) {
    let script = Arc::new(ScriptTransport::new());
    for step in steps {
        script.push(PATH, step);
    }
    let transport = Transport::script(Arc::clone(&script));
    (script, transport)
}

fn json_step(body: Value) -> Step {
    Step::Json(body)
}

fn params(call: &Outbound) -> &[(String, String)] {
    &call.query
}

fn param<'a>(pairs: &'a [(String, String)], key: &str) -> &'a str {
    pairs
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
        .unwrap_or("")
}

fn methods(calls: &[Outbound]) -> Vec<&str> {
    calls
        .iter()
        .map(|call| param(params(call), "method"))
        .collect()
}

fn draft() -> ListingDraft {
    ListingDraft {
        title: "Ceramic Cup".into(),
        description: Some("desc".into()),
        price: Some(12.5),
        currency: Some("CNY".into()),
        stock: Some(3),
        skus: vec![ListingSku {
            name: "颜色分类:军绿色".into(),
            price: Some(12.5),
            stock: Some(3),
            code: Some("SKU-1".into()),
        }],
        images: vec!["/tmp/a.jpg".into()],
        weight_kg: None,
        dimensions_cm: None,
        brand: None,
    }
}

fn target() -> ListingTarget {
    ListingTarget {
        shop_id: "shop-1".into(),
        category_id: "50021288".into(),
        attributes: vec![ListingAttribute {
            id: "20000".into(),
            value: "盈讯".into(),
            values: vec![],
        }],
    }
}

fn schema_xml() -> String {
    r#"<itemSchema>
      <field id="title" name="宝贝标题" type="input"><rules><rule name="requiredRule" value="true"/></rules></field>
      <field id="price" name="一口价" type="input"><rules><rule name="requiredRule" value="true"/></rules></field>
      <field id="quantity" name="总数量" type="input"><rules><rule name="requiredRule" value="true"/></rules></field>
      <field id="desc" name="宝贝描述" type="input"></field>
      <field id="images" name="电脑端宝贝图片" type="complex"><rules><rule name="requiredRule" value="true"/></rules><fields><field id="images_0" name="主图" type="input"><rules><rule name="requiredRule" value="true"/></rules></field></fields></field>
      <field id="catProp" name="类目属性" type="complex"><fields><field id="p-20000" name="品牌" type="singleCheck"><rules><rule name="requiredRule" value="true"/></rules><options><option displayName="盈讯" value="3275069"/></options></field></fields></field>
      <field id="sku" name="SKU" type="multiComplex"><fields><field id="props" name="属性对" type="complex"><fields><field id="p-1627207" name="颜色分类" type="singleCheck"><rules><rule name="requiredRule" value="true"/></rules><options><option displayName="军绿色" value="3232483"/></options></field></fields></field><field id="skuPrice" name="价格" type="input"><rules><rule name="requiredRule" value="true"/></rules></field><field id="skuStock" name="库存" type="input"><rules><rule name="requiredRule" value="true"/></rules></field><field id="skuOuterId" name="商家编码" type="input"></field></fields></field>
    </itemSchema>"#
        .to_string()
}

fn schema_response() -> Value {
    json!({"alibaba_item_publish_schema_get_response": {"result": schema_xml()}})
}

fn picture_response() -> Value {
    json!({
        "picture_upload_response": {
            "picture": {
                "picture_id": 123,
                "picture_path": "http://img07.taobaocdn.com/imgextra/i7/22670458/T2dD0kXb4cXXXXXXXX_!!22670458.jpg",
                "title": "title"
            }
        }
    })
}

fn permission_error() -> Value {
    json!({
        "error_response": {
            "code": 11,
            "msg": "Insufficient ISV Permissions",
            "sub_code": "isv.permission-api-package-limit",
            "sub_msg": "scope ids is 381"
        }
    })
}

fn business_error() -> Value {
    json!({
        "error_response": {
            "code": 50,
            "msg": "Remote service error",
            "sub_code": "isv.invalid-parameter",
            "sub_msg": "非法参数"
        }
    })
}

fn assert_rejected(error: &CommerceError, needle: &str) {
    assert!(matches!(error, CommerceError::Rejected(_)), "{error}");
    assert!(error.message().contains(needle), "{error}");
}

#[test]
fn sign_matches_official_md5_example() {
    let params = [
        ("method", "taobao.item.seller.get"),
        ("app_key", "12345678"),
        ("session", "test"),
        ("timestamp", "2016-01-01 12:00:00"),
        ("format", "json"),
        ("v", "2.0"),
        ("sign_method", "md5"),
        ("fields", "num_iid,title,nick,price,num"),
        ("num_iid", "11223344"),
    ];
    assert_eq!(
        sign_top("helloworld", &params),
        "66987CB115214E59E6EC978214934FB8"
    );
}

#[test]
fn metric_windows_use_beijing_time() {
    assert_eq!(top_timestamp(NOW), "2026-10-02 08:00:00");
    let (start, end) = metric_window(MetricRange::Today, NOW);
    assert_eq!(
        (top_timestamp(start), top_timestamp(end)),
        ("2026-10-02 00:00:00".into(), "2026-10-02 08:00:00".into())
    );
    let (start, end) = metric_window(MetricRange::Yesterday, NOW);
    assert_eq!(
        (top_timestamp(start), top_timestamp(end)),
        ("2026-10-01 00:00:00".into(), "2026-10-01 23:59:59".into())
    );
    let (start, _) = metric_window(MetricRange::Last7, NOW);
    assert_eq!(top_timestamp(start), "2026-09-26 00:00:00");
    let (start, _) = metric_window(MetricRange::Last30, NOW);
    assert_eq!(top_timestamp(start), "2026-09-03 00:00:00");
}

#[test]
fn capabilities_cover_the_documented_windows() {
    let caps = capabilities();
    assert_eq!(caps.platform, Platform::Taobao);
    assert_eq!(caps.metrics, MetricKey::ALL);
    assert!(caps.listing && caps.categories && caps.products && caps.orders);
    assert!(caps.notes.iter().any(|note| note.contains("for_shelved")));
}

#[test]
fn categories_follow_itemcats_document() {
    let body = json!({
        "itemcats_get_response": {
            "item_cats": {
                "item_cat": [
                    {"cid": 50011999, "parent_cid": 0, "name": "单方精油", "is_parent": true, "status": "normal"},
                    {"cid": 9, "parent_cid": 0, "name": "已删", "is_parent": false, "status": "deleted"}
                ]
            }
        }
    });
    let categories = map_categories(&body);
    assert_eq!(categories.len(), 1);
    assert_eq!(categories[0].id, "50011999");
    assert_eq!(categories[0].name, "单方精油");
    assert_eq!(categories[0].parent_id, "0");
    assert!(!categories[0].leaf);
    let single = json!({"item_cats": {"item_cat": {"cid": 16, "parent_cid": 50011999, "name": "叶子", "is_parent": false, "status": "normal"}}});
    let categories = map_categories(&single);
    assert_eq!(categories.len(), 1);
    assert!(categories[0].leaf);
}

#[tokio::test]
async fn category_permission_is_not_an_empty_success() {
    let (script, transport) = recorded(vec![json_step(permission_error())]);
    let error = fetch_categories(&transport, &shop(), NOW, Some("50011999"))
        .await
        .unwrap_err();
    assert_rejected(&error, "isv.permission-api-package-limit");
    let call = &script.calls()[0];
    assert_eq!(call.host, "https://eco.taobao.com");
    assert_eq!(call.path, PATH);
    assert_eq!(param(params(call), "method"), "taobao.itemcats.get");
    assert_eq!(param(params(call), "parent_cid"), "50011999");
    assert_eq!(param(params(call), "sign_method"), "md5");
    assert_eq!(param(params(call), "sign").len(), 32);
    assert!(call
        .header_value("content-type")
        .is_some_and(|value| value.contains("application/x-www-form-urlencoded")));
}

#[tokio::test]
async fn invalid_session_is_rejected() {
    let (_script, transport) = recorded(vec![json_step(
        json!({"error_response": {"code": 27, "msg": "Invalid Session"}}),
    )]);
    let error = fetch_categories(&transport, &shop(), NOW, None)
        .await
        .unwrap_err();
    assert_rejected(&error, "Invalid Session");
}

#[tokio::test]
async fn empty_session_does_not_dispatch() {
    let (script, transport) = recorded(vec![]);
    let mut shop = shop();
    shop.access_token.clear();
    let error = fetch_categories(&transport, &shop, NOW, None).await.unwrap_err();
    assert_rejected(&error, "session");
    assert!(script.calls().is_empty());
}

#[test]
fn attributes_follow_itemprops_document() {
    let body = json!({
        "itemprops_get_response": {
            "item_props": {
                "item_prop": [
                    {
                        "pid": 20000,
                        "name": "颜色",
                        "must": true,
                        "multi": true,
                        "is_enum_prop": true,
                        "status": "normal",
                        "prop_values": {"prop_value": [{"vid": 3232483, "name": "军绿色", "status": "normal"}, {"vid": 1, "name": "废弃", "status": "deleted"}]}
                    },
                    {"pid": 100, "name": "自定义", "must": false, "multi": false, "is_enum_prop": false, "is_taosir": true, "status": "normal"},
                    {"pid": 7, "name": "已删属性", "must": false, "multi": false, "status": "deleted"}
                ]
            }
        }
    });
    let attributes = map_attributes(&body);
    assert_eq!(attributes.len(), 2);
    assert_eq!(attributes[0].id, "20000");
    assert!(attributes[0].required);
    assert_eq!(attributes[0].input, AttributeInput::MultiSelect);
    assert_eq!(attributes[0].options.len(), 1);
    assert_eq!(attributes[0].options[0].id, "3232483");
    assert_eq!(attributes[0].options[0].name, "军绿色");
    assert_eq!(attributes[1].input, AttributeInput::Number);
    assert!(attributes[1].unit.is_none());
}

#[tokio::test]
async fn attribute_package_permission_is_rejected() {
    let (_script, transport) = recorded(vec![json_step(json!({
        "error_response": {"code": 11, "msg": "Insufficient ISV Permissions", "sub_code": "isv.permission-api-package-empty", "sub_msg": "权限不足"}
    }))]);
    let error = fetch_attributes(&transport, &shop(), NOW, "44343")
        .await
        .unwrap_err();
    assert_rejected(&error, "isv.permission-api-package-empty");
}

#[tokio::test]
async fn metrics_sum_paid_orders_refunds_and_live_count() {
    let (script, transport) = recorded(vec![
        json_step(json!({
            "trades_sold_get_response": {
                "has_next": true,
                "trades": {"trade": [
                    {
                        "tid": 2231958349_i64,
                        "status": "TRADE_FINISHED",
                        "payment": "200.07",
                        "created": "2026-10-02 01:00:00",
                        "buyer_open_uid": "AAHk5d-EAAeGwJedwSFu0XXX",
                        "orders": {"order": [{"sku_id": "5937146", "sku_properties_name": "颜色:桔色;尺码:M", "outer_sku_id": "81893848", "price": "200.07", "num": 1}]}
                    },
                    {"tid": 2, "status": "WAIT_BUYER_PAY", "payment": "9.00", "buyer_nick": "unpaid", "created": "2026-10-02 02:00:00"}
                ]}
            }
        })),
        json_step(json!({
            "trades_sold_get_response": {
                "has_next": false,
                "trades": {"trade": {
                    "tid": 3,
                    "status": "WAIT_SELLER_SEND_GOODS",
                    "payment": "0.00",
                    "buyer_open_uid": "buyer-2",
                    "created": "2026-10-02 03:00:00"
                }}
            }
        })),
        json_step(json!({
            "refunds_receive_get_response": {
                "has_next": false,
                "refunds": {"refund": [
                    {"refund_id": "83477", "status": "SUCCESS", "refund_fee": "10.00", "dispute_type": "REFUND"},
                    {"refund_id": "9", "status": "CLOSED", "refund_fee": "3.00", "dispute_type": "REFUND"}
                ]}
            }
        })),
        json_step(
            json!({"items_onsale_get_response": {"total_results": 150, "items": {"item": []}}}),
        ),
    ]);
    let metrics = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Today, NOW)
        .await
        .unwrap();
    assert_eq!(metrics.shop_id, "shop-1");
    assert_eq!(metrics.fetched_at, "2026-10-02T00:00:00+00:00");
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&2.0));
    assert_eq!(metrics.values.get(&MetricKey::PendingShipment), Some(&1.0));
    assert!((metrics.values[&MetricKey::Gmv] - 200.07).abs() < 0.001);
    assert_eq!(metrics.values.get(&MetricKey::Buyers), Some(&2.0));
    assert_eq!(metrics.values.get(&MetricKey::RefundOrders), Some(&1.0));
    assert!((metrics.values[&MetricKey::RefundAmount] - 10.0).abs() < 0.001);
    assert_eq!(metrics.values.get(&MetricKey::ProductsLive), Some(&150.0));
    assert_eq!(metrics.currency.as_deref(), Some("CNY"));
    assert!(metrics.unsupported.is_empty());
    assert_eq!(
        param(params(&script.calls()[0]), "start_created"),
        "2026-10-02 00:00:00"
    );
    assert_eq!(
        param(params(&script.calls()[0]), "end_created"),
        "2026-10-02 08:00:00"
    );
    assert!(param(params(&script.calls()[0]), "type").contains("eticket"));
    assert_eq!(
        methods(&script.calls()),
        vec![
            "taobao.trades.sold.get",
            "taobao.trades.sold.get",
            "taobao.refunds.receive.get",
            "taobao.items.onsale.get"
        ]
    );
}

#[tokio::test]
async fn missing_payment_does_not_become_zero_gmv() {
    let (_script, transport) = recorded(vec![
        json_step(
            json!({"trades_sold_get_response": {"has_next": false, "trades": {"trade": {"tid": 1, "status": "TRADE_FINISHED", "buyer_open_uid": "aa", "created": "2026-10-02 01:00:00"}}}}),
        ),
        json_step(
            json!({"refunds_receive_get_response": {"has_next": false, "refunds": {"refund": []}}}),
        ),
        json_step(json!({"items_onsale_get_response": {"total_results": 0}})),
    ]);
    let metrics = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Today, NOW)
        .await
        .unwrap();
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&1.0));
    assert!(metrics.unsupported.contains(&MetricKey::Gmv));
    assert_eq!(metrics.values.get(&MetricKey::Gmv), Some(&0.0));
    assert_eq!(metrics.currency, None);
    assert_eq!(metrics.values.get(&MetricKey::RefundAmount), Some(&0.0));
    assert!(!metrics.unsupported.contains(&MetricKey::RefundAmount));
    assert_eq!(metrics.values.get(&MetricKey::ProductsLive), Some(&0.0));
    assert!(!metrics.unsupported.contains(&MetricKey::ProductsLive));
}

#[tokio::test]
async fn refund_permission_fails_the_metric_call() {
    let (_script, transport) = recorded(vec![
        json_step(
            json!({"trades_sold_get_response": {"has_next": false, "trades": {"trade": []}}}),
        ),
        json_step(permission_error()),
    ]);
    let error = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Yesterday, NOW)
        .await
        .unwrap_err();
    assert_rejected(&error, "isv.permission-api-package-limit");
}

#[tokio::test]
async fn trade_permission_is_not_a_zero_dashboard() {
    let (_script, transport) = recorded(vec![json_step(permission_error())]);
    let error = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Last7, NOW)
        .await
        .unwrap_err();
    assert!(matches!(error, CommerceError::Rejected(_)));
}

#[tokio::test]
async fn publish_maps_category_prop_and_sku_then_reads_status() {
    let (script, transport) = recorded(vec![
        json_step(schema_response()),
        json_step(picture_response()),
        json_step(
            json!({"alibaba_item_publish_submit_response": {"item_id": 634830531619_i64, "create_time": "2020-12-12 00:00:00", "market": "taobao"}}),
        ),
        json_step(
            json!({"item_seller_get_response": {"item": {"num_iid": 634830531619_i64, "approve_status": "onsale", "violation": false}}}),
        ),
    ]);
    let outcome = push_listing(
        &transport,
        &shop(),
        &draft(),
        &target(),
        &[("Bule.jpg".into(), b"png".to_vec())],
        NOW,
        None,
    )
    .await;
    assert_eq!(outcome.status, ListingStatus::Live, "{:?}", outcome.reason);
    assert_eq!(outcome.remote_id.as_deref(), Some("634830531619"));
    let calls = script.calls();
    assert_eq!(
        methods(&calls),
        vec![
            "alibaba.item.publish.schema.get",
            "taobao.picture.upload",
            "alibaba.item.publish.submit",
            "taobao.item.seller.get"
        ]
    );
    assert_eq!(param(params(&calls[0]), "market"), "taobao");
    assert_eq!(param(params(&calls[0]), "cat_id"), "50021288");
    assert_eq!(param(params(&calls[0]), "item_type"), "b");
    assert_eq!(param(params(&calls[1]), "picture_category_id"), "0");
    assert_eq!(param(params(&calls[1]), "image_input_title"), "Bule.jpg");
    assert_eq!(param(params(&calls[1]), "is_https"), "true");
    assert!(calls[1].query.iter().all(|(key, _)| key != "img"));
    assert!(!calls[1].mutating);
    assert_eq!(calls[1].file_name.as_deref(), Some("Bule.jpg"));
    assert_eq!(calls[1].file_bytes.as_deref(), Some(b"png".as_slice()));
    assert_eq!(calls[1].header_value("x-dsivio-file-field"), Some("img"));
    let schema = param(params(&calls[2]), "schema");
    assert!(schema.contains("Ceramic Cup"));
    assert!(schema.contains("3275069"));
    assert!(schema.contains("3232483"));
    assert!(schema.contains("12.50"));
    assert!(schema.contains("SKU-1"));
    assert!(schema.contains(
        "http://img07.taobaocdn.com/imgextra/i7/22670458/T2dD0kXb4cXXXXXXXX_!!22670458.jpg"
    ));
    assert!(calls[2].mutating);
    assert!(calls[2]
        .header_value("content-type")
        .is_some_and(|value| value.contains("application/x-www-form-urlencoded")));
    let again = sign_top(
        "helloworld",
        &calls[2]
            .query
            .iter()
            .filter(|(key, _)| key != "sign")
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>(),
    );
    assert_eq!(param(&calls[2].query, "sign"), again);
}

#[test]
fn seller_sku_properties_name_splits_into_labels() {
    let body = json!({
        "item_seller_get_response": {
            "item": {
                "skus": {"sku": [{
                    "sku_id": 123,
                    "properties": "1243:1215;5626:5125",
                    "properties_name": "20000:3275069:品牌:盈讯;1753146:3485013:型号:F908;-1234:-5678:自定义属性1:属性值1",
                    "price": "200.07",
                    "quantity": 3,
                    "outer_id": "12345"
                }]}
            }
        }
    });
    let skus = map_seller_skus(&body);
    assert_eq!(skus[0].remote_id.as_deref(), Some("123"));
    assert_eq!(
        skus[0].name.as_deref(),
        Some("品牌:盈讯;型号:F908;自定义属性1:属性值1")
    );
    assert_eq!(skus[0].properties.as_deref(), Some("1243:1215;5626:5125"));
    assert_eq!(skus[0].currency.as_deref(), Some("CNY"));
    assert_eq!(skus[0].stock, Some(3));
    assert!((skus[0].price.unwrap() - 200.07).abs() < 0.001);
}

#[tokio::test]
async fn required_schema_field_rejects_before_submit() {
    let xml = r#"<itemSchema><field id="title" name="宝贝标题" type="input"><rules><rule name="requiredRule" value="true"/></rules></field><field id="supplierInfo" name="供应商信息" type="input"><rules><rule name="requiredRule" value="true"/></rules></field></itemSchema>"#;
    let (script, transport) = recorded(vec![json_step(
        json!({"alibaba_item_publish_schema_get_response": {"result": xml}}),
    )]);
    let mut draft = draft();
    draft.skus.clear();
    let mut target = target();
    target.attributes.clear();
    let outcome = push_listing(&transport, &shop(), &draft, &target, &[], NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.reason.unwrap().contains("supplierInfo"));
    assert_eq!(
        methods(&script.calls()),
        vec!["alibaba.item.publish.schema.get"]
    );
}

#[tokio::test]
async fn structured_submit_error_is_rejected() {
    let (script, transport) = recorded(vec![
        json_step(schema_response()),
        json_step(picture_response()),
        json_step(business_error()),
    ]);
    let outcome = push_listing(
        &transport,
        &shop(),
        &draft(),
        &target(),
        &[("Bule.jpg".into(), b"png".to_vec())],
        NOW,
        None,
    )
    .await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    let reason = outcome.reason.unwrap();
    assert!(reason.contains("isv.invalid-parameter"));
    assert!(reason.contains("非法参数"));
    assert_eq!(methods(&script.calls()).len(), 3);
}

#[tokio::test]
async fn submit_disconnect_after_dispatch_is_uncertain() {
    let (_script, transport) = recorded(vec![
        json_step(schema_response()),
        json_step(picture_response()),
        fault_after(),
    ]);
    let outcome = push_listing(
        &transport,
        &shop(),
        &draft(),
        &target(),
        &[("Bule.jpg".into(), b"png".to_vec())],
        NOW,
        None,
    )
    .await;
    assert_eq!(outcome.status, ListingStatus::Uncertain);
    assert!(outcome.remote_id.is_none());
}

fn fault_after() -> Step {
    Step::Fault(TransportFault::AfterDispatch("reset".into()))
}

#[tokio::test]
async fn schema_disconnect_before_dispatch_is_failed() {
    let (_script, transport) = recorded(vec![Step::Fault(TransportFault::BeforeDispatch(
        "connect".into(),
    ))]);
    let outcome = push_listing(
        &transport,
        &shop(),
        &draft(),
        &target(),
        &[("Bule.jpg".into(), b"png".to_vec())],
        NOW,
        None,
    )
    .await;
    assert_eq!(outcome.status, ListingStatus::Failed);
}

#[tokio::test]
async fn existing_remote_only_queries_status() {
    let (script, transport) = recorded(vec![json_step(
        json!({"item_seller_get_response": {"item": {"approve_status": "instock", "violation": false}}}),
    )]);
    let outcome = push_listing(
        &transport,
        &shop(),
        &draft(),
        &target(),
        &[],
        NOW,
        Some("634830531619"),
    )
    .await;
    assert_eq!(outcome.status, ListingStatus::Reviewing);
    assert_eq!(outcome.reason.as_deref(), Some("approve_status=instock"));
    assert_eq!(outcome.remote_id.as_deref(), Some("634830531619"));
    assert_eq!(methods(&script.calls()), vec!["taobao.item.seller.get"]);
}

#[tokio::test]
async fn refresh_maps_violation_and_delete() {
    let (banned_script, transport) = recorded(vec![json_step(
        json!({"item_seller_get_response": {"item": {"approve_status": "onsale", "violation": true}}}),
    )]);
    let (status, reason) = refresh_remote(&transport, &shop(), NOW, "1").await.unwrap();
    assert_eq!(status, ListingStatus::Banned);
    assert_eq!(reason.as_deref(), Some("violation"));
    assert_eq!(param(params(&banned_script.calls()[0]), "num_iid"), "1");
    let (_script, transport) = recorded(vec![json_step(json!({
        "error_response": {"code": 50, "msg": "Remote service error", "sub_code": "isv.item-is-delete:invalid-numIid", "sub_msg": "商品已经被删除"}
    }))]);
    let (status, reason) = refresh_remote(&transport, &shop(), NOW, "1").await.unwrap();
    assert_eq!(status, ListingStatus::Rejected);
    let reason = reason.unwrap();
    assert!(reason.contains("DELETE"));
    assert!(reason.contains("isv.item-is-delete"));
}

#[test]
fn item_status_table() {
    assert_eq!(map_item_status("onsale", false).0, ListingStatus::Live);
    assert_eq!(
        map_item_status("instock", false).0,
        ListingStatus::Reviewing
    );
    assert_eq!(map_item_status("onsale", true).0, ListingStatus::Banned);
}

#[tokio::test]
async fn empty_title_does_not_dispatch() {
    let (script, transport) = recorded(vec![]);
    let mut draft = draft();
    draft.title = "  ".into();
    let outcome = push_listing(&transport, &shop(), &draft, &target(), &[], NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(script.calls().is_empty());
}

#[tokio::test]
async fn other_platform_is_unsupported() {
    let (script, transport) = recorded(vec![]);
    let mut shop = shop();
    shop.platform = Platform::Shopee;
    let error = fetch_categories(&transport, &shop, NOW, None).await.unwrap_err();
    assert!(matches!(error, CommerceError::Unsupported(_)));
    assert!(script.calls().is_empty());
}

#[tokio::test]
async fn products_page_price_currency_and_inventory_permission() {
    let (script, transport) = recorded(vec![json_step(json!({
        "items_onsale_get_response": {
            "total_results": 150,
            "items": {"item": [
                {"num_iid": 1489161932, "title": "Google test item", "approve_status": "onsale", "price": "5.00", "num": 8888, "outer_id": "34143554352"},
                {"num_iid": 2, "title": "缺价格", "approve_status": "onsale", "num": 1}
            ]}
        }
    }))]);
    let page = fetch_products(&transport, &shop(), "shop-1", Some("2"), NOW).await.unwrap();
    assert_eq!(page.shop_id, "shop-1");
    assert_eq!(page.next_cursor.as_deref(), Some("3"));
    assert_eq!(page.items[0].id, "1489161932");
    assert_eq!(page.items[0].price, Some(5.0));
    assert_eq!(page.items[0].currency.as_deref(), Some("CNY"));
    assert_eq!(page.items[0].stock, Some(8888));
    assert_eq!(page.items[0].sku.as_deref(), Some("34143554352"));
    assert_eq!(page.items[1].price, None);
    assert_eq!(page.items[1].currency, None);
    assert_eq!(param(params(&script.calls()[0]), "page_no"), "2");
    assert_eq!(param(params(&script.calls()[0]), "page_size"), "40");
    assert_eq!(
        param(params(&script.calls()[0]), "method"),
        "taobao.items.onsale.get"
    );

    let (script, transport) = recorded(vec![json_step(
        json!({"items_onsale_get_response": {"total_results": 1, "items": {"item": []}}}),
    )]);
    let page = fetch_products(&transport, &shop(), "shop-1", None, NOW).await.unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some("for_shelved:1"));
    assert_eq!(param(params(&script.calls()[0]), "page_no"), "1");

    let (script, transport) = recorded(vec![json_step(permission_error())]);
    let error = fetch_products(&transport, &shop(), "shop-1", Some("for_shelved:1"), NOW)
        .await
        .unwrap_err();
    assert_rejected(&error, "isv.permission-api-package-limit");
    assert_eq!(
        param(params(&script.calls()[0]), "method"),
        "taobao.items.inventory.get"
    );
    assert_eq!(param(params(&script.calls()[0]), "banner"), "for_shelved");
}

#[tokio::test]
async fn orders_page_maps_sku_date_and_currency() {
    let (script, transport) = recorded(vec![json_step(json!({
        "trades_sold_get_response": {
            "has_next": true,
            "trades": {"trade": [{
                "tid": 2231958349_i64,
                "status": "WAIT_SELLER_SEND_GOODS",
                "payment": "200.07",
                "created": "2000-01-01 00:00:00",
                "buyer_open_uid": "AAHk5d-EAAeGwJedwSFu0XXX",
                "orders": {"order": [{"sku_id": "5937146", "sku_properties_name": "颜色:桔色;尺码:M", "outer_sku_id": "81893848", "price": "200.07", "num": 2}]}
            }]}
        }
    }))]);
    let page = fetch_orders(&transport, &shop(), "shop-1", MetricRange::Yesterday, None, NOW)
        .await
        .unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some("2"));
    assert_eq!(page.range, MetricRange::Yesterday);
    let order = &page.items[0];
    assert_eq!(order.id, "2231958349");
    assert_eq!(order.status, "WAIT_SELLER_SEND_GOODS");
    assert!((order.amount.unwrap() - 200.07).abs() < 0.001);
    assert_eq!(order.currency.as_deref(), Some("CNY"));
    assert_eq!(order.buyer.as_deref(), Some("AAHk5d-EAAeGwJedwSFu0XXX"));
    assert_eq!(
        order.created_at.as_deref(),
        Some("2000-01-01T00:00:00+08:00")
    );
    assert_eq!(order.lines[0].title, "颜色:桔色;尺码:M");
    assert_eq!(order.lines[0].quantity, 2);
    assert_eq!(order.lines[0].sku.as_deref(), Some("81893848"));
    assert_eq!(
        param(params(&script.calls()[0]), "start_created"),
        "2026-10-01 00:00:00"
    );
    assert_eq!(
        param(params(&script.calls()[0]), "end_created"),
        "2026-10-01 23:59:59"
    );
}

#[tokio::test]
async fn page_zero_is_rejected_before_dispatch() {
    let (script, transport) = recorded(vec![]);
    let error = fetch_orders(&transport, &shop(), "shop-1", MetricRange::Today, Some("0"), NOW)
        .await
        .unwrap_err();
    assert!(matches!(error, CommerceError::Invalid(_)));
    assert!(script.calls().is_empty());
    let error = fetch_products(&transport, &shop(), "shop-1", Some("nope"), NOW)
        .await
        .unwrap_err();
    assert!(matches!(error, CommerceError::Invalid(_)));
}

#[test]
fn http_500_on_submit_is_uncertain() {
    let error = super::classify(
        true,
        Inbound {
            status: 500,
            body: json!({"error": "down"}),
        },
    );
    assert!(matches!(error, Err(super::CallFail::Uncertain(_))));
    let error = super::classify(
        false,
        Inbound {
            status: 500,
            body: json!({}),
        },
    );
    assert!(matches!(error, Err(super::CallFail::Failed(_))));
}

#[test]
fn fill_schema_rejects_unknown_required_field() {
    let xml = "<itemSchema><field id=\"supplierInfo\" name=\"供应商信息\" type=\"input\"><rules><rule name=\"requiredRule\" value=\"true\"/></rules></field></itemSchema>";
    let mut draft = draft();
    draft.skus.clear();
    let mut target = target();
    target.attributes.clear();
    let error = fill_schema(xml, &draft, &target, &[]).unwrap_err();
    assert!(error.contains("supplierInfo"));
}
