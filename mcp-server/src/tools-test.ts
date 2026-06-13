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

  console.log('M5 OK — MCP knowledge ops work end-to-end')
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})
