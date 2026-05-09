import { useState } from 'react'
import { CalendarIcon } from '@phosphor-icons/react'
import { Button } from '~/components/ui/button'
import { Calendar } from '~/components/ui/calendar'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { dateFromIso, formatLongDate, isoFromDate } from '~/model/dates'

export interface EntityDateFieldProps {
  label: string
  iso: string
  onChange: (newIso: string) => boolean
  collisionMessage?: string
}

export function EntityDateField({
  label,
  iso,
  onChange,
  collisionMessage = 'A daily note already exists for that day.',
}: EntityDateFieldProps) {
  const [open, setOpen] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const selected = dateFromIso(iso)

  const handleSelect = (date: Date | undefined) => {
    if (!date) return
    const next = isoFromDate(date)
    if (next === iso) {
      setOpen(false)
      return
    }
    const ok = onChange(next)
    if (!ok) {
      setError(collisionMessage)
      return
    }
    setError(null)
    setOpen(false)
  }

  const handleOpenChange = (next: boolean) => {
    setOpen(next)
    if (!next) setError(null)
  }

  return (
    <div className="flex items-center gap-3 px-12 py-3">
      <span className="w-24 text-sm text-muted-foreground">{label}</span>
      <Popover open={open} onOpenChange={handleOpenChange}>
        <PopoverTrigger asChild>
          <Button variant="ghost" size="sm" className="h-8 gap-2 px-2 font-normal">
            <CalendarIcon className="size-4 text-muted-foreground" />
            {formatLongDate(iso)}
          </Button>
        </PopoverTrigger>
        <PopoverContent align="start" className="w-auto p-2">
          <Calendar
            mode="single"
            selected={selected}
            onSelect={handleSelect}
            month={selected}
            weekStartsOn={1}
          />
          {error && <div className="px-2 pb-1 text-sm text-destructive">{error}</div>}
        </PopoverContent>
      </Popover>
    </div>
  )
}
