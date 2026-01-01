import { style } from '@vanilla-extract/css';
import { vars } from './theme.css';

export const container = style({
  position: 'relative',
});

export const iconTrigger = style({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  position: 'relative',
  width: '28px',
  height: '28px',
  background: 'transparent',
  border: 'none',
  borderRadius: vars.radius.sm,
  color: vars.color.textMuted,
  cursor: 'pointer',
  ':hover': {
    color: vars.color.textSecondary,
    background: vars.color.bgTertiary,
  },
  ':focus': {
    outline: 'none',
    color: vars.color.accent,
  },
});

export const iconTriggerActive = style({
  color: vars.color.accent,
});

export const badge = style({
  position: 'absolute',
  top: '2px',
  insetInlineEnd: '2px',
  minWidth: '14px',
  height: '14px',
  paddingInline: '3px',
  background: vars.color.accent,
  borderRadius: '7px',
  color: vars.color.bgPrimary,
  fontSize: '9px',
  fontWeight: 600,
  lineHeight: '14px',
  textAlign: 'center',
});

export const dropdown = style({
  position: 'absolute',
  top: '100%',
  insetInlineEnd: 0,
  width: '220px',
  marginBlockStart: vars.space.xs,
  background: vars.color.bgSecondary,
  border: `1px solid ${vars.color.bgTertiary}`,
  borderRadius: vars.radius.md,
  boxShadow: '0 4px 12px rgba(0, 0, 0, 0.3)',
  zIndex: 1000,
  maxHeight: '300px',
  display: 'flex',
  flexDirection: 'column',
});

export const searchInput = style({
  padding: vars.space.sm,
  background: vars.color.bgPrimary,
  border: 'none',
  borderBottom: `1px solid ${vars.color.bgTertiary}`,
  color: vars.color.textPrimary,
  fontSize: vars.font.sizeSm,
  ':focus': {
    outline: 'none',
  },
  '::placeholder': {
    color: vars.color.textMuted,
  },
});

export const optionsList = style({
  flex: 1,
  overflowY: 'auto',
  paddingBlock: vars.space.xs,
  paddingInline: 0,
  margin: 0,
  listStyle: 'none',
});

export const option = style({
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.sm,
  width: '100%',
  paddingBlock: vars.space.xs,
  paddingInline: vars.space.sm,
  background: 'none',
  border: 'none',
  color: vars.color.textPrimary,
  fontSize: vars.font.sizeSm,
  textAlign: 'start',
  cursor: 'pointer',
  ':hover': {
    background: vars.color.bgTertiary,
  },
});

export const optionSelected = style({
  background: vars.color.accent,
  ':hover': {
    background: vars.color.accentHover,
  },
});

export const noResults = style({
  padding: vars.space.md,
  color: vars.color.textMuted,
  fontSize: vars.font.sizeSm,
  textAlign: 'center',
  fontStyle: 'italic',
  listStyle: 'none',
});
