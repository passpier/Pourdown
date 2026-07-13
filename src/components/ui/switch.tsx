import * as React from "react"
import { cn } from "@/lib/utils"

export interface SwitchProps {
  checked: boolean
  onCheckedChange: (checked: boolean) => void
  disabled?: boolean
  id?: string
  "aria-label"?: string
}

/**
 * Minimal controlled toggle switch. No Radix/headless-ui dependency in this
 * project (see components/ui) — a native <button role="switch"> is
 * sufficient and keeps behavior keyboard- and screen-reader-accessible
 * without pulling in a new package.
 */
const Switch = React.forwardRef<HTMLButtonElement, SwitchProps>(
  ({ checked, onCheckedChange, disabled, id, className, ...props }: SwitchProps & { className?: string }, ref) => {
    return (
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        id={id}
        ref={ref}
        disabled={disabled}
        onClick={() => onCheckedChange(!checked)}
        className={cn(
          "relative inline-flex h-5 w-9 flex-shrink-0 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
          className
        )}
        style={{
          // The app's Tailwind base tokens (bg-primary/bg-input/ring) are
          // never populated by applyTheme() — only a specific subset of
          // shadcn vars is wired (see theme/utils.ts). Reuse the
          // theme-provided checkbox tokens instead, since a switch is the
          // same on/off semantics and every ThemeDefinition already
          // supplies a contrast-checked pair for it.
          backgroundColor: checked ? 'hsl(var(--checkbox-checked))' : 'hsl(var(--checkbox-unchecked))',
        }}
        {...props}
      >
        <span
          className={cn(
            "inline-block h-3.5 w-3.5 transform rounded-full shadow transition-transform",
            checked ? "translate-x-[18px]" : "translate-x-1"
          )}
          style={{ backgroundColor: 'hsl(var(--bg-primary))' }}
        />
      </button>
    )
  }
)
Switch.displayName = "Switch"

export { Switch }
