import { computePosition, flip, shift } from '@floating-ui/dom'
import { ReactRenderer } from '@tiptap/react'
import { posToDOMRect, type Editor } from '@tiptap/core'
import type { SuggestionOptions } from '@tiptap/suggestion'
import type { ComponentType } from 'react'

export interface SuggestionMenuHandle {
  onKeyDown: (props: { event: KeyboardEvent }) => boolean
}

function updatePosition(editor: Editor, element: HTMLElement) {
  const virtualElement = {
    getBoundingClientRect: () =>
      posToDOMRect(editor.view, editor.state.selection.from, editor.state.selection.to),
  }
  void computePosition(virtualElement, element, {
    placement: 'bottom-start',
    strategy: 'absolute',
    middleware: [shift(), flip()],
  }).then(({ x, y, strategy }) => {
    element.style.position = strategy
    element.style.left = `${x}px`
    element.style.top = `${y}px`
  })
}

export function createSuggestionPopup<TItem>(
  MenuComponent: ComponentType<{ items: TItem[]; command: (item: TItem) => void }>,
): SuggestionOptions<TItem>['render'] {
  return () => {
    let component: ReactRenderer<SuggestionMenuHandle>

    return {
      onStart(props) {
        component = new ReactRenderer(MenuComponent, {
          props,
          editor: props.editor,
        })
        if (!props.clientRect) return
        component.element.style.position = 'absolute'
        document.body.appendChild(component.element)
        updatePosition(props.editor, component.element as HTMLElement)
      },

      onUpdate(props) {
        component.updateProps(props)
        if (!props.clientRect) return
        updatePosition(props.editor, component.element as HTMLElement)
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
  }
}
