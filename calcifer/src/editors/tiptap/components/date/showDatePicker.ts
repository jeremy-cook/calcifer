import { computePosition, flip, shift } from '@floating-ui/dom'
import { ReactRenderer } from '@tiptap/react'
import { posToDOMRect, type Editor } from '@tiptap/core'
import { DatePickerPopup } from './DatePickerPopup'

interface ShowDatePickerOptions {
  initialDate?: string
  onConfirm?: (date: string) => void
  referenceEl?: Element
}

export function showDatePicker(editor: Editor, options: ShowDatePickerOptions = {}) {
  const { initialDate, onConfirm, referenceEl } = options

  function destroy() {
    component.destroy()
    component.element.remove()
    document.removeEventListener('mousedown', onOutsideClick, true)
  }

  const component = new ReactRenderer(DatePickerPopup, {
    props: {
      initialDate,
      onSelect(date: string) {
        destroy()
        if (onConfirm) {
          onConfirm(date)
        } else {
          editor.chain().focus().insertDateChip(date).run()
        }
      },
      onClose() {
        destroy()
        editor.commands.focus()
      },
    },
    editor,
  })

  component.element.style.position = 'absolute'
  document.body.appendChild(component.element)

  const reference = referenceEl ?? {
    getBoundingClientRect: () => posToDOMRect(editor.view, editor.state.selection.from, editor.state.selection.to),
  }

  computePosition(reference as Element, component.element as HTMLElement, {
    placement: 'bottom-start',
    strategy: 'absolute',
    middleware: [shift(), flip()],
  }).then(({ x, y, strategy }) => {
    const el = component.element as HTMLElement
    el.style.position = strategy
    el.style.left = `${x}px`
    el.style.top = `${y}px`
  })

  function onOutsideClick(e: MouseEvent) {
    if (!component.element.contains(e.target as Node)) {
      destroy()
    }
  }
  document.addEventListener('mousedown', onOutsideClick, true)
}
