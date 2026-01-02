import { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { X, Upload, Monitor, Cloud } from 'lucide-react';
import { api } from '../api';
import { FileBrowser } from './FileBrowser';
import * as styles from '../styles/settingsModal.css';

interface Props {
  open: boolean;
  onClose: () => void;
  onOpen: (projects: { path: string; mode: string }[]) => void;
}

interface WorkspaceFile {
  version: number;
  name: string;
  projects: { path: string; mode: string }[];
}

type SourceMode = 'local' | 'remote';

export function OpenWorkspaceModal({ open, onClose, onOpen }: Props) {
  const { t } = useTranslation();
  const [allowRemote, setAllowRemote] = useState(false);
  const [sourceMode, setSourceMode] = useState<SourceMode>('local');
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [workspace, setWorkspace] = useState<WorkspaceFile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [workspacesDir, setWorkspacesDir] = useState<string | undefined>(undefined);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setSelectedFile(null);
      setWorkspace(null);
      setError(null);
      setLoading(false);
      setWorkspacesDir(undefined);

      api.config.get().then((config) => {
        setAllowRemote(config.allow_remote_workspaces);
        if (config.allow_remote_workspaces) {
          api.workspaces.dir().then((res) => {
            setWorkspacesDir(res.path);
          }).catch(() => {});
        }
      }).catch(() => {
        setAllowRemote(false);
      });
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

  const handleFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    setSelectedFile(file);
    setError(null);

    try {
      const text = await file.text();
      const parsed = JSON.parse(text) as WorkspaceFile;

      if (!parsed.version || !parsed.projects || !Array.isArray(parsed.projects)) {
        throw new Error('Invalid workspace file format');
      }

      setWorkspace(parsed);
    } catch {
      setError(t('openWorkspace.invalidFile'));
      setWorkspace(null);
    }
  };

  const handleRemoteSelect = async (path: string, isDir: boolean) => {
    if (isDir || !path.endsWith('.mkspc')) {
      setWorkspace(null);
      return;
    }

    setError(null);
    setLoading(true);

    try {
      const response = await fetch(`/api/workspaces/open`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path }),
      });
      if (!response.ok) {
        throw new Error('Failed to load workspace');
      }
      const data = await response.json();
      setWorkspace({ version: 1, name: data.name, projects: data.projects });
    } catch {
      setError(t('openWorkspace.invalidFile'));
      setWorkspace(null);
    } finally {
      setLoading(false);
    }
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (workspace) {
      onOpen(workspace.projects);
    }
  };

  const handleOverlayClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) onClose();
  };

  const handleBrowseClick = () => {
    fileInputRef.current?.click();
  };

  if (!open) return null;

  const isRemoteMode = allowRemote && sourceMode === 'remote';

  return (
    <div className={styles.overlay} onClick={handleOverlayClick} role="dialog" aria-modal="true">
      <div className={styles.modal} style={{ width: isRemoteMode ? '700px' : '420px', height: isRemoteMode ? '500px' : 'auto' }}>
        <div className={styles.header}>
          <h2 className={styles.title}>
            <Upload size={16} style={{ marginRight: '8px' }} />
            {t('openWorkspace.title')}
          </h2>
          <button className={styles.closeButton} onClick={onClose} aria-label={t('settings.close')}>
            <X size={16} />
          </button>
        </div>

        <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 }}>
          <div className={styles.content} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
            {allowRemote && (
              <div className={styles.section}>
                <div style={{ display: 'flex', gap: '8px' }}>
                  <button
                    type="button"
                    className={`${styles.buttonSecondary} ${sourceMode === 'local' ? styles.sidebarButtonActive : ''}`}
                    onClick={() => setSourceMode('local')}
                    style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', gap: '6px' }}
                  >
                    <Monitor size={14} />
                    {t('openWorkspace.local')}
                  </button>
                  <button
                    type="button"
                    className={`${styles.buttonSecondary} ${sourceMode === 'remote' ? styles.sidebarButtonActive : ''}`}
                    onClick={() => setSourceMode('remote')}
                    style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', gap: '6px' }}
                  >
                    <Cloud size={14} />
                    {t('openWorkspace.remote')}
                  </button>
                </div>
              </div>
            )}

            <div className={styles.section}>
              <p className={styles.description}>
                {isRemoteMode ? t('openWorkspace.descriptionRemote') : t('openWorkspace.description')}
              </p>
            </div>

            {isRemoteMode ? (
              <div className={styles.section} style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
                <FileBrowser onSelect={handleRemoteSelect} selectMode="file" initialPath={workspacesDir} />
              </div>
            ) : (
              <div className={styles.section}>
                <input
                  ref={fileInputRef}
                  type="file"
                  accept=".mkspc,application/json"
                  onChange={handleFileChange}
                  style={{ display: 'none' }}
                />
                <button
                  type="button"
                  className={styles.buttonSecondary}
                  onClick={handleBrowseClick}
                  style={{ width: '100%' }}
                >
                  {selectedFile ? selectedFile.name : t('openWorkspace.browse')}
                </button>
              </div>
            )}

            {error && (
              <div className={styles.section}>
                <p className={styles.hint} style={{ color: 'var(--color-error)' }}>{error}</p>
              </div>
            )}

            {loading && (
              <div className={styles.section}>
                <p className={styles.hint}>{t('fileBrowser.loading')}</p>
              </div>
            )}

            {workspace && !loading && (
              <div className={styles.section}>
                <p className={styles.hint}>
                  {t('openWorkspace.willOpen', { name: workspace.name, count: workspace.projects.length })}
                </p>
              </div>
            )}
          </div>

          <div className={styles.footer}>
            <button type="button" className={styles.buttonSecondary} onClick={onClose}>
              {t('settings.cancel')}
            </button>
            <button type="submit" className={styles.buttonPrimary} disabled={!workspace || loading}>
              {t('openWorkspace.open')}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
