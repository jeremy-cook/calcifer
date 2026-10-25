//! The structure registry: the one place structure data is authored (ADR 7).
//! `StructureService.ListStructures` serves it to the browser, the MCP tools read
//! it directly, and the helpers below derive what the write paths need from it.
//! A structure's natural keys (`keys`) are enforced by the `entity_keys` table
//! (ADR 10): the registry declares them, the write paths fill the table.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    Richtext,
    Text,
    Number,
    Date,
    Select,
    Relation,
    Relations,
}

#[derive(Debug)]
pub struct SelectOption {
    pub key: &'static str,
    pub label: &'static str,
}

#[derive(Debug)]
pub struct PropertyDef {
    pub id: &'static str,
    pub kind: PropertyKind,
    /// Empty = none; clients fall back to the id.
    pub label: &'static str,
    /// Select only, in display (and priority-rank) order.
    pub options: &'static [SelectOption],
    /// Select only.
    pub default_option: Option<&'static str>,
    /// Relation/relations only; `None` = any structure.
    pub target_structure: Option<&'static str>,
}

/// Where a natural key's value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// The entity's name, trimmed and lowercased (see `fold_name`).
    Name,
    /// A property's value, already canonical (a date is an ISO day).
    Property(&'static str),
}

/// A natural key: a value that identifies an entity of the structure besides its
/// id. No two entities of a structure share a key's value.
#[derive(Debug)]
pub struct KeyDef {
    pub key: &'static str,
    pub source: KeySource,
}

#[derive(Debug)]
pub struct StructureDef {
    pub type_: &'static str,
    pub name: &'static str,
    pub plural: &'static str,
    /// One line, for the agent (`list_structures`).
    pub description: &'static str,
    pub properties: &'static [PropertyDef],
    pub creatable: bool,
    pub mentionable: bool,
    pub name_editable: bool,
    pub keys: &'static [KeyDef],
    /// Facet ids this structure accepts.
    pub facets: &'static [&'static str],
    /// Structure types its entities may hold direct relationships to (ADR 12);
    /// empty = none. It limits what can be attached with `AddRelationship`,
    /// never what a document may mention.
    pub relates_to: &'static [&'static str],
}

/// A facet: a fixed group of properties an entity can have attached or not,
/// if its structure accepts it. Properties only: keys, naming rules and rich
/// text belong to the structure. Property ids are namespaced as `<id>.<name>`.
#[derive(Debug)]
pub struct FacetDef {
    pub id: &'static str,
    pub name: &'static str,
    pub plural: &'static str,
    /// One line, for the agent (`list_structures`).
    pub description: &'static str,
    pub properties: &'static [PropertyDef],
}

impl StructureDef {
    /// Whether the (folded) name is the identity: the structure declares a key
    /// sourced from its name.
    pub fn unique_names(&self) -> bool {
        self.name_key().is_some()
    }

    /// The key sourced from the name, if the structure declares one.
    pub fn name_key(&self) -> Option<&'static KeyDef> {
        self.keys.iter().find(|k| k.source == KeySource::Name)
    }
}

/// The canonical form of a name used as a key: trimmed and lowercased, with
/// Unicode case folding (unlike SQLite's `NOCASE`, which only folds ASCII).
pub fn fold_name(name: &str) -> String {
    name.trim().to_lowercase()
}

const fn prop(id: &'static str, kind: PropertyKind, label: &'static str) -> PropertyDef {
    PropertyDef {
        id,
        kind,
        label,
        options: &[],
        default_option: None,
        target_structure: None,
    }
}

const fn select(
    id: &'static str,
    label: &'static str,
    options: &'static [SelectOption],
    default_option: &'static str,
) -> PropertyDef {
    PropertyDef {
        options,
        default_option: Some(default_option),
        ..prop(id, PropertyKind::Select, label)
    }
}

const fn relations(id: &'static str, label: &'static str, target: &'static str) -> PropertyDef {
    PropertyDef {
        target_structure: Some(target),
        ..prop(id, PropertyKind::Relations, label)
    }
}

const fn option(key: &'static str, label: &'static str) -> SelectOption {
    SelectOption { key, label }
}

/// Every structure, in the order clients list them (sidebar, backlink groups).
pub const STRUCTURES: &[StructureDef] = &[
    StructureDef {
        type_: "Note",
        name: "Note",
        plural: "Notes",
        description: "a concept/topic note (the main building block; reference with [[Name]])",
        properties: &[
            relations("tags", "Tags", "Tag"),
            prop("content", PropertyKind::Richtext, ""),
        ],
        creatable: true,
        mentionable: true,
        name_editable: true,
        keys: &[],
        facets: &["todo"],
        relates_to: &["Note"],
    },
    StructureDef {
        type_: "Tag",
        name: "Tag",
        plural: "Tags",
        description: "a label, written #tag in note content",
        properties: &[],
        creatable: true,
        // Reached via `#`, not bare `@`.
        mentionable: false,
        name_editable: true,
        keys: &[KeyDef {
            key: "name",
            source: KeySource::Name,
        }],
        facets: &[],
        relates_to: &[],
    },
    StructureDef {
        type_: "DailyNote",
        name: "Daily Note",
        plural: "Daily Notes",
        description: "a journal entry for a calendar day",
        properties: &[
            prop("date", PropertyKind::Date, ""),
            prop("content", PropertyKind::Richtext, ""),
        ],
        // Created per day by Resolve's date key, never from "+ New"; the server
        // derives the name from the date.
        creatable: false,
        mentionable: false,
        name_editable: false,
        keys: &[KeyDef {
            key: "date",
            source: KeySource::Property("date"),
        }],
        facets: &[],
        relates_to: &[],
    },
];

/// Every facet, in the order clients list them.
pub const FACETS: &[FacetDef] = &[FacetDef {
    id: "todo",
    name: "To-do",
    plural: "To-dos",
    description: "makes an entity actionable: status, priority and a due date",
    properties: &[
        select(
            "todo.status",
            "Status",
            &[option("open", "Open"), option("done", "Done")],
            "open",
        ),
        select(
            "todo.priority",
            "Priority",
            &[
                option("none", "None"),
                option("low", "Low"),
                option("medium", "Medium"),
                option("high", "High"),
            ],
            "none",
        ),
        prop("todo.due", PropertyKind::Date, "Due"),
    ],
}];

pub fn structure(structure_type: &str) -> Option<&'static StructureDef> {
    STRUCTURES.iter().find(|s| s.type_ == structure_type)
}

/// A structure's declared property by id. Write validation of select values and
/// relation targets (I-2, I-9) looks definitions up here.
pub fn property(structure_type: &str, id: &str) -> Option<&'static PropertyDef> {
    structure(structure_type)?
        .properties
        .iter()
        .find(|p| p.id == id)
}

pub fn facet(id: &str) -> Option<&'static FacetDef> {
    FACETS.iter().find(|f| f.id == id)
}

/// The facet whose namespace (`<facet>.`) a property id is in, if any. Structure
/// property ids never contain a `.`, so they belong to none.
pub fn facet_of_property(property_id: &str) -> Option<&'static FacetDef> {
    let (namespace, _) = property_id.split_once('.')?;
    facet(namespace)
}

/// A property's definition on an entity of `structure_type` with `facets`
/// attached: the structure's own properties first, then the attached facets'.
/// A facet property is found only when its facet is in `facets`.
pub fn property_for(
    structure_type: &str,
    facets: &[&str],
    property_id: &str,
) -> Option<&'static PropertyDef> {
    property(structure_type, property_id).or_else(|| {
        let f = facet_of_property(property_id)?;
        if !facets.contains(&f.id) {
            return None;
        }
        f.properties.iter().find(|p| p.id == property_id)
    })
}

/// Select property ids (and their default option keys) written when the facet
/// is attached.
pub fn facet_select_defaults(facet: &FacetDef) -> Vec<(&'static str, &'static str)> {
    facet
        .properties
        .iter()
        .filter(|p| p.kind == PropertyKind::Select)
        .filter_map(|p| p.default_option.map(|d| (p.id, d)))
        .collect()
}

/// `relation`/`relations` property ids declared on the facet (full ids).
pub fn facet_relation_properties(facet: &FacetDef) -> Vec<&'static str> {
    facet
        .properties
        .iter()
        .filter(|p| matches!(p.kind, PropertyKind::Relation | PropertyKind::Relations))
        .map(|p| p.id)
        .collect()
}

fn properties_of(structure_type: &str) -> impl Iterator<Item = &'static PropertyDef> {
    structure(structure_type)
        .map_or(&[][..], |s| s.properties)
        .iter()
}

/// A structure's declared rich-text property ids: the property half of every
/// `RichTextRef` it can have (I-22). Unknown types get none.
pub fn richtext_properties(structure_type: &str) -> Vec<&'static str> {
    properties_of(structure_type)
        .filter(|p| p.kind == PropertyKind::Richtext)
        .map(|p| p.id)
        .collect()
}

/// Select property ids (and their default option keys) a freshly-created entity
/// of this structure should carry. Unknown types and structures with no
/// defaulted select properties get none.
pub fn select_defaults(structure_type: &str) -> Vec<(&'static str, &'static str)> {
    properties_of(structure_type)
        .filter(|p| p.kind == PropertyKind::Select)
        .filter_map(|p| p.default_option.map(|d| (p.id, d)))
        .collect()
}

/// `relation`/`relations` property ids declared on this structure. Link sync
/// clears these even when absent from an update, so removing the last ref drops
/// its link rows. Unknown types get none.
pub fn relation_properties(structure_type: &str) -> Vec<&'static str> {
    properties_of(structure_type)
        .filter(|p| matches!(p.kind, PropertyKind::Relation | PropertyKind::Relations))
        .map(|p| p.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The derived helpers must keep returning what the hand-written per-structure
    // lists did before the registry replaced them.
    #[test]
    fn derived_helpers_match_previous_lists() {
        assert_eq!(richtext_properties("Note"), ["content"]);
        assert!(richtext_properties("Tag").is_empty());
        assert_eq!(richtext_properties("DailyNote"), ["content"]);
        assert!(richtext_properties("Nope").is_empty());

        assert!(select_defaults("Note").is_empty());
        assert!(select_defaults("Tag").is_empty());
        assert!(select_defaults("DailyNote").is_empty());
        assert!(select_defaults("Nope").is_empty());

        assert_eq!(relation_properties("Note"), ["tags"]);
        assert!(relation_properties("Tag").is_empty());
        assert!(relation_properties("DailyNote").is_empty());
        assert!(relation_properties("Nope").is_empty());
    }

    #[test]
    fn property_def_looks_up_by_structure_and_id() {
        let date = property("DailyNote", "date").expect("DailyNote.date");
        assert_eq!(date.kind, PropertyKind::Date);
        let tags = property("Note", "tags").expect("Note.tags");
        assert_eq!(tags.target_structure, Some("Tag"));
        assert!(property("Note", "nope").is_none());
        // Facet properties aren't the structure's own.
        assert!(property("Note", "todo.priority").is_none());
        assert!(property("Nope", "content").is_none());
    }

    #[test]
    fn property_keys_name_declared_properties() {
        for s in STRUCTURES {
            for k in s.keys {
                if let KeySource::Property(id) = k.source {
                    assert!(
                        property(s.type_, id).is_some(),
                        "{} key {:?} names undeclared property {id:?}",
                        s.type_,
                        k.key
                    );
                }
            }
        }
    }

    #[test]
    fn unique_names_follows_a_name_key() {
        let unique: Vec<&str> = STRUCTURES
            .iter()
            .filter(|s| s.unique_names())
            .map(|s| s.type_)
            .collect();
        assert_eq!(unique, ["Tag"]);
    }

    #[test]
    fn fold_name_trims_and_folds_unicode_case() {
        assert_eq!(fold_name("  Reading "), "reading");
        assert_eq!(fold_name("CAFÉ"), fold_name("Café"));
    }

    #[test]
    fn select_defaults_are_declared_options() {
        for s in STRUCTURES {
            for p in s.properties {
                if let Some(default) = p.default_option {
                    assert!(
                        p.options.iter().any(|o| o.key == default),
                        "{}.{} default {default:?} is not one of its options",
                        s.type_,
                        p.id
                    );
                }
            }
        }
    }

    #[test]
    fn facet_property_ids_are_namespaced() {
        for f in FACETS {
            let prefix = format!("{}.", f.id);
            for p in f.properties {
                assert!(
                    p.id.starts_with(&prefix) && p.id.len() > prefix.len(),
                    "facet {} property {:?} is not namespaced {prefix:?}",
                    f.id,
                    p.id
                );
                assert_eq!(facet_of_property(p.id).map(|f| f.id), Some(f.id));
            }
        }
    }

    #[test]
    fn facets_declare_no_rich_text() {
        for f in FACETS {
            for p in f.properties {
                assert_ne!(
                    p.kind,
                    PropertyKind::Richtext,
                    "facet {} declares rich-text property {:?}",
                    f.id,
                    p.id
                );
            }
        }
    }

    #[test]
    fn structure_property_ids_have_no_dot() {
        for s in STRUCTURES {
            for p in s.properties {
                assert!(!p.id.contains('.'), "{}.{} contains a dot", s.type_, p.id);
            }
        }
    }

    #[test]
    fn accepted_facets_are_declared() {
        for s in STRUCTURES {
            for id in s.facets {
                assert!(
                    facet(id).is_some(),
                    "{} accepts undeclared facet {id:?}",
                    s.type_
                );
            }
        }
    }

    #[test]
    fn facet_select_defaults_are_declared_options() {
        for f in FACETS {
            for (id, default) in facet_select_defaults(f) {
                let p = f.properties.iter().find(|p| p.id == id).expect("property");
                assert!(
                    p.options.iter().any(|o| o.key == default),
                    "{id} default {default:?} is not one of its options"
                );
            }
        }
        let todo = facet("todo").expect("todo facet");
        assert_eq!(
            facet_select_defaults(todo),
            [("todo.status", "open"), ("todo.priority", "none")]
        );
        assert!(facet_relation_properties(todo).is_empty());
    }

    #[test]
    fn property_for_checks_structure_then_attached_facets() {
        let tags = property_for("Note", &[], "tags").expect("Note.tags");
        assert_eq!(tags.target_structure, Some("Tag"));
        assert!(property_for("Note", &["todo"], "content").is_some());

        assert!(property_for("Note", &[], "todo.status").is_none());
        let status = property_for("Note", &["todo"], "todo.status").expect("todo.status");
        assert_eq!(status.default_option, Some("open"));

        assert!(property_for("Note", &["todo"], "nope").is_none());
        assert!(property_for("Note", &["todo"], "todo.nope").is_none());
        assert!(property_for("Note", &["todo"], "nope.status").is_none());
        assert!(property_for("Nope", &[], "content").is_none());
    }

    #[test]
    fn relates_to_names_known_structures() {
        for s in STRUCTURES {
            for target in s.relates_to {
                assert!(
                    structure(target).is_some(),
                    "{} relates to undeclared structure {target:?}",
                    s.type_
                );
            }
        }
        assert_eq!(structure("Note").expect("Note").relates_to, ["Note"]);
        assert!(structure("Tag").expect("Tag").relates_to.is_empty());
        assert!(structure("DailyNote")
            .expect("DailyNote")
            .relates_to
            .is_empty());
    }

    // `@` ids are reserved for link scopes the server derives (ADR 12, rule 9).
    #[test]
    fn no_property_id_starts_with_at() {
        let structure_props = STRUCTURES.iter().flat_map(|s| s.properties);
        let facet_props = FACETS.iter().flat_map(|f| f.properties);
        for p in structure_props.chain(facet_props) {
            assert!(!p.id.starts_with('@'), "property {:?} starts with @", p.id);
        }
    }
}
