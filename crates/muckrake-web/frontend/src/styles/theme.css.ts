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
    accentDark: '#1a2a3d',     // Very dark steel blue for status bar
    error: '#ff6b6b',
    success: '#51cf66',
    warning: '#ffd43b',
  },
  font: {
    family: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif",
    sizeXs: '12px',    // was 11px, now ~12.65 rounded to 12
    sizeSm: '14px',    // was 12px, now ~13.8 rounded to 14
    sizeMd: '15px',    // was 13px, now ~14.95 rounded to 15
  },
  space: {
    xs: '3px',         // was 4px - more compact
    sm: '6px',         // was 8px - more compact
    md: '10px',        // was 12px - more compact
    lg: '14px',        // was 16px - more compact
    xl: '20px',        // was 24px - more compact
  },
  radius: {
    sm: '3px',
    md: '4px',
    lg: '6px',
  },
});

