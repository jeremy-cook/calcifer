import { computePosition, flip, shift } from '@floating-ui/dom'
import { ReactRenderer } from '@tiptap/react'
import { posToDOMRect, type Editor } from '@tiptap/core'
import type { SuggestionOptions } from '@tiptap/suggestion'
import { SuggestionMenu, type SuggestionMenuHandle } from './SuggestionMenu'

export type HashtagItem = {
  id: string
  label: string
}

export const HASHTAG_ITEMS: HashtagItem[] = [
  { id: '1', label: 'design' },
  { id: '2', label: 'engineering' },
  { id: '3', label: 'product' },
  { id: '4', label: 'frontend' },
  { id: '5', label: 'backend' },
  { id: '6', label: 'ux' },
  { id: '7', label: 'research' },
  { id: '8', label: 'infrastructure' },
  { id: '9', label: 'security' },
  { id: '10', label: 'performance' },
]

function updatePosition(editor: Editor, element: HTMLElement) {
  const virtualElement = {
    getBoundingClientRect: () =>
      posToDOMRect(editor.view, editor.state.selection.from, editor.state.selection.to),
  }
  computePosition(virtualElement, element, {
    placement: 'bottom-start',
    strategy: 'absolute',
    middleware: [shift(), flip()],
  }).then(({ x, y, strategy }: { x: number; y: number; strategy: 'fixed' | 'absolute' }) => {
    element.style.position = strategy
    element.style.left = `${x}px`
    element.style.top = `${y}px`
  })
}

export const hashtagSuggestion: Omit<SuggestionOptions<HashtagItem>, 'editor'> = {
  char: '#',

  items: ({ query }) => {
    const q = query.toLowerCase()
    return HASHTAG_ITEMS.filter((item) => item.label.toLowerCase().includes(q)).slice(0, 8)
  },

  render: () => {
    let component: ReactRenderer<SuggestionMenuHandle>

    return {
      onStart(props) {
        component = new ReactRenderer(SuggestionMenu, {
          props,
          editor: props.editor,
        })
        if (!props.clientRect) return
        component.element.style.position = 'absolute'
        document.body.appendChild(component.element)
        updatePosition(props.editor, component.element)
      },

      onUpdate(props) {
        component.updateProps(props)
        if (!props.clientRect) return
        updatePosition(props.editor, component.element)
      },

      onKeyDown(props) {
        if (props.event.key === 'Escape') {
          component.destroy()
          component.element.remove()
          return true
        }
        return component.ref?.onKeyDown(props) ?? false
      },

      onExit() {
        component.destroy()
        component.element.remove()
      },
    }
  },
}
