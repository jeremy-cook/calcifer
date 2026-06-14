// M5 verification: exercise the knowledge ops against the running server.
// (The MCP server itself is a thin stdio wrapper over these.) Run: pnpm test:tools
import * as ops from './tools.js'

function assert(cond: unknown, msg: string): void {
  if (!cond) throw new Error('FAIL: ' + msg)
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

  // Second create for the same day is a no-op (already_exists handled internally).
  const again = await ops.createDailyNote(date)
  assert(/ready for 2026-06-13/.test(again), 'create_daily_note is idempotent per day')

  await ops.appendToDailyNote(date, 'Read about [[Transformers]] today.')
  const dailyBack = await ops.getBacklinks('Transformers')
  assert(/June 13, 2026/.test(dailyBack), 'daily note links back to Transformers under its long-date name')

  console.log('Unit D OK — backlinks + daily-note ops work end-to-end')
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})
