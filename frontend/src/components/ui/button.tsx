import { cva, type VariantProps } from 'class-variance-authority'
import { Slot } from '@radix-ui/react-slot'
import { cn } from '@/lib/utils'

const buttonVariants = cva(
  'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-[var(--radius)] text-sm font-medium tracking-wide ring-offset-background transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:opacity-50 disabled:cursor-not-allowed',
  {
    variants: {
      variant: {
        default:     'bg-primary text-primary-foreground shadow-sm hover:opacity-90 active:scale-[0.98]',
        destructive: 'bg-destructive text-white shadow-sm hover:opacity-90 active:scale-[0.98]',
        secondary:   'bg-secondary text-secondary-foreground border border-border hover:bg-muted active:scale-[0.98]',
        ghost:       'hover:bg-muted hover:text-foreground',
        link:        'text-primary underline-offset-4 hover:underline',
        success:     'bg-success text-white shadow-sm hover:opacity-90 active:scale-[0.98]',
      },
      size: {
        default: 'h-9 px-5 py-2',
        sm:      'h-8 px-4 text-xs',
        xs:      'h-7 px-3 text-xs',
        lg:      'h-11 px-8 text-base',
        icon:    'h-9 w-9',
      },
    },
    defaultVariants: { variant: 'default', size: 'default' },
  }
)

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean
}

export function Button({ className, variant, size, asChild = false, ...props }: ButtonProps) {
  const Comp = asChild ? Slot : 'button'
  return <Comp className={cn(buttonVariants({ variant, size, className }))} {...props} />
}
