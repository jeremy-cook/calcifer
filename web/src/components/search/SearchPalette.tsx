import { useEffect, useState } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { Command } from 'cmdk'
import { formatDistanceToNow } from 'date-fns'
import { MagnifyingGlassIcon, PlusIcon } from '@phosphor-icons/react'
import type { MatchRange } from 'calcifer/v1/services_pb'
import { MIN_TEXT_QUERY, useTextSearch, useTitleMatches, type TextHit } from '~/model/search'
import { entityUpdatedAtDate, getOrCreateEntityForMention, type Entity } from '~/model/store'
import { structurePresentation, useStructures } from '~/model/structures'

const DEBOUNCE_MS = 200

// The top bar's Search button and the ⌘K palette it opens: titles from the replica as
// you type, then note text and meaning from the server, then "New note".
export function SearchPalette() {
  const navigate = useNavigate()
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [debounced, setDebounced] = useState('')

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setOpen((o) => !o)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  useEffect(() => {
    const timer = setTimeout(() => setDebounced(query), DEBOUNCE_MS)
    return () => clearTimeout(timer)
  }, [query])

  const titles = useTitleMatches(query)
  const text = useTextSearch(debounced)
  const trimmed = query.trim()

  const handleOpenChange = (next: boolean) => {
    setOpen(next)
    if (!next) {
      setQuery('')
      setDebounced('')
    }
  }

  const openEntity = (id: string) => {
    handleOpenChange(false)
    void navigate({ to: '/e/$id', params: { id } })
  }

  const handleNewNote = async () => {
    const entity = await getOrCreateEntityForMention('Note', trimmed)
    openEntity(entity.id)
  }

  // A note already listed by title isn't listed again for its text.
  const titleIds = new Set(titles.map((e) => e.id))
  const textHits = (text.data ?? []).filter((hit) => !titleIds.has(hit.entity.id))

  // The best result stays selected as results arrive, so Enter opens it rather than
  // "New note", which was first while the text search was still running.
  const firstValue = titles[0] ? `title:${titles[0].id}` : textHits[0] ? `text:${textHits[0].entity.id}` : '__new__'
  const [selected, setSelected] = useState(firstValue)
  const [selectedFor, setSelectedFor] = useState(firstValue)
  if (firstValue !== selectedFor) {
    setSelectedFor(firstValue)
    setSelected(firstValue)
  }

  const ITEM_CLASS =
    'flex cursor-default items-start gap-2.5 rounded-[9px] px-2.5 py-2 text-sm outline-none data-[selected=true]:bg-primary/15'
  const GROUP_CLASS =
    '[&_[cmdk-group-heading]]:px-2.5 [&_[cmdk-group-heading]]:pt-2 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:text-[0.7rem] [&_[cmdk-group-heading]]:font-bold [&_[cmdk-group-heading]]:tracking-widest [&_[cmdk-group-heading]]:text-muted-foreground [&_[cmdk-group-heading]]:uppercase'

  const renderTextStatus = () => {
    if (trimmed.length < MIN_TEXT_QUERY) return null
    if (text.isError) return <StatusLine>Couldn't search the text.</StatusLine>
    if (text.isFetching && textHits.length === 0) return <StatusLine>Searching…</StatusLine>
    if (textHits.length === 0) return <StatusLine>No other notes mention it.</StatusLine>
    return null
  }

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="ml-auto flex h-8 w-64 items-center gap-2 rounded-lg bg-muted px-2.5 text-sm text-muted-foreground hover:bg-accent hover:text-foreground"
      >
        <MagnifyingGlassIcon className="size-4" />
        Search
        <kbd className="ml-auto rounded border border-border px-1.5 font-sans text-[0.7rem] font-semibold">⌘K</kbd>
      </button>
      <Command.Dialog
        open={open}
        onOpenChange={handleOpenChange}
        label="Search"
        shouldFilter={false}
        value={selected}
        onValueChange={setSelected}
        overlayClassName="fixed inset-0 z-50 bg-black/60"
        contentClassName="menu-surface fixed top-[12vh] left-1/2 z-50 w-[620px] max-w-[calc(100vw-2rem)] -translate-x-1/2 overflow-hidden rounded-2xl"
      >
        <div className="flex h-14 items-center gap-3 border-b border-border px-4">
          <MagnifyingGlassIcon className="size-5 shrink-0 text-muted-foreground" />
          <Command.Input
            value={query}
            onValueChange={setQuery}
            placeholder="Search notes…"
            className="h-full flex-1 bg-transparent text-lg outline-none placeholder:text-muted-foreground"
          />
        </div>
        <Command.List className="max-h-[60vh] overflow-y-auto p-1.5">
          {titles.length > 0 && (
            <Command.Group heading="Titles" className={GROUP_CLASS}>
              {titles.map((entity) => (
                <Command.Item
                  key={entity.id}
                  value={`title:${entity.id}`}
                  onSelect={() => openEntity(entity.id)}
                  className={ITEM_CLASS}
                >
                  <TitleRow entity={entity} query={trimmed} />
                </Command.Item>
              ))}
            </Command.Group>
          )}
          {trimmed.length >= MIN_TEXT_QUERY && (
            <Command.Group heading="In the text" className={GROUP_CLASS}>
              {textHits.map((hit) => (
                <Command.Item
                  key={hit.entity.id}
                  value={`text:${hit.entity.id}`}
                  onSelect={() => openEntity(hit.entity.id)}
                  className={ITEM_CLASS}
                >
                  <TextRow hit={hit} />
                </Command.Item>
              ))}
              {renderTextStatus()}
            </Command.Group>
          )}
          {trimmed && (
            <Command.Item value="__new__" onSelect={() => void handleNewNote()} className={ITEM_CLASS}>
              <PlusIcon className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
              <span>New note &quot;{trimmed}&quot;</span>
            </Command.Item>
          )}
          {!trimmed && <StatusLine>Type to search titles and note text.</StatusLine>}
        </Command.List>
        <div className="flex gap-4 border-t border-border px-3.5 py-2 text-xs text-muted-foreground">
          <span>
            <Kbd>↑↓</Kbd> move
          </span>
          <span>
            <Kbd>⏎</Kbd> open
          </span>
          <span>
            <Kbd>esc</Kbd> close
          </span>
        </div>
      </Command.Dialog>
    </>
  )
}

interface TitleRowProps {
  entity: Entity
  query: string
}

function TitleRow({ entity, query }: TitleRowProps) {
  const { icon: Icon, color } = structurePresentation(entity.structureType)
  const structureName = useStructures().find((s) => s.type === entity.structureType)?.name ?? entity.structureType
  const start = entity.name.toLowerCase().indexOf(query.toLowerCase())
  const matches = start >= 0 ? [{ start, end: start + query.length }] : []

  return (
    <>
      <Icon weight="duotone" className="mt-0.5 size-4 shrink-0" style={{ color }} aria-hidden />
      <span className="min-w-0 flex-1 truncate font-semibold">
        <Highlighted text={entity.name} matches={matches} />
      </span>
      <span className="shrink-0 pt-0.5 text-xs text-muted-foreground">
        {structureName} · {formatDistanceToNow(entityUpdatedAtDate(entity), { addSuffix: true })}
      </span>
    </>
  )
}

interface TextRowProps {
  hit: TextHit
}

function TextRow({ hit }: TextRowProps) {
  const { icon: Icon, color } = structurePresentation(hit.entity.structureType)

  const renderSnippet = () =>
    hit.snippet ? (
      <span className="truncate text-[0.8125rem] text-muted-foreground">
        <Highlighted text={hit.snippet} matches={hit.matches} />
      </span>
    ) : (
      <span className="text-[0.8125rem] text-muted-foreground italic">Related by meaning</span>
    )

  return (
    <>
      <Icon weight="duotone" className="mt-0.5 size-4 shrink-0" style={{ color }} aria-hidden />
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate font-semibold">{hit.entity.name}</span>
        {renderSnippet()}
      </span>
    </>
  )
}

interface HighlightedProps {
  text: string
  // Half-open [start, end) ranges in UTF-16 code units, in order.
  matches: readonly Pick<MatchRange, 'start' | 'end'>[]
}

function Highlighted({ text, matches }: HighlightedProps) {
  const parts: React.ReactNode[] = []
  let at = 0
  matches.forEach(({ start, end }, i) => {
    if (start < at || end > text.length) return
    parts.push(text.slice(at, start))
    parts.push(
      <mark key={i} className="rounded-[3px] bg-primary/30 px-0.5 text-accent-foreground">
        {text.slice(start, end)}
      </mark>,
    )
    at = end
  })
  parts.push(text.slice(at))
  return <>{parts}</>
}

interface StatusLineProps {
  children: React.ReactNode
}

function StatusLine({ children }: StatusLineProps) {
  return <p className="px-2.5 py-2 text-sm text-muted-foreground">{children}</p>
}

interface KbdProps {
  children: React.ReactNode
}

function Kbd({ children }: KbdProps) {
  return (
    <kbd className="mr-1 rounded border border-border px-1.5 font-sans text-[0.7rem] font-semibold">{children}</kbd>
  )
}
