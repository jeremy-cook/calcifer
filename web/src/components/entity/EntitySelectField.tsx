import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '~/components/ui/select'
import type { SelectOption } from '~/model/structures'

export interface EntitySelectFieldProps {
  label: string
  value: string
  options: readonly SelectOption[]
  onChange: (value: string) => void
}

export function EntitySelectField({ label, value, options, onChange }: EntitySelectFieldProps) {
  return (
    <div className="flex items-center gap-3 px-12 py-3">
      <span className="w-24 text-sm text-muted-foreground">{label}</span>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger size="sm" className="h-8">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {options.map((option) => (
            <SelectItem key={option.key} value={option.key}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}
