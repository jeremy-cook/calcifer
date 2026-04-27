import { ArrowsOutSimpleIcon, DotsThreeIcon, TagIcon } from '@phosphor-icons/react'

export function DailyNoteSection() {
  return (
    <section className="flex flex-col gap-3">
      <header className="flex items-center justify-between">
        <h2 className="text-base font-semibold">Daily note</h2>
        <div className="flex items-center gap-1 text-muted-foreground">
          <button type="button" aria-label="Expand" className="rounded p-1 hover:bg-muted hover:text-foreground">
            <ArrowsOutSimpleIcon className="size-4" />
          </button>
          <button type="button" aria-label="More" className="rounded p-1 hover:bg-muted hover:text-foreground">
            <DotsThreeIcon className="size-4" />
          </button>
        </div>
      </header>
      <div className="flex flex-col gap-2 text-muted-foreground">
        <button
          type="button"
          className="flex items-center gap-2 self-start rounded-md px-2 py-1 text-sm hover:bg-muted"
        >
          <TagIcon className="size-4" /> Tags
        </button>
        <p className="px-2 text-sm">Text</p>
      </div>
    </section>
  )
}
