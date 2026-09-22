use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn signing_matches_reference_vector() {
    let ak = "0123456789abcdef0123456789abcdeftest-access-id";
    let headers = signed_headers(
        ak,
        r#"{"imgBase64":"test","pageSize":10}"#,
        "1790070000",
        "abc12345",
    )
    .unwrap();
    assert_eq!(headers["x-csk-ak"], "test-access-id");
    assert_eq!(
        headers["x-csk-sign"],
        "Sx/4RXdCj7za1wRQCa9ijja676a6llp9zZYNWN7Iy9E="
    );
    let encoded = URL_SAFE_NO_PAD.encode(ak);
    assert_eq!(keys(&encoded).unwrap(), keys(ak).unwrap());
    assert!(keys("").is_err());
    assert!(keys("short").is_err());
}
#[test]
fn parses_products_without_fabricating_missing_metrics() {
    let rows=response_products(json!({"success":true,"model":{"data":[
        {"itemId":987622522091u64,"title":"样品","currentPrice":12.8,"company":"工厂","imageUrl":"//img.alicdn.com/a.jpg","skuId":"123","score":0.99},
        {"itemId":"987622522091","title":"另一个规格","skuId":"456"}
    ]}})).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "987622522091");
    assert_eq!(rows[0].price.as_deref(), Some("12.8"));
    assert_eq!(rows[1].price, None);
    assert_eq!(rows[1].sold_count, None);
    assert_eq!(
        rows[0].image_url.as_deref(),
        Some("https://img.alicdn.com/a.jpg")
    );
    assert!(response_products(json!({"data":{}})).is_err());
    assert!(
        response_products(json!({"success":false,"code":"SignatureInvalid"}))
            .unwrap_err()
            .contains("签名")
    );
    assert!(response_products(json!({"data":{"data":[]}}))
        .unwrap()
        .is_empty());
}
#[test]
fn image_processing_limits_and_white_alpha() {
    assert!(image_base64("data:image/png;base64,aGVsbG8=").is_err());
    assert!(image_base64("data:image/svg+xml;base64,abc").is_err());
    let image = image::RgbaImage::from_pixel(1000, 500, image::Rgba([0, 0, 0, 0]));
    let mut buf = Cursor::new(Vec::new());
    image.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    let output = image_base64(&format!(
        "data:image/png;base64,{}",
        STANDARD.encode(buf.into_inner())
    ))
    .unwrap();
    let resized = image::load_from_memory(&STANDARD.decode(output).unwrap())
        .unwrap()
        .to_rgb8();
    assert_eq!(resized.dimensions(), (800, 400));
    assert!(resized.get_pixel(0, 0)[0] > 250);
}
#[tokio::test]
async fn posts_signed_body_and_handles_http_and_business_errors() {
    for (status, payload, expected) in [
        (
            "200 OK",
            r#"{"success":true,"data":{"data":[{"itemId":123,"title":"商品"}]}}"#,
            None,
        ),
        (
            "200 OK",
            r#"{"success":false,"code":"QosApiFrequencyLimit"}"#,
            Some("频率"),
        ),
        (
            "401 Unauthorized",
            r#"{"secret":"must-not-leak"}"#,
            Some("HTTP 401"),
        ),
        ("200 OK", "not-json", Some("JSON")),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let payload = payload.to_owned();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut b = [0; 4096];
                let n = stream.read(&mut b).await.unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&b[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
                    let len = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .unwrap()
                        .parse::<usize>()
                        .unwrap();
                    if request.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            let request = String::from_utf8(request).unwrap();
            assert!(request.to_ascii_lowercase().contains("x-csk-sign:"));
            assert!(request.ends_with(r#"{"imgBase64":"abc"}"#));
            stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",payload.len()).as_bytes()).await.unwrap();
        });
        let result = request_products(
            &reqwest::Client::new(),
            &format!("http://{address}{PATH}"),
            "0123456789abcdef0123456789abcdeftest-access-id",
            r#"{"imgBase64":"abc"}"#.into(),
        )
        .await;
        server.await.unwrap();
        if let Some(expected) = expected {
            let error = result.unwrap_err();
            assert!(error.contains(expected), "{error}");
            assert!(!error.contains("must-not-leak"));
        } else {
            assert_eq!(result.unwrap()[0].id, "123");
        }
    }
}

#[tokio::test]
async fn prevents_overlapping_searches_and_releases_slot_after_failure() {
    let request = || LookalikeRequest {
        image: String::new(),
        name: "test".into(),
        sort: ProductSort::Relevance,
        limit: 10,
        purchase_amount: 1,
    };
    let permit = SEARCH_SLOT.try_acquire().unwrap();
    assert!(search(String::new(), request())
        .await
        .unwrap_err()
        .contains("正在进行"));
    drop(permit);
    assert!(search(String::new(), request())
        .await
        .unwrap_err()
        .contains("AK"));
    assert!(SEARCH_SLOT.try_acquire().is_ok());
}
