use serde::{Deserialize, Serialize};
use tonic::Status;

use crate::structures::{self, PropertyKind};

/// What a list is ordered by: a request's `order_by` (AIP-132) parsed. Ties
/// always go to the lower id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Order {
    pub key: OrderKey,
    pub descending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum OrderKey {
    UpdatedAt,
    CreatedAt,
    /// The folded name, `entities.name_key`.
    Name,
    /// A scalar property's `value_key`. Entities without it come last.
    Property(String),
}

/// Where the previous page ended: its last entity's order key and id. A key is
/// a timestamp (millis), a folded name or a value key, or null for an entity
/// without the ordering property.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum Cursor {
    Millis(i64),
    Text(String),
    Missing,
}

/// A page token's contents: the canonical `order_by` it was made for, and the
/// cursor. Hex-encoded JSON, so opaque to clients without a dependency.
#[derive(Serialize, Deserialize)]
struct Token {
    order: String,
    key: Cursor,
    id: String,
}

impl Order {
    /// Parse `order_by`: one key and an optional ` asc` or ` desc`. Empty is
    /// `updated_at desc`. A property key must name a scalar property some
    /// structure or facet declares.
    pub fn parse(order_by: &str) -> Result<Self, Status> {
        let invalid = |why: &str| Status::invalid_argument(format!("order_by {order_by:?}: {why}"));
        let words: Vec<&str> = order_by.split_whitespace().collect();
        let (field, descending) = match words.as_slice() {
            [] => {
                return Ok(Self {
                    key: OrderKey::UpdatedAt,
                    descending: true,
                })
            }
            [field] | [field, "asc"] => (*field, false),
            [field, "desc"] => (*field, true),
            _ => return Err(invalid("expected one field and an optional asc or desc")),
        };
        let key = match field {
            "updated_at" => OrderKey::UpdatedAt,
            "created_at" => OrderKey::CreatedAt,
            "name" => OrderKey::Name,
            other => match other.strip_prefix("properties.") {
                Some(id) => {
                    scalar_property(id).map_err(|why| invalid(&why))?;
                    OrderKey::Property(id.to_string())
                }
                None => return Err(invalid("unknown field")),
            },
        };
        Ok(Self { key, descending })
    }

    /// The order as one canonical string, which a page token carries.
    fn canonical(&self) -> String {
        let field = match &self.key {
            OrderKey::UpdatedAt => "updated_at".to_string(),
            OrderKey::CreatedAt => "created_at".to_string(),
            OrderKey::Name => "name".to_string(),
            OrderKey::Property(id) => format!("properties.{id}"),
        };
        let direction = if self.descending { "desc" } else { "asc" };
        format!("{field} {direction}")
    }

    /// A page token for the page after the entity with `key` and `id`.
    pub fn token(&self, key: Cursor, id: String) -> String {
        let token = Token {
            order: self.canonical(),
            key,
            id,
        };
        let json = serde_json::to_vec(&token).expect("a token serializes");
        json.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The cursor in `page_token`, which must have been made for this order.
    /// Empty is the first page.
    pub fn cursor(&self, page_token: &str) -> Result<Option<(Cursor, String)>, Status> {
        if page_token.is_empty() {
            return Ok(None);
        }
        let invalid = || Status::invalid_argument("invalid page_token");
        if !page_token.len().is_multiple_of(2) {
            return Err(invalid());
        }
        let bytes = (0..page_token.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(page_token.get(i..i + 2)?, 16).ok())
            .collect::<Option<Vec<u8>>>()
            .ok_or_else(invalid)?;
        let token: Token = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if token.order != self.canonical() {
            return Err(Status::invalid_argument(format!(
                "page_token is for order_by {:?}, not {:?}",
                token.order,
                self.canonical()
            )));
        }
        Ok(Some((token.key, token.id)))
    }
}

/// Whether `property_id` is one a list can filter or order by: declared by a
/// structure or a facet with a scalar kind. Returns its declared kinds; the
/// error says why not.
pub(super) fn scalar_property(property_id: &str) -> Result<Vec<PropertyKind>, String> {
    let kinds: Vec<PropertyKind> = structures::declarations(property_id)
        .map(|p| p.kind)
        .collect();
    if kinds.is_empty() {
        return Err(format!(
            "no structure or facet declares property {property_id:?}"
        ));
    }
    let scalar = |k: &PropertyKind| {
        matches!(
            k,
            PropertyKind::Date | PropertyKind::Select | PropertyKind::Text | PropertyKind::Number
        )
    };
    if !kinds.iter().any(scalar) {
        return Err(format!(
            "{property_id} isn't a date, select, text or number property"
        ));
    }
    Ok(kinds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_by_parses_a_field_and_a_direction() {
        let parse = |s: &str| Order::parse(s).expect(s);
        assert_eq!(
            parse(""),
            Order {
                key: OrderKey::UpdatedAt,
                descending: true
            }
        );
        assert_eq!(
            parse("name"),
            Order {
                key: OrderKey::Name,
                descending: false
            }
        );
        assert_eq!(
            parse("  created_at   desc "),
            Order {
                key: OrderKey::CreatedAt,
                descending: true
            }
        );
        assert_eq!(
            parse("properties.todo.due asc"),
            Order {
                key: OrderKey::Property("todo.due".into()),
                descending: false
            }
        );
    }

    #[test]
    fn bad_orders_are_refused() {
        for bad in [
            "nope",
            "name sideways",
            "name, updated_at",
            "properties.content",
            "properties.tags",
            "properties.nope",
        ] {
            let err = Order::parse(bad).expect_err(bad);
            assert_eq!(err.code(), tonic::Code::InvalidArgument, "{bad}");
        }
    }

    #[test]
    fn a_token_round_trips_for_its_order_only() {
        let by_name = Order::parse("name").expect("name");
        let token = by_name.token(Cursor::Text("apollo".into()), "id-1".into());
        assert_eq!(
            by_name.cursor(&token).expect("cursor"),
            Some((Cursor::Text("apollo".into()), "id-1".to_string()))
        );
        let by_due = Order::parse("properties.todo.due").expect("due");
        let token = by_due.token(Cursor::Missing, "id-2".into());
        assert_eq!(
            by_due.cursor(&token).expect("cursor"),
            Some((Cursor::Missing, "id-2".to_string()))
        );
        let err = by_name.cursor(&token).expect_err("another order's token");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        for bad in ["zz", "abc", "7b7d"] {
            assert!(by_name.cursor(bad).is_err(), "{bad}");
        }
        assert_eq!(by_name.cursor("").expect("first page"), None);
    }
}
