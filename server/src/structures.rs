//! The structure registry: the one place structure data is authored (ADR 7).
//! `StructureService.ListStructures` serves it to the browser, the MCP tools read
//! it directly, and the helpers below derive what the write paths need from it.
//! A structure's natural keys (`keys`) are enforced by the `entity_keys` table
//! (ADR 10): the registry declares them, the write paths fill the table.

use std::sync::LazyLock;

use regex::Regex;

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
    /// Date only: its values are days on the calendar (`ListCalendar`).
    pub on_calendar: bool,
}

/// Where a natural key's value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// The entity's name, folded by `fold_name`.
    Name,
    /// A property's value, already canonical (a date is an ISO day).
    Property(&'static str),
}

/// What a structure's names may be. Every path that names an entity checks it:
/// CreateEntity, RenameEntity and a creating ResolveEntity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameRule {
    Any,
    /// Letters and digits with single dots between them (`v1.2`,
    /// `program.apollo`), so `#name` reads as one word wherever it's written:
    /// the agent's markdown, the editor's `#` menu and quick-add.
    Dotted,
}

static DOTTED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\p{L}\p{N}]+(?:\.[\p{L}\p{N}]+)*$").unwrap());
static NOT_LETTERS_OR_DIGITS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^\p{L}\p{N}]+").unwrap());

impl NameRule {
    /// Whether `name` (already trimmed) follows the rule.
    pub fn allows(self, name: &str) -> bool {
        match self {
            NameRule::Any => true,
            NameRule::Dotted => DOTTED.is_match(name),
        }
    }

    /// `name` rewritten to follow the rule. Dotted: each run of characters
    /// other than letters and digits becomes one dot, and none is kept at
    /// either end (`apollo-11` -> `apollo.11`). Empty when nothing is left.
    pub fn fit(self, name: &str) -> String {
        match self {
            NameRule::Any => name.trim().to_string(),
            NameRule::Dotted => NOT_LETTERS_OR_DIGITS
                .split(name)
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("."),
        }
    }

    /// The `n`th name from `base`, for picking a free one: `base n`, or
    /// `base.n` under the dotted rule.
    pub fn numbered(self, base: &str, n: usize) -> String {
        match self {
            NameRule::Any => format!("{base} {n}"),
            NameRule::Dotted => format!("{base}.{n}"),
        }
    }

    /// What the rule asks of a name, for an error.
    pub fn describe(self) -> &'static str {
        match self {
            NameRule::Any => "any text",
            NameRule::Dotted => "letters and digits with single dots between them, like v1.2",
        }
    }
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
    pub name_rule: NameRule,
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

    /// The name of an entity created without one: `Untitled <name>`, fitted to
    /// the name rule (`Untitled.Tag`).
    pub fn default_name(&self) -> String {
        self.name_rule.fit(&format!("Untitled {}", self.name))
    }

    /// Why `name` (already trimmed) breaks the name rule, naming the name that
    /// would follow it; `None` when it doesn't break it.
    pub fn name_rule_violation(&self, name: &str) -> Option<String> {
        if self.name_rule.allows(name) {
            return None;
        }
        let fitted = self.name_rule.fit(name);
        let hint = if fitted.is_empty() {
            String::new()
        } else {
            format!("; try {fitted:?}")
        };
        Some(format!(
            "{name:?} isn't a valid {0} name: a {0} name is {1}{hint}",
            self.name,
            self.name_rule.describe()
        ))
    }
}

/// The one rule for comparing names (I-83): trimmed, and lowercased a character
/// at a time, with Unicode case folding (unlike SQLite's `NOCASE`, which only
/// folds ASCII). Every name comparison goes through it: the stored key
/// (`entities.name_key`, a name key's `entity_keys` value), the resolver, the
/// MCP tools and the markdown converter's reference grouping. Character by
/// character, so no character folds by context (`str::to_lowercase` writes a
/// word-final `Σ` as `ς`): the fold of a prefix is a prefix of the fold, and a
/// prefix match on the key finds what the folded query starts.
pub fn fold_name(name: &str) -> String {
    name.trim().chars().flat_map(char::to_lowercase).collect()
}

const fn prop(id: &'static str, kind: PropertyKind, label: &'static str) -> PropertyDef {
    PropertyDef {
        id,
        kind,
        label,
        options: &[],
        default_option: None,
        target_structure: None,
        on_calendar: false,
    }
}

/// A date property whose values are days on the calendar.
const fn calendar_date(id: &'static str, label: &'static str) -> PropertyDef {
    PropertyDef {
        on_calendar: true,
        ..prop(id, PropertyKind::Date, label)
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
        name_rule: NameRule::Any,
        keys: &[],
        facets: &["todo"],
        relates_to: &["Note"],
    },
    StructureDef {
        type_: "Tag",
        name: "Tag",
        plural: "Tags",
        description: "a label, written #tag in note content; its name is letters and digits with single dots between them (v1.2)",
        properties: &[],
        creatable: true,
        // Reached via `#`, not bare `@`.
        mentionable: false,
        name_editable: true,
        name_rule: NameRule::Dotted,
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
            calendar_date("date", ""),
            prop("content", PropertyKind::Richtext, ""),
        ],
        // Created per day by Resolve's date key, never from "+ New"; the server
        // derives the name from the date.
        creatable: false,
        mentionable: false,
        name_editable: false,
        name_rule: NameRule::Any,
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
    name: "ToDo",
    plural: "ToDos",
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
        calendar_date("todo.due", "Due"),
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

/// Every declaration of `property_id`: the structures' and the facets'. A
/// structure property id may be declared by several structures; a facet's is
/// declared once, by its facet.
pub fn declarations(property_id: &str) -> impl Iterator<Item = &'static PropertyDef> + '_ {
    let structure_props = STRUCTURES.iter().flat_map(|s| s.properties);
    let facet_props = FACETS.iter().flat_map(|f| f.properties);
    structure_props
        .chain(facet_props)
        .filter(move |p| p.id == property_id)
}

/// The ids of the properties flagged `on_calendar`, each once.
pub fn calendar_properties() -> Vec<&'static str> {
    let structure_props = STRUCTURES.iter().flat_map(|s| s.properties);
    let facet_props = FACETS.iter().flat_map(|f| f.properties);
    let mut ids: Vec<&str> = structure_props
        .chain(facet_props)
        .filter(|p| p.on_calendar)
        .map(|p| p.id)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
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
        assert_eq!(fold_name("Сергей Королёв"), fold_name("сергей королёв"));
    }

    #[test]
    fn fold_name_keeps_prefixes() {
        let name = "ΟΔΟΣ ΑΘΗΝΑΣ";
        assert!(fold_name(name).starts_with(&fold_name("ΟΔΟΣ")));
        assert_eq!(fold_name("ΟΔΟΣ"), fold_name("οδοσ"));
    }

    #[test]
    fn dotted_names_are_words_with_single_dots() {
        for name in ["v1.2", "program.apollo", "apollo11", "Королёв", "a"] {
            assert!(NameRule::Dotted.allows(name), "{name}");
        }
        for name in [
            "apollo-11",
            "apollo_11",
            "Apollo 11",
            ".v1",
            "v1.",
            "v1..2",
            "",
        ] {
            assert!(!NameRule::Dotted.allows(name), "{name}");
        }
        assert!(NameRule::Any.allows("Apollo 11"));
    }

    #[test]
    fn fit_makes_a_dotted_name() {
        assert_eq!(NameRule::Dotted.fit("apollo-11"), "apollo.11");
        assert_eq!(NameRule::Dotted.fit(" Apollo 11 "), "Apollo.11");
        assert_eq!(NameRule::Dotted.fit("_a__b--c."), "a.b.c");
        assert_eq!(NameRule::Dotted.fit("v1.2"), "v1.2");
        assert_eq!(NameRule::Dotted.fit("---"), "");
        assert_eq!(NameRule::Any.fit(" Apollo 11 "), "Apollo 11");
    }

    #[test]
    fn default_names_follow_the_name_rule() {
        for s in STRUCTURES {
            assert!(s.name_rule.allows(&s.default_name()), "{}", s.type_);
        }
        assert_eq!(
            structure("Tag").expect("Tag").default_name(),
            "Untitled.Tag"
        );
        assert_eq!(
            structure("Note").expect("Note").default_name(),
            "Untitled Note"
        );
    }

    #[test]
    fn a_violation_names_the_rule_and_a_fitted_name() {
        let tag = structure("Tag").expect("Tag");
        assert_eq!(
            tag.name_rule_violation("apollo-11").as_deref(),
            Some(
                r#""apollo-11" isn't a valid Tag name: a Tag name is letters and digits with single dots between them, like v1.2; try "apollo.11""#
            )
        );
        assert!(tag
            .name_rule_violation("---")
            .is_some_and(|m| !m.contains("try")));
        assert_eq!(tag.name_rule_violation("v1.2"), None);
        assert_eq!(
            structure("Note")
                .expect("Note")
                .name_rule_violation("Apollo 11"),
            None
        );
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

    #[test]
    fn only_date_properties_are_on_the_calendar() {
        let structure_props = STRUCTURES.iter().flat_map(|s| s.properties);
        let facet_props = FACETS.iter().flat_map(|f| f.properties);
        for p in structure_props.chain(facet_props) {
            assert!(
                !p.on_calendar || p.kind == PropertyKind::Date,
                "{} is on the calendar but isn't a date",
                p.id
            );
        }
        assert_eq!(calendar_properties(), ["date", "todo.due"]);
    }

    #[test]
    fn declarations_cover_structures_and_facets() {
        let due: Vec<_> = declarations("todo.due").map(|p| p.kind).collect();
        assert_eq!(due, [PropertyKind::Date]);
        assert_eq!(declarations("content").count(), 2);
        assert_eq!(declarations("nope").count(), 0);
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
