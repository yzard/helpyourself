use helpyourself::documents::{parse_pdf_layer, pdf_evidence};
use helpyourself::reports::ExtractionInputRequest;

pub fn mixed_pdf() -> Vec<u8> {
    use lopdf::{Document, Object, Stream, dictionary};
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let font = document
        .add_object(dictionary! {"Type"=>"Font", "Subtype"=>"Type1", "BaseFont"=>"Helvetica"});
    let image = document.add_object(Stream::new(dictionary! {"Type"=>"XObject", "Subtype"=>"Image", "Width"=>2, "Height"=>2, "ColorSpace"=>"DeviceRGB", "BitsPerComponent"=>8}, vec![255; 12]));
    let mut pages = Vec::new();
    for text in [
        Some("LDL Cholesterol 2.586 mmol/L reference <2.586"),
        None,
        Some("Report heading only"),
    ] {
        let mut content = b"q 50 0 0 50 20 20 cm /Im1 Do Q\n".to_vec();
        if let Some(text) = text {
            content
                .extend_from_slice(format!("BT /F1 14 Tf 30 250 Td ({text}) Tj ET\n").as_bytes());
        }
        let stream = document.add_object(Stream::new(dictionary! {}, content));
        let resources =
            dictionary! {"Font"=>dictionary! {"F1"=>font}, "XObject"=>dictionary! {"Im1"=>image}};
        pages.push(document.add_object(dictionary! {"Type"=>"Page", "Parent"=>pages_id, "MediaBox"=>vec![0.into(),0.into(),600.into(),300.into()], "Resources"=>resources, "Contents"=>stream}));
    }
    document.objects.insert(pages_id, Object::Dictionary(dictionary! {"Type"=>"Pages", "Kids"=>pages.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(), "Count"=>3}));
    let catalog = document.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages_id});
    document.trailer.set("Root", catalog);
    let mut output = Vec::new();
    document.save_to(&mut output).unwrap();
    output
}

#[tokio::test]
async fn real_poppler_preserves_text_positions_and_does_not_skip_mixed_pages() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mixed.pdf");
    let cpu = helpyourself::execution::CpuExecutor::new(2).unwrap();
    std::fs::write(&path, mixed_pdf()).unwrap();
    let text = pdf_evidence(&cpu, &path, 1).await.unwrap();
    let layer = text.text_layer.unwrap();
    assert_eq!(layer.status, "available");
    assert!(layer.text.contains("2.586 mmol/L"));
    assert!(layer.words.iter().all(|word| {
        word.bounding_box
            .iter()
            .all(|value| (0.0..=1.0).contains(value))
    }));
    assert!(text.raw_bbox_xml.unwrap().contains("xMin"));
    assert_eq!(
        pdf_evidence(&cpu, &path, 2)
            .await
            .unwrap()
            .text_layer
            .unwrap()
            .status,
        "empty"
    );
    let mixed = pdf_evidence(&cpu, &path, 3).await.unwrap();
    assert_eq!(mixed.text_layer.unwrap().text, "Report heading only");
    assert_eq!(mixed.image_kind, "pdf_render_2400");
}

#[test]
fn invalid_or_excessive_layers_are_explicit_and_never_partial_success() {
    for xml in [
        "not XML",
        "<page width=\"0\" height=\"10\"/>",
        "<page width=\"10\" height=\"10\"><line><word xMin=\"NaN\" yMin=\"1\" xMax=\"2\" yMax=\"2\">x</word></line></page>",
    ] {
        let evidence = parse_pdf_layer(xml.into());
        assert_eq!(evidence.text_layer.unwrap().status, "unavailable");
        assert!(evidence.error_code.is_some());
    }
    let words = (0..4097)
        .map(|_| "<word xMin=\"1\" yMin=\"1\" xMax=\"2\" yMax=\"2\">x</word>")
        .collect::<String>();
    let evidence = parse_pdf_layer(format!(
        "<page width=\"10\" height=\"10\"><line>{words}</line></page>"
    ));
    let layer = evidence.text_layer.unwrap();
    assert_eq!(layer.status, "limit_exceeded");
    assert!(layer.text.is_empty() && layer.words.is_empty());
}

#[tokio::test]
async fn evidence_is_archived_exported_owner_scoped_and_deleted_with_report() {
    let (_directory, mut state) = crate::fixture().await;
    let alice = crate::add_user(&state, "alice").await;
    let bob = crate::add_user(&state, "bob").await;
    let (address, server) = super::provider::mock().await;
    let mut config = (*state.config).clone();
    config.ocr.url = address.trim_end_matches("/v1").into();
    state.config = std::sync::Arc::new(config);
    let app = helpyourself::app::create_application(state.clone());
    let token = crate::token(&state, "alice").await;
    let (status, upload) = crate::upload(
        &app,
        &token,
        &uuid::Uuid::new_v4().to_string(),
        &mixed_pdf(),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let report = upload["file"]["file_id"].as_str().unwrap();
    helpyourself::worker::extract_next(&state).await.unwrap();
    let document = state.database.report(&alice.user_id, report).await.unwrap();
    let inputs = document["extraction_inputs"].as_array().unwrap();
    assert_eq!(inputs.len(), 3);
    assert_eq!(document["pages"].as_array().unwrap().len(), 3);
    let first = inputs.iter().find(|input| input["page"] == 1).unwrap();
    assert_eq!(first["text_status"], "available");
    let request = || ExtractionInputRequest {
        report_id: report.into(),
        run_id: first["run_id"].as_str().unwrap().into(),
        page: 1,
    };
    let evidence = state
        .database
        .extraction_input(&alice.user_id, request())
        .await
        .unwrap();
    assert!(
        evidence["text_layer"]["text"]
            .as_str()
            .unwrap()
            .contains("mmol/L")
    );
    assert!(
        state
            .database
            .extraction_input(&bob.user_id, request())
            .await
            .is_err()
    );
    let body = serde_json::json!({"report_id":report,"run_id":first["run_id"],"page":1});
    let (status, _) = crate::request(&app, "/api/v1/reports/input/get", None, body.clone()).await;
    assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
    let bob_token = crate::token(&state, "bob").await;
    let (status, _) = crate::request(
        &app,
        "/api/v1/reports/input/get",
        Some(&bob_token),
        body.clone(),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    let (status, received) =
        crate::request(&app, "/api/v1/reports/input/get", Some(&token), body).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(received, evidence);
    let export = state.database.request_export(&alice.user_id).await.unwrap();
    helpyourself::maintenance::export_next(&state)
        .await
        .unwrap();
    let path = state
        .config
        .data_dir
        .join(format!("exports/{}/{export}.zip", alice.user_id));
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    use std::io::Read;
    let mut archived = String::new();
    zip.by_name("extraction_inputs.jsonl")
        .unwrap()
        .read_to_string(&mut archived)
        .unwrap();
    assert!(archived.contains("2.586 mmol/L"));
    drop(zip);
    state
        .database
        .delete_report(
            &alice.user_id,
            report,
            document["report"]["revision"].as_i64().unwrap(),
        )
        .await
        .unwrap();
    assert!(
        state
            .database
            .extraction_input(&alice.user_id, request())
            .await
            .is_err()
    );
    server.abort();
}
