import { NodeViewWrapper, type NodeViewProps } from '@tiptap/react'
import { formatDate } from '~/lib/tiptap-extension-date'
import { showDatePicker } from './showDatePicker'

export function DateChipView({ node, updateAttributes, editor }: NodeViewProps) {
  const { date } = node.attrs as { date: string }

  function handleClick(e: React.MouseEvent<HTMLSpanElement>) {
    showDatePicker(editor, {
      initialDate: date,
      referenceEl: e.currentTarget,
      onConfirm: (newDate) => updateAttributes({ date: newDate }),
    })
  }

  return (
    <NodeViewWrapper as="span" className="date-chip" onClick={handleClick} contentEditable={false}>
      {formatDate(date)}
    </NodeViewWrapper>
  )
}
