import { style } from '@vanilla-extract/css';
import { vars } from './theme.css';

export const menuBar = style({
  display: 'flex',
  alignItems: 'center',
  height: '28px',
  background: vars.color.bgSecondary,
  borderBottom: `1px solid ${vars.color.bgTertiary}`,
  padding: `0 ${vars.space.sm}`,
  userSelect: 'none',
});

export const menuBarMenus = style({
  display: 'flex',
});

export const menuContainer = style({
  position: 'relative',
});

export const menuButton = style({
  background: 'none',
  border: 'none',
  color: vars.color.textSecondary,
  padding: `${vars.space.xs} ${vars.space.sm}`,
  fontSize: vars.font.sizeSm,
  cursor: 'pointer',
  borderRadius: vars.radius.sm,
  ':hover': {
    background: vars.color.bgTertiary,
    color: vars.color.textPrimary,
  },
});

export const menuButtonActive = style({
  background: vars.color.bgTertiary,
  color: vars.color.textPrimary,
});

export const menuDropdown = style({
  position: 'absolute',
  top: '100%',
  left: 0,
  minWidth: '200px',
  background: vars.color.bgSecondary,
  border: `1px solid ${vars.color.bgTertiary}`,
  borderRadius: vars.radius.md,
  boxShadow: '0 4px 12px rgba(0, 0, 0, 0.3)',
  padding: `${vars.space.xs} 0`,
  zIndex: 1000,
});

export const menuItem = style({
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
  width: '100%',
  background: 'none',
  border: 'none',
  color: vars.color.textPrimary,
  padding: `6px ${vars.space.md}`,
  fontSize: vars.font.sizeSm,
  textAlign: 'left',
  cursor: 'pointer',
  ':hover': {
    background: vars.color.accent,
  },
});

export const menuItemDisabled = style({
  color: vars.color.textSecondary,
  opacity: 0.5,
  cursor: 'default',
  ':hover': {
    background: 'none',
  },
});

export const menuItemLabel = style({});

export const menuItemShortcut = style({
  color: vars.color.textSecondary,
  fontSize: vars.font.sizeXs,
  marginLeft: vars.space.xl,
});

export const menuSeparator = style({
  height: '1px',
  background: vars.color.bgTertiary,
  margin: `${vars.space.xs} ${vars.space.sm}`,
});

export const menuBarSpacer = style({
  flex: 1,
});

export const projectSelector = style({
  background: 'none',
  border: 'none',
  color: vars.color.textPrimary,
  fontSize: vars.font.sizeSm,
  fontWeight: 500,
  padding: `${vars.space.xs} ${vars.space.sm}`,
  cursor: 'pointer',
  borderRadius: vars.radius.sm,
  ':hover': {
    background: vars.color.bgTertiary,
  },
});

export const dropdownArrow = style({
  fontSize: '8px',
  marginLeft: vars.space.xs,
  opacity: 0.7,
});
