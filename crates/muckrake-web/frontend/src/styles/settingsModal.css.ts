import { style, keyframes } from '@vanilla-extract/css';
import { vars } from './theme.css';

const fadeIn = keyframes({
  from: { opacity: 0 },
  to: { opacity: 1 },
});

const slideIn = keyframes({
  from: { opacity: 0, transform: 'translateY(-10px)' },
  to: { opacity: 1, transform: 'translateY(0)' },
});

export const overlay = style({
  position: 'fixed',
  inset: 0,
  backgroundColor: 'rgba(0, 0, 0, 0.6)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  zIndex: 2000,
  animation: `${fadeIn} 100ms ease-out`,
});

export const modal = style({
  backgroundColor: vars.color.bgSecondary,
  borderRadius: vars.radius.md,
  border: `1px solid ${vars.color.bgTertiary}`,
  boxShadow: '0 4px 24px rgba(0, 0, 0, 0.4)',
  width: '520px',
  maxWidth: '90vw',
  maxHeight: '85vh',
  display: 'flex',
  flexDirection: 'column',
  animation: `${slideIn} 150ms ease-out`,
});

export const header = style({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  padding: `${vars.space.sm} ${vars.space.md}`,
  borderBottom: `1px solid ${vars.color.bgTertiary}`,
});

export const title = style({
  margin: 0,
  fontSize: vars.font.sizeMd,
  fontWeight: 600,
  color: vars.color.textPrimary,
});

export const closeButton = style({
  background: 'none',
  border: 'none',
  color: vars.color.textSecondary,
  cursor: 'pointer',
  padding: '2px',
  borderRadius: vars.radius.sm,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  ':hover': {
    backgroundColor: vars.color.bgTertiary,
    color: vars.color.textPrimary,
  },
});

export const body = style({
  display: 'flex',
  flex: 1,
  overflow: 'hidden',
  minHeight: 0,
});

export const sidebar = style({
  width: '120px',
  flexShrink: 0,
  borderRight: `1px solid ${vars.color.bgTertiary}`,
  padding: vars.space.xs,
  display: 'flex',
  flexDirection: 'column',
  gap: '2px',
});

export const sidebarButton = style({
  background: 'none',
  border: 'none',
  color: vars.color.textSecondary,
  cursor: 'pointer',
  padding: `${vars.space.xs} ${vars.space.sm}`,
  borderRadius: vars.radius.sm,
  textAlign: 'left',
  fontSize: vars.font.sizeSm,
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.xs,
  ':hover': {
    backgroundColor: vars.color.bgTertiary,
    color: vars.color.textPrimary,
  },
});

export const sidebarButtonActive = style({
  backgroundColor: vars.color.bgTertiary,
  color: vars.color.accent,
});

export const content = style({
  flex: 1,
  padding: vars.space.md,
  overflowY: 'auto',
  minHeight: 0,
});

export const section = style({
  marginBottom: vars.space.md,
  ':last-child': {
    marginBottom: 0,
  },
});

export const sectionTitle = style({
  margin: 0,
  marginBottom: vars.space.sm,
  fontSize: vars.font.sizeSm,
  fontWeight: 600,
  color: vars.color.textSecondary,
  textTransform: 'uppercase',
  letterSpacing: '0.5px',
});

export const row = style({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  padding: `${vars.space.xs} 0`,
  gap: vars.space.md,
  minHeight: '28px',
});

export const rowStacked = style({
  display: 'flex',
  flexDirection: 'column',
  gap: vars.space.xs,
  padding: `${vars.space.xs} 0`,
});

export const label = style({
  fontSize: vars.font.sizeSm,
  color: vars.color.textPrimary,
  flexShrink: 0,
});

export const labelWithHint = style({
  display: 'flex',
  flexDirection: 'column',
  gap: '1px',
});

export const hint = style({
  fontSize: vars.font.sizeXs,
  color: vars.color.textMuted,
  fontWeight: 'normal',
});

export const description = style({
  fontSize: vars.font.sizeSm,
  color: vars.color.textSecondary,
  lineHeight: 1.5,
  margin: 0,
});

export const select = style({
  padding: `3px ${vars.space.sm}`,
  backgroundColor: vars.color.bgPrimary,
  border: `1px solid ${vars.color.bgTertiary}`,
  borderRadius: vars.radius.sm,
  color: vars.color.textPrimary,
  fontSize: vars.font.sizeSm,
  cursor: 'pointer',
  minWidth: '160px',
  ':focus': {
    outline: 'none',
    borderColor: vars.color.accent,
  },
});

export const input = style({
  padding: `3px ${vars.space.sm}`,
  backgroundColor: vars.color.bgPrimary,
  border: `1px solid ${vars.color.bgTertiary}`,
  borderRadius: vars.radius.sm,
  color: vars.color.textPrimary,
  fontSize: vars.font.sizeSm,
  boxSizing: 'border-box',
  ':focus': {
    outline: 'none',
    borderColor: vars.color.accent,
  },
  '::placeholder': {
    color: vars.color.textMuted,
  },
});

export const inputSmall = style([input, {
  width: '80px',
  textAlign: 'right',
}]);

export const checkbox = style({
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.xs,
  cursor: 'pointer',
  fontSize: vars.font.sizeSm,
  color: vars.color.textPrimary,
});

export const checkboxInput = style({
  width: '14px',
  height: '14px',
  accentColor: vars.color.accent,
  cursor: 'pointer',
  margin: 0,
});

export const warningBanner = style({
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.sm,
  padding: vars.space.sm,
  backgroundColor: 'rgba(255, 212, 59, 0.1)',
  border: `1px solid rgba(255, 212, 59, 0.3)`,
  borderRadius: vars.radius.sm,
  marginTop: vars.space.xs,
  fontSize: vars.font.sizeXs,
  color: vars.color.warning,
  lineHeight: 1.4,
});

export const tagList = style({
  display: 'flex',
  flexWrap: 'wrap',
  gap: vars.space.xs,
  marginTop: vars.space.xs,
});

export const tag = style({
  display: 'inline-flex',
  alignItems: 'center',
  gap: '3px',
  padding: `2px ${vars.space.sm}`,
  backgroundColor: vars.color.bgTertiary,
  borderRadius: vars.radius.sm,
  fontSize: vars.font.sizeXs,
  color: vars.color.textSecondary,
});

export const tagRemove = style({
  background: 'none',
  border: 'none',
  color: vars.color.textMuted,
  cursor: 'pointer',
  padding: 0,
  display: 'flex',
  alignItems: 'center',
  ':hover': {
    color: vars.color.error,
  },
});

export const inlineInput = style({
  display: 'flex',
  gap: vars.space.xs,
  alignItems: 'center',
});

export const footer = style({
  display: 'flex',
  justifyContent: 'flex-end',
  gap: vars.space.sm,
  padding: `${vars.space.sm} ${vars.space.md}`,
  borderTop: `1px solid ${vars.color.bgTertiary}`,
});

export const button = style({
  padding: `${vars.space.xs} ${vars.space.md}`,
  borderRadius: vars.radius.sm,
  fontSize: vars.font.sizeSm,
  fontWeight: 500,
  cursor: 'pointer',
  border: 'none',
});

export const buttonSecondary = style([button, {
  backgroundColor: vars.color.bgTertiary,
  color: vars.color.textPrimary,
  ':hover': {
    backgroundColor: vars.color.bgPrimary,
  },
}]);

export const buttonPrimary = style([button, {
  backgroundColor: vars.color.accent,
  color: '#000',
  ':hover': {
    backgroundColor: vars.color.accentHover,
  },
}]);

export const buttonSmall = style({
  padding: `3px ${vars.space.sm}`,
  borderRadius: vars.radius.sm,
  fontSize: vars.font.sizeXs,
  fontWeight: 500,
  cursor: 'pointer',
  border: 'none',
  backgroundColor: vars.color.bgTertiary,
  color: vars.color.textSecondary,
  ':hover': {
    backgroundColor: vars.color.bgPrimary,
    color: vars.color.textPrimary,
  },
  ':disabled': {
    opacity: 0.5,
    cursor: 'default',
  },
});

export const emptyState = style({
  padding: vars.space.lg,
  textAlign: 'center',
  color: vars.color.textMuted,
  fontSize: vars.font.sizeSm,
  fontStyle: 'italic',
});

export const searchContainer = style({
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.xs,
  flex: 1,
  maxWidth: '200px',
  margin: `0 ${vars.space.md}`,
  padding: `3px ${vars.space.sm}`,
  backgroundColor: vars.color.bgPrimary,
  border: `1px solid ${vars.color.bgTertiary}`,
  borderRadius: vars.radius.sm,
  ':focus-within': {
    borderColor: vars.color.accent,
  },
});

export const searchIcon = style({
  color: vars.color.textMuted,
  flexShrink: 0,
});

export const searchInput = style({
  flex: 1,
  border: 'none',
  background: 'none',
  color: vars.color.textPrimary,
  fontSize: vars.font.sizeSm,
  outline: 'none',
  minWidth: 0,
  '::placeholder': {
    color: vars.color.textMuted,
  },
});

export const searchClear = style({
  background: 'none',
  border: 'none',
  color: vars.color.textMuted,
  cursor: 'pointer',
  padding: '2px',
  display: 'flex',
  alignItems: 'center',
  borderRadius: vars.radius.sm,
  ':hover': {
    color: vars.color.textPrimary,
    backgroundColor: vars.color.bgTertiary,
  },
});

export const sidebarButtonDimmed = style({
  opacity: 0.4,
});

export const fallbackChain = style({
  display: 'flex',
  flexDirection: 'column',
  gap: '2px',
});

export const fallbackItem = style({
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.sm,
  padding: `${vars.space.xs} ${vars.space.sm}`,
  backgroundColor: vars.color.bgPrimary,
  borderRadius: vars.radius.sm,
  fontSize: vars.font.sizeSm,
});

export const fallbackItemDisabled = style({
  opacity: 0.5,
});

export const fallbackHandle = style({
  color: vars.color.textMuted,
  cursor: 'grab',
  display: 'flex',
  alignItems: 'center',
  ':hover': {
    color: vars.color.textSecondary,
  },
});

export const fallbackLabel = style({
  flex: 1,
  color: vars.color.textPrimary,
});

export const fallbackControls = style({
  display: 'flex',
  alignItems: 'center',
  gap: '2px',
});

export const fallbackButton = style({
  background: 'none',
  border: 'none',
  color: vars.color.textMuted,
  cursor: 'pointer',
  padding: '2px',
  display: 'flex',
  alignItems: 'center',
  borderRadius: vars.radius.sm,
  ':hover': {
    color: vars.color.textPrimary,
    backgroundColor: vars.color.bgTertiary,
  },
  ':disabled': {
    opacity: 0.3,
    cursor: 'default',
  },
});

export const warningOverlay = style({
  position: 'absolute',
  inset: 0,
  backgroundColor: 'rgba(0, 0, 0, 0.7)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  zIndex: 100,
  borderRadius: vars.radius.md,
});

export const warningDialog = style({
  backgroundColor: vars.color.bgSecondary,
  borderRadius: vars.radius.md,
  border: `1px solid ${vars.color.error}`,
  padding: vars.space.lg,
  maxWidth: '400px',
  margin: vars.space.md,
});

export const warningHeader = style({
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.sm,
  marginBottom: vars.space.md,
});

export const warningIcon = style({
  color: vars.color.error,
  flexShrink: 0,
});

export const warningTitle = style({
  margin: 0,
  fontSize: vars.font.sizeMd,
  fontWeight: 600,
  color: vars.color.error,
});

export const warningMessage = style({
  fontSize: vars.font.sizeSm,
  color: vars.color.textSecondary,
  lineHeight: 1.5,
  marginBottom: vars.space.lg,
});

export const warningActions = style({
  display: 'flex',
  justifyContent: 'flex-end',
  gap: vars.space.sm,
});

export const buttonDanger = style([button, {
  backgroundColor: vars.color.error,
  color: '#fff',
  ':hover': {
    backgroundColor: '#e55a5a',
  },
}]);
