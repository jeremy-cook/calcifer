use super::*;

#[test]
fn tools_that_take_a_note_or_a_day_declare_date_and_require_no_name() {
    let defs = tool_definitions();
    for tool in ["get_note", "query_text_location", "edit_note"] {
        let def = defs.iter().find(|t| t["name"] == tool).unwrap();
        let schema = &def["inputSchema"];
        assert_eq!(
            schema["properties"]["date"],
            json!({ "type": "string" }),
            "{tool}"
        );
        assert_eq!(
            schema["properties"]["name"],
            json!({ "type": "string" }),
            "{tool}"
        );
        let required = schema["required"].as_array().unwrap();
        assert!(!required.contains(&json!("name")), "{tool}: {required:?}");
        assert!(!required.contains(&json!("date")), "{tool}: {required:?}");
    }
}

#[tokio::test]
async fn list_structures_describes_the_registry() {
    let f = fixture().await;
    let listed = f.k.list_structures();
    assert!(
        listed.contains("- Note: ") && listed.contains("- DailyNote: "),
        "{listed}"
    );
    assert!(listed.contains("(relations -> Tag)"), "{listed}");
}

#[tokio::test]
async fn list_structures_shows_relates_to() {
    let f = fixture().await;
    let listed = f.k.list_structures();
    let entries: Vec<&str> = listed.split("\n- ").collect();
    let entry = |prefix: &str| {
        entries
            .iter()
            .find(|e| e.trim_start_matches("- ").starts_with(prefix))
            .unwrap_or_else(|| panic!("no {prefix} in {listed}"))
    };
    let note = entry("Note: ");
    assert!(note.lines().any(|l| l == "  relates_to: Note"), "{listed}");
    assert!(
        note.find("  facets: ") < note.find("  relates_to: "),
        "relates_to comes after facets: {listed}"
    );
    for other in ["Tag: ", "DailyNote: ", "facet todo "] {
        assert!(!entry(other).contains("relates_to"), "{listed}");
    }
}

#[tokio::test]
async fn list_structures_shows_facets() {
    let f = fixture().await;
    let listed = f.k.list_structures();
    let note = &listed[listed.find("- Note: ").unwrap()..];
    assert!(
        note.lines().take(3).any(|l| l == "  facets: todo"),
        "{listed}"
    );
    let facet = &listed[listed.find("- facet todo (ToDo): ").unwrap()..];
    assert!(
        facet.contains("todo.status (select: open|done, default open)"),
        "{listed}"
    );
    assert!(facet.contains("todo.due (date)"), "{listed}");
    assert!(facet.contains("\n  accepted by: Note"), "{listed}");
}
