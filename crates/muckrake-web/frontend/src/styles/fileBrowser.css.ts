import { style } from '@vanilla-extract/css';
import { vars } from './theme.css';

export const container = style({
  display: 'flex',
  flexDirection: 'column',
  gap: vars.space.sm,
  height: '100%',
});

export const columnsWrapper = style({
  display: 'flex',
  flex: 1,
  overflow: 'hidden',
  border: `1px solid ${vars.color.bgTertiary}`,
  borderRadius: vars.radius.sm,
  backgroundColor: vars.color.bgPrimary,
  minHeight: '300px',
});

export const columnsScroller = style({
  display: 'flex',
  flex: 1,
  overflowX: 'auto',
  overflowY: 'hidden',
});

export const column = style({
  display: 'flex',
  flexDirection: 'column',
  width: '180px',
  minWidth: '180px',
  flexShrink: 0,
  overflowY: 'auto',
  borderRight: `1px solid ${vars.color.bgTertiary}`,
  ':last-child': {
    borderRight: 'none',
  },
});

export const columnLoading = style({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  padding: vars.space.md,
  color: vars.color.textMuted,
  fontSize: vars.font.sizeSm,
});

export const columnEmpty = style({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  padding: vars.space.md,
  color: vars.color.textMuted,
  fontSize: vars.font.sizeSm,
  fontStyle: 'italic',
});

export const entry = style({
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.xs,
  padding: `3px ${vars.space.sm}`,
  cursor: 'pointer',
  fontSize: vars.font.sizeSm,
  color: vars.color.textPrimary,
  userSelect: 'none',
  ':hover': {
    backgroundColor: vars.color.bgTertiary,
  },
});

export const entrySelected = style({
  backgroundColor: vars.color.accent,
  color: '#000',
  ':hover': {
    backgroundColor: vars.color.accentHover,
  },
});

export const entryHidden = style({
  opacity: 0.5,
});

export const entryIcon = style({
  flexShrink: 0,
  color: 'inherit',
});

export const entryName = style({
  flex: 1,
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
});

export const entryChevron = style({
  flexShrink: 0,
  color: 'inherit',
  marginLeft: 'auto',
});

export const pathBar = style({
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.xs,
});

export const pathInput = style({
  flex: 1,
  padding: `3px ${vars.space.sm}`,
  backgroundColor: vars.color.bgPrimary,
  border: `1px solid ${vars.color.bgTertiary}`,
  borderRadius: vars.radius.sm,
  color: vars.color.textPrimary,
  fontSize: vars.font.sizeSm,
  ':focus': {
    outline: 'none',
    borderColor: vars.color.accent,
  },
  '::placeholder': {
    color: vars.color.textMuted,
  },
});

export const pathButton = style({
  padding: `3px ${vars.space.sm}`,
  backgroundColor: vars.color.bgTertiary,
  border: 'none',
  borderRadius: vars.radius.sm,
  color: vars.color.textSecondary,
  fontSize: vars.font.sizeSm,
  cursor: 'pointer',
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.xs,
  ':hover': {
    backgroundColor: vars.color.bgSecondary,
    color: vars.color.textPrimary,
  },
});

export const toggleHidden = style({
  padding: `3px ${vars.space.sm}`,
  backgroundColor: 'transparent',
  border: `1px solid ${vars.color.bgTertiary}`,
  borderRadius: vars.radius.sm,
  color: vars.color.textSecondary,
  fontSize: vars.font.sizeSm,
  cursor: 'pointer',
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.xs,
  ':hover': {
    backgroundColor: vars.color.bgTertiary,
    color: vars.color.textPrimary,
  },
});

export const toggleHiddenActive = style({
  backgroundColor: vars.color.bgTertiary,
  color: vars.color.textPrimary,
});
