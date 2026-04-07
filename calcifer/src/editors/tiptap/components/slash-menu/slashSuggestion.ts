import { computePosition, flip, shift } from '@floating-ui/dom'
import { ReactRenderer } from '@tiptap/react'
import { posToDOMRect, type Editor } from '@tiptap/core'
import type { SuggestionOptions } from '@tiptap/suggestion'
import { SLASH_ITEMS, type SlashCommandItem } from '~/lib/tiptap-extension-slash-command'
import { SlashCommandMenu, type SlashCommandMenuHandle } from './SlashCommandMenu'

function updatePosition(editor: Editor, element: HTMLElement) {
  const virtualElement = {
    getBoundingClientRect: () => posToDOMRect(editor.view, editor.state.selection.from, editor.state.selection.to),
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

export const slashSuggestion: Omit<SuggestionOptions<SlashCommandItem>, 'editor'> = {
  char: '/',

  items: ({ query }) => {
    if (!query) return SLASH_ITEMS
    const q = query.toLowerCase()
    return SLASH_ITEMS.filter(
      (item) =>
        item.title.toLowerCase().includes(q) ||
        item.group.toLowerCase().includes(q) ||
        item.searchTerms?.some((t) => t.includes(q)),
    ).slice(0, 12)
  },

  render: () => {
    let component: ReactRenderer<SlashCommandMenuHandle>

    return {
      onStart(props) {
        component = new ReactRenderer(SlashCommandMenu, { props, editor: props.editor })
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
