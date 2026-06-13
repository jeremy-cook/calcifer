// TipTap JSON -> markdown. mention -> [[label]], hashtag -> #label, dateChip -> ISO.
import type { TTNode } from './types.js'

function inlineToMd(content?: TTNode[]): string {
  if (!content) return ''
  return content
    .map((n) => {
      switch (n.type) {
        case 'text':
          return n.text ?? ''
        case 'mention':
          return `[[${n.attrs?.label ?? ''}]]`
        case 'hashtag':
          return `#${n.attrs?.label ?? ''}`
        case 'dateChip':
          return String(n.attrs?.date ?? '')
        default:
          return inlineToMd(n.content)
      }
    })
    .join('')
}

function listItemText(li: TTNode): string {
  return inlineToMd(li.content?.[0]?.content)
}

function blockToMd(node: TTNode): string {
  switch (node.type) {
    case 'heading':
      return `${'#'.repeat((node.attrs?.level as number) || 1)} ${inlineToMd(node.content)}`
    case 'bulletList':
      return (node.content ?? []).map((li) => `- ${listItemText(li)}`).join('\n')
    case 'orderedList':
      return (node.content ?? []).map((li, idx) => `${idx + 1}. ${listItemText(li)}`).join('\n')
    case 'blockquote':
      return (node.content ?? []).map((p) => `> ${inlineToMd(p.content)}`).join('\n')
    case 'codeBlock':
      return `\`\`\`${(node.attrs?.language as string) ?? ''}\n${node.content?.[0]?.text ?? ''}\n\`\`\``
    case 'paragraph':
    default:
      return inlineToMd(node.content)
  }
}

export function fromTipTap(doc: TTNode): string {
  if (!doc.content) return ''
  return (
    doc.content
      .map(blockToMd)
      .filter((s) => s.length > 0)
      .join('\n\n') + '\n'
  )
}
