use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Json, Query};
use static_embedding_server::server::{self, EmbeddingRequest, QueryParams};
use static_embedding_server::server::state::{AppState, Model};

#[derive(Clone)]
struct MockModel;

impl Model for MockModel {
    fn encode(&self, inputs: &[String]) -> Vec<Vec<f32>> {
        inputs.iter().map(|_| vec![1.0, 2.0]).collect()
    }
}

fn make_state() -> Arc<AppState> {
    let mut models: HashMap<String, Arc<dyn Model>> = HashMap::new();
    models.insert("default".into(), Arc::new(MockModel));
    Arc::new(AppState { models, default_model: "default".into(), startup_time: std::time::SystemTime::now() })
}

#[tokio::test]
async fn embeddings_handler_happy_path() {
    let state = make_state();
    let req = EmbeddingRequest { input: vec!["hi".into(), "there".into()], model: None };
    let params = QueryParams { model: None };
    let res = server::embeddings_handler(axum::extract::State(state), Query(params), Json(req)).await;
    let axum::response::Json(resp) = res;
    assert_eq!(resp.data.len(), 2);
    assert_eq!(resp.data[0].embedding, vec![1.0, 2.0]);
    assert_eq!(resp.model, "default");
}
