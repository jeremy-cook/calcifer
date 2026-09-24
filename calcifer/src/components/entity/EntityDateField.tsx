import { useState } from 'react'
import { CalendarIcon, XIcon } from '@phosphor-icons/react'
import { Button } from '~/components/ui/button'
import { Calendar } from '~/components/ui/calendar'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { dateFromIso, formatLongDate, isoFromDate } from '~/model/dates'

export interface EntityDateFieldProps {
  label: string
  iso?: string
  onChange: (newIso: string) => void
  onClear?: () => void
  // Returns an error message to keep the picker open and show it, or null to accept.
  validate?: (newIso: string) => string | null
}

export function EntityDateField({ label, iso, onChange, onClear, validate }: EntityDateFieldProps) {
  const [open, setOpen] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const selected = iso ? dateFromIso(iso) : undefined

  const handleSelect = (date: Date | undefined) => {
    if (!date) return
    const next = isoFromDate(date)
    if (next === iso) {
      setOpen(false)
      return
    }
    const validationError = validate?.(next) ?? null
    if (validationError) {
      setError(validationError)
      return
    }
    onChange(next)
    setError(null)
    setOpen(false)
  }

  const handleOpenChange = (next: boolean) => {
    setOpen(next)
    if (!next) setError(null)
  }

  const handleClear = (e: React.MouseEvent) => {
    e.stopPropagation()
    onClear?.()
  }

  return (
    <div className="flex items-center gap-3 px-12 py-3">
      <span className="w-24 text-sm text-muted-foreground">{label}</span>
      <Popover open={open} onOpenChange={handleOpenChange}>
        <PopoverTrigger asChild>
          <Button variant="ghost" size="sm" className="h-8 gap-2 px-2 font-normal">
            <CalendarIcon className="size-4 text-muted-foreground" />
            {iso ? formatLongDate(iso) : <span className="text-muted-foreground">Set date</span>}
          </Button>
        </PopoverTrigger>
        <PopoverContent align="start" className="w-auto p-2">
          <Calendar
            mode="single"
            selected={selected}
            onSelect={handleSelect}
            month={selected ?? new Date()}
            weekStartsOn={1}
          />
          {error && <div className="px-2 pb-1 text-sm text-destructive">{error}</div>}
        </PopoverContent>
      </Popover>
      {onClear && iso && (
        <Button variant="ghost" size="icon" className="size-6" onClick={handleClear} aria-label="Clear date">
          <XIcon className="size-3.5 text-muted-foreground" />
        </Button>
      )}
    </div>
  )
}
