import { useEffect, useState } from 'react'
import { Link } from '@tanstack/react-router'
import {
  CaretRightIcon,
  ListBulletsIcon,
  StackIcon,
} from '@phosphor-icons/react'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '~/components/ui/collapsible'
import { useBacklinks, type Backlink } from '~/model/backlinks'
import { structurePresentation, useStructure } from '~/model/structures'
import { cn } from '~/lib/utils'

interface BacklinksPanelProps {
  entityId: string
}

type ViewMode = 'grouped' | 'flat'

export function BacklinksPanel({ entityId }: BacklinksPanelProps) {
  const backlinks = useBacklinks(entityId)
  const [open, setOpen] = useState(false)
  const [mode, setMode] = useState<ViewMode>('grouped')

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && (e.key === 'B' || e.key === 'b')) {
        e.preventDefault()
        setOpen((o) => !o)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <BacklinksHeader
        count={backlinks.length}
        mode={mode}
        onToggleMode={() => setMode((m) => (m === 'grouped' ? 'flat' : 'grouped'))}
      />
      <CollapsibleContent className="pt-3">
        {open && <BacklinksList backlinks={backlinks} mode={mode} />}
      </CollapsibleContent>
    </Collapsible>
  )
}

interface BacklinksHeaderProps {
  count: number
  mode: ViewMode
  onToggleMode: () => void
}

function BacklinksHeader({ count, mode, onToggleMode }: BacklinksHeaderProps) {
  const ToggleIcon = mode === 'grouped' ? StackIcon : ListBulletsIcon
  const toggleLabel = mode === 'grouped' ? 'Switch to flat view' : 'Switch to grouped view'

  const handleToggleClick = (e: React.MouseEvent) => {
    e.stopPropagation()
    e.preventDefault()
    onToggleMode()
  }

  return (
    <div className="flex items-center gap-2">
      <CollapsibleTrigger className="group flex flex-1 items-center gap-2 text-left text-sm font-medium text-muted-foreground hover:text-foreground">
        <CaretRightIcon className="size-3.5 transition-transform group-data-[state=open]:rotate-90" />
        <span>Backlinks ({count})</span>
      </CollapsibleTrigger>
      <button
        type="button"
        onClick={handleToggleClick}
        className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
        aria-label={toggleLabel}
        title={toggleLabel}
      >
        <ToggleIcon className="size-4" />
      </button>
    </div>
  )
}

interface BacklinksListProps {
  backlinks: Backlink[]
  mode: ViewMode
}

function BacklinksList({ backlinks, mode }: BacklinksListProps) {
  if (backlinks.length === 0) {
    return <p className="text-sm text-muted-foreground">No backlinks.</p>
  }

  if (mode === 'flat') {
    return <FlatBacklinks backlinks={backlinks} />
  }

  return <GroupedBacklinks backlinks={backlinks} />
}

interface FlatBacklinksProps {
  backlinks: Backlink[]
}

function FlatBacklinks({ backlinks }: FlatBacklinksProps) {
  const sorted = [...backlinks].sort(
    (a, b) => b.mostRecentAt.getTime() - a.mostRecentAt.getTime(),
  )
  return (
    <ul className="flex flex-col gap-0.5">
      {sorted.map((bl) => (
        <BacklinkRow key={bl.entityId} backlink={bl} />
      ))}
    </ul>
  )
}

interface GroupedBacklinksProps {
  backlinks: Backlink[]
}

function GroupedBacklinks({ backlinks }: GroupedBacklinksProps) {
  const groups = groupBacklinksByStructure(backlinks)
  return (
    <div className="flex flex-col gap-4">
      {groups.map((group) => (
        <BacklinkSection key={group.structureType} group={group} />
      ))}
    </div>
  )
}

interface BacklinkGroup {
  structureType: string
  items: Backlink[]
}

function groupBacklinksByStructure(backlinks: Backlink[]): BacklinkGroup[] {
  const groups: BacklinkGroup[] = []
  for (const bl of backlinks) {
    const last = groups[groups.length - 1]
    if (last && last.structureType === bl.structureType) {
      last.items.push(bl)
    } else {
      groups.push({ structureType: bl.structureType, items: [bl] })
    }
  }
  return groups
}

interface BacklinkSectionProps {
  group: BacklinkGroup
}

function BacklinkSection({ group }: BacklinkSectionProps) {
  const plural = useStructure(group.structureType)?.plural ?? group.structureType
  const heading = `${plural} (${group.items.length})`

  return (
    <section className="flex flex-col gap-1">
      <h3 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
        {heading}
      </h3>
      <ul className="flex flex-col gap-0.5">
        {group.items.map((bl) => (
          <BacklinkRow key={bl.entityId} backlink={bl} />
        ))}
      </ul>
    </section>
  )
}

interface BacklinkRowProps {
  backlink: Backlink
}

function BacklinkRow({ backlink }: BacklinkRowProps) {
  const { icon: Icon, color } = structurePresentation(backlink.structureType)

  return (
    <li>
      <Link
        to="/e/$id"
        params={{ id: backlink.entityId }}
        className={cn(
          'flex items-center gap-2 rounded-md px-2 py-1 text-sm hover:bg-muted',
        )}
      >
        <Icon className="size-4 shrink-0" style={{ color }} aria-hidden />
        <span className="truncate">{backlink.name}</span>
      </Link>
    </li>
  )
}
