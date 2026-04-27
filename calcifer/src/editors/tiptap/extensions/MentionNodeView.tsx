import { NodeViewWrapper, type NodeViewProps } from '@tiptap/react'
import { router } from '~/router'

export function MentionNodeView({ node, extension }: NodeViewProps) {
  const { id, label, structureType, char } = node.attrs as {
    id: string
    label: string
    structureType: string
    char: string
  }

  const className = (extension.options.HTMLAttributes?.class as string | undefined) ?? 'mention'

  const handleClick = () => {
    if (!id) return
    void router.navigate({ to: '/e/$id', params: { id } })
  }

  return (
    <NodeViewWrapper as="span" className={className} data-structure-type={structureType} onClick={handleClick}>
      {char}
      {label}
    </NodeViewWrapper>
  )
}
