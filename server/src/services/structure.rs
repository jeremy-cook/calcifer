use tonic::{Request, Response, Status};

use crate::proto::{
    structure_service_server::StructureService as StructureServiceTrait, ListStructuresRequest,
    ListStructuresResponse,
};
use crate::structures::STRUCTURES;

/// Serves the static registry in `structures.rs`. No state: the registry never
/// changes while the server runs.
pub struct StructureService;

#[tonic::async_trait]
impl StructureServiceTrait for StructureService {
    /// Every structure, in table order.
    async fn list(
        &self,
        _req: Request<ListStructuresRequest>,
    ) -> Result<Response<ListStructuresResponse>, Status> {
        Ok(Response::new(ListStructuresResponse {
            structures: STRUCTURES.iter().map(Into::into).collect(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::entity_service_server::EntityService as _;
    use crate::proto::{
        property_value, resolve_entity_request, PropertyKind, ResolveEntityRequest, StructureDef,
    };
    use crate::test_support::{entity_service, memory_pool};

    async fn list() -> Vec<StructureDef> {
        StructureService
            .list(Request::new(ListStructuresRequest {}))
            .await
            .expect("list")
            .into_inner()
            .structures
    }

    fn options(s: &StructureDef, property_id: &str) -> Vec<(String, String)> {
        let def = s
            .properties
            .iter()
            .find(|p| p.id == property_id)
            .expect("property");
        def.options
            .iter()
            .map(|o| (o.key.clone(), o.label.clone()))
            .collect()
    }

    /// `expected` appear in `actual`, in this relative order. Pins the options
    /// the clients rely on without pinning the full list, so adding an option
    /// stays a one-file change to `structures.rs`.
    fn assert_contains_in_order(actual: &[(String, String)], expected: &[(&str, &str)]) {
        let positions: Vec<usize> = expected
            .iter()
            .map(|(k, l)| {
                actual
                    .iter()
                    .position(|(ak, al)| ak == k && al == l)
                    .unwrap_or_else(|| panic!("missing option {k:?} / {l:?} in {actual:?}"))
            })
            .collect();
        assert!(
            positions.windows(2).all(|w| w[0] < w[1]),
            "out of order: {actual:?}"
        );
    }

    #[tokio::test]
    async fn list_returns_every_structure_in_table_order() {
        let structures = list().await;
        let types: Vec<&str> = structures.iter().map(|s| s.r#type.as_str()).collect();
        assert_eq!(types, ["Note", "Tag", "DailyNote", "Todo"]);

        let tag = &structures[1];
        assert!(!tag.mentionable && tag.unique_names && tag.creatable);
        let daily = &structures[2];
        assert_eq!(daily.name, "Daily Note");
        assert!(!daily.creatable && !daily.mentionable && !daily.name_editable);
    }

    #[tokio::test]
    async fn list_carries_todo_selects_and_relation_target() {
        let structures = list().await;
        let todo = structures
            .iter()
            .find(|s| s.r#type == "Todo")
            .expect("Todo");
        assert_eq!(
            (todo.name.as_str(), todo.plural.as_str()),
            ("To-do", "To-dos")
        );

        let ids: Vec<&str> = todo.properties.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["status", "priority", "due", "tags", "content"]);

        let status = &todo.properties[0];
        assert_eq!(status.kind(), PropertyKind::Select);
        assert_eq!(status.label, "Status");
        assert_contains_in_order(
            &options(todo, "status"),
            &[("open", "Open"), ("done", "Done")],
        );
        assert_eq!(status.default_option, "open");

        let priority = &todo.properties[1];
        assert_eq!(priority.kind(), PropertyKind::Select);
        // Option order is rank order for the to-do priority sort.
        assert_contains_in_order(
            &options(todo, "priority"),
            &[
                ("none", "None"),
                ("low", "Low"),
                ("medium", "Medium"),
                ("high", "High"),
            ],
        );
        assert_eq!(priority.default_option, "none");

        let tags = &todo.properties[3];
        assert_eq!(tags.kind(), PropertyKind::Relations);
        assert_eq!(tags.target_structure, "Tag");

        let content = &todo.properties[4];
        assert_eq!(content.kind(), PropertyKind::Richtext);
        assert!(content.options.is_empty() && content.default_option.is_empty());
    }

    // Every field of the table reaches the wire unchanged.
    #[tokio::test]
    async fn list_mirrors_the_table() {
        let structures = list().await;
        assert_eq!(structures.len(), STRUCTURES.len());
        for (wire, def) in structures.iter().zip(STRUCTURES) {
            assert_eq!(wire.r#type, def.type_);
            assert_eq!(wire.name, def.name);
            assert_eq!(wire.plural, def.plural);
            assert_eq!(wire.description, def.description);
            assert_eq!(
                (
                    wire.creatable,
                    wire.mentionable,
                    wire.unique_names,
                    wire.name_editable
                ),
                (
                    def.creatable,
                    def.mentionable,
                    def.unique_names,
                    def.name_editable
                )
            );
            assert_eq!(wire.properties.len(), def.properties.len());
            for (wp, dp) in wire.properties.iter().zip(def.properties) {
                assert_eq!(wp.id, dp.id);
                assert_eq!(wp.kind(), PropertyKind::from(dp.kind));
                assert_eq!(wp.label, dp.label);
                assert_eq!(wp.default_option, dp.default_option.unwrap_or_default());
                assert_eq!(wp.target_structure, dp.target_structure.unwrap_or_default());
                let keys: Vec<(&str, &str)> = wp
                    .options
                    .iter()
                    .map(|o| (o.key.as_str(), o.label.as_str()))
                    .collect();
                let table: Vec<(&str, &str)> =
                    dp.options.iter().map(|o| (o.key, o.label)).collect();
                assert_eq!(keys, table);
            }
        }
    }

    // The get-or-create path builds new entities from the same table, so a
    // server-minted Todo carries exactly the defaults List advertises.
    #[tokio::test]
    async fn resolved_todo_carries_listed_select_defaults() {
        let svc = entity_service(memory_pool().await);
        let entity = svc
            .resolve(Request::new(ResolveEntityRequest {
                structure_type: "Todo".to_string(),
                key: Some(resolve_entity_request::Key::Name("Water plants".to_string())),
                create_if_missing: true,
            }))
            .await
            .expect("resolve")
            .into_inner()
            .entity
            .expect("entity");

        let structures = list().await;
        let todo = structures
            .iter()
            .find(|s| s.r#type == "Todo")
            .expect("Todo");
        for def in todo
            .properties
            .iter()
            .filter(|p| !p.default_option.is_empty())
        {
            let value = entity.properties.get(&def.id).and_then(|v| v.value.clone());
            assert_eq!(
                value,
                Some(property_value::Value::Select(def.default_option.clone()))
            );
        }
    }
}
