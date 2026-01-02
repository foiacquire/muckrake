import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { X, FolderOpen } from 'lucide-react';
import { FileBrowser } from './FileBrowser';
import * as styles from '../styles/settingsModal.css';

interface Props {
  open: boolean;
  onClose: () => void;
  onOpen: (path: string) => void;
}

export function OpenProjectModal({ open, onClose, onOpen }: Props) {
  const { t } = useTranslation();
  const [selectedPath, setSelectedPath] = useState('');

  useEffect(() => {
    if (open) {
      setSelectedPath('');
    }
  }, [open]);

  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    if (open) {
      document.addEventListener('keydown', handleEscape);
      return () => document.removeEventListener('keydown', handleEscape);
    }
  }, [open, onClose]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (selectedPath.trim()) {
      onOpen(selectedPath.trim());
    }
  };

  const handleSelect = (path: string, isDir: boolean) => {
    if (isDir) {
      setSelectedPath(path);
    }
  };

  const handleOverlayClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) onClose();
  };

  if (!open) return null;

  return (
    <div className={styles.overlay} onClick={handleOverlayClick} role="dialog" aria-modal="true">
      <div className={styles.modal} style={{ width: '700px', height: '500px' }}>
        <div className={styles.header}>
          <h2 className={styles.title}>
            <FolderOpen size={16} style={{ marginRight: '8px' }} />
            {t('openProject.title')}
          </h2>
          <button className={styles.closeButton} onClick={onClose} aria-label={t('settings.close')}>
            <X size={16} />
          </button>
        </div>

        <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 }}>
          <div className={styles.content} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
            <div className={styles.section}>
              <p className={styles.description}>{t('openProject.description')}</p>
            </div>

            <div className={styles.section} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
              <FileBrowser onSelect={handleSelect} selectMode="directory" />
            </div>
          </div>

          <div className={styles.footer}>
            <button type="button" className={styles.buttonSecondary} onClick={onClose}>
              {t('settings.cancel')}
            </button>
            <button type="submit" className={styles.buttonPrimary} disabled={!selectedPath.trim()}>
              {t('openProject.open')}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
