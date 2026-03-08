import { cn } from '@/lib/utils'

export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {}

export function Input({ className, ...props }: InputProps) {
  return (
    <input
      className={cn(
        'flex h-9 w-full rounded-[var(--radius)] border border-border bg-input px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground',
        'transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:border-primary',
        'hover:border-muted-foreground/50',
        'disabled:opacity-50 disabled:cursor-not-allowed',
        className
      )}
      {...props}
    />
  )
}

export function NativeSelect({ className, ...props }: React.SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      className={cn(
        'flex h-9 w-full rounded-[var(--radius)] border border-border bg-input px-3 py-1.5 text-sm text-foreground',
        'transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:border-primary',
        'hover:border-muted-foreground/50',
        'disabled:opacity-50 disabled:cursor-not-allowed',
        className
      )}
      {...props}
    />
  )
}
