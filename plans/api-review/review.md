# Source: architecture review, Part 1 (proto and frontend use of it)

Verbatim copy of the reviewer's findings (received 2026-09-25). Task files cite findings
by their letter-number (A1, B3, …). Line numbers were correct when the review was
written; re-check them before relying on them.

---

The core ideas behind the API are sound. The server builds the link graph from note
content (ADR 3), and it owns the list of structures (ADR 7). The main problem is that the
"server is in charge" approach is only half done:

- Writes: most write RPCs still take a whole Entity that the client built. That includes
  fields only the server should set. The server also has special-case RPCs that only the
  MCP server calls, while the frontend does the same jobs its own way.
- Sync: in practice the frontend is a full copy of the database. It lists every entity
  once and works out everything else itself. But Watch wasn't designed to keep a copy
  like that in sync.

Findings are ordered by importance. Each one says which way to move the work: into the
proto/server (→ proto) or into the frontend (→ FE).

## A. Service shape

### A1 · Entity is both the write input and the read output · high · → proto
- Create and Update take a full Entity, including fields the server should own: id,
  links, referenced_dates, created_at and updated_at.
- Create actually saves the client-supplied links and referenced_dates
  (server/src/services/entity.rs:49-51). That contradicts ADR 3's rule that no client
  writes links.
- Update silently ignores those same fields.
- Fix: make Entity output-only. Replace the inputs with:
  - CreateEntityRequest { structure_type, optional name, initial properties }, where the
    server creates the id, timestamps and default properties.
  - A Rename RPC plus the existing SetProperty, in place of Update. This also fixes I-15.

### A2 · Default entities are built in four places · medium · → proto
- Frontend: buildEntityMessage and buildDailyNoteMessage (calcifer/src/model/store.ts:27-85).
- Server: build_resolved_entity and build_daily_note (entity.rs:408-474). One of them has
  a comment that says it "mirrors the FE's buildEntityMessage".
- The default name Untitled X is set in store.ts:22 and compared again in
  routes/e.$id.tsx:166.
- The frontend creates the id itself "so the editor has its richtext refs immediately".
  But NewButton waits for the server's response before navigating anyway, so this gains
  nothing. Fixed by A1.

### A3 · RichTextRef stored as a property value adds nothing · medium · → proto
- A RichTextRef is just (entity.id, property.id), and both are already known from
  context. The registry already says which properties are rich text.
- Because the value is stored anyway:
  - it has to be created on every new entity (see A2);
  - the server never checks that entity_id matches the entity that owns it;
  - if the row is missing, the editor renders nothing (e.$id.tsx:209,
    DailyNoteSection.tsx:148).
- Fix: drop the richtext case from PropertyValue and address a document by
  (entity_id, declared property_id).

### A4 · Daily notes are created and moved differently by each client · medium · → proto
- Frontend: creates daily notes with Create and names them with formatLongDate on the
  client. It moves them with Update plus a client-side rename (withDailyNoteDate).
- MCP: calls CreateDailyNote. If it gets AlreadyExists, it lists every DailyNote and
  scans their properties (mcp-server/src/tools.ts:113-125).
- Server: SetProperty(date) updates date_key but not the name, so any client using the
  proper single-property path ends up with a name that no longer matches the date. The
  server doesn't enforce name_editable=false either.
- Fix:
  - The server derives a DailyNote's name from its date on every write.
  - Merge ResolveByName and CreateDailyNote into one get-or-create RPC:
    Resolve { oneof key { name, date }, create_if_missing } → { entity, created }.
  - Moving a daily note then becomes a plain SetProperty(date).

### A5 · Search has two RPCs for one job · low · → proto
- Search(query, limit) and Retrieve(query, k, hybrid) return the same response, and
  Retrieve falls back to lexical search anyway.
- Fix: one Search(query, limit, mode) with a SearchMode enum. The MCP server already
  models exactly this (tools.ts:74).
- Separately, snippets mark matches with [ ], which clashes with [[wikilink]] syntax.
  Return match ranges instead.

### A6 · ListBacklinks takes the wrong request and returns too little · low · → proto
- It takes an EntityRef but only reads id; the MCP server fills in structureType: ''.
- It returns plain entities with no link details (which property the link came from, or
  when).
- The frontend never calls it. It works out backlinks itself, and the two methods
  disagree: the frontend excludes an entity linking to itself, the server doesn't.
- Fix: use ListBacklinksRequest { entity_id }, or state that this RPC is only for the
  agent (see B3).

### A7 · Looking up an entity by name isn't reliable for most structures · medium · → proto
- Only Tag has unique_names. But [[wikilinks]], @ mentions and the MCP get_note all look
  up Notes and Todos by name.
- find_by_name uses LIMIT 1 with no ORDER BY, so which of two same-named notes you get is
  undefined.
- structure_type isn't checked, so an empty or unknown type creates an entity anyway.
- Fix: either make names unique wherever lookup by name is used, or make the lookup
  deterministic and document it. Reject unknown structure types.

### A8 · Wrong error message on a unique-name clash · low · → proto
- map_unique_violation in Create and Update always says "a DailyNote for this date
  already exists".
- Creating or renaming a Tag onto an existing Tag name (which hits one_tag_per_name) gets
  that message.

## B. Sync and consistency

### B1 · RichText.Put sends no Watch event and has no conflict check · high · → proto
- Only entity.rs publishes to the watch hub. A Put changes the entity's links,
  referenced_dates and search index, but no other tab hears about it.
- There's also no event for the document itself. So when the agent appends to a note
  that's open in the browser, the editor never sees the new text. Its next debounced save
  replaces the whole document and wipes out the agent's append.
- Put also doesn't check that the entity exists or that the property is a declared
  rich-text property, so orphaned documents are possible.
- Fix:
  - Put publishes an Upserted event (and bumps the entity's updated_at).
  - Add a rich-text-changed event.
  - Add expected_updated_at to Put and return FailedPrecondition on a mismatch. The MCP
    append, which reads, edits and writes back, gets the same protection.

### B2 · RichText.Get uses NotFound for "nothing saved yet" · low · → proto
- Both clients turn NotFound into an empty document (model/richtext.ts:41, tools.ts:28).
- Fix: return an empty document for a declared property, and keep NotFound for an entity
  or property that doesn't exist.

### B3 · Watch isn't built for keeping a full copy in sync · high · decide first, then → proto and → FE
- The frontend keeps a full copy of every entity, which is reasonable for a single user
  (ADR 5). But:
  - Race at startup: the initial List and the Watch subscription start at the same time
    (App.tsx:14-21), so events that land between them are lost.
  - Lost events go unnoticed: the server silently drops events for a subscriber that
    falls behind (entity.rs:863-868), and nothing tells the client to reload.
  - Reconnects: after a reconnect the client doesn't refetch the list.
  - Refetch storm: every event and every mutation invalidates ['entities'], so the
    frontend reloads the full list each time. The server serves that as 1 + 4N queries.
    Each debounced keystroke save means a full reload, and each SetProperty means two
    (one from onSettled, one from the Watch echo).
- Fix:
  - Make Watch start with a snapshot (or accept a since revision) and carry a revision
    number on each event.
  - When a subscriber falls behind, send an explicit resync signal instead of dropping
    events.
  - Have the frontend apply Upserted/Deleted directly to the cached list instead of
    refetching.
  - State that the filtered List, ListBacklinks and Search are there for the agent.

## C. Message shapes

### C1 · repeated Property should be map<string, PropertyValue> · medium · → proto
- Every consumer repeats properties.find(p => p.id === id)?.value?.value: store.ts,
  todos.ts, DailyNoteSection, e.$id.tsx and MCP tools.ts:121.
- Property is {id=1, value=2}, which is exactly how protobuf encodes a map entry. So the
  change is wire-compatible and only breaks source code. A map also rules out duplicate
  ids.

### C2 · The contract is undocumented · low · → proto
- Undocumented fields: date values are ISO yyyy-MM-dd; links, referenced_dates and
  RichText.updated_at are output-only; relation values only need the target id.
- RichText.doc is TipTap JSON that the server parses (links.rs). The editor's node types
  (mention, hashtag, date chip, and their attributes) are therefore part of the API, but
  the proto says nothing about them, and the MCP server had to reimplement them.

### C3 · Unused property kinds, and structure flags only the frontend enforces · low
- Unused kinds: no structure declares relation, text or number. The frontend silently
  renders nothing for them (e.$id.tsx:257). Either remove them or render them.
- Flags: the server doesn't enforce creatable or name_editable. That needs A1/A4 first.

### C4 · Daily notes are sorted by name, which is wrong · medium · → FE · confirmed
- listByStructure (store.ts:324) sorts DailyNotes by name descending.
- Names look like "June 13, 2026", so the list comes out alphabetical by month name, and
  "June 9" sorts above "June 13".
- Fix: sort by the date property.

## D. The frontend's API layer

### D1 · Proto and Connect details leak into components · low · → FE
- Components build proto messages directly (e.$id.tsx:251, EntityRelationsField.tsx:37)
  and branch on ConnectError codes (DailyNoteDateField.tsx:2).
- Fix: keep reading proto types in components, as ADR 2 intends. Route writes through
  model helpers (setRelations(entity, id, ids), setDate, setSelect) and add isAlreadyExists
  next to isNotFound in api.ts. Then nothing under components/ imports @bufbuild/protobuf
  or @connectrpc.

### D2 · Duplicated code in the model layer · low · → FE
- Timestamp-to-milliseconds conversion is written three times (backlinks.ts:14,
  store.ts:316, todos.ts:117); timestampMs from @bufbuild/protobuf/wkt already does this.
- The List query function is written twice (App.tsx:16, store.ts:92). Export an
  entitiesQuery the way structuresQuery is exported.
- The delete path and the create mutation each exist twice.
- A raw ['entities'] key is used in places instead of qk.
- The Watch consumer lives in App.tsx instead of model/.

### D3 · Relation edits resend the whole list · low
- Adding or removing a tag sends the full relations list. If the browser and the agent
  tag something at the same time, one change is lost. This is related to I-14.
- Leave it unless the agent starts tagging.

## Suggested target shape

```proto
service EntityService {
  rpc Get(GetEntityRequest) returns (Entity);
  rpc List(ListEntitiesRequest) returns (ListEntitiesResponse);
  rpc Create(CreateEntityRequest) returns (Entity);        // server mints id, defaults, name
  rpc Rename(RenameEntityRequest) returns (Entity);        // replaces Update (I-15)
  rpc SetProperty(SetPropertyRequest) returns (Entity);
  rpc Delete(DeleteEntityRequest) returns (DeleteEntityResponse);
  rpc Resolve(ResolveRequest) returns (ResolveResponse);   // oneof key { name, date }; create_if_missing
  rpc ListBacklinks(ListBacklinksRequest) returns (ListBacklinksResponse);
  rpc Watch(WatchRequest) returns (stream WatchResponse);  // snapshot + revisioned events + resync
}
service RichTextService { Get; Put /* + expected_updated_at, publishes events */ }
service SearchService   { Search /* query, limit, mode */ }
service StructureService { List }
```

Moving to this would delete most of store.ts's builders (buildEntityMessage,
buildDailyNoteMessage, withDailyNoteDate, useUpdateEntity). It would also close I-15 and
give I-13 (the buf lint naming errors) a natural moment to rename to buf's conventions.

Suggested order: B1 → C4 → A1–A4 together → B3 → C1 → the rest.

---

## Added during planning (not in the original review)

### P1 · The editor never takes in outside changes
`EntityRichTextField` mounts `TiptapEditor` with `doc` as initial content only (keyed
remount per entity/property). A rich-text event alone won't make an agent's append
appear in an open editor; the editor needs a path to accept outside content. Folded into
T06.

### P2 · Pruning an empty daily note can delete content the agent just appended
`DailyNoteSection`'s unmount effect deletes the day's note if the *cached* doc is empty
(`getRichTextSnapshot`). Nothing refreshes that cache when the agent appends (B1), so a
stale empty cache can delete a note that now has content. Confirmed by reading; not
reproduced. Logged as I-35, fixed in T06.
