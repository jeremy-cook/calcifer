import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { Button } from '~/components/ui/button'
import { COLOR_SWATCHES } from '../formattingOptions'

interface ColorPickerPopoverProps {
  color: string
  onChange: (color: string) => void
  onClear: () => void
  icon: React.ReactNode
  label: string
  disabled?: boolean
}

export function ColorPickerPopover({ color, onChange, onClear, icon, label, disabled }: ColorPickerPopoverProps) {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className="relative h-8 min-w-8 flex-col gap-0 px-1.5"
          aria-label={label}
          disabled={disabled}
        >
          <span className="pointer-events-none">{icon}</span>
          <span
            className="absolute bottom-1 left-1.5 right-1.5 h-0.5 rounded-full"
            style={{ backgroundColor: color || 'transparent' }}
          />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-auto p-2" align="start">
        <div className="grid grid-cols-8 gap-1">
          {COLOR_SWATCHES.map((swatch) => (
            <button
              key={swatch}
              className="size-5 rounded-sm ring-offset-1 transition-shadow hover:ring-2 hover:ring-ring focus:outline-none focus:ring-2 focus:ring-ring"
              style={{ backgroundColor: swatch }}
              aria-label={swatch}
              onClick={() => onChange(swatch)}
            />
          ))}
        </div>
        <div className="mt-2 flex items-center gap-2">
          <input
            type="color"
            value={color || '#000000'}
            onChange={(e) => onChange(e.target.value)}
            className="h-7 w-full cursor-pointer rounded border border-input"
            aria-label="Custom color"
          />
          <Button variant="ghost" size="sm" className="shrink-0" onClick={onClear}>
            None
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  )
}
