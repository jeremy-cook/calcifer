import { DayHeading } from './DayHeading'
import { DailyNoteSection } from './DailyNoteSection'
import { DateReferencesSection } from './DateReferencesSection'
import { CreatedAt } from './CreatedAt'

interface DayViewProps {
  iso: string
}

export function DayView({ iso }: DayViewProps) {
  return (
    <div className="flex flex-col gap-6 overflow-y-auto px-12 py-8">
      <DayHeading iso={iso} />
      <Divider />
      <DailyNoteSection />
      <Divider />
      <DateReferencesSection />
      <Divider />
      <CreatedAt />
    </div>
  )
}

function Divider() {
  return <hr className="border-border" />
}
