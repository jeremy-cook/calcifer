// Smoke test: prove a non-browser Node process can drive the tonic core.
// Run the server (cd server && cargo run), then: pnpm smoke
import { entityClient } from './calciferClient.js'

async function main(): Promise<void> {
  const before = await entityClient.list({ structureType: '' })
  console.log(`list: ${before.entities.length} entities`)
  for (const e of before.entities) console.log(`  - ${e.structureType}: ${e.name}`)

  const name = `Node smoke ${new Date().toISOString().slice(11, 19)}`
  console.log(`create: "${name}"`)
  const created = await entityClient.create({ structureType: 'Note', name })
  console.log(`  -> created ${created.id}`)

  const after = await entityClient.list({ structureType: '' })
  console.log(`list: ${after.entities.length} entities (was ${before.entities.length})`)

  if (after.entities.length !== before.entities.length + 1) {
    throw new Error('entity count did not grow by exactly 1')
  }
  console.log('OK — Node ↔ tonic round-trip works')
}

main().catch((err) => {
  console.error('smoke failed:', err)
  process.exit(1)
})
