import { type ReactNode } from 'react';
import * as styles from '../styles/statusBar.css';

interface StatusSlot {
  id: string;
  content: ReactNode;
  position: 'left' | 'right';
  priority?: number;
}

interface Props {
  slots: StatusSlot[];
}

export function StatusBar({ slots }: Props) {
  const leftSlots = slots
    .filter((s) => s.position === 'left')
    .sort((a, b) => (a.priority ?? 0) - (b.priority ?? 0));

  const rightSlots = slots
    .filter((s) => s.position === 'right')
    .sort((a, b) => (a.priority ?? 0) - (b.priority ?? 0));

  return (
    <div className={styles.statusBar}>
      <div className={styles.statusBarLeft}>
        {leftSlots.map((slot) => (
          <div key={slot.id} className={styles.statusSlot}>
            {slot.content}
          </div>
        ))}
      </div>
      <div className={styles.statusBarRight}>
        {rightSlots.map((slot) => (
          <div key={slot.id} className={styles.statusSlot}>
            {slot.content}
          </div>
        ))}
      </div>
    </div>
  );
}
