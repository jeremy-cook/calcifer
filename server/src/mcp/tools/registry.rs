use crate::structures::{PropertyDef, PropertyKind, FACETS, STRUCTURES};

use super::Knowledge;

impl Knowledge {
    /// The registry lives in `structures.rs`; this only formats it.
    pub fn list_structures(&self) -> String {
        let lines: Vec<String> = STRUCTURES
            .iter()
            .map(|s| {
                let mut line = format!("- {}: {}", s.type_, s.description);
                if !s.properties.is_empty() {
                    let props: Vec<String> = s.properties.iter().map(describe_property).collect();
                    line += &format!("\n  properties: {}", props.join(", "));
                }
                if !s.facets.is_empty() {
                    line += &format!("\n  facets: {}", s.facets.join(", "));
                }
                if !s.relates_to.is_empty() {
                    line += &format!("\n  relates_to: {}", s.relates_to.join(", "));
                }
                line
            })
            .chain(FACETS.iter().map(|f| {
                let props: Vec<String> = f.properties.iter().map(describe_property).collect();
                let accepted: Vec<&str> = STRUCTURES
                    .iter()
                    .filter(|s| s.facets.contains(&f.id))
                    .map(|s| s.type_)
                    .collect();
                format!(
                    "- facet {} ({}): {}\n  properties: {}\n  accepted by: {}",
                    f.id,
                    f.name,
                    f.description,
                    props.join(", "),
                    accepted.join(", ")
                )
            }))
            .collect();
        lines.join("\n")
    }
}

/// e.g. "priority (select: none|low|medium|high, default none)", "tags (relations -> Tag)".
pub(super) fn describe_property(p: &PropertyDef) -> String {
    let kind = match p.kind {
        PropertyKind::Richtext => "richtext",
        PropertyKind::Text => "text",
        PropertyKind::Number => "number",
        PropertyKind::Date => "date",
        PropertyKind::Select => "select",
        PropertyKind::Relation => "relation",
        PropertyKind::Relations => "relations",
    };
    match p.kind {
        PropertyKind::Select => {
            let keys: Vec<&str> = p.options.iter().map(|o| o.key).collect();
            let default = p
                .default_option
                .map(|d| format!(", default {d}"))
                .unwrap_or_default();
            format!("{} ({kind}: {}{default})", p.id, keys.join("|"))
        }
        PropertyKind::Relation | PropertyKind::Relations => {
            format!(
                "{} ({kind} -> {})",
                p.id,
                p.target_structure.unwrap_or("any")
            )
        }
        _ => format!("{} ({kind})", p.id),
    }
}
