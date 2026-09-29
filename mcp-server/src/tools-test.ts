// M5 verification: exercise the knowledge ops against the running server.
// (The MCP server itself is a thin stdio wrapper over these.) Run: pnpm test:tools
import { entityClient } from './calciferClient.js'
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

// get_note's body: between the "# Name" heading and the "---" footer.
function noteBody(note: string): string {
  return note.slice(note.indexOf('\n\n') + 2, note.lastIndexOf('\n\n---\nLinked from:'))
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
  await ops.createNote(rtName, noteBody(rtNote))
  const linksAfter = await linkTargets(rtId)
  assert(JSON.stringify(linksAfter) === JSON.stringify(linksBefore), `round trip keeps the links: ${JSON.stringify(linksBefore)} -> ${JSON.stringify(linksAfter)}`)
  assert((await entityCount()) === countBefore, 'round trip creates no entity')

  // I-58: a missing non-creatable target stays plain text instead of creating anything.
  const countBeforeMiss = await entityCount()
  const missing = await ops.createNote(`Missing Day ${stamp}`, 'On [[DailyNote/February 1, 1901]] nothing happened.')
  assert(/Derived links: 0/.test(missing), 'a missing DailyNote mention derives no link')
  assert((await entityCount()) === countBeforeMiss + 1, 'a missing DailyNote mention creates only the note itself')

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
