import { style } from '@vanilla-extract/css';
import { vars } from './theme.css';

export const statusBar = style({
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
  height: '22px',
  background: vars.color.accentDark,
  padding: `0 ${vars.space.sm}`,
  fontSize: vars.font.sizeXs,
  color: 'white',
});

export const statusBarLeft = style({
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.md,
});

export const statusBarRight = style({
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.md,
});

export const statusSlot = style({
  display: 'flex',
  alignItems: 'center',
  gap: vars.space.xs,
});
