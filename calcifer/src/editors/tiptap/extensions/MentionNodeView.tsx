import { NodeViewWrapper, type NodeViewProps } from '@tiptap/react'
import { router } from '~/router'
import { useEntity } from '~/model/store'

export function MentionNodeView({ node, extension }: NodeViewProps) {
  const { id, label, structureType, char } = node.attrs as {
    id: string
    label: string
    structureType: string
    char: string
  }

  // Render the live entity name so renames propagate to every chip. The stored
  // `label` attr is the fallback while loading, and for targets since deleted.
  const { entity, status } = useEntity(id)
  const liveName = entity?.name
  const missing = Boolean(id) && status === 'missing'
  const className = (extension.options.HTMLAttributes?.class as string | undefined) ?? 'mention'

  const handleClick = () => {
    if (!id) return
    void router.navigate({ to: '/e/$id', params: { id } })
  }

  return (
    // `data-type` (the node's name: `mention` or `hashtag`) and `data-id` let the
    // relationships section find a chip to jump to.
    <NodeViewWrapper
      as="span"
      className={className}
      data-type={node.type.name}
      data-id={id}
      data-structure-type={structureType}
      data-broken={missing || undefined}
      onClick={handleClick}
    >
      {char}
      {liveName ?? label}
    </NodeViewWrapper>
  )
}
