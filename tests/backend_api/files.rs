use helpyourself::{config::Config, files::inspect_document};

#[test]
fn detects_real_content_and_rejects_bad_or_oversized_images() {
    let configuration: Config = toml::from_str(helpyourself::config::TEMPLATE).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("misleading.pdf");
    std::fs::write(&path, crate::png()).unwrap();
    assert_eq!(
        inspect_document(&path, &configuration.storage).unwrap(),
        ("image/png".into(), 1)
    );
    let mut small = configuration.storage.clone();
    small.maximum_image_pixels = 1;
    assert!(inspect_document(&path, &small).is_err());
    std::fs::write(&path, b"not a report").unwrap();
    assert!(inspect_document(&path, &configuration.storage).is_err());
    std::fs::write(&path, b"%PDF-broken").unwrap();
    assert!(inspect_document(&path, &configuration.storage).is_err());
}

#[test]
fn parses_pdf_pages_and_enforces_page_limit() {
    let configuration: Config = toml::from_str(helpyourself::config::TEMPLATE).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("report.pdf");
    std::fs::write(&path, synthetic_pdf()).unwrap();
    assert_eq!(
        inspect_document(&path, &configuration.storage).unwrap(),
        ("application/pdf".into(), 1)
    );
    let mut limits = configuration.storage.clone();
    limits.maximum_pdf_pages = 0;
    assert!(inspect_document(&path, &limits).is_err());
}

pub fn synthetic_pdf() -> Vec<u8> {
    use lopdf::{Document, Object, dictionary};
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let page_id = document.add_object(dictionary! {"Type"=>"Page", "Parent"=>pages_id, "MediaBox"=>vec![0.into(),0.into(),100.into(),100.into()]});
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {"Type"=>"Pages", "Kids"=>vec![page_id.into()], "Count"=>1}),
    );
    let catalog = document.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages_id});
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

#[tokio::test]
async fn heic_original_is_preserved_and_jpeg_is_only_a_processing_copy() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "alice").await;
    let token = crate::token(&state, "alice").await;
    let app = helpyourself::app::create_application(state.clone());
    let mut jpeg = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(2, 2)
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .unwrap();
    let original = b"\0\0\0\x18ftypheic\0\0\0\0heicmif1synthetic opaque original";
    let mut body = b"--photo\r\nContent-Disposition: form-data; name=\"file\"; filename=\"processing.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n".to_vec();
    body.extend_from_slice(jpeg.get_ref());
    body.extend_from_slice(b"\r\n--photo\r\nContent-Disposition: form-data; name=\"original\"; filename=\"camera.heic\"\r\nContent-Type: image/heic\r\n\r\n");
    body.extend_from_slice(original);
    body.extend_from_slice(b"\r\n--photo--\r\n");
    let upload_id = uuid::Uuid::new_v4().to_string();
    let request = || {
        Request::builder()
            .method("POST")
            .uri("/api/v1/files/upload")
            .header("authorization", format!("Bearer {token}"))
            .header("x-upload-id", &upload_id)
            .header("content-type", "multipart/form-data; boundary=photo")
            .body(Body::from(body.clone()))
            .unwrap()
    };
    let response = app.clone().oneshot(request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let file = &result["file"];
    assert_eq!(file["original_name"], "camera.heic");
    assert_eq!(file["content_type"], "image/heic");
    assert_eq!(
        std::fs::read(
            state
                .config
                .server
                .data_dir
                .join(file["relative_path"].as_str().unwrap())
        )
        .unwrap(),
        original
    );
    assert_eq!(
        std::fs::read(
            state
                .config
                .server
                .data_dir
                .join(file["processing_path"].as_str().unwrap())
        )
        .unwrap(),
        jpeg.get_ref().as_slice()
    );
    let response = app.clone().oneshot(request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let replay: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(replay["replayed"], true);
    let file_id = file["file_id"].as_str().unwrap();
    state
        .database
        .delete_report(&user.user_id, file_id, 1)
        .await
        .unwrap();
    helpyourself::maintenance::cleanup(&state).await.unwrap();
    assert!(
        !state
            .config
            .server
            .data_dir
            .join(file["relative_path"].as_str().unwrap())
            .exists()
    );
    assert!(
        !state
            .config
            .server
            .data_dir
            .join(file["processing_path"].as_str().unwrap())
            .exists()
    );
}
