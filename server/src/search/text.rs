//! What search reads an entity as: the text its FTS row and its embedding
//! chunks are built from. The embed worker chunks it with the name and Tags as
//! the first chunk.

use std::collections::HashMap;

use sqlx::SqliteConnection;

use crate::embed::chunk::chunk_docs;
use crate::tiptap::text::extract_plain_text;

/// An entity as search reads it: what the FTS row and the embedding chunks are
/// built from.
pub(crate) struct SearchText {
    pub(super) name: String,
    /// The Tags its properties hold, in the order they were added.
    pub(super) tags: Vec<String>,
    docs: Vec<String>,
    /// The current name of each entity it links to, by id: what a chip reads as.
    names: HashMap<String, String>,
}

impl SearchText {
    /// The entity's search text as it is now, or None when there's no entity.
    pub(crate) async fn load(
        conn: &mut SqliteConnection,
        entity_id: &str,
    ) -> sqlx::Result<Option<Self>> {
        let Some(name) = sqlx::query_scalar!("SELECT name FROM entities WHERE id = ?", entity_id)
            .fetch_optional(&mut *conn)
            .await?
        else {
            return Ok(None);
        };
        // A Tag in a property, not a `#tag` chip: a chip's link is scoped to its
        // doc's property, which has no `properties` row.
        let mut tags = sqlx::query_scalar!(
            r#"SELECT t.name AS "name!" FROM links l JOIN entities t ON t.id = l.target_id
               WHERE l.entity_id = ? AND t.structure_type = 'Tag'
                 AND l.source_property_id IN
                     (SELECT property_id FROM properties WHERE entity_id = l.entity_id)
               ORDER BY l.created_at, l.link_id"#,
            entity_id
        )
        .fetch_all(&mut *conn)
        .await?;
        let mut seen = std::collections::HashSet::new();
        tags.retain(|tag| seen.insert(tag.clone()));
        let docs = sqlx::query_scalar!(
            r#"SELECT doc AS "doc!" FROM richtext WHERE entity_id = ? ORDER BY property_id"#,
            entity_id
        )
        .fetch_all(&mut *conn)
        .await?;
        let names = sqlx::query!(
            r#"SELECT DISTINCT l.target_id AS "id!", t.name AS "name!"
               FROM links l JOIN entities t ON t.id = l.target_id
               WHERE l.entity_id = ?"#,
            entity_id
        )
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|r| (r.id, r.name))
        .collect();
        Ok(Some(Self {
            name,
            tags,
            docs,
            names,
        }))
    }

    /// The FTS `body`: each doc's plain text, joined by a space.
    pub(super) fn body(&self) -> String {
        self.docs
            .iter()
            .filter_map(|doc| extract_plain_text(doc, &self.names).ok())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The embedding chunks: the name and the Tags, then the docs' blocks,
    /// packed. Never empty, since a name never is.
    pub(crate) fn chunks(&self) -> Vec<String> {
        let mut header = self.name.clone();
        if !self.tags.is_empty() {
            header.push_str("\nTags: ");
            header.push_str(&self.tags.join(", "));
        }
        chunk_docs(&header, &self.docs, &self.names)
    }
}
