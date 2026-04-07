import { forwardRef, useImperativeHandle, useMemo } from 'react'
import { Command } from 'cmdk'
import type { SlashCommandItem } from '~/lib/tiptap-extension-slash-command'

interface SlashCommandMenuProps {
  items: SlashCommandItem[]
  command: (item: SlashCommandItem) => void
}

export interface SlashCommandMenuHandle {
  onKeyDown: (props: { event: KeyboardEvent }) => boolean
}

const NAVIGATION_KEYS = ['ArrowUp', 'ArrowDown', 'Enter']

export const SlashCommandMenu = forwardRef<SlashCommandMenuHandle, SlashCommandMenuProps>(
  ({ items, command }, ref) => {
    useImperativeHandle(ref, () => ({
      onKeyDown({ event }) {
        if (!NAVIGATION_KEYS.includes(event.key)) return false
        // Forward the keydown into cmdk so it handles selection
        const el = document.getElementById('slash-cmd')
        el?.dispatchEvent(new KeyboardEvent('keydown', { key: event.key, bubbles: true, cancelable: true }))
        return true
      },
    }))

    const groups = useMemo(
      () =>
        items.reduce<Record<string, SlashCommandItem[]>>(
          (acc, item) => ({ ...acc, [item.group]: [...(acc[item.group] ?? []), item] }),
          {},
        ),
      [items],
    )

    return (
      <Command id="slash-cmd" className="slash-menu" onKeyDown={(e) => e.stopPropagation()}>
        <Command.List>
          <Command.Empty className="slash-menu-empty">No results</Command.Empty>
          {Object.entries(groups).map(([group, groupItems]) => (
            <Command.Group key={group} heading={group} className="slash-menu-group">
              {groupItems.map((item) => (
                <Command.Item
                  key={item.title}
                  value={item.title}
                  className="slash-menu-item"
                  onSelect={() => command(item)}
                >
                  <span className="slash-menu-item-title">{item.title}</span>
                  <span className="slash-menu-item-subtitle">{item.subtitle}</span>
                </Command.Item>
              ))}
            </Command.Group>
          ))}
        </Command.List>
      </Command>
    )
  },
)

SlashCommandMenu.displayName = 'SlashCommandMenu'
