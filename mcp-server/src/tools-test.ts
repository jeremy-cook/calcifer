// M5 verification: exercise the knowledge ops against the running server.
// (The MCP server itself is a thin stdio wrapper over these.) Run: pnpm test:tools
import { entityClient, richTextClient } from './calciferClient.js'
import * as ops from './tools.js'

function assert(cond: unknown, msg: string): void {
  if (!cond) throw new Error('FAIL: ' + msg)
}

async function noteId(name: string): Promise<string> {
  const r = await entityClient.resolveEntity({ structureType: 'Note', key: { case: 'name', value: name }, createIfMissing: false })
  return r.entity!.id
}

// A note's derived link targets, as sorted "Structure:id" strings.
async function linkTargets(id: string): Promise<string[]> {
  const ent = (await entityClient.getEntity({ id })).entity!
  return ent.links.map((l) => `${l.target!.structureType}:${l.target!.id}`).sort()
}

async function entityCount(): Promise<number> {
  return (await entityClient.listEntities({ structureType: '' })).entities.length
}

// get_note's body: everything after the frontmatter's closing "---".
function noteBody(note: string): string {
  return note.slice(note.indexOf('\n---\n', 3) + 5)
}

// get_note's frontmatter version, the token edit_note checks line numbers against.
function noteVersion(note: string): number {
  return Number(note.match(/^version: (\d+)$/m)![1])
}

async function main(): Promise<void> {
  // The marquee: agent creates a note that references another note + a tag.
  const r = await ops.createNote('Transformers', 'An architecture built on [[Attention]]. #ml')
  console.log('create_note:', r)
  assert(/Derived links: 2/.test(r), 'create_note derived 2 links')

  const back = await ops.getBacklinks('Attention')
  console.log('get_backlinks(Attention):', back.replace(/\n/g, ' '))
  assert(back.includes('Transformers'), 'Attention is backlinked from Transformers')

  const found = await ops.searchNotes('trans')
  assert(found.includes('Transformers'), 'search finds Transformers')

  const note = await ops.getNote('Transformers')
  assert(note.includes('[[Attention]]') && note.includes('#ml'), 'get_note round-trips refs')

  await ops.appendToNote('Transformers', 'They use [[Positional Encoding]] too.')
  const note2 = await ops.getNote('Transformers')
  assert(note2.includes('[[Positional Encoding]]'), 'append added content')

  // Writing to a note that doesn't exist is an error; only create_note makes one.
  const countBeforeAppend = await entityCount()
  const ghost = `No Such Note ${Date.now()}`
  const ghostWrites = {
    append_to_note: () => ops.appendToNote(ghost, 'x'),
    link_notes: () => ops.linkNotes(ghost, 'Attention'),
  }
  for (const [op, write] of Object.entries(ghostWrites)) {
    const err = await write().then(() => null, (e: Error) => e)
    assert(err && /No note named/.test(err.message), `${op} on a missing note is an error: ${err?.message ?? 'did not reject'}`)
  }
  assert((await entityCount()) === countBeforeAppend, 'a rejected append creates no entity')

  // Backlinks now resolve server-side (EntityService.ListBacklinks).
  const back2 = await ops.getBacklinks('Positional Encoding')
  console.log('get_backlinks(Positional Encoding):', back2.replace(/\n/g, ' '))
  assert(back2.includes('Transformers'), 'server-side backlinks see Transformers')

  // Daily notes: get-or-create + append, idempotent per calendar day.
  const date = '2026-06-13'
  const created = await ops.createDailyNote(date)
  console.log('create_daily_note:', created)
  assert(created.includes(date), 'create_daily_note acknowledges the date')

  // Second create for the same day is a no-op that returns the same note.
  const again = await ops.createDailyNote(date)
  assert(/for 2026-06-13 already existed/.test(again), 'create_daily_note is idempotent per day')
  assert(again.includes(created.match(/\(([^)]+)\)/)![1]), 'create_daily_note returns the same note')

  await ops.appendToDailyNote(date, 'Read about [[Transformers]] today.')
  const dailyBack = await ops.getBacklinks('Transformers')
  assert(/June 13, 2026/.test(dailyBack), 'daily note links back to Transformers under its long-date name')

  // I-53: get_note writes a chip from its target's current name, not the stored label.
  const stamp = Date.now()
  const oldName = `Rename Target ${stamp}`
  const newName = `Renamed Target ${stamp}`
  await ops.createNote(`Rename Source ${stamp}`, `Points at [[${oldName}]].`)
  await entityClient.renameEntity({ id: await noteId(oldName), name: newName })
  const renamedNote = await ops.getNote(`Rename Source ${stamp}`)
  console.log('get_note after rename:', noteBody(renamedNote))
  assert(renamedNote.includes(`[[${newName}]]`) && !renamedNote.includes(oldName), 'get_note shows a renamed target by its new name')

  // I-53/I-58: get_note -> create_note links the same targets and creates nothing,
  // for a Note, a Tag, an alias, a Todo and a DailyNote (2026-06-13, created above).
  const todoName = `Ship ${stamp}`
  await entityClient.createEntity({ structureType: 'Todo', name: todoName })
  const rtName = `Round Trip ${stamp}`
  await ops.createNote(
    rtName,
    `Uses [[RT Note ${stamp}]] and [[RT Alias ${stamp}|the alias]]. #rt${stamp}\n\n- do [[Todo/${todoName}]] on [[DailyNote/June 13, 2026]]`,
  )
  const rtId = await noteId(rtName)
  const linksBefore = await linkTargets(rtId)
  assert(linksBefore.length === 5, `round-trip note has 5 links, got ${JSON.stringify(linksBefore)}`)
  assert(linksBefore.some((l) => l.startsWith('Todo:')) && linksBefore.some((l) => l.startsWith('DailyNote:')), 'Todo and DailyNote mentions keep their structure')
  const countBefore = await entityCount()
  const rtNote = await ops.getNote(rtName)
  console.log('get_note(round trip):', noteBody(rtNote).replace(/\n/g, ' '))
  assert(rtNote.includes(`[[Todo/${todoName}]]`), 'get_note writes a Todo mention as [[Todo/Name]]')
  assert(rtNote.includes('[[DailyNote/June 13, 2026]]'), 'get_note writes a DailyNote mention as [[DailyNote/Name]]')
  assert(rtNote.includes(`[[RT Alias ${stamp}]]`) && !rtNote.includes('the alias'), 'get_note writes an alias as the target name')
  assert(rtNote.includes(`#rt${stamp}`), 'get_note keeps the hashtag as #tag')
  assert(rtNote.startsWith(`---\nname: ${JSON.stringify(rtName)}\nversion: `) && /\nlinked_from: \[.*\]\n---\n/.test(rtNote), 'get_note starts with name/version/linked_from frontmatter')
  // The whole result, frontmatter included, goes back in: create_note ignores the frontmatter.
  await ops.createNote(rtName, rtNote)
  assert(noteBody(await ops.getNote(rtName)) === noteBody(rtNote), 'create_note drops the frontmatter instead of saving it as text')
  const linksAfter = await linkTargets(rtId)
  assert(JSON.stringify(linksAfter) === JSON.stringify(linksBefore), `round trip keeps the links: ${JSON.stringify(linksBefore)} -> ${JSON.stringify(linksAfter)}`)
  assert((await entityCount()) === countBefore, 'round trip creates no entity')

  // I-58: a missing non-creatable target stays plain text instead of creating anything.
  const countBeforeMiss = await entityCount()
  const missing = await ops.createNote(`Missing Day ${stamp}`, 'On [[DailyNote/February 1, 1901]] nothing happened.')
  assert(/Derived links: 0/.test(missing), 'a missing DailyNote mention derives no link')
  assert((await entityCount()) === countBeforeMiss + 1, 'a missing DailyNote mention creates only the note itself')

  // edit_note: replace / insert / delete a line range of the body, checked against get_note's version.
  // The body below is line 1 "Alpha.", 3 "## Heading", 5-6 the list, 8 "Last ...": blocks sit on
  // lines 1, 3, 5-6 and 8 with a blank line between them.
  const editName = `Edit Subject ${stamp}`
  const editLink = `Edit Link ${stamp}`
  await ops.createNote(editName, `Alpha.\n\n## Heading\n\n- one\n- two\n\nLast [[${editLink}]].`)
  const editId = await noteId(editName)
  const read = async () => {
    const n = await ops.getNote(editName)
    return { n, body: noteBody(n), version: noteVersion(n) }
  }
  const rejects = async (p: Promise<unknown>, pattern: RegExp, msg: string) => {
    const err = await p.then(() => null, (e: Error) => e)
    assert(err && pattern.test(err.message), `${msg}: ${err?.message ?? 'did not reject'}`)
  }

  let cur = await read()
  console.log('get_note(edit subject):', cur.body.replace(/\n/g, ' '))
  assert(cur.body === `Alpha.\n\n## Heading\n\n- one\n- two\n\nLast [[${editLink}]].`, 'the body is plain markdown, no line markers')

  // query_text_location: quote text, get whole-block line ranges and the version back.
  const found1 = await ops.queryTextLocation(editName, 'two')
  assert(found1.startsWith(`version: ${cur.version}\n\nlines 5-6 (1 match):\n- one\n- two`), `a match inside a list reports the whole list, got ${JSON.stringify(found1)}`)
  const found2 = await ops.queryTextLocation(editName, 'Alpha.\n\n## Heading')
  assert(/lines 1-1 \(1 match\):\nAlpha\.\n\nlines 3-3 \(1 match\):\n## Heading/.test(found2), 'a match across blocks reports each block')
  const found3 = await ops.queryTextLocation(editName, 'o')
  assert(/^version: \d+\n\nlines 5-6 \(2 matches\):\n- one\n- two$/.test(found3), `matches are counted per block, got ${JSON.stringify(found3)}`)
  const found4 = await ops.queryTextLocation(editName, 'n')
  assert(/lines 3-3 \(1 match\)[^]*lines 5-6 \(1 match\)[^]*lines 8-8 \(1 match\)/.test(found4), `every block with a match is listed in order, got ${JSON.stringify(found4)}`)
  assert((await ops.queryTextLocation(editName, 'nowhere in the note')).startsWith('No match'), 'no match says so')
  await rejects(ops.queryTextLocation(editName, ''), /query is empty/, 'an empty query is rejected')
  await rejects(ops.queryTextLocation(`No Such Note ${stamp}`, 'x'), /No note named/, 'query_text_location on a missing note is an error')

  // The ranges query_text_location returns go straight into edit_note.
  const where = (await ops.queryTextLocation(editName, '## Heading')).match(/lines (\d+)-(\d+)/)!
  await ops.editNote(editName, Number(where[1]), Number(where[2]), '## Renamed heading', cur.version)
  const afterReplace = (await read()).body
  assert(afterReplace.includes('## Renamed heading') && !afterReplace.includes('## Heading'), 'edit_note replaces a block')
  assert(afterReplace.includes('- one\n- two') && afterReplace.includes(`[[${editLink}]]`), 'edit_note leaves the other blocks alone')

  await rejects(ops.editNote(editName, 1, 1, 'Stale', cur.version), /changed since you read/, 'a stale version is rejected')
  assert(!(await read()).body.includes('Stale'), 'a stale edit writes nothing')

  cur = await read()
  await rejects(ops.editNote(editName, 6, 6, 'x', cur.version), /line 6 is inside the block at lines 5-6.*start at 5/, 'a start inside a block is rejected')
  await rejects(ops.editNote(editName, 5, 5, 'x', cur.version), /line 5 is inside the block at lines 5-6.*end at 6/, 'an end inside a block is rejected')
  await rejects(ops.editNote(editName, 2, 2, 'x', cur.version), /contain no block/, 'a range of only blank lines is rejected')
  assert((await read()).body === cur.body, 'a rejected range writes nothing')

  await ops.editNote(editName, 1, 0, 'First.', cur.version)
  cur = await read()
  assert(cur.body.startsWith('First.\n\nAlpha.\n\n## Renamed heading'), 'end = start - 1 inserts before the line')

  // Blank lines at either edge of a range are fine: lines 2-4 are blank, "Alpha.", blank.
  await ops.editNote(editName, 2, 4, 'Alpha two.', cur.version)
  cur = await read()
  assert(cur.body.startsWith('First.\n\nAlpha two.\n\n## Renamed heading'), 'a range edged by blank lines replaces the block between them')

  // Now: First. 1, Alpha two. 3, heading 5, list 7-8, Last 10.
  await ops.editNote(editName, 10, 10, 'Last, rewritten.', cur.version)
  assert((await linkTargets(editId)).length === 0, 'replacing the block that held a link drops the derived link')

  cur = await read()
  await ops.editNote(editName, 5, 8, '', cur.version)
  cur = await read()
  assert(cur.body === 'First.\n\nAlpha two.\n\nLast, rewritten.', `empty markdown deletes the range, got ${JSON.stringify(cur.body)}`)

  await ops.editNote(editName, 6, 5, 'Appended.', cur.version)
  cur = await read()
  assert(cur.body === 'First.\n\nAlpha two.\n\nLast, rewritten.\n\nAppended.', 'start = last line + 1 appends')

  await rejects(ops.editNote(editName, 99, 99, 'x', cur.version), /start 99 is outside/, 'start past the end is rejected')
  await rejects(ops.editNote(editName, 1, 99, 'x', cur.version), /end 99 is outside/, 'end past the last line is rejected')
  await rejects(ops.editNote(editName, 3, 1, 'x', cur.version), /end 1 is outside/, 'end before start - 1 is rejected')
  const countBeforeEdit = await entityCount()
  await rejects(ops.editNote(`No Such Note ${stamp}`, 1, 1, 'x', 0), /No note named/, 'edit_note does not create a missing note')
  assert((await entityCount()) === countBeforeEdit, 'a rejected edit creates no entity')

  await ops.editNote(editName, 1, 7, '', cur.version)
  cur = await read()
  assert(cur.body === '', `deleting every block leaves an empty body, got ${JSON.stringify(cur.body)}`)
  await ops.editNote(editName, 1, 0, 'Fresh.', cur.version)
  assert((await read()).body === 'Fresh.', 'an empty note takes an insert at line 1')
  const freshDoc = JSON.parse((await richTextClient.getRichText(await ops.docRef('Note', editId))).richText!.doc) as { content: unknown[] }
  assert(freshDoc.content.length === 1, 'inserting into an empty note leaves no stray empty paragraph')

  // A leading --- block that isn't key: value lines is note content, not frontmatter.
  await ops.createNote(`Rule ${stamp}`, '---\nplain text\n---')
  assert(noteBody(await ops.getNote(`Rule ${stamp}`)).includes('plain text'), 'create_note keeps a leading --- block that is not frontmatter')

  // Semantic retrieval (M8): two thematically related notes with NO shared
  // keywords. A paraphrased query that overlaps neither should surface both via
  // semantic/hybrid search where lexical (FTS5) finds nothing.
  await ops.createNote('Sailboat Maintenance', 'Trimming the canvas and patching the hull keeps a vessel gliding across open water.')
  await ops.createNote('Aircraft Servicing', 'Mechanics inspect the fuselage and tune the engines so a plane stays aloft through the sky.')
  const semQuery = 'keeping a craft moving through its medium'

  // Embedding runs on an async background worker, and on a fresh server the
  // model may still be loading, so poll for up to 60 s until both notes are
  // retrievable. The assertions below fail loudly if they never appear.
  for (let i = 0; i < 120; i++) {
    const s = await ops.searchNotes(semQuery, 5, 'semantic')
    if (/Sailboat/.test(s) && /Aircraft/.test(s)) break
    await new Promise((r) => setTimeout(r, 500))
  }

  const lexical = await ops.searchNotes(semQuery, 5, 'lexical')
  console.log('search(lexical):', lexical.replace(/\n/g, ' '))
  assert(!/Sailboat|Aircraft/.test(lexical), 'lexical search misses keyword-free paraphrase')

  const semantic = await ops.searchNotes(semQuery, 5, 'semantic')
  console.log('search(semantic):', semantic.replace(/\n/g, ' '))
  assert(/Sailboat/.test(semantic) && /Aircraft/.test(semantic), 'semantic retrieval surfaces both related notes')

  const hybrid = await ops.searchNotes(semQuery, 5, 'hybrid')
  console.log('search(hybrid):', hybrid.replace(/\n/g, ' '))
  assert(/Sailboat/.test(hybrid) || /Aircraft/.test(hybrid), 'hybrid retrieval surfaces related notes')

  console.log('Unit D OK — backlinks + daily-note ops work end-to-end')
  console.log('Unit B OK — semantic/hybrid retrieval beats lexical on paraphrase')
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})
