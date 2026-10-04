use serde_json::{Value, json};

pub async fn mock() -> (String, tokio::task::JoinHandle<()>) {
    async fn respond(axum::Json(request): axum::Json<Value>) -> axum::Json<Value> {
        let content = if request["messages"][0]["content"]
            .as_str()
            .is_some_and(|prompt| prompt.contains("Review confirmed lipid"))
        {
            let input: Value =
                serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
            json!({"summary":"Discuss the observed lipid trend.","findings":[{"topic":"lipid_risk","title":"Lipid trend","hypothesis":"A risk factor to discuss, not a diagnosis.",
                "observation_ids":[input["observations"][0]["observation_id"]],"evidence_source_ids":["medlineplus-cholesterol"],"other_explanations":["Measurement context"],"missing_information":["Family history"],"questions_for_clinician":["How should this trend be evaluated?"]}]}).to_string()
        } else {
            let mut payload = crate::observation_payload();
            payload.metric_id = None;
            json!({"observations":[payload],"warnings":[]}).to_string()
        };
        axum::Json(json!({"choices":[{"message":{"content":content}}]}))
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new().route("/v1/chat/completions", axum::routing::post(respond))
                .route("/api/v1/documents/extract", axum::routing::post(|headers: axum::http::HeaderMap, axum::Json(request): axum::Json<Value>| async move {
                    assert_eq!(headers["authorization"], "Bearer synthetic-ocr-service-key-123456789");
                    assert!(request["image_url"].as_str().unwrap().starts_with("data:image/png;base64,"));
                    assert!(request.get("model").is_none());
                    assert!(request.get("text_layer").is_some());
                    let mut observation = crate::observation_payload();
                    observation.metric_id = None;
                    observation.source.page = request["page"].as_i64().unwrap();
                    let page = json!({"observations":[observation],"warnings":[]});
                    axum::Json(json!({"model":"qwen3.8-27b-ninfer-nvfp4","engine":"ninfer","prompt_version":"laboratory-page-v2","content":page.to_string(),"raw_response_body":"complete native output","observations":page["observations"],"warnings":[]}))
                })),
        )
        .await
        .unwrap();
    });
    (format!("http://{address}/v1"), task)
}

#[test]
fn structured_response_requires_valid_complete_json() {
    assert_eq!(
        helpyourself::provider::parse_json::<Value>("```json\n{\"ok\":true}\n```").unwrap()["ok"],
        true
    );
    assert!(helpyourself::provider::parse_json::<Value>("```json {} ").is_err());
    assert!(helpyourself::provider::parse_json::<Value>("not json").is_err());
}

#[tokio::test]
async fn temporary_provider_failure_retries_but_auth_failure_does_not() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    for code in [429, 401] {
        let attempts = Arc::new(AtomicUsize::new(0));
        let counter = attempts.clone();
        let router = axum::Router::new().route(
            "/v1/chat/completions",
            axum::routing::post(move |headers: axum::http::HeaderMap| {
                let counter = counter.clone();
                async move {
                    assert_eq!(headers["authorization"], "Bearer synthetic-analysis-key");
                    if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                        (
                            axum::http::StatusCode::from_u16(code).unwrap(),
                            axum::Json(json!({})),
                        )
                    } else {
                        (
                            axum::http::StatusCode::OK,
                            axum::Json(json!({"choices":[{"message":{"content":"ready"}}]})),
                        )
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let mut config: helpyourself::config::Config =
            toml::from_str(helpyourself::config::TEMPLATE).unwrap();
        config.providers.analysis.enabled = true;
        config.providers.analysis.api_key = "synthetic-analysis-key".into();
        config.providers.analysis.base_url = format!("http://{address}/v1");
        let result = helpyourself::provider::probe(&config.providers.analysis).await;
        assert_eq!(result.is_ok(), code == 429);
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            if code == 429 { 2 } else { 1 }
        );
        task.abort();
    }
}
