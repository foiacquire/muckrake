import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Folder, File, ChevronRight, Home, Eye, EyeOff } from 'lucide-react';
import { api } from '../api';
import type { FileEntry } from '../types';
import * as styles from '../styles/fileBrowser.css';

interface ColumnData {
  path: string;
  entries: FileEntry[];
  loading: boolean;
  selectedPath: string | null;
}

interface Props {
  onSelect?: (path: string, isDir: boolean) => void;
  selectMode?: 'directory' | 'file' | 'both';
  initialPath?: string;
}

export function FileBrowser({ onSelect, selectMode = 'both', initialPath }: Props) {
  const { t } = useTranslation();
  const [columns, setColumns] = useState<ColumnData[]>([]);
  const [pathInput, setPathInput] = useState('');
  const [showHidden, setShowHidden] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [initializing, setInitializing] = useState(true);
  const scrollerRef = useRef<HTMLDivElement>(null);

  const loadDirectory = useCallback(async (path: string): Promise<{ entries: FileEntry[]; parent: string | null }> => {
    try {
      const result = await api.files.list(path);
      return { entries: result.entries, parent: result.parent };
    } catch (err) {
      console.error('Failed to load directory:', path, err);
      return { entries: [], parent: null };
    }
  }, []);

  const initializeBrowser = useCallback(async (startPath?: string) => {
    setInitializing(true);
    setError(null);
    try {
      let path = startPath;
      if (!path) {
        const home = await api.files.home();
        path = home.path;
      }

      setPathInput(path);
      const { entries } = await loadDirectory(path);
      setColumns([{ path, entries, loading: false, selectedPath: null }]);
    } catch (err) {
      console.error('Failed to initialize browser:', err);
      setError(err instanceof Error ? err.message : 'Failed to load');
    } finally {
      setInitializing(false);
    }
  }, [loadDirectory]);

  useEffect(() => {
    initializeBrowser(initialPath);
  }, [initialPath, initializeBrowser]);

  const handleEntryClick = async (columnIndex: number, entry: FileEntry) => {
    const newColumns = columns.slice(0, columnIndex + 1);
    newColumns[columnIndex] = { ...newColumns[columnIndex], selectedPath: entry.path };

    if (entry.is_dir) {
      newColumns.push({ path: entry.path, entries: [], loading: true, selectedPath: null });
      setColumns(newColumns);
      setPathInput(entry.path);

      const { entries } = await loadDirectory(entry.path);
      setColumns((prev) => {
        const updated = [...prev];
        if (updated[columnIndex + 1]) {
          updated[columnIndex + 1] = { ...updated[columnIndex + 1], entries, loading: false };
        }
        return updated;
      });

      setTimeout(() => {
        if (scrollerRef.current) {
          scrollerRef.current.scrollLeft = scrollerRef.current.scrollWidth;
        }
      }, 0);

      if (selectMode === 'directory' || selectMode === 'both') {
        onSelect?.(entry.path, true);
      }
    } else {
      setColumns(newColumns);
      setPathInput(entry.path);
      if (selectMode === 'file' || selectMode === 'both') {
        onSelect?.(entry.path, false);
      }
    }
  };

  const handlePathGo = async () => {
    const path = pathInput.trim();
    if (!path) return;
    await initializeBrowser(path);
  };

  const handlePathKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      handlePathGo();
    }
  };

  const handleHome = async () => {
    const home = await api.files.home();
    await initializeBrowser(home.path);
  };

  const filterEntries = (entries: FileEntry[]): FileEntry[] => {
    if (showHidden) return entries;
    return entries.filter((e) => !e.is_hidden);
  };

  if (initializing) {
    return (
      <div className={styles.container}>
        <div className={styles.columnsWrapper}>
          <div className={styles.columnsScroller}>
            <div className={styles.column}>
              <div className={styles.columnLoading}>{t('fileBrowser.loading')}</div>
            </div>
          </div>
        </div>
        <div className={styles.pathBar}>
          <input
            type="text"
            className={styles.pathInput}
            value=""
            disabled
            placeholder="/path/to/directory"
          />
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className={styles.container}>
        <div className={styles.columnsWrapper}>
          <div className={styles.columnsScroller}>
            <div className={styles.column}>
              <div className={styles.columnEmpty}>{error}</div>
            </div>
          </div>
        </div>
        <div className={styles.pathBar}>
          <button
            type="button"
            className={styles.pathButton}
            onClick={handleHome}
            title={t('fileBrowser.home')}
          >
            <Home size={14} />
          </button>
          <input
            type="text"
            className={styles.pathInput}
            value={pathInput}
            onChange={(e) => setPathInput(e.target.value)}
            onKeyDown={handlePathKeyDown}
            placeholder="/path/to/directory"
          />
          <button type="button" className={styles.pathButton} onClick={handlePathGo}>
            {t('fileBrowser.go')}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className={styles.container}>
      <div className={styles.columnsWrapper}>
        <div className={styles.columnsScroller} ref={scrollerRef}>
          {columns.map((column, idx) => (
            <div key={column.path} className={styles.column}>
              {column.loading ? (
                <div className={styles.columnLoading}>{t('fileBrowser.loading')}</div>
              ) : filterEntries(column.entries).length === 0 ? (
                <div className={styles.columnEmpty}>{t('fileBrowser.empty')}</div>
              ) : (
                filterEntries(column.entries).map((entry) => {
                  const isSelected = column.selectedPath === entry.path;
                  const entryClasses = [
                    styles.entry,
                    isSelected && styles.entrySelected,
                    entry.is_hidden && styles.entryHidden,
                  ]
                    .filter(Boolean)
                    .join(' ');

                  return (
                    <div
                      key={entry.path}
                      className={entryClasses}
                      onClick={() => handleEntryClick(idx, entry)}
                    >
                      <span className={styles.entryIcon}>
                        {entry.is_dir ? <Folder size={14} /> : <File size={14} />}
                      </span>
                      <span className={styles.entryName}>{entry.name}</span>
                      {entry.is_dir && (
                        <span className={styles.entryChevron}>
                          <ChevronRight size={12} />
                        </span>
                      )}
                    </div>
                  );
                })
              )}
            </div>
          ))}
        </div>
      </div>

      <div className={styles.pathBar}>
        <button
          type="button"
          className={styles.pathButton}
          onClick={handleHome}
          title={t('fileBrowser.home')}
        >
          <Home size={14} />
        </button>
        <input
          type="text"
          className={styles.pathInput}
          value={pathInput}
          onChange={(e) => setPathInput(e.target.value)}
          onKeyDown={handlePathKeyDown}
          placeholder="/path/to/directory"
        />
        <button type="button" className={styles.pathButton} onClick={handlePathGo}>
          {t('fileBrowser.go')}
        </button>
        <button
          type="button"
          className={`${styles.toggleHidden} ${showHidden ? styles.toggleHiddenActive : ''}`}
          onClick={() => setShowHidden(!showHidden)}
          title={t('fileBrowser.showHidden')}
        >
          {showHidden ? <Eye size={14} /> : <EyeOff size={14} />}
        </button>
      </div>
    </div>
  );
}
