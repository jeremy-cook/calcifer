import { UserIcon } from '@phosphor-icons/react'

export function DateReferencesSection() {
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-base font-semibold">Date references</h2>
      <div className="flex flex-col gap-2 text-sm">
        <ExampleRow />
      </div>
    </section>
  )
}

function ExampleRow() {
  return (
    <div className="flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-muted">
      <span className="font-medium">Eric Rules</span>
      <span className="ml-auto flex items-center gap-1 rounded-md border border-border px-2 py-0.5 text-xs text-chart-5">
        <UserIcon className="size-3" /> Person
      </span>
    </div>
  )
}
