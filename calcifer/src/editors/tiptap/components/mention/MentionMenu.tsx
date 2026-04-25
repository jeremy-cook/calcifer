import { forwardRef, useImperativeHandle, useMemo, useState } from 'react'
import { PlusIcon } from '@phosphor-icons/react'
import { Command } from 'cmdk'
import type { SuggestionMenuHandle } from '~/editors/tiptap/components/suggestionPopup'
import type { EntitySuggestionItem } from './types'

interface MentionMenuProps {
  items: EntitySuggestionItem[]
  command: (item: EntitySuggestionItem) => void
}

const NAVIGATION_KEYS = ['ArrowUp', 'ArrowDown', 'Enter']

function itemKey(item: EntitySuggestionItem, index: number): string {
  return item.isCreate ? `__create__:${index}` : item.id
}

export const MentionMenu = forwardRef<SuggestionMenuHandle, MentionMenuProps>(({ items, command }, ref) => {
  const keys = useMemo(() => items.map(itemKey), [items])
  const [value, setValue] = useState<string>(keys[0] ?? '')
  const activeValue = keys.includes(value) ? value : (keys[0] ?? '')
  const activeIndex = Math.max(0, keys.indexOf(activeValue))

  useImperativeHandle(ref, () => ({
    onKeyDown({ event }) {
      if (!NAVIGATION_KEYS.includes(event.key)) return false
      if (items.length === 0) return true
      if (event.key === 'Enter') {
        command(items[activeIndex] ?? items[0])
        return true
      }
      const nextIdx =
        event.key === 'ArrowDown'
          ? (activeIndex + 1) % items.length
          : (activeIndex - 1 + items.length) % items.length
      setValue(keys[nextIdx])
      return true
    },
  }))

  const renderItem = (item: EntitySuggestionItem, index: number) => {
    if (item.isCreate) {
      return (
        <Command.Item
          key={keys[index]}
          value={keys[index]}
          className="slash-menu-item"
          onSelect={() => command(item)}
        >
          <PlusIcon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
          <span className="slash-menu-item-title">{item.createLabel ?? `Create "${item.label}"`}</span>
        </Command.Item>
      )
    }
    return (
      <Command.Item
        key={keys[index]}
        value={keys[index]}
        className="slash-menu-item"
        onSelect={() => command(item)}
      >
        <span className="size-2 shrink-0 rounded-full" style={{ background: item.color }} aria-hidden />
        <span className="slash-menu-item-title">{item.label}</span>
      </Command.Item>
    )
  }

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
        {items.map(renderItem)}
      </Command.List>
    </Command>
  )
})

MentionMenu.displayName = 'MentionMenu'
