import { forwardRef, useImperativeHandle, useState } from 'react'
import { Command } from 'cmdk'
import type { SuggestionMenuHandle } from '~/editors/tiptap/components/suggestionPopup'
import type { EntitySuggestionItem } from './types'

interface MentionMenuProps {
  items: EntitySuggestionItem[]
  command: (item: EntitySuggestionItem) => void
}

const NAVIGATION_KEYS = ['ArrowUp', 'ArrowDown', 'Enter']

export const MentionMenu = forwardRef<SuggestionMenuHandle, MentionMenuProps>(({ items, command }, ref) => {
  const [value, setValue] = useState<string>(items[0]?.id ?? '')
  const activeValue = items.some((i) => i.id === value) ? value : (items[0]?.id ?? '')

  useImperativeHandle(ref, () => ({
    onKeyDown({ event }) {
      if (!NAVIGATION_KEYS.includes(event.key)) return false
      if (items.length === 0) return true
      if (event.key === 'Enter') {
        const selected = items.find((i) => i.id === activeValue) ?? items[0]
        if (selected) command(selected)
        return true
      }
      const idx = Math.max(
        0,
        items.findIndex((i) => i.id === activeValue),
      )
      const nextIdx = event.key === 'ArrowDown' ? (idx + 1) % items.length : (idx - 1 + items.length) % items.length
      const next = items[nextIdx]
      if (next) setValue(next.id)
      return true
    },
  }))

  return (
    <Command
      className="slash-menu"
      shouldFilter={false}
      value={activeValue}
      onValueChange={setValue}
      onKeyDown={(e) => e.stopPropagation()}
    >
      <Command.List>
        <Command.Empty className="slash-menu-empty">No results</Command.Empty>
        {items.map((item) => (
          <Command.Item key={item.id} value={item.id} className="slash-menu-item" onSelect={() => command(item)}>
            <span className="size-2 shrink-0 rounded-full" style={{ background: item.color }} aria-hidden />
            <span className="slash-menu-item-title">{item.label}</span>
          </Command.Item>
        ))}
      </Command.List>
    </Command>
  )
})

MentionMenu.displayName = 'MentionMenu'
