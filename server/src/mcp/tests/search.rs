use super::*;

#[tokio::test]
async fn search_finds_notes_by_prefix() {
    let f = fixture().await;
    f.k.create_note("Transformers", "An architecture.", None)
        .await
        .unwrap();
    let found =
        f.k.search_notes("trans", 10, SearchMode::Hybrid)
            .await
            .unwrap();
    assert!(found.contains("[Note] Transformers"), "{found}");
    assert_eq!(
        f.k.search_notes("zzzz", 10, SearchMode::Lexical)
            .await
            .unwrap(),
        "No matches for \"zzzz\"."
    );
}
