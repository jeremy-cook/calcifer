use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Mode {
    Lexical,
    Semantic,
    Hybrid,
}

#[derive(Deserialize)]
pub(super) struct SearchArgs {
    pub(super) query: String,
    pub(super) limit: Option<u32>,
    pub(super) mode: Option<Mode>,
}

#[derive(Deserialize)]
pub(super) struct NameArgs {
    pub(super) name: String,
}

#[derive(Deserialize)]
pub(super) struct GetNoteArgs {
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) date: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct QueryTextArgs {
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) date: Option<String>,
    pub(super) query: String,
}

#[derive(Deserialize)]
pub(super) struct EditArgs {
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) date: Option<String>,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) markdown: String,
    pub(super) version: i64,
}

#[derive(Deserialize)]
pub(super) struct NoteArgs {
    pub(super) name: String,
    pub(super) markdown: String,
}

#[derive(Deserialize)]
pub(super) struct CreateNoteArgs {
    pub(super) name: String,
    pub(super) markdown: String,
    #[serde(default)]
    pub(super) tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub(super) struct TagArgs {
    pub(super) name: String,
    #[serde(default)]
    pub(super) add: Vec<String>,
    #[serde(default)]
    pub(super) remove: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct LinkArgs {
    pub(super) from: String,
    pub(super) to: String,
}

#[derive(Deserialize)]
pub(super) struct DateArgs {
    pub(super) date: String,
}

#[derive(Deserialize)]
pub(super) struct DailyArgs {
    pub(super) date: String,
    pub(super) markdown: String,
}
