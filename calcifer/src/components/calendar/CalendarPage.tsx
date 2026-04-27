import { useState } from 'react'
import { todayIso } from '~/model/dates'
import { CalendarToolbar } from './CalendarToolbar'
import { DayView } from './DayView'
import { MiniCalendar } from './MiniCalendar'

export function CalendarPage() {
  const [iso, setIso] = useState(todayIso)

  return (
    <div className="flex h-full flex-col">
      <CalendarToolbar iso={iso} onChangeIso={setIso} />
      <div className="grid min-h-0 flex-1 grid-cols-[1fr_320px]">
        <DayView iso={iso} />
        <MiniCalendar iso={iso} onSelect={setIso} />
      </div>
    </div>
  )
}
