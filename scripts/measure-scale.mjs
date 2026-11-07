#!/usr/bin/env -S node --disable-warning=ExperimentalWarning
// Measures what ADR 9's rollout changes, at sizes agents will reach: seeds scratch
// databases with 5,000, 20,000 and 50,000 entities through the server's own write
// paths, then records, against each, the Watch snapshot's size and how long it
// takes to arrive, and the round trip of one rich-text save. The rollout's first
// step takes the baseline with it and its last step is checked against it.
//
//   scripts/measure-scale.mjs [sizes]      seed what's missing, then measure each size
//   scripts/measure-scale.mjs serve <size> run the server on that size's database, for a browser
//
// sizes: comma-separated entity counts (default 5000,20000,50000).
//
// Environment: CALCIFER_SCALE_DIR (default $TMPDIR/calcifer-scale) holds one
// database per size and its server's log; a database already there is measured
// as it is. CALCIFER_SERVER is the server binary (default
// server/target/release/calcifer-server; build it first). FASTEMBED_CACHE_DIR is
// the embedding model's cache (default server/.fastembed_cache). With a model,
// the script waits for the embed worker to finish before it measures, which
// takes about a minute and a half per thousand entities.
// CALCIFER_SCALE_EMBED=off runs the server without one instead (lexical-only:
// nothing embeds, so nothing runs beside the measurements). The snapshot and a
// save's write don't read embeddings, so their numbers are comparable either way.
//
// No dependencies: it speaks gRPC-Web over fetch and writes the few protobuf
// fields it needs by hand, so it runs with plain `node` (24 or later, for
// node:sqlite).

import { spawn } from 'node:child_process'
import { existsSync, mkdirSync, openSync, statSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { DatabaseSync } from 'node:sqlite'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const scaleDir = resolve(process.env.CALCIFER_SCALE_DIR ?? join(tmpdir(), 'calcifer-scale'))
const serverBin = resolve(process.env.CALCIFER_SERVER ?? join(root, 'server/target/release/calcifer-server'))
const modelCache = resolve(process.env.FASTEMBED_CACHE_DIR ?? join(root, 'server/.fastembed_cache'))
const embed = process.env.CALCIFER_SCALE_EMBED !== 'off'
const realDb = join(root, 'server/calcifer.db')
const basePort = 18_180

// ---- protobuf: just the wire types the requests below use ----

const encoder = new TextEncoder()
const decoder = new TextDecoder()

function varint(n) {
  const out = []
  while (n > 0x7f) {
    out.push((n & 0x7f) | 0x80)
    n = Math.floor(n / 128)
  }
  out.push(n)
  return out
}

function concat(parts) {
  const out = new Uint8Array(parts.reduce((sum, p) => sum + p.length, 0))
  let at = 0
  for (const p of parts) {
    out.set(p, at)
    at += p.length
  }
  return out
}

// A length-delimited field: a string, or a message already encoded.
function field(no, value) {
  const bytes = typeof value === 'string' ? encoder.encode(value) : value
  return concat([Uint8Array.from([...varint((no << 3) | 2), ...varint(bytes.length)]), bytes])
}

function boolField(no) {
  return Uint8Array.from([...varint(no << 3), 1])
}

// The fields of an encoded message, as [number, wire type, value] (a value is
// a number for varints and the raw bytes for everything else).
function* fields(bytes) {
  let at = 0
  const readVarint = () => {
    let n = 0
    let shift = 1
    for (;;) {
      const b = bytes[at++]
      n += (b & 0x7f) * shift
      if (b < 0x80) return n
      shift *= 128
    }
  }
  while (at < bytes.length) {
    const key = readVarint()
    const wire = key & 7
    if (wire === 0) yield [key >> 3, wire, readVarint()]
    else if (wire === 2) {
      const len = readVarint()
      yield [key >> 3, wire, bytes.subarray(at, at + len)]
      at += len
    } else if (wire === 1) at += 8
    else if (wire === 5) at += 4
    else throw new Error(`wire type ${wire}`)
  }
}

function fieldBytes(bytes, no) {
  for (const [n, , value] of fields(bytes)) if (n === no) return value
  return undefined
}

// PropertyValue: date = 3, select = 4, relations = 7 (EntityRefList { refs = 1 }).
const dateValue = (iso) => field(3, iso)
const selectValue = (key) => field(4, key)
const relationsValue = (ids) => field(7, concat(ids.map((id) => field(1, field(1, id)))))

// ---- gRPC-Web ----

// One unary call; the response message's bytes. A non-zero grpc-status throws.
async function call(port, method, body) {
  const frame = concat([Uint8Array.from([0, ...u32(body.length)]), body])
  const res = await fetch(`http://127.0.0.1:${port}/api/calcifer.v1.${method}`, {
    method: 'POST',
    headers: { 'content-type': 'application/grpc-web+proto', 'x-grpc-web': '1' },
    body: frame,
  })
  const bytes = new Uint8Array(await res.arrayBuffer())
  let message
  let trailers = `grpc-status: ${res.headers.get('grpc-status') ?? ''}\r\ngrpc-message: ${res.headers.get('grpc-message') ?? ''}`
  for (let at = 0; at < bytes.length; ) {
    const len = new DataView(bytes.buffer, bytes.byteOffset + at + 1, 4).getUint32(0)
    const payload = bytes.subarray(at + 5, at + 5 + len)
    if (bytes[at] & 0x80) trailers = decoder.decode(payload)
    else message = payload
    at += 5 + len
  }
  const status = /grpc-status:\s*(\d+)/.exec(trailers)?.[1]
  if (status !== '0') {
    const why = /grpc-message:\s*(.*)/.exec(trailers)?.[1]
    throw new Error(`${method}: grpc-status ${status} ${decodeURIComponent(why ?? '')}`)
  }
  return message ?? new Uint8Array()
}

function u32(n) {
  return [(n >>> 24) & 0xff, (n >>> 16) & 0xff, (n >>> 8) & 0xff, n & 0xff]
}

// The id of the Entity in field 1 of a Create or Resolve response.
const entityId = (response) => decoder.decode(fieldBytes(fieldBytes(response, 1), 1))

async function createEntity(port, { structureType, name, properties = {}, facets = [] }) {
  const parts = [field(2, structureType), field(3, name)]
  for (const [id, value] of Object.entries(properties)) parts.push(field(4, concat([field(1, id), field(2, value)])))
  for (const facet of facets) parts.push(field(5, facet))
  return entityId(await call(port, 'EntityService/CreateEntity', concat(parts)))
}

async function resolveDailyNote(port, iso) {
  const body = concat([field(1, 'DailyNote'), field(3, iso), boolField(4)])
  return entityId(await call(port, 'EntityService/ResolveEntity', body))
}

async function putRichText(port, entityId, doc) {
  const body = concat([field(1, entityId), field(2, 'content'), field(3, JSON.stringify(doc))])
  await call(port, 'RichTextService/PutRichText', body)
}

// ---- seeding ----

// mulberry32: the same data on every run.
function random(seed) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

const WORDS = (
  'orbit capsule booster lunar module docking hatch telemetry guidance thrust ' +
  'payload reentry heat shield splashdown crew rendezvous ascent descent stage ' +
  'gimbal propellant oxidizer valve pressure abort sequence countdown launch pad ' +
  'tracking station antenna signal relay command service engine nozzle chamber ' +
  'trajectory apogee perigee inclination window burn correction attitude thruster ' +
  'sensor reading timeline review design test failure fix risk budget schedule ' +
  'contract vendor prototype mockup simulator training checklist procedure manual'
).split(' ')

function isoDay(offsetDays) {
  const d = new Date()
  d.setUTCDate(d.getUTCDate() + offsetDays)
  return d.toISOString().slice(0, 10)
}

const text = (t) => ({ type: 'text', text: t })
const mention = (e) => ({ type: 'mention', attrs: { id: e.id, label: e.name, structureType: 'Note' } })
const hashtag = (e) => ({ type: 'hashtag', attrs: { id: e.id, label: e.name } })
const dateChip = (iso) => ({ type: 'dateChip', attrs: { date: iso } })

// A note's document: paragraphs of prose with `links` mentions of earlier notes,
// a #tag or two and a date chip or two spread through them.
function noteDoc(rand, notes, tags, links) {
  const pick = (list) => list[Math.floor(rand() * list.length)]
  const words = (n) => Array.from({ length: n }, () => pick(WORDS)).join(' ')
  const chips = []
  for (let i = 0; i < links && notes.length > 0; i++) chips.push(mention(pick(notes)))
  for (let i = 0, n = 1 + Math.floor(rand() * 2); i < n; i++) chips.push(hashtag(pick(tags)))
  for (let i = 0, n = Math.floor(rand() * 3); i < n; i++) chips.push(dateChip(isoDay(Math.floor(rand() * 240) - 180)))
  const paragraphs = []
  for (let p = 0, n = 3 + Math.floor(rand() * 3); p < n; p++) {
    const content = [text(`${words(12)} `)]
    for (let c = p; c < chips.length; c += n) content.push(chips[c], text(` ${words(6)} `))
    paragraphs.push({ type: 'paragraph', content })
  }
  return { type: 'doc', content: paragraphs }
}

// Runs `task(i)` for i in [0, count) with `width` in flight.
async function pool(count, width, task) {
  let next = 0
  await Promise.all(
    Array.from({ length: width }, async () => {
      while (next < count) await task(next++)
    }),
  )
}

function progress(label, done, total) {
  if (done % 500 === 0 || done === total) process.stdout.write(`\r  ${label} ${done}/${total}`)
  if (done === total) process.stdout.write('\n')
}

// `size` entities: 1% Tags, a Daily Note for each of up to two years of days, the
// rest Notes. A Note has one to three Tags, about ten links to earlier Notes, a
// #tag or two and up to two date chips in its text; one in six is a ToDo, most
// with a due date. Every write goes through the RPCs the browser and agents use.
async function seed(port, size) {
  const rand = random(size)
  const tagCount = Math.max(20, Math.floor(size / 100))
  const dayCount = Math.min(730, Math.floor(size / 20))
  const noteCount = size - tagCount - dayCount

  const tags = []
  await pool(tagCount, 4, async (i) => {
    const name = `topic.${i}`
    tags.push({ id: await createEntity(port, { structureType: 'Tag', name }), name })
    progress('tags', tags.length, tagCount)
  })

  const notes = []
  let made = 0
  await pool(noteCount, 6, async (i) => {
    const r = random(size * 7919 + i)
    const name = `${WORDS[i % WORDS.length]} ${WORDS[Math.floor(r() * WORDS.length)]} ${i}`
    const ownTags = Array.from({ length: 1 + Math.floor(r() * 3) }, () => tags[Math.floor(r() * tags.length)].id)
    const properties = { tags: relationsValue([...new Set(ownTags)]) }
    const facets = []
    if (r() < 1 / 6) {
      facets.push('todo')
      properties['todo.status'] = selectValue(r() < 0.7 ? 'open' : 'done')
      properties['todo.priority'] = selectValue(['none', 'low', 'medium', 'high'][Math.floor(r() * 4)])
      if (r() < 0.8) properties['todo.due'] = dateValue(isoDay(Math.floor(r() * 120) - 60))
    }
    const id = await createEntity(port, { structureType: 'Note', name, properties, facets })
    await putRichText(port, id, noteDoc(r, notes, tags, 8 + Math.floor(r() * 5)))
    notes.push({ id, name })
    progress('notes', ++made, noteCount)
  })

  made = 0
  await pool(dayCount, 4, async (i) => {
    const r = random(size * 104729 + i)
    const id = await resolveDailyNote(port, isoDay(-i))
    await putRichText(port, id, noteDoc(r, notes, tags, 5))
    progress('daily notes', ++made, dayCount)
  })
}

// ---- the server ----

function dbPath(size) {
  const path = join(scaleDir, `scale-${size}.db`)
  if (resolve(path) === realDb) throw new Error('refusing to use server/calcifer.db')
  return path
}

async function startServer(size) {
  if (!existsSync(serverBin)) throw new Error(`no server at ${serverBin}; run cargo build --release in server/`)
  // Its own ports per size, clear of a running Calcifer's 8080 and 8081.
  const port = basePort + 2 * (Math.floor(size / 1000) % 500)
  const log = openSync(join(scaleDir, `scale-${size}.log`), 'a')
  const env = {
    PATH: process.env.PATH,
    HOME: process.env.HOME,
    DATABASE_URL: `sqlite://${dbPath(size)}`,
    CALCIFER_ADDR: `127.0.0.1:${port}`,
    CALCIFER_MCP_ADDR: `127.0.0.1:${port + 1}`,
    CALCIFER_UI_DIR: join(root, 'web/dist'),
    RUST_LOG: 'warn',
    // Without the model: an empty cache, and a download that fails at once.
    ...(embed
      ? { FASTEMBED_CACHE_DIR: modelCache }
      : { FASTEMBED_CACHE_DIR: join(scaleDir, 'no-model'), HF_ENDPOINT: 'http://127.0.0.1:9' }),
  }
  // The scratch directory as cwd, so no .env is found and nothing lands in the repo.
  const child = spawn(serverBin, [], { cwd: scaleDir, env, stdio: ['ignore', log, log] })
  const exited = new Promise((_, reject) => child.on('exit', (code) => reject(new Error(`server exited ${code}`))))
  for (const started = Date.now(); Date.now() - started < 600_000; ) {
    try {
      await Promise.race([call(port, 'StructureService/ListStructures', new Uint8Array()), exited])
      return { port, stop: () => child.kill('SIGTERM') }
    } catch (e) {
      if (/server exited/.test(String(e))) throw e
      await new Promise((r) => setTimeout(r, 250))
    }
  }
  throw new Error('server did not start')
}

// Waits until every entity has embedding chunks, or the count has stopped
// changing for 15 s (no model, or an entity that has nothing to embed).
async function settleEmbeddings(size) {
  const db = new DatabaseSync(dbPath(size), { readOnly: true })
  const missing = db.prepare('SELECT COUNT(*) AS n FROM entities WHERE id NOT IN (SELECT entity_id FROM chunks)')
  let last = -1
  let still = Date.now()
  let shown = 0
  for (;;) {
    const { n } = missing.get()
    if (Date.now() - shown > 30_000) {
      console.log(`  embedding: ${n} entities left`)
      shown = Date.now()
    }
    if (n !== last) still = Date.now()
    if (n === 0 || Date.now() - still > 15_000) break
    last = n
    await new Promise((r) => setTimeout(r, 1000))
  }
  db.close()
}

// ---- measurements ----

// Opens WatchEntities and reads its first message, the snapshot: its size in
// bytes and the milliseconds from the request to its last byte.
async function snapshot(port) {
  const controller = new AbortController()
  const started = performance.now()
  const res = await fetch(`http://127.0.0.1:${port}/api/calcifer.v1.EntityService/WatchEntities`, {
    method: 'POST',
    headers: { 'content-type': 'application/grpc-web+proto', 'x-grpc-web': '1' },
    body: new Uint8Array(5),
    signal: controller.signal,
  })
  const reader = res.body.getReader()
  const chunks = []
  let have = 0
  let need = Infinity
  while (have < need) {
    const { value, done } = await reader.read()
    if (done) throw new Error('watch ended before its snapshot')
    chunks.push(value)
    have += value.length
    if (need === Infinity && have >= 5) {
      const head = concat(chunks).subarray(0, 5)
      need = 5 + new DataView(head.buffer, head.byteOffset + 1, 4).getUint32(0)
    }
  }
  const ms = performance.now() - started
  controller.abort()
  return { bytes: need - 5, ms }
}

// The newest Note's document, saved `count` times with one word changed each
// time, as an editor's debounced save sends it; the round trip of each save.
async function saves(port, size, count) {
  const db = new DatabaseSync(dbPath(size), { readOnly: true })
  const row = db
    .prepare(
      `SELECT r.entity_id AS id, r.doc AS doc, (SELECT COUNT(*) FROM links l WHERE l.entity_id = r.entity_id) AS links
       FROM richtext r JOIN entities e ON e.id = r.entity_id
       WHERE e.structure_type = 'Note' ORDER BY e.created_at DESC LIMIT 1`,
    )
    .get()
  db.close()
  const doc = JSON.parse(row.doc)
  const times = []
  for (let i = 0; i < count; i++) {
    doc.content[0].content[0] = text(`edit ${i} `)
    const started = performance.now()
    await putRichText(port, row.id, doc)
    times.push(performance.now() - started)
  }
  return { times, links: row.links }
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b)
  return sorted[Math.floor(sorted.length / 2)]
}

async function measure(size) {
  console.log(`${size} entities`)
  const fresh = !existsSync(dbPath(size))
  const server = await startServer(size)
  try {
    if (fresh) {
      const started = Date.now()
      await seed(server.port, size)
      console.log(`  seeded in ${Math.round((Date.now() - started) / 1000)} s`)
    }
    if (embed) await settleEmbeddings(size)
    await snapshot(server.port) // warm the page cache
    const snaps = []
    for (let i = 0; i < 5; i++) snaps.push(await snapshot(server.port))
    await saves(server.port, size, 5) // warm
    const save = await saves(server.port, size, 30)
    const counts = new DatabaseSync(dbPath(size), { readOnly: true })
    const n = counts.prepare('SELECT (SELECT COUNT(*) FROM entities) AS entities, (SELECT COUNT(*) FROM links) AS links').get()
    counts.close()
    return {
      size,
      entities: n.entities,
      links: n.links,
      dbMB: statSync(dbPath(size)).size / 1e6,
      snapshotMB: snaps[0].bytes / 1e6,
      snapshotMs: median(snaps.map((s) => s.ms)),
      saveMs: median(save.times),
      saveMaxMs: Math.max(...save.times),
      saveLinks: save.links,
    }
  } finally {
    server.stop()
  }
}

async function main() {
  mkdirSync(scaleDir, { recursive: true })
  const [command, arg] = process.argv.slice(2)
  if (command === 'serve') {
    const size = Number(arg)
    const server = await startServer(size)
    console.log(`serving ${dbPath(size)} on http://127.0.0.1:${server.port}; Ctrl-C stops it`)
    process.on('SIGINT', () => {
      server.stop()
      process.exit(0)
    })
    await new Promise(() => {})
  }
  const sizes = (command ?? '5000,20000,50000').split(',').map(Number)
  const results = []
  for (const size of sizes) results.push(await measure(size))
  const fmt = (n, digits = 1) => n.toFixed(digits)
  console.log('\n| entities | links | db MB | snapshot MB | snapshot ms | save ms (median) | save ms (max) |')
  console.log('|---:|---:|---:|---:|---:|---:|---:|')
  for (const r of results) {
    console.log(
      `| ${r.entities} | ${r.links} | ${fmt(r.dbMB)} | ${fmt(r.snapshotMB)} | ${fmt(r.snapshotMs, 0)} | ${fmt(r.saveMs)} | ${fmt(r.saveMaxMs)} |`,
    )
  }
  console.log(`\nA save is the newest Note's document (${results.at(-1)?.saveLinks} links) through PutRichText.`)
  console.log(JSON.stringify(results))
}

await main()
