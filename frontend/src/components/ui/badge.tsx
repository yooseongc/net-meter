import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '@/lib/utils'

const badgeVariants = cva(
  'inline-flex items-center rounded-full px-2.5 py-0.5 text-[10px] font-bold uppercase tracking-wider transition-colors',
  {
    variants: {
      variant: {
        default:     'bg-primary text-primary-foreground',
        secondary:   'bg-secondary text-secondary-foreground border border-border',
        success:     'bg-success text-white',
        warning:     'bg-warning text-white',
        destructive: 'bg-destructive text-white',
        purple:      'bg-purple text-white',
        outline:     'border border-border text-foreground',
        // test type variants
        cps:  'bg-success text-white',
        cc:   'bg-primary text-primary-foreground',
        bw:   'bg-warning text-white',
      },
    },
    defaultVariants: { variant: 'default' },
  }
)

export interface BadgeProps
  extends React.HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeVariants> {}

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <span className={cn(badgeVariants({ variant }), className)} {...props} />
}
