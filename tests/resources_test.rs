use static_embedding_server::resources;

#[test]
fn list_resources_contains_instructions() {
    let list = resources::list_resources();
    assert!(!list.is_empty());
    assert!(list.iter().any(|r| r.raw.uri == "embedtool://instructions"));
}

#[test]
fn read_instructions_resource() {
    let result = resources::read_resource("embedtool://instructions");
    assert!(result.is_some());
    let result = result.unwrap();
    assert_eq!(result.contents.len(), 1);
}
