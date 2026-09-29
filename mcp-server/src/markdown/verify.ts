// M4 verification: pure round-trips (no server) + live link-derivation (against
// the running tonic server). Run: pnpm test:md   (server must be up)
import { entityClient, richTextClient, structureClient } from '../calciferClient.js'
import { docRef } from '../tools.js'
import { toTipTap } from './parse.js'
import { chipIds, fromTipTap } from './serialize.js'
import type { ChipTarget, Resolver, TTNode } from './types.js'

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

// The registry's structure types, as the MCP server reads them from ListStructures.
const TYPES: ReadonlySet<string> = new Set(['Note', 'Tag', 'DailyNote', 'Todo'])

// A fake server: ids are "Structure:Name". A DailyNote isn't creatable, so only
// "June 13, 2026" exists; any other DailyNote name is a miss.
const fake: Resolver = async (st, name) =>
  st === 'DailyNote' && name !== 'June 13, 2026' ? null : { id: `${st}:${name}`, name }

const toTT = (md: string) => toTipTap(md, fake, TYPES)

// Every chip's target, read back from its fake id (all targets still exist).
function fakeTargets(doc: TTNode): Map<string, ChipTarget> {
  return new Map(
    chipIds(doc).map((id) => {
      const colon = id.indexOf(':')
      return [id, { structureType: id.slice(0, colon), name: id.slice(colon + 1) }]
    }),
  )
}

const toMd = (doc: TTNode, targets = fakeTargets(doc)) => fromTipTap(doc, targets, TYPES).trim()

function mentions(node: TTNode): Record<string, unknown>[] {
  const out: Record<string, unknown>[] = []
  const walk = (n: TTNode) => {
    if (n.type === 'mention' || n.type === 'hashtag') out.push(n.attrs ?? {})
    ;(n.content ?? []).forEach(walk)
  }
  walk(node)
  return out
}

async function pureRoundTrip(): Promise<void> {
  const cases = [
    'See [[Photosynthesis]] and #biology on 2026-06-15.',
    '# Heading\n\nA paragraph with [[Krebs Cycle]] and #metabolism.',
    '- one\n- two with [[Link]]\n- three',
    'Search by *meaning*, not keywords, via **mean pooling** and `cosine similarity`.',
    'Ship [[Todo/Ship It]] by [[DailyNote/June 13, 2026]]; see [[Tag/ml]].',
    'A note named [[Note/Todo/Ship]] and one named [[Foo/Bar]].',
  ]
  for (const md of cases) {
    const back = toMd(await toTT(md))
    assert(back === md.trim(), `round-trip mismatch:\n  in:  ${JSON.stringify(md)}\n  out: ${JSON.stringify(back)}`)
  }
  const t = nodeTypes(await toTT(cases[0]))
  assert(
    t.includes('mention') && t.includes('hashtag') && t.includes('dateChip'),
    'emits mention/hashtag/dateChip nodes',
  )

  // Inline marks land on text nodes (bold/italic/code), not literal characters.
  const marked = await toTT('*a* **b** `c`')
  const marks = new Set<string>()
  const collect = (n: TTNode) => {
    n.marks?.forEach((m) => marks.add(m.type))
    ;(n.content ?? []).forEach(collect)
  }
  collect(marked)
  assert(marks.has('italic') && marks.has('bold') && marks.has('code'), 'emits italic/bold/code marks')
  // Underscores in prose must NOT become emphasis (snake_case survives).
  const us = toMd(await toTT('the one_daily_note_per_day index'))
  assert(us === 'the one_daily_note_per_day index', `snake_case mangled: ${us}`)

  // Only real calendar days become date chips; impossible ones stay plain text.
  const dates = async (md: string) => {
    const out: string[] = []
    const walk = (n: TTNode) => {
      if (n.type === 'dateChip') out.push(String(n.attrs?.date))
      ;(n.content ?? []).forEach(walk)
    }
    walk(await toTT(md))
    return out
  }
  assert((await dates('Due 2026-02-28.')).join() === '2026-02-28', '2026-02-28 is a date chip')
  assert((await dates('Due 2028-02-29.')).join() === '2028-02-29', 'a leap day is a date chip')
  for (const bad of ['2026-02-30', '2026-13-45', '2027-02-29', '2026-04-31', '2026-00-10', '2026-01-00']) {
    assert((await dates(`Due ${bad}.`)).length === 0, `${bad} is not a date chip`)
    assert(toMd(await toTT(`Due ${bad}.`)) === `Due ${bad}.`, `${bad} stays plain text`)
  }

  // [[Structure/Name]] targets that structure only when the prefix is a registry type.
  const [todo] = mentions(await toTT('[[Todo/Ship]]'))
  assert(todo.structureType === 'Todo' && todo.id === 'Todo:Ship' && todo.label === 'Ship', `[[Todo/Ship]] is a Todo mention: ${JSON.stringify(todo)}`)
  const [foo] = mentions(await toTT('[[Foo/Bar]]'))
  assert(foo.structureType === 'Note' && foo.id === 'Note:Foo/Bar', `[[Foo/Bar]] is a Note named Foo/Bar: ${JSON.stringify(foo)}`)
  const [lower] = mentions(await toTT('[[todo/Ship]]'))
  assert(lower.structureType === 'Note' && lower.id === 'Note:todo/Ship', 'the prefix must match a structure type exactly')

  // [[name|label]] is still accepted, but written back from the target's name.
  const aliased = await toTT('See [[Attention|attn]].')
  assert(mentions(aliased)[0].id === 'Note:Attention', '[[name|label]] targets name')
  assert(toMd(aliased) === 'See [[Attention]].', `alias written as the target's name: ${toMd(aliased)}`)

  // A non-creatable target that doesn't exist becomes plain text, not a chip.
  const missDay = await toTT('On [[DailyNote/June 14, 2026]] we shipped.')
  assert(mentions(missDay).length === 0, 'missing DailyNote is not a mention')
  assert(toMd(missDay) === 'On June 14, 2026 we shipped.', `missing DailyNote as plain text: ${toMd(missDay)}`)

  // Chips are written from the target's current name; a gone target as its label.
  const renamed = await toTT('See [[Old Name]], [[Gone]] and #oldtag.')
  const targets = new Map<string, ChipTarget>([
    ['Note:Old Name', { structureType: 'Note', name: 'New Name' }],
    ['Tag:oldtag', { structureType: 'Tag', name: 'newtag' }],
  ])
  const renamedMd = toMd(renamed, targets)
  assert(renamedMd === 'See [[New Name]], Gone and #newtag.', `chips from current names: ${renamedMd}`)
  console.log('✓ pure round-trip (6 cases) + node shapes + marks + snake_case safety + structure prefixes + current names')
}

async function liveDerivation(): Promise<void> {
  const resolve: Resolver = async (structureType, name) => {
    const r = await entityClient.resolveEntity({ structureType, key: { case: 'name', value: name }, createIfMissing: true })
    return { id: r.entity!.id, name: r.entity!.name }
  }
  const types = new Set((await structureClient.listStructures({})).structures.map((s) => s.type))
  const md = 'Notes on [[Krebs Cycle]] — central to #metabolism. Reviewed 2026-07-04.'
  const doc = await toTipTap(md, resolve, types)

  const { id: srcId } = (await entityClient.createEntity({ structureType: 'Note', name: `MD test ${Date.now()}` }))
    .entity!
  await richTextClient.putRichText({ ...(await docRef('Note', srcId)), doc: JSON.stringify(doc) })

  const ent = (await entityClient.getEntity({ id: srcId })).entity!
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
