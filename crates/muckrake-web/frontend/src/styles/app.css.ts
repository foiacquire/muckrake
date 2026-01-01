import { style, globalStyle } from '@vanilla-extract/css';
import { vars } from './theme.css';

export const app = style({
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
  background: vars.color.bgPrimary,
  color: vars.color.textPrimary,
});

export const appBody = style({
  flex: 1,
  display: 'flex',
  minHeight: 0,
});

export const mainContent = style({
  flex: 1,
  minWidth: 0,
  background: vars.color.bgPrimary,
  display: 'flex',
});

globalStyle(`${mainContent} > svg`, {
  flex: 1,
});

export const loadingState = style({
  flex: 1,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  color: vars.color.textSecondary,
});

export const errorState = style({
  flex: 1,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  color: vars.color.error,
});
