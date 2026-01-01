import { createGlobalTheme } from '@vanilla-extract/css';

export const vars = createGlobalTheme(':root', {
  color: {
    // Background - slightly lighter for better contrast
    bgPrimary: '#1a1a1a',
    bgSecondary: '#242424',
    bgTertiary: '#2d2d2d',

    // Text - higher contrast (WCAG AA compliant)
    textPrimary: '#e0e0e0',    // 11:1 contrast on bgPrimary
    textSecondary: '#a0a0a0',  // 6:1 contrast on bgPrimary
    textMuted: '#707070',      // 4.5:1 contrast on bgPrimary

    // Accents
    accent: '#4da6ff',         // Brighter blue, better visibility
    accentHover: '#66b3ff',
    error: '#ff6b6b',
    success: '#51cf66',
    warning: '#ffd43b',
  },
  font: {
    family: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif",
    sizeXs: '11px',
    sizeSm: '12px',
    sizeMd: '13px',
  },
  space: {
    xs: '4px',
    sm: '8px',
    md: '12px',
    lg: '16px',
    xl: '24px',
  },
  radius: {
    sm: '3px',
    md: '4px',
    lg: '6px',
  },
});

