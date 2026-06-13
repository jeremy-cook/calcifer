// M4 verification: pure round-trips (no server) + live link-derivation (against
// the running tonic server). Run: pnpm test:md   (server must be up)
import { entityClient, richTextClient } from '../calciferClient.js'
import { toTipTap } from './parse.js'
import { fromTipTap } from './serialize.js'
import type { TTNode } from './types.js'

function assert(cond: unknown, msg: string): void {
  if (!cond) throw new Error('FAIL: ' + msg)
}

function nodeTypes(node: TTNode): string[] {
  const out: string[] = []
  const walk = (n: TTNode) => {
    out.push(n.type)
    ;(n.content ?? []).forEach(walk)
  }
  walk(node)
  return out
}

async function pureRoundTrip(): Promise<void> {
  const fake = async (st: string, name: string) => ({ id: `${st}:${name.toLowerCase()}`, name })
  const cases = [
    'See [[Photosynthesis]] and #biology on 2026-06-15.',
    '# Heading\n\nA paragraph with [[Krebs Cycle]] and #metabolism.',
    '- one\n- two with [[Link]]\n- three',
  ]
  for (const md of cases) {
    const doc = await toTipTap(md, fake)
    const back = fromTipTap(doc).trim()
    assert(back === md.trim(), `round-trip mismatch:\n  in:  ${JSON.stringify(md)}\n  out: ${JSON.stringify(back)}`)
  }
  const t = nodeTypes(await toTipTap(cases[0], fake))
  assert(
    t.includes('mention') && t.includes('hashtag') && t.includes('dateChip'),
    'emits mention/hashtag/dateChip nodes',
  )
  console.log('✓ pure round-trip (3 cases) + node shapes')
}

async function liveDerivation(): Promise<void> {
  const resolve = async (structureType: string, name: string) => {
    const r = await entityClient.resolveByName({ structureType, name, createIfMissing: true })
    return { id: r.entity!.id, name: r.entity!.name }
  }
  const md = 'Notes on [[Krebs Cycle]] — central to #metabolism. Reviewed 2026-07-04.'
  const doc = await toTipTap(md, resolve)

  const srcId = crypto.randomUUID()
  await entityClient.create({ entity: { id: srcId, structureType: 'Note', name: `MD test ${Date.now()}` } })
  await richTextClient.put({ ref: { entityId: srcId, propertyId: 'content' }, doc: JSON.stringify(doc) })

  const ent = await entityClient.get({ id: srcId })
  assert(ent.links.length === 2, `expected 2 derived links, got ${ent.links.length}`)
  assert(ent.referencedDates.includes('2026-07-04'), 'date reference derived')
  const kinds = ent.links.map((l) => l.target?.structureType).sort()
  assert(kinds[0] === 'Note' && kinds[1] === 'Tag', `expected [Note, Tag], got ${JSON.stringify(kinds)}`)
  console.log(`✓ live derivation — ${ent.links.length} links ${JSON.stringify(kinds)}, dates ${JSON.stringify(ent.referencedDates)}`)
}

async function main(): Promise<void> {
  await pureRoundTrip()
  await liveDerivation()
  console.log('M4 OK — markdown ↔ TipTap converter; agent markdown derives the graph server-side')
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})
