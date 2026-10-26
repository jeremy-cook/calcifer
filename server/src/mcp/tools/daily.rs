use tonic::Request;

use calcifer::v1::entity_service_server::EntityService as EntityServiceTrait;
use calcifer::v1::{resolve_entity_request, ResolveEntityRequest};

use super::doc::doc_ref;
use super::{Knowledge, ToolError, ToolResult};

impl Knowledge {
    /// Get-or-create the DailyNote for `date` (ISO "YYYY-MM-DD"). The server keeps
    /// one per day and names it for the day.
    pub(super) async fn resolve_daily_note(
        &self,
        date: &str,
    ) -> Result<(String, String, bool), ToolError> {
        let req = ResolveEntityRequest {
            structure_type: String::new(),
            key: Some(resolve_entity_request::Key::Date(date.to_string())),
            create_if_missing: true,
        };
        let res = self
            .entity
            .resolve_entity(Request::new(req))
            .await?
            .into_inner();
        let entity = res.entity.unwrap_or_default();
        Ok((entity.id, entity.name, res.created))
    }

    pub async fn create_daily_note(&self, date: &str) -> ToolResult {
        let (id, name, created) = self.resolve_daily_note(date).await?;
        let outcome = if created {
            "created"
        } else {
            "already existed"
        };
        Ok(format!(
            "Daily note \"{name}\" ({id}) for {date} {outcome}."
        ))
    }

    pub async fn append_to_daily_note(&self, date: &str, markdown: &str) -> ToolResult {
        // Links first, so a bad one doesn't leave a new, empty day behind.
        let addition = self.parse_markdown(markdown).await?;
        let (id, name, _) = self.resolve_daily_note(date).await?;
        self.append_doc(
            &doc_ref("DailyNote", &id)?,
            &format!("Daily note \"{name}\""),
            &addition,
        )
        .await?;
        let ent = self.entity(&id).await?;
        Ok(format!(
            "Appended to daily note \"{name}\" ({id}); now {} wikilink(s).",
            ent.links.len()
        ))
    }
}
