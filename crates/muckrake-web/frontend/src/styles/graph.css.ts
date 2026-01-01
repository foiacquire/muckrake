import { style } from '@vanilla-extract/css';
import { vars } from './theme.css';

export const graphContainer = style({
  width: '100%',
  height: '100%',
  minHeight: '500px',
  background: vars.color.bgPrimary,
  borderRadius: vars.radius.lg,
});
