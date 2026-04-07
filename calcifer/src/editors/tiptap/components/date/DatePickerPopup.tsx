import { useState } from 'react'
import { Calendar } from '~/components/ui/calendar'
import { Button } from '~/components/ui/button'

interface DatePickerPopupProps {
  onSelect: (date: string) => void
  onClose: () => void
  initialDate?: string
}

export function DatePickerPopup({ onSelect, onClose, initialDate }: DatePickerPopupProps) {
  const [selected, setSelected] = useState<Date | undefined>(() => {
    if (!initialDate) return new Date()
    const [y, m, d] = initialDate.split('-').map(Number)
    return new Date(y, m - 1, d)
  })

  function confirm() {
    if (!selected) return onClose()
    const iso = [
      selected.getFullYear(),
      String(selected.getMonth() + 1).padStart(2, '0'),
      String(selected.getDate()).padStart(2, '0'),
    ].join('-')
    onSelect(iso)
  }

  return (
    <div
      className="date-picker-popup"
      onKeyDown={(e) => {
        e.stopPropagation()
        if (e.key === 'Escape') onClose()
      }}
    >
      <Calendar mode="single" selected={selected} onSelect={setSelected} />
      <div className="date-picker-popup-footer">
        <Button variant="ghost" size="sm" onClick={onClose}>
          Cancel
        </Button>
        <Button size="sm" onClick={confirm}>
          Insert
        </Button>
      </div>
    </div>
  )
}
