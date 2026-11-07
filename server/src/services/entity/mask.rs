use prost_types::FieldMask;
use tonic::Status;

use calcifer::v1::Entity;

/// What a read returns of each entity: a `read_mask` (AIP-157) parsed. `id` and
/// `structure_type` always come back. A field left out is empty, and the
/// queries for it don't run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadMask {
    pub name: bool,
    pub properties: PropertyMask,
    pub links: bool,
    pub referenced_dates: bool,
    pub facets: bool,
    pub created_at: bool,
    pub updated_at: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PropertyMask {
    All,
    /// These property ids; none when empty.
    Only(Vec<String>),
}

impl ReadMask {
    /// Every field: what an unset mask means.
    pub fn all() -> Self {
        Self {
            name: true,
            properties: PropertyMask::All,
            links: true,
            referenced_dates: true,
            facets: true,
            created_at: true,
            updated_at: true,
        }
    }

    fn none() -> Self {
        Self {
            name: false,
            properties: PropertyMask::Only(vec![]),
            links: false,
            referenced_dates: false,
            facets: false,
            created_at: false,
            updated_at: false,
        }
    }

    /// Parse a request's `read_mask`. Unset, or no paths, or `*`, is every
    /// field. A path is an Entity field, or `properties.<id>` for one property
    /// (`properties.todo.due`: the id is everything after the first dot). An
    /// unknown path is `InvalidArgument`.
    pub fn parse(mask: Option<&FieldMask>) -> Result<Self, Status> {
        let Some(mask) = mask.filter(|m| !m.paths.is_empty()) else {
            return Ok(Self::all());
        };
        let mut read = Self::none();
        for path in &mask.paths {
            match path.as_str() {
                "*" => return Ok(Self::all()),
                // Always returned.
                "id" | "structure_type" => {}
                "name" => read.name = true,
                "properties" => read.properties = PropertyMask::All,
                "links" => read.links = true,
                "referenced_dates" => read.referenced_dates = true,
                "facets" => read.facets = true,
                "created_at" => read.created_at = true,
                "updated_at" => read.updated_at = true,
                other => match other.strip_prefix("properties.") {
                    Some(id) if !id.is_empty() => {
                        if let PropertyMask::Only(ids) = &mut read.properties {
                            ids.push(id.to_string());
                        }
                    }
                    _ => {
                        return Err(Status::invalid_argument(format!(
                            "read_mask: unknown path {other:?}"
                        )))
                    }
                },
            }
        }
        Ok(read)
    }

    /// The property ids to load: None for all of them.
    pub fn property_ids(&self) -> Option<&[String]> {
        match &self.properties {
            PropertyMask::All => None,
            PropertyMask::Only(ids) => Some(ids),
        }
    }

    /// Whether no property is wanted, so their query can be skipped.
    pub fn no_properties(&self) -> bool {
        matches!(&self.properties, PropertyMask::Only(ids) if ids.is_empty())
    }

    /// Clear the plain fields the mask leaves out. The child rows (properties,
    /// links, dates, facets) are left out by not loading them.
    pub fn apply(&self, entity: &mut Entity) {
        if !self.name {
            entity.name.clear();
        }
        if !self.created_at {
            entity.created_at = None;
        }
        if !self.updated_at {
            entity.updated_at = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(
        clippy::unnecessary_wraps,
        reason = "fills a request's Option<FieldMask>"
    )]
    fn mask(paths: &[&str]) -> Option<FieldMask> {
        Some(FieldMask {
            paths: paths.iter().map(ToString::to_string).collect(),
        })
    }

    #[test]
    fn no_mask_is_every_field() {
        assert_eq!(ReadMask::parse(None).expect("parse"), ReadMask::all());
        assert_eq!(
            ReadMask::parse(mask(&[]).as_ref()).expect("parse"),
            ReadMask::all()
        );
        assert_eq!(
            ReadMask::parse(mask(&["name", "*"]).as_ref()).expect("parse"),
            ReadMask::all()
        );
    }

    #[test]
    fn paths_pick_fields() {
        let read = ReadMask::parse(mask(&["name", "id", "facets"]).as_ref()).expect("parse");
        assert!(read.name && read.facets);
        assert!(!read.links && !read.referenced_dates && !read.created_at && !read.updated_at);
        assert!(read.no_properties());
    }

    #[test]
    fn a_property_path_names_one_property() {
        let read = ReadMask::parse(mask(&["properties.todo.due", "properties.tags"]).as_ref())
            .expect("parse");
        assert_eq!(
            read.property_ids(),
            Some(&["todo.due".to_string(), "tags".to_string()][..])
        );
        let read =
            ReadMask::parse(mask(&["properties.todo.due", "properties"]).as_ref()).expect("parse");
        assert_eq!(read.property_ids(), None);
    }

    #[test]
    fn unknown_paths_are_refused() {
        for path in ["nope", "properties.", "links.id"] {
            let err = ReadMask::parse(mask(&[path]).as_ref()).expect_err(path);
            assert_eq!(err.code(), tonic::Code::InvalidArgument, "{path}");
        }
    }
}
