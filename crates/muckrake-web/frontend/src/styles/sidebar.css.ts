import { style } from '@vanilla-extract/css';
import { vars } from './theme.css';

export const sidebar = style({
  width: '240px',
  minWidth: '200px',
  background: vars.color.bgSecondary,
  borderInlineEnd: `1px solid ${vars.color.bgTertiary}`,
  display: 'flex',
  flexDirection: 'column',
  overflow: 'hidden',
});

export const sidebarContent = style({
  flex: 1,
  overflowY: 'auto',
  overflowX: 'hidden',
});

export const sidebarSection = style({
  borderBottom: `1px solid ${vars.color.bgTertiary}`,
});

export const sectionHeader = style({
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.sm,
  width: '100%',
  paddingBlock: vars.space.sm,
  paddingInline: vars.space.md,
  fontSize: vars.font.sizeXs,
  fontWeight: 600,
  textTransform: 'uppercase',
  letterSpacing: '0.05em',
  color: vars.color.textMuted,
  background: 'transparent',
  border: 'none',
  cursor: 'pointer',
  userSelect: 'none',
  ':hover': {
    color: vars.color.textSecondary,
  },
});

export const sectionToggle = style({
  fontSize: '8px',
  transition: 'transform 0.15s ease',
});

export const sectionToggleCollapsed = style({
  transform: 'rotate(-90deg)',
});

export const sectionTitle = style({
  flex: 1,
});

export const sectionCount = style({
  fontSize: vars.font.sizeXs,
  color: vars.color.textMuted,
  fontWeight: 400,
});

export const sectionContent = style({
  paddingBlock: vars.space.xs,
  paddingInline: 0,
  margin: 0,
  listStyle: 'none',
});

export const sectionContentCollapsed = style({
  display: 'none',
});

export const entityDot = style({
  width: '8px',
  height: '8px',
  borderRadius: '50%',
  flexShrink: 0,
});

export const treeItemRow = style({
  display: 'flex',
  alignItems: 'center',
  width: '100%',
  paddingBlock: vars.space.xs,
  paddingInline: vars.space.md,
  background: 'none',
  border: 'none',
  color: vars.color.textPrimary,
  fontSize: vars.font.sizeSm,
  textAlign: 'start',
  cursor: 'pointer',
  gap: vars.space.sm,
  ':hover': {
    background: vars.color.bgTertiary,
  },
});

export const treeItemRowSelected = style({
  background: vars.color.accent,
  ':hover': {
    background: vars.color.accentHover,
  },
});

export const treeLabel = style({
  flex: 1,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const actionBar = style({
  display: 'flex',
  justifyContent: 'flex-end',
  padding: vars.space.sm,
  background: vars.color.bgPrimary,
  borderBottom: `1px solid ${vars.color.bgTertiary}`,
});

export const emptyState = style({
  paddingBlock: vars.space.sm,
  paddingInline: vars.space.md,
  fontSize: vars.font.sizeSm,
  color: vars.color.textMuted,
  fontStyle: 'italic',
  listStyle: 'none',
});
