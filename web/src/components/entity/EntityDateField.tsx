import { useState } from 'react'
import { CalendarBlankIcon, PlusIcon, XIcon } from '@phosphor-icons/react'
import { Calendar } from '~/components/ui/calendar'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { PILL_CLASS, PILL_CLEAR_CLASS, PILL_EMPTY_CLASS } from '~/components/entity/property-pill'
import { dateFromIso, formatLongDate, formatShortDate, isoFromDate } from '~/model/dates'

export interface EntityDateFieldProps {
  label: string
  // False: the pill shows the date alone, for a date that names the entity (a daily note's).
  showLabel?: boolean
  iso?: string
  onChange: (newIso: string) => void
  onClear?: () => void
  // Returns an error message to keep the picker open and show it, or null to accept.
  validate?: (newIso: string) => string | null
  // An error from outside the picker (e.g. the server rejected the change), shown beside the date.
  error?: string | null
}

export function EntityDateField({
  label,
  showLabel = true,
  iso,
  onChange,
  onClear,
  validate,
  error: externalError,
}: EntityDateFieldProps) {
  const [open, setOpen] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const selected = iso ? dateFromIso(iso) : undefined
  // The month the calendar shows; it opens on the selected date's.
  const [month, setMonth] = useState(() => selected ?? new Date())

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
    if (next) setMonth(selected ?? new Date())
    else setError(null)
  }

  const handleClear = (e: React.MouseEvent) => {
    e.stopPropagation()
    onClear?.()
  }

  const renderSetTrigger = () => (
    <span className={PILL_CLASS}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="inline-flex items-center gap-1.5"
          aria-label={`${label}: ${formatLongDate(iso!)}`}
        >
          <CalendarBlankIcon weight="duotone" className="text-muted-foreground" aria-hidden />
          {showLabel && <span className="font-medium text-muted-foreground">{label}</span>}
          {formatShortDate(iso!)}
        </button>
      </PopoverTrigger>
      {onClear && (
        <button type="button" className={PILL_CLEAR_CLASS} onClick={handleClear} aria-label="Clear date">
          <XIcon />
        </button>
      )}
    </span>
  )

  const renderEmptyTrigger = () => (
    <PopoverTrigger asChild>
      <button type="button" className={PILL_EMPTY_CLASS}>
        <PlusIcon />
        {label}
      </button>
    </PopoverTrigger>
  )

  return (
    <>
      <Popover open={open} onOpenChange={handleOpenChange}>
        {iso ? renderSetTrigger() : renderEmptyTrigger()}
        <PopoverContent align="start" className="w-auto p-2">
          <Calendar
            mode="single"
            selected={selected}
            onSelect={handleSelect}
            month={month}
            onMonthChange={setMonth}
            weekStartsOn={1}
          />
          {error && <div className="px-2 pb-1 text-sm text-destructive">{error}</div>}
        </PopoverContent>
      </Popover>
      {externalError && <span className="text-sm text-destructive">{externalError}</span>}
    </>
  )
}
