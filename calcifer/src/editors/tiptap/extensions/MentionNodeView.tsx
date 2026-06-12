import { NodeViewWrapper, type NodeViewProps } from '@tiptap/react'
import { router } from '~/router'
import { useEntityStore } from '~/model/store'

export function MentionNodeView({ node, extension }: NodeViewProps) {
  const { id, label, structureType, char } = node.attrs as {
    id: string
    label: string
    structureType: string
    char: string
  }

  // Render the live entity name so renames propagate to every chip. The stored
  // `label` attr is only a fallback for targets that have since been deleted.
  const liveName = useEntityStore((s) => (id ? s.entities[id]?.title : undefined))
  const missing = Boolean(id) && liveName === undefined
  const className = (extension.options.HTMLAttributes?.class as string | undefined) ?? 'mention'

  const handleClick = () => {
    if (!id) return
    void router.navigate({ to: '/e/$id', params: { id } })
  }

  return (
    <NodeViewWrapper
      as="span"
      className={className}
      data-structure-type={structureType}
      data-broken={missing || undefined}
      onClick={handleClick}
    >
      {char}
      {liveName ?? label}
    </NodeViewWrapper>
  )
}
