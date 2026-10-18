/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        // Design system — dark dev-tool palette
        bg: '#0e1016',
        surface: '#161922',
        'surface-2': '#1d212c',
        'surface-3': '#252a37',
        border: '#2b313f',
        'border-strong': '#3a4252',
        text: '#e7e9ee',
        muted: '#9aa2b1',
        faint: '#6b7385',
        primary: {
          DEFAULT: '#6366f1',
          hover: '#7c80f5',
          soft: 'rgba(99,102,241,0.14)',
        },
        accent: {
          DEFAULT: '#2dd4bf',
          soft: 'rgba(45,212,191,0.14)',
        },
        // task states
        todo: '#64748b',
        prod: '#f59e0b',
        review: '#a855f7',
        done: '#22c55e',
        danger: {
          DEFAULT: '#ef4444',
          soft: 'rgba(239,68,68,0.14)',
        },
        warn: '#eab308',
      },
      borderRadius: {
        xl: '14px',
        '2xl': '18px',
      },
      boxShadow: {
        card: '0 1px 2px rgba(0,0,0,0.4), 0 2px 8px rgba(0,0,0,0.25)',
        pop: '0 12px 40px rgba(0,0,0,0.5)',
        'edit-pending': '0 4px 0 -1px rgba(234,179,8,0.9), 0 8px 18px rgba(234,179,8,0.25)',
      },
      fontFamily: {
        sans: ['Inter', 'Segoe UI', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'Consolas', 'monospace'],
      },
      keyframes: {
        'fade-in': { '0%': { opacity: 0 }, '100%': { opacity: 1 } },
        'scale-in': {
          '0%': { opacity: 0, transform: 'translateY(6px) scale(0.98)' },
          '100%': { opacity: 1, transform: 'translateY(0) scale(1)' },
        },
        'slide-in': {
          '0%': { opacity: 0, transform: 'translateX(12px)' },
          '100%': { opacity: 1, transform: 'translateX(0)' },
        },
      },
      animation: {
        'fade-in': 'fade-in 0.18s ease-out',
        'scale-in': 'scale-in 0.16s cubic-bezier(0.16,1,0.3,1)',
        'slide-in': 'slide-in 0.2s ease-out',
      },
    },
  },
  plugins: [],
};
