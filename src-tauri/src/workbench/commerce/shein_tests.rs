//! Doc-shaped replays of the SHEIN Open Platform responses cited in `shein.rs`.
//! These are not live shop captures.

use super::super::transport::{Outbound, ResolvedShop, ScriptTransport, Step, Transport, TransportFault};
use super::super::types::{
    AttributeInput, CommerceError, ListingDraft, ListingStatus, ListingTarget, MetricKey, MetricRange,
};
use super::{fetch_attributes, fetch_categories, fetch_metrics, fetch_orders, fetch_products, push_listing, refresh_remote};
use crate::workbench::shops::Platform;
use serde_json::{json, Value};
use std::sync::Arc;

const NOW: i64 = 1_711_929_600;
const OPEN_KEY: &str = "open-key";
const SECRET: &str = "seller-secret";

fn shop() -> ResolvedShop {
    ResolvedShop {
        platform: Platform::Shein,
        partner_id: OPEN_KEY.into(),
        partner_key: SECRET.into(),
        access_token: String::new(),
        remote_id: "supplier-1".into(),
        region: Some("自运营".into()),
        name: "demo".into(),
    }
}

fn scripted() -> (Arc<ScriptTransport>, Transport) {
    let script = Arc::new(ScriptTransport::new());
    (script.clone(), Transport::script(script))
}

fn ok(info: Value) -> Value {
    json!({"code": "0", "msg": "OK", "info": info})
}

fn header<'a>(call: &'a Outbound, name: &str) -> &'a str {
    call.headers.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str()).unwrap_or("")
}

fn assert_signed(call: &Outbound, path: &str) {
    assert_eq!(call.host, "https://openapi.sheincorp.com");
    assert_eq!(call.path, path);
    assert_eq!(header(call, "x-lt-openKeyId"), OPEN_KEY);
    assert_eq!(header(call, "x-lt-timestamp"), (NOW * 1000).to_string());
    let signature = header(call, "x-lt-signature");
    let random = &signature[..5];
    let expected = super::sign_shein(OPEN_KEY, SECRET, path, NOW * 1000, random).expect("sign");
    assert_eq!(signature, expected);
}

fn category_tree() -> Value {
    ok(json!({"data": [{
        "category_id": 2028,
        "product_type_id": 0,
        "parent_category_id": 0,
        "category_name": "Women",
        "last_category": false,
        "children": [{
            "category_id": 1727,
            "product_type_id": 1080,
            "parent_category_id": 2028,
            "category_name": "Dresses",
            "last_category": true,
            "children": []
        }]
    }]}))
}

fn attribute_template() -> Value {
    ok(json!({"data": [{
        "product_type_id": 1080,
        "main_attribute_status": 3,
        "attribute_infos": [
            {
                "attribute_id": 27,
                "attribute_name": "Color",
                "attribute_type": 1,
                "attribute_label": 1,
                "attribute_mode": 2,
                "attribute_status": 3,
                "data_dimension": 1,
                "attribute_value_info_list": [
                    {"attribute_value_id": 100, "attribute_value": "Black", "is_show": 1},
                    {"attribute_value_id": 101, "attribute_value": "默认", "is_show": 1},
                    {"attribute_value_id": 102, "attribute_value": "Hidden", "is_show": 2}
                ]
            },
            {
                "attribute_id": 87,
                "attribute_name": "Size",
                "attribute_type": 1,
                "attribute_label": 0,
                "attribute_mode": 2,
                "attribute_status": 3,
                "data_dimension": 1,
                "attribute_value_info_list": [
                    {"attribute_value_id": 200, "attribute_value": "S", "is_show": 1},
                    {"attribute_value_id": 201, "attribute_value": "M", "is_show": 1}
                ]
            },
            {
                "attribute_id": 55,
                "attribute_name": "Material",
                "attribute_type": 4,
                "attribute_label": 0,
                "attribute_mode": 3,
                "attribute_status": 3,
                "data_dimension": 1,
                "attribute_value_info_list": [
                    {"attribute_value_id": 300, "attribute_value": "Cotton", "is_show": 1}
                ]
            },
            {
                "attribute_id": 61,
                "attribute_name": "Length",
                "attribute_type": 4,
                "attribute_label": 0,
                "attribute_mode": 0,
                "attribute_status": 2,
                "data_dimension": 1,
                "attribute_value_info_list": []
            },
            {
                "attribute_id": 9,
                "attribute_name": "Internal",
                "attribute_type": 4,
                "attribute_label": 0,
                "attribute_mode": 3,
                "attribute_status": 1,
                "data_dimension": 1,
                "attribute_value_info_list": []
            }
        ]
    }]}))
}

fn fill_standard() -> Value {
    ok(json!({
        "default_language": "en",
        "currency": "",
        "supplier_code_in_spu_dimension": false,
        "fill_in_standard_list": [{"field_key": "brand_code", "required": false, "show": false}]
    }))
}

fn sites() -> Value {
    ok(json!({"data": [{
        "main_site": "shein",
        "sub_site_list": [
            {"site_abbr": "shein-us", "site_status": 1, "currency": "USD", "site_name": "United States"},
            {"site_abbr": "shein-mx", "site_status": 0, "currency": "MXN", "site_name": "Mexico"}
        ]
    }]}))
}

fn warehouses(codes: &[&str]) -> Value {
    ok(json!({"list": codes.iter().map(|code| json!({
        "warehouseCode": code,
        "warehouseName": code,
        "warehouseType": 1
    })).collect::<Vec<_>>()}))
}

fn upload_ok() -> Value {
    ok(json!({
        "image_url": "https://img.ltwebstatic.com/images/2024/dress.jpg",
        "width": 1340,
        "height": 1785
    }))
}

fn publish_ok() -> Value {
    ok(json!({
        "success": true,
        "spu_name": "MM2412222464",
        "version": "SPMP241222341208032",
        "skc_list": [{"sku_list": [{"sku_code": "I5m2h7t1x"}]}]
    }))
}

fn queue_reads(script: &ScriptTransport, warehouse_codes: &[&str]) {
    script.push("/open-api/goods/query-category-tree", Step::Json(category_tree()));
    script.push("/open-api/goods/query-attribute-template", Step::Json(attribute_template()));
    script.push("/open-api/goods/query-publish-fill-in-standard", Step::Json(fill_standard()));
    script.push("/open-api/goods/query-site-list", Step::Json(sites()));
    script.push("/open-api/msc/warehouse/list", Step::Json(warehouses(warehouse_codes)));
}

fn draft() -> ListingDraft {
    serde_json::from_value(json!({
        "title": "Cotton dress",
        "description": "Soft cotton",
        "price": 19.9,
        "currency": "USD",
        "stock": 4,
        "skus": [
            {"name": "S", "price": 19.9, "stock": 4, "code": "SKU-S"},
            {"name": "M", "price": 21.0, "stock": 0, "code": "SKU-M"}
        ],
        "weightKg": 0.25,
        "dimensionsCm": {"l": 30.0, "w": 20.0, "h": 2.0}
    }))
    .expect("draft")
}

fn target(with_material: bool) -> ListingTarget {
    let mut attributes = vec![json!({"id": "27", "value": "Black"})];
    if with_material {
        attributes.push(json!({"id": "55", "value": "Cotton"}));
    }
    serde_json::from_value(json!({"shopId": "shop-1", "categoryId": "1727", "attributes": attributes})).expect("target")
}

fn images() -> Vec<(String, Vec<u8>)> {
    vec![("dress.jpg".into(), vec![0xff, 0xd8, 0xff, 0xd9])]
}

fn document(state: i64, reason: Option<&str>) -> Value {
    let failed = reason.map(|content| vec![json!({"content": content})]).unwrap_or_default();
    ok(json!({"data": [{
        "spuName": "MM2412222464",
        "skcList": [{"documentState": state, "failedReason": failed}]
    }]}))
}

fn order_list(rows: Value, count: i64) -> Value {
    ok(json!({"count": count, "orderList": rows}))
}

fn order_row(order_no: &str, status: &str, created: &str) -> Value {
    json!({"orderNo": order_no, "orderStatus": status, "orderCreateTime": created, "orderUpdateTime": created})
}

#[test]
fn sign_matches_shops_vector() {
    let sign = super::sign_shein("open", "secret", "/open-api/test", 1_583_398_764_000, "1aa34").unwrap();
    assert_eq!(
        sign,
        "1aa34NWI2NTVkODMwYzI3YWVkMjY4Y2YxNmRjMWY0MTNkYWVmNjAzODA3YTJkMjk1OWI3YzI3YmMxNzU5ZjczMWRkNw=="
    );
}

#[tokio::test]
async fn categories_are_signed_and_attributes_follow_the_template() {
    let (script, transport) = scripted();
    script.push("/open-api/goods/query-category-tree", Step::Json(category_tree()));
    let all = fetch_categories(&transport, &shop(), NOW, None).await.expect("categories");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, "2028");
    assert!(!all[0].leaf);
    assert_eq!(all[1].id, "1727");
    assert!(all[1].leaf);
    assert_eq!(all[1].parent_id, "2028");
    let call = &script.calls()[0];
    assert_signed(call, "/open-api/goods/query-category-tree");
    assert_eq!(call.method, "POST");
    assert!(!call.mutating);
    assert_eq!(header(call, "language"), "en");
    assert_eq!(header(call, "Content-Type"), "application/json;charset=UTF-8");
    assert_eq!(call.json, Some(json!({})));

    script.push("/open-api/goods/query-category-tree", Step::Json(category_tree()));
    let children = fetch_categories(&transport, &shop(), NOW, Some("2028")).await.expect("children");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].id, "1727");

    script.push("/open-api/goods/query-category-tree", Step::Json(category_tree()));
    script.push("/open-api/goods/query-attribute-template", Step::Json(attribute_template()));
    let attributes = fetch_attributes(&transport, &shop(), NOW, "1727").await.expect("attributes");
    let ids: Vec<_> = attributes.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, vec!["27", "87", "55", "61"]);
    assert!(attributes.iter().all(|item| item.id != "9"));
    let color = attributes.iter().find(|item| item.id == "27").unwrap();
    assert!(color.required);
    assert_eq!(color.input, AttributeInput::Select);
    assert_eq!(color.options.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(), vec!["Black", "默认"]);
    let size = attributes.iter().find(|item| item.id == "87").unwrap();
    assert!(size.required);
    assert_eq!(size.input, AttributeInput::Select);
    let material = attributes.iter().find(|item| item.id == "55").unwrap();
    assert!(material.required);
    assert_eq!(material.input, AttributeInput::Select);
    let length = attributes.iter().find(|item| item.id == "61").unwrap();
    assert!(!length.required);
    assert_eq!(length.input, AttributeInput::Number);
    let template_call = script.calls().into_iter().rev().find(|call| call.path.ends_with("query-attribute-template")).unwrap();
    assert_signed(&template_call, "/open-api/goods/query-attribute-template");
    assert_eq!(template_call.json, Some(json!({"product_type_id_list": [1080]})));
}

#[tokio::test]
async fn publish_sends_documented_skc_sku_and_attribute_payload() {
    let (script, transport) = scripted();
    queue_reads(&script, &["WH1"]);
    script.push("/open-api/goods/upload-pic", Step::Json(upload_ok()));
    script.push("/open-api/goods/product/publishOrEdit", Step::Json(publish_ok()));
    let outcome = push_listing(&transport, &shop(), &draft(), &target(true), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Reviewing);
    assert_eq!(outcome.remote_id.as_deref(), Some("MM2412222464"));
    assert!(outcome.reason.is_none());

    let calls = script.calls();
    let paths: Vec<_> = calls.iter().map(|call| call.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "/open-api/goods/query-category-tree",
            "/open-api/goods/query-attribute-template",
            "/open-api/goods/query-publish-fill-in-standard",
            "/open-api/goods/query-site-list",
            "/open-api/msc/warehouse/list",
            "/open-api/goods/upload-pic",
            "/open-api/goods/product/publishOrEdit",
        ]
    );
    let upload = &calls[5];
    assert!(upload.mutating);
    assert_eq!(upload.query, vec![("image_type".into(), "1".into())]);
    assert_eq!(upload.file_name.as_deref(), Some("dress.jpg"));
    assert_eq!(upload.file_bytes.as_deref(), Some(images()[0].1.as_slice()));
    assert!(upload.json.is_none());
    assert!(upload.headers.iter().all(|(name, _)| name != "Content-Type"));
    assert_signed(upload, "/open-api/goods/upload-pic");

    let publish = &calls[6];
    assert!(publish.mutating);
    assert_signed(publish, "/open-api/goods/product/publishOrEdit");
    assert_eq!(header(publish, "language"), "en");
    let body = publish.json.clone().unwrap();
    assert_eq!(body["category_id"], json!(1727));
    assert_eq!(body["product_type_id"], json!(1080));
    assert_eq!(body["source_system"], json!("OpenAPI"));
    assert_eq!(body["suit_flag"], json!(0));
    assert!(body.get("brand_code").is_none());
    assert!(body.get("edit_type").is_none());
    assert!(body.get("spu_name").is_none());
    assert!(body.get("supplier_code").is_none());
    assert_eq!(body["multi_language_name_list"], json!([{"language": "en", "name": "Cotton dress"}]));
    assert_eq!(body["multi_language_desc_list"], json!([{"language": "en", "name": "Soft cotton"}]));
    assert_eq!(
        body["product_attribute_list"],
        json!([{"attribute_id": 55, "attribute_value_id": 300}])
    );
    assert_eq!(body["site_list"], json!([{"main_site": "shein", "sub_site_list": ["shein-us"]}]));
    let skc = &body["skc_list"][0];
    assert_eq!(skc["supplier_code"], json!("SKU-S"));
    assert_eq!(skc["sale_attribute"], json!({"attribute_id": 27, "attribute_value_id": 100}));
    assert_eq!(
        skc["image_info"]["image_info_list"],
        json!([{"image_sort": 1, "image_type": 1, "image_url": "https://img.ltwebstatic.com/images/2024/dress.jpg"}])
    );
    let skus = skc["sku_list"].as_array().unwrap();
    assert_eq!(skus.len(), 2);
    assert_eq!(skus[0]["supplier_sku"], json!("SKU-S"));
    assert_eq!(skus[0]["mall_state"], json!(1));
    assert_eq!(skus[0]["weight"], json!(250.0));
    assert_eq!(skus[0]["length"], json!("30"));
    assert_eq!(skus[0]["width"], json!("20"));
    assert_eq!(skus[0]["height"], json!("2"));
    assert_eq!(skus[0]["sale_attribute_list"], json!([{"attribute_id": 87, "attribute_value_id": 200}]));
    assert_eq!(
        skus[0]["price_info_list"],
        json!([{"base_price": 19.9, "currency": "USD", "sub_site": "shein-us"}])
    );
    assert!(skus[0].get("cost_info").is_none());
    assert_eq!(
        skus[0]["stock_info_list"],
        json!([{"inventory_num": 4, "supplier_warehouse_id": "WH1"}])
    );
    assert_eq!(skus[1]["supplier_sku"], json!("SKU-M"));
    assert_eq!(skus[1]["sale_attribute_list"], json!([{"attribute_id": 87, "attribute_value_id": 201}]));
    assert_eq!(skus[1]["stock_info_list"][0]["inventory_num"], json!(0));
    assert_eq!(skus[1]["price_info_list"][0]["base_price"], json!(21.0));
}

#[tokio::test]
async fn missing_required_attribute_stops_before_upload() {
    let (script, transport) = scripted();
    queue_reads(&script, &["WH1"]);
    let outcome = push_listing(&transport, &shop(), &draft(), &target(false), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.remote_id.is_none());
    let reason = outcome.reason.unwrap();
    assert!(reason.contains("Material"), "{reason}");
    assert!(reason.contains("55"), "{reason}");
    let calls = script.calls();
    let paths: Vec<_> = calls.iter().map(|call| call.path.as_str()).collect();
    assert!(!paths.iter().any(|path| path.contains("upload-pic") || path.contains("publishOrEdit")));
}

#[tokio::test]
async fn missing_package_fields_make_no_request() {
    let (script, transport) = scripted();
    let mut incomplete = draft();
    incomplete.weight_kg = None;
    let outcome = push_listing(&transport, &shop(), &incomplete, &target(true), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.reason.unwrap().contains("weightKg"));
    assert!(script.calls().is_empty());
}

#[tokio::test]
async fn multiple_merchant_warehouses_stop_before_upload() {
    let (script, transport) = scripted();
    queue_reads(&script, &["WH1", "WH2"]);
    let outcome = push_listing(&transport, &shop(), &draft(), &target(true), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.reason.unwrap().contains("supplier_warehouse_id"));
    let calls = script.calls();
    let paths: Vec<_> = calls.iter().map(|call| call.path.as_str()).collect();
    assert_eq!(*paths.last().unwrap(), "/open-api/msc/warehouse/list");
    assert!(!paths.iter().any(|path| path.contains("upload-pic")));
}

#[tokio::test]
async fn pre_valid_result_is_a_completed_rejection() {
    let (script, transport) = scripted();
    queue_reads(&script, &["WH1"]);
    script.push("/open-api/goods/upload-pic", Step::Json(upload_ok()));
    script.push(
        "/open-api/goods/product/publishOrEdit",
        Step::Json(ok(json!({
            "success": false,
            "pre_valid_result": [{"messages": ["标题长度不符合要求"]}]
        }))),
    );
    let outcome = push_listing(&transport, &shop(), &draft(), &target(true), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Rejected);
    assert!(outcome.remote_id.is_none());
    assert_eq!(outcome.reason.as_deref(), Some("标题长度不符合要求"));
}

#[tokio::test]
async fn publish_after_dispatch_is_uncertain_and_not_success() {
    let (script, transport) = scripted();
    queue_reads(&script, &["WH1"]);
    script.push("/open-api/goods/upload-pic", Step::Json(upload_ok()));
    script.push(
        "/open-api/goods/product/publishOrEdit",
        Step::Fault(TransportFault::AfterDispatch("连接中断".into())),
    );
    let outcome = push_listing(&transport, &shop(), &draft(), &target(true), &images(), NOW, None).await;
    assert_eq!(outcome.status, ListingStatus::Uncertain);
    assert!(outcome.remote_id.is_none());
    assert_eq!(outcome.reason.as_deref(), Some("连接中断"));
    assert_eq!(
        script.calls().iter().filter(|call| call.path.ends_with("publishOrEdit")).count(),
        1
    );
}

#[tokio::test]
async fn document_state_refreshes_live_rejected_and_reviewing() {
    let (script, transport) = scripted();
    script.push("/open-api/goods/query-document-state", Step::Json(document(2, None)));
    script.push("/open-api/goods/query-document-state", Step::Json(document(3, Some("图片不符合规范"))));
    script.push("/open-api/goods/query-document-state", Step::Json(document(1, None)));
    let live = refresh_remote(&transport, &shop(), NOW, "MM2412222464").await.expect("live");
    assert_eq!(live, (ListingStatus::Live, None));
    let rejected = refresh_remote(&transport, &shop(), NOW, "MM2412222464").await.expect("rejected");
    assert_eq!(rejected.0, ListingStatus::Rejected);
    assert_eq!(rejected.1.as_deref(), Some("图片不符合规范"));
    let reviewing = refresh_remote(&transport, &shop(), NOW, "MM2412222464").await.expect("reviewing");
    assert_eq!(reviewing, (ListingStatus::Reviewing, None));
    let calls = script.calls();
    assert!(calls.iter().all(|call| call.path == "/open-api/goods/query-document-state"));
    assert_eq!(calls[0].json, Some(json!({"spuList": [{"spuName": "MM2412222464"}]})));
    assert!(calls[0].json.as_ref().unwrap()["spuList"][0].get("version").is_none());

    let (script, transport) = scripted();
    script.push("/open-api/goods/query-document-state", Step::Json(document(2, None)));
    let mut incomplete = draft();
    incomplete.weight_kg = None;
    let outcome = push_listing(&transport, &shop(), &incomplete, &target(true), &[], NOW, Some("MM2412222464")).await;
    assert_eq!(outcome.status, ListingStatus::Live);
    assert_eq!(outcome.remote_id.as_deref(), Some("MM2412222464"));
    assert_eq!(script.calls().len(), 1);
}

#[tokio::test]
async fn metrics_keep_real_zeros_and_omit_missing_money() {
    let (script, transport) = scripted();
    script.push(
        "/open-api/order/order-list",
        Step::Json(order_list(
            json!([
                order_row("GSON1", "2", "2024-04-01 01:00:00"),
                order_row("GSON2", "4", "2024-04-01 02:00:00"),
                order_row("GSON3", "6", "2024-04-01 03:00:00")
            ]),
            3,
        )),
    );
    script.push(
        "/open-api/order/order-detail",
        Step::Json(json!({
            "code": 0,
            "msg": "OK",
            "info": [
                {"orderNo": "GSON1", "orderStatus": 2, "productTotalPrice": 19.9, "orderCurrency": "USD"},
                {"orderNo": "GSON2", "orderStatus": 4, "productTotalPrice": 0, "orderCurrency": "USD"},
                {"orderNo": "GSON3", "orderStatus": 6, "productTotalPrice": 50, "orderCurrency": "EUR"}
            ]
        })),
    );
    script.push(
        "/open-api/return-order/list",
        Step::Json(ok(json!({"count": 1, "returnOrderList": [{
            "returnOrderNo": "R100",
            "returnOrderStatus": 2,
            "addTime": "2024-04-01 04:00:00"
        }]}))),
    );
    script.push(
        "/open-api/return-order/details",
        Step::Json(ok(json!([{"returnOrderNo": "R100", "returnOrderStatus": 2, "noReturnGoodsSign": 0, "returnGoodsInfoList": [{
            "currency": "USD",
            "estimateIncomeMoney": 3.0
        }]}]))),
    );
    script.push("/open-api/goods/searchProduct", Step::Json(ok(json!({"meta": {"count": 4}, "data": []}))));
    let metrics = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Today, NOW).await.expect("metrics");
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&2.0));
    assert_eq!(metrics.values.get(&MetricKey::PendingShipment), Some(&1.0));
    assert_eq!(metrics.values.get(&MetricKey::Gmv), Some(&19.9));
    assert_eq!(metrics.currency.as_deref(), Some("USD"));
    assert_eq!(metrics.values.get(&MetricKey::RefundOrders), Some(&1.0));
    assert_eq!(metrics.values.get(&MetricKey::RefundAmount), Some(&3.0));
    assert_eq!(metrics.values.get(&MetricKey::ProductsLive), Some(&4.0));
    assert!(!metrics.values.contains_key(&MetricKey::Buyers));
    assert_eq!(metrics.unsupported, vec![MetricKey::Buyers]);
    let list = &script.calls()[0];
    assert_eq!(list.json.as_ref().unwrap()["queryType"], json!(1));
    assert_eq!(list.json.as_ref().unwrap()["startTime"], json!("2024-04-01 00:00:00"));
    assert_eq!(list.json.as_ref().unwrap()["endTime"], json!("2024-04-01 08:00:00"));
    assert_eq!(list.json.as_ref().unwrap()["pageSize"], json!(30));
    assert_eq!(script.calls()[1].json.as_ref().unwrap()["orderNoList"], json!(["GSON1", "GSON2", "GSON3"]));
    assert_eq!(script.calls()[3].json.as_ref().unwrap()["returnOrderNoList"], json!(["R100"]));
    let live = &script.calls()[4];
    assert_eq!(live.json.as_ref().unwrap()["skcShelfStatus"], json!(1));
    assert_eq!(live.json.as_ref().unwrap()["pageSize"], json!(1));

    let (script, transport) = scripted();
    script.push("/open-api/order/order-list", Step::Json(order_list(json!([]), 0)));
    script.push("/open-api/return-order/list", Step::Json(ok(json!({"count": 0, "returnOrderList": []}))));
    script.push("/open-api/goods/searchProduct", Step::Json(ok(json!({"meta": {"count": 0}, "data": []}))));
    let zeros = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Today, NOW).await.expect("zeros");
    assert_eq!(zeros.values.get(&MetricKey::Orders), Some(&0.0));
    assert_eq!(zeros.values.get(&MetricKey::Gmv), Some(&0.0));
    assert_eq!(zeros.values.get(&MetricKey::PendingShipment), Some(&0.0));
    assert_eq!(zeros.values.get(&MetricKey::RefundOrders), Some(&0.0));
    assert_eq!(zeros.values.get(&MetricKey::RefundAmount), Some(&0.0));
    assert_eq!(zeros.values.get(&MetricKey::ProductsLive), Some(&0.0));
    assert_eq!(zeros.unsupported, vec![MetricKey::Buyers]);
    assert_eq!(script.calls().len(), 3);

    let (script, transport) = scripted();
    script.push(
        "/open-api/order/order-list",
        Step::Json(order_list(
            json!([
                order_row("GSON1", "2", "2024-04-01 01:00:00"),
                order_row("GSON2", "2", "2024-04-01 02:00:00")
            ]),
            2,
        )),
    );
    script.push(
        "/open-api/order/order-detail",
        Step::Json(ok(json!([
            {"orderNo": "GSON1", "orderStatus": 2, "productTotalPrice": 10, "orderCurrency": "USD"},
            {"orderNo": "GSON2", "orderStatus": 2, "orderCurrency": "EUR"}
        ]))),
    );
    script.push(
        "/open-api/return-order/list",
        Step::Json(ok(json!({"count": 2, "returnOrderList": [
            {"returnOrderNo": "R1", "returnOrderStatus": 2},
            {"returnOrderNo": "R2", "returnOrderStatus": 9}
        ]}))),
    );
    script.push(
        "/open-api/return-order/details",
        Step::Json(ok(json!([
            {"returnOrderNo": "R1", "returnOrderStatus": 2, "noReturnGoodsSign": 1, "returnGoodsInfoList": [{"estimateIncomeMoney": 8, "currency": "USD"}]},
            {"returnOrderNo": "R2", "returnOrderStatus": 9, "noReturnGoodsSign": 0, "returnGoodsInfoList": [{"sellerCurrencyPrice": 4, "currency": "USD"}]}
        ]))),
    );
    script.push("/open-api/goods/searchProduct", Step::Json(ok(json!({"data": []}))));
    let missing = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Today, NOW).await.expect("missing");
    assert_eq!(missing.values.get(&MetricKey::Orders), Some(&2.0));
    assert!(!missing.values.contains_key(&MetricKey::Gmv));
    assert!(!missing.values.contains_key(&MetricKey::RefundAmount));
    assert_eq!(missing.values.get(&MetricKey::RefundOrders), Some(&1.0));
    assert!(!missing.values.contains_key(&MetricKey::ProductsLive));
    assert!(!missing.values.contains_key(&MetricKey::Buyers));
    for key in [MetricKey::Gmv, MetricKey::RefundAmount, MetricKey::ProductsLive, MetricKey::Buyers] {
        assert!(missing.unsupported.contains(&key), "{key:?}");
    }
}

#[tokio::test]
async fn auth_failure_rejects_instead_of_zero_metrics() {
    let (script, transport) = scripted();
    script.push("/open-api/order/order-list", Step::Json(json!({"code": "9999400", "msg": "signature error"})));
    let error = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Today, NOW).await.expect_err("auth");
    assert!(matches!(error, CommerceError::Rejected(_)));
    assert!(error.message().contains("signature"));
    assert_eq!(script.calls().len(), 1);

    let (script, transport) = scripted();
    script.push(
        "/open-api/order/order-list",
        Step::Json(json!({"code": "9996002", "msg": "店铺类型不支持，该接口仅支持自运营和半托管"})),
    );
    let error = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Today, NOW).await.expect_err("mode");
    assert!(matches!(error, CommerceError::Rejected(message) if message.contains("自运营")));
    assert_eq!(script.calls().len(), 1);
}

#[tokio::test]
async fn last7_metrics_split_into_documented_48h_windows() {
    let (start, end) = super::metric_window(MetricRange::Last7, NOW);
    let chunks = super::time_chunks(start, end);
    assert_eq!(chunks.len(), 4);
    for (chunk_start, chunk_end) in &chunks {
        assert!(chunk_end - chunk_start < 48 * 3600);
    }
    let (script, transport) = scripted();
    for _ in &chunks {
        script.push("/open-api/order/order-list", Step::Json(order_list(json!([]), 0)));
        script.push("/open-api/return-order/list", Step::Json(ok(json!({"count": 0, "returnOrderList": []}))));
    }
    script.push("/open-api/goods/searchProduct", Step::Json(ok(json!({"meta": {"count": 0}, "data": []}))));
    let metrics = fetch_metrics(&transport, &shop(), "shop-1", MetricRange::Last7, NOW).await.expect("last7");
    assert_eq!(metrics.values.get(&MetricKey::Orders), Some(&0.0));
    assert_eq!(metrics.values.get(&MetricKey::Gmv), Some(&0.0));
    let calls = script.calls();
    let order_calls: Vec<_> = calls.iter().filter(|call| call.path.ends_with("order-list")).collect();
    let return_calls: Vec<_> = calls.iter().filter(|call| call.path.ends_with("return-order/list")).collect();
    assert_eq!(order_calls.len(), chunks.len());
    assert_eq!(return_calls.len(), chunks.len());
    for (call, (chunk_start, chunk_end)) in order_calls.iter().zip(&chunks) {
        assert_eq!(call.json.as_ref().unwrap()["startTime"], json!(super::beijing_text(*chunk_start)));
        assert_eq!(call.json.as_ref().unwrap()["endTime"], json!(super::beijing_text(*chunk_end)));
        assert_eq!(call.json.as_ref().unwrap()["queryType"], json!(1));
    }
}

#[tokio::test]
async fn products_page_uses_search_product() {
    let (script, transport) = scripted();
    script.push(
        "/open-api/goods/searchProduct",
        Step::Json(ok(json!({
            "meta": {"count": 2},
            "data": [
                {
                    "spuName": "MM2412222464",
                    "spuShelfStatus": 1,
                    "categoryId": 1727,
                    "skcList": [{
                        "skcName": "skc1",
                        "skcTitle": [{"language": "en", "title": "Cotton dress"}],
                        "skuList": [{"skuCode": "I5m2h7t1x"}]
                    }]
                },
                {"spuName": "MM000", "spuShelfStatus": 0, "skcList": []}
            ]
        }))),
    );
    let page = fetch_products(&transport, &shop(), NOW, 1).await.expect("products");
    assert_eq!(page.page, 1);
    assert_eq!(page.page_size, 10);
    assert_eq!(page.total, Some(2));
    assert_eq!(page.items[0].remote_id, "MM2412222464");
    assert_eq!(page.items[0].title.as_deref(), Some("Cotton dress"));
    assert_eq!(page.items[0].status, ListingStatus::Live);
    assert_eq!(page.items[0].skus, vec!["I5m2h7t1x".to_string()]);
    assert_eq!(page.items[1].status, ListingStatus::Reviewing);
    let body = script.calls()[0].json.clone().unwrap();
    assert_eq!(body["pageNum"], json!(1));
    assert_eq!(body["pageSize"], json!(10));
    assert!(body.get("skcShelfStatus").is_none());
}

#[tokio::test]
async fn orders_page_stays_inside_one_48h_window() {
    let (script, transport) = scripted();
    script.push(
        "/open-api/order/order-list",
        Step::Json(order_list(json!([order_row("GSON1", "2", "2024-04-01 01:00:00")]), 1)),
    );
    script.push(
        "/open-api/order/order-detail",
        Step::Json(ok(json!([{"orderNo": "GSON1", "orderStatus": 2, "productTotalPrice": 19.9, "orderCurrency": "USD"}]))),
    );
    let page = fetch_orders(&transport, &shop(), NOW, MetricRange::Today, 1).await.expect("orders");
    assert_eq!(page.page, 1);
    assert_eq!(page.page_size, 30);
    assert_eq!(page.total, Some(1));
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].remote_id, "GSON1");
    assert_eq!(page.items[0].status, "2");
    assert_eq!(page.items[0].amount, Some(19.9));
    assert_eq!(page.items[0].currency.as_deref(), Some("USD"));
    assert_eq!(page.items[0].created_at.as_deref(), Some("2024-04-01 01:00:00"));
    assert_eq!(script.calls().len(), 2);

    let (script, transport) = scripted();
    script.push("/open-api/order/order-list", Step::Json(order_list(json!([]), 0)));
    let yesterday = fetch_orders(&transport, &shop(), NOW, MetricRange::Yesterday, 1).await.expect("yesterday");
    assert!(yesterday.items.is_empty());
    let body = script.calls()[0].json.clone().unwrap();
    assert_eq!(body["startTime"], json!("2024-03-31 00:00:00"));
    assert_eq!(body["endTime"], json!("2024-03-31 23:59:59"));
    assert_eq!(script.calls().len(), 1);

    let (script, transport) = scripted();
    let error = fetch_orders(&transport, &shop(), NOW, MetricRange::Last7, 1).await.expect_err("last7");
    assert!(matches!(error, CommerceError::Invalid(_)));
    assert!(error.message().contains("48"));
    assert!(script.calls().is_empty());

    let (script, transport) = scripted();
    let error = fetch_orders(&transport, &shop(), NOW, MetricRange::Today, 0).await.expect_err("page");
    assert!(matches!(error, CommerceError::Invalid(_)));
    assert!(script.calls().is_empty());
}
