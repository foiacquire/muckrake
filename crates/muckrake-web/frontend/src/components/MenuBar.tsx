import { useState, useRef, useEffect } from 'react';
import * as styles from '../styles/menuBar.css';

type MenuItem =
  | { separator: true }
  | {
      label: string;
      shortcut?: string;
      action?: () => void;
      disabled?: boolean;
    };

interface Menu {
  label: string;
  items: MenuItem[];
}

interface Props {
  menus: Menu[];
  workspaceName?: string;
  onWorkspaceClick?: () => void;
}

export function MenuBar({ menus, workspaceName, onWorkspaceClick }: Props) {
  const [openMenu, setOpenMenu] = useState<number | null>(null);
  const menuBarRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuBarRef.current && !menuBarRef.current.contains(e.target as Node)) {
        setOpenMenu(null);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const handleMenuClick = (index: number) => {
    setOpenMenu(openMenu === index ? null : index);
  };

  const handleItemClick = (item: Exclude<MenuItem, { separator: true }>) => {
    if (!item.disabled && item.action) {
      item.action();
    }
    setOpenMenu(null);
  };

  return (
    <div className={styles.menuBar} ref={menuBarRef}>
      <div className={styles.menuBarMenus}>
        {menus.map((menu, index) => (
          <div key={menu.label} className={styles.menuContainer}>
            <button
              className={`${styles.menuButton} ${openMenu === index ? styles.menuButtonActive : ''}`}
              onClick={() => handleMenuClick(index)}
              onMouseEnter={() => openMenu !== null && setOpenMenu(index)}
            >
              {menu.label}
            </button>
            {openMenu === index && (
              <div className={styles.menuDropdown}>
                {menu.items.map((item, itemIndex) =>
                  'separator' in item ? (
                    <div key={itemIndex} className={styles.menuSeparator} />
                  ) : (
                    <button
                      key={item.label}
                      className={`${styles.menuItem} ${item.disabled ? styles.menuItemDisabled : ''}`}
                      onClick={() => handleItemClick(item)}
                      disabled={item.disabled}
                    >
                      <span className={styles.menuItemLabel}>{item.label}</span>
                      {item.shortcut && (
                        <span className={styles.menuItemShortcut}>{item.shortcut}</span>
                      )}
                    </button>
                  )
                )}
              </div>
            )}
          </div>
        ))}
      </div>
      <div className={styles.menuBarSpacer} />
      {workspaceName && (
        <button className={styles.workspaceSelector} onClick={onWorkspaceClick}>
          {workspaceName} <span className={styles.dropdownArrow}>▼</span>
        </button>
      )}
    </div>
  );
}
