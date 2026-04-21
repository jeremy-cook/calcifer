import { forwardRef, useImperativeHandle } from 'react'
import { Command } from 'cmdk'

type SuggestionItem = { id: string; label: string }

interface SuggestionMenuProps {
  items: SuggestionItem[]
  command: (item: SuggestionItem) => void
}

export interface SuggestionMenuHandle {
  onKeyDown: (props: { event: KeyboardEvent }) => boolean
}

const NAVIGATION_KEYS = ['ArrowUp', 'ArrowDown', 'Enter']

export const SuggestionMenu = forwardRef<SuggestionMenuHandle, SuggestionMenuProps>(({ items, command }, ref) => {
  useImperativeHandle(ref, () => ({
    onKeyDown({ event }) {
      if (!NAVIGATION_KEYS.includes(event.key)) return false
      const el = document.getElementById('suggestion-cmd')
      el?.dispatchEvent(new KeyboardEvent('keydown', { key: event.key, bubbles: true, cancelable: true }))
      return true
    },
  }))

  return (
    <Command id="suggestion-cmd" className="slash-menu" onKeyDown={(e) => e.stopPropagation()}>
      <Command.List>
        <Command.Empty className="slash-menu-empty">No results</Command.Empty>
        {items.map((item) => (
          <Command.Item key={item.id} value={item.label} className="slash-menu-item" onSelect={() => command(item)}>
            <span className="slash-menu-item-title">{item.label}</span>
          </Command.Item>
        ))}
      </Command.List>
    </Command>
  )
})

SuggestionMenu.displayName = 'SuggestionMenu'
