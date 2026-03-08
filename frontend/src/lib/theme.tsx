import { createContext, useContext, useEffect, useState } from 'react'

type Theme = 'light' | 'dark' | 'system'

interface ThemeContextValue {
  theme: Theme
  setTheme: (t: Theme) => void
  resolvedTheme: 'light' | 'dark'
}

const ThemeContext = createContext<ThemeContextValue | null>(null)

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(() => {
    const stored = localStorage.getItem('net-meter-theme') as Theme | null
    return stored ?? 'dark'
  })

  const resolvedTheme: 'light' | 'dark' =
    theme === 'system'
      ? (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')
      : theme

  useEffect(() => {
    const root = document.documentElement
    root.classList.remove('light', 'dark')
    root.classList.add(resolvedTheme)
  }, [resolvedTheme])

  const setTheme = (t: Theme) => {
    setThemeState(t)
    localStorage.setItem('net-meter-theme', t)
  }

  return (
    <ThemeContext.Provider value={{ theme, setTheme, resolvedTheme }}>
      {children}
    </ThemeContext.Provider>
  )
}

export function useTheme() {
  const ctx = useContext(ThemeContext)
  if (!ctx) throw new Error('useTheme must be used within ThemeProvider')
  return ctx
}

/** Returns chart-specific colors that adapt to the current theme */
export function useChartColors() {
  const { resolvedTheme } = useTheme()
  const dark = resolvedTheme === 'dark'
  return {
    grid:        dark ? '#21262d' : '#e2e8f0',
    axis:        dark ? '#8b949e' : '#656d76',
    tooltipBg:   dark ? '#161b22' : '#ffffff',
    tooltipBorder: dark ? '#30363d' : '#d0d7de',
    tooltipColor: dark ? '#e6edf3' : '#1f2328',
    chartBg:     dark ? '#161b22' : '#ffffff',
    chartBorder: dark ? '#30363d' : '#d0d7de',
  }
}
