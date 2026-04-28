import { NodeViewWrapper, type NodeViewProps } from '@tiptap/react'
import { useNavigate } from '@tanstack/react-router'
import { formatDate } from '~/lib/tiptap-extension-date'
import { showDatePicker } from './showDatePicker'

export function DateChipView({ node, updateAttributes, editor }: NodeViewProps) {
  const { date } = node.attrs as { date: string }
  const navigate = useNavigate()

  function handleClick(e: React.MouseEvent<HTMLSpanElement>) {
    if (e.altKey) {
      showDatePicker(editor, {
        initialDate: date,
        referenceEl: e.currentTarget,
        onConfirm: (newDate) => updateAttributes({ date: newDate }),
      })
      return
    }
    void navigate({ to: '/calendar', search: { date } })
  }

  return (
    <NodeViewWrapper
      as="span"
      className="date-chip"
      onClick={handleClick}
      title="Open in calendar (alt-click to edit)"
      contentEditable={false}
    >
      {formatDate(date)}
    </NodeViewWrapper>
  )
}
