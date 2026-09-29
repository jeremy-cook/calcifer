// TipTap JSON -> markdown. Chips are written from their target's current name,
// looked up by id in `targets`: a Note mention -> [[Name]], any other structure
// -> [[Structure/Name]], hashtag -> #Name, dateChip -> ISO. A chip whose target
// is gone is written as its stored label in plain text, so writing it back
// creates nothing. Marks round-trip: code -> `…` (verbatim, exclusive),
// bold -> **…**, italic -> *…*.
import { DEFAULT_MENTION_STRUCTURE, splitStructurePrefix, type ChipTarget, type TTNode } from './types.js'

function applyMarks(text: string, marks?: { type: string }[]): string {
  if (!marks?.length) return text
  const has = (t: string) => marks.some((m) => m.type === t)
  if (has('code')) return `\`${text}\`` // code is verbatim; no nested emphasis
  let out = text
  if (has('bold')) out = `**${out}**`
  if (has('italic')) out = `*${out}*`
  return out
}

const isChip = (n: TTNode) => n.type === 'mention' || n.type === 'hashtag'

// The ids of every mention/hashtag in the doc, de-duplicated.
export function chipIds(doc: TTNode): string[] {
  const ids = new Set<string>()
  const walk = (n: TTNode) => {
    if (isChip(n) && n.attrs?.id) ids.add(String(n.attrs.id))
    ;(n.content ?? []).forEach(walk)
  }
  walk(doc)
  return [...ids]
}

interface Context {
  targets: ReadonlyMap<string, ChipTarget>
  structureTypes: ReadonlySet<string>
}

// [[Name]] for a Note, unless its name itself reads as [[Structure/Name]].
function mentionToMd({ name, structureType }: ChipTarget, ctx: Context): string {
  const bare =
    structureType === DEFAULT_MENTION_STRUCTURE &&
    splitStructurePrefix(name, ctx.structureTypes).structureType === DEFAULT_MENTION_STRUCTURE
  return bare ? `[[${name}]]` : `[[${structureType}/${name}]]`
}

function chipToMd(n: TTNode, ctx: Context): string {
  const target = ctx.targets.get(String(n.attrs?.id ?? ''))
  if (!target) return String(n.attrs?.label ?? '')
  return n.type === 'hashtag' ? `#${target.name}` : mentionToMd(target, ctx)
}

function inlineToMd(content: TTNode[] | undefined, ctx: Context): string {
  if (!content) return ''
  return content
    .map((n) => {
      switch (n.type) {
        case 'text':
          return applyMarks(n.text ?? '', n.marks)
        case 'mention':
        case 'hashtag':
          return chipToMd(n, ctx)
        case 'dateChip':
          return String(n.attrs?.date ?? '')
        default:
          return inlineToMd(n.content, ctx)
      }
    })
    .join('')
}

function listItemText(li: TTNode, ctx: Context): string {
  return inlineToMd(li.content?.[0]?.content, ctx)
}

function blockToMd(node: TTNode, ctx: Context): string {
  switch (node.type) {
    case 'heading':
      return `${'#'.repeat((node.attrs?.level as number) || 1)} ${inlineToMd(node.content, ctx)}`
    case 'bulletList':
      return (node.content ?? []).map((li) => `- ${listItemText(li, ctx)}`).join('\n')
    case 'orderedList':
      return (node.content ?? []).map((li, idx) => `${idx + 1}. ${listItemText(li, ctx)}`).join('\n')
    case 'blockquote':
      return (node.content ?? []).map((p) => `> ${inlineToMd(p.content, ctx)}`).join('\n')
    case 'codeBlock':
      return `\`\`\`${(node.attrs?.language as string) ?? ''}\n${node.content?.[0]?.text ?? ''}\n\`\`\``
    case 'paragraph':
    default:
      return inlineToMd(node.content, ctx)
  }
}

// `targets` maps each chip id (see `chipIds`) to its target's current identity;
// a missing id means the target no longer exists.
export function fromTipTap(
  doc: TTNode,
  targets: ReadonlyMap<string, ChipTarget>,
  structureTypes: ReadonlySet<string>,
): string {
  if (!doc.content) return ''
  const ctx: Context = { targets, structureTypes }
  return (
    doc.content
      .map((b) => blockToMd(b, ctx))
      .filter((s) => s.length > 0)
      .join('\n\n') + '\n'
  )
}
