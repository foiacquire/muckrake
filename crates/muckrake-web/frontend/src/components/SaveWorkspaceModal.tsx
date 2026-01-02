import { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { X, Download } from 'lucide-react';
import { api } from '../api';
import * as styles from '../styles/settingsModal.css';

interface Props {
  open: boolean;
  onClose: () => void;
}

interface WorkspaceFile {
  version: number;
  name: string;
  projects: { path: string; mode: string }[];
}

export function SaveWorkspaceModal({ open, onClose }: Props) {
  const { t } = useTranslation();
  const [name, setName] = useState('My Workspace');
  const [saving, setSaving] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setName('My Workspace');
      inputRef.current?.focus();
      inputRef.current?.select();
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

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;

    setSaving(true);
    try {
      // Get list of open projects
      const { projects } = await api.session.listProjects();

      // Create workspace file content
      const workspace: WorkspaceFile = {
        version: 1,
        name: name.trim(),
        projects: projects.map((p) => ({ path: p.path, mode: 'readwrite' })),
      };

      // Generate and download the file
      const blob = new Blob([JSON.stringify(workspace, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const safeName = name.trim().replace(/[^a-zA-Z0-9_-]/g, '_');

      const a = document.createElement('a');
      a.href = url;
      a.download = `${safeName}.mkspc`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);

      onClose();
    } catch (err) {
      console.error('Failed to save workspace:', err);
    } finally {
      setSaving(false);
    }
  };

  const handleOverlayClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) onClose();
  };

  const safeName = name.trim().replace(/[^a-zA-Z0-9_-]/g, '_');

  if (!open) return null;

  return (
    <div className={styles.overlay} onClick={handleOverlayClick} role="dialog" aria-modal="true">
      <div className={styles.modal} style={{ width: '420px' }}>
        <div className={styles.header}>
          <h2 className={styles.title}>
            <Download size={16} style={{ marginRight: '8px' }} />
            {t('saveWorkspace.title')}
          </h2>
          <button className={styles.closeButton} onClick={onClose} aria-label={t('settings.close')}>
            <X size={16} />
          </button>
        </div>

        <form onSubmit={handleSubmit}>
          <div className={styles.content}>
            <div className={styles.section}>
              <p className={styles.description}>{t('saveWorkspace.description')}</p>
            </div>
            <div className={styles.section}>
              <div className={styles.rowStacked}>
                <label className={styles.label}>{t('saveWorkspace.nameLabel')}</label>
                <input
                  ref={inputRef}
                  type="text"
                  className={styles.input}
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="My Workspace"
                  style={{ width: '100%' }}
                />
              </div>
              <p className={styles.hint}>
                {t('saveWorkspace.willDownload')}: <code>{safeName}.mkspc</code>
              </p>
            </div>
          </div>

          <div className={styles.footer}>
            <button type="button" className={styles.buttonSecondary} onClick={onClose}>
              {t('settings.cancel')}
            </button>
            <button type="submit" className={styles.buttonPrimary} disabled={!name.trim() || saving}>
              {saving ? t('app.loading') : t('saveWorkspace.download')}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
