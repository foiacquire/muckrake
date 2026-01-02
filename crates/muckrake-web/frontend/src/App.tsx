import { useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { MenuBar } from './components/MenuBar';
import { Sidebar } from './components/Sidebar';
import { StatusBar } from './components/StatusBar';
import { EntityGraph } from './components/Graph';
import { SettingsModal } from './components/SettingsModal';
import { OpenProjectModal } from './components/OpenProjectModal';
import { OpenWorkspaceModal } from './components/OpenWorkspaceModal';
import { SaveWorkspaceModal } from './components/SaveWorkspaceModal';
import { useEntities, useRelationships } from './hooks';
import * as styles from './styles/app.css';

function App() {
  const { t } = useTranslation();
  const { entities, loading, error } = useEntities();
  const [selectedId, setSelectedId] = useState<string>();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [openProjectOpen, setOpenProjectOpen] = useState(false);
  const [openWorkspaceOpen, setOpenWorkspaceOpen] = useState(false);
  const [saveWorkspaceOpen, setSaveWorkspaceOpen] = useState(false);

  const entityIds = useMemo(() => entities.map((e) => e.id), [entities]);
  const { relationships } = useRelationships(entityIds);

  const handleOpenProject = async (path: string) => {
    try {
      const response = await fetch('/api/session/project/open', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path }),
      });
      if (!response.ok) {
        const error = await response.text();
        console.error('Open project failed:', error);
      }
    } catch (err) {
      console.error('Open project failed:', err);
    }
    setOpenProjectOpen(false);
  };

  const handleOpenWorkspace = async (projects: { path: string; mode: string }[]) => {
    // Open each project in the workspace
    for (const project of projects) {
      try {
        const response = await fetch('/api/session/project/open', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ path: project.path }),
        });
        if (!response.ok) {
          const error = await response.text();
          console.error(`Failed to open project ${project.path}:`, error);
        }
      } catch (err) {
        console.error(`Failed to open project ${project.path}:`, err);
      }
    }
    setOpenWorkspaceOpen(false);
  };

  const menus = useMemo(() => [
    {
      label: t('menu.file.label'),
      items: [
        { label: t('menu.file.openProject'), shortcut: 'Ctrl+O', action: () => setOpenProjectOpen(true) },
        { label: t('menu.file.openWorkspace'), shortcut: 'Ctrl+Shift+O', action: () => setOpenWorkspaceOpen(true) },
        { separator: true as const },
        { label: t('menu.file.saveWorkspace'), shortcut: 'Ctrl+S', action: () => setSaveWorkspaceOpen(true) },
        { separator: true as const },
        { label: t('menu.file.settings'), shortcut: 'Ctrl+,', action: () => setSettingsOpen(true) },
      ],
    },
  ], [t]);

  const handleSelect = (id: string) => {
    setSelectedId(id);
  };

  const statusSlots = useMemo(() => [
    {
      id: 'stats',
      position: 'left' as const,
      content: (
        <>
          {t('statusBar.entities', { count: entities.length })} · {t('statusBar.relationships', { count: relationships.length })}
        </>
      ),
    },
    {
      id: 'zoom',
      position: 'right' as const,
      content: <>100%</>,
    },
  ], [t, entities.length, relationships.length]);

  if (loading) {
    return (
      <div className={styles.app}>
        <MenuBar menus={menus} workspaceName={t('app.loading')} />
        <div className={styles.appBody}>
          <div className={styles.loadingState}>{t('app.loading')}</div>
        </div>
        <StatusBar slots={[]} />
      </div>
    );
  }

  if (error) {
    return (
      <div className={styles.app}>
        <MenuBar menus={menus} workspaceName={t('app.unknownWorkspace')} />
        <div className={styles.appBody}>
          <div className={styles.errorState}>
            <p>{t('app.error.loadFailed', { message: error.message })}</p>
          </div>
        </div>
        <StatusBar slots={[]} />
      </div>
    );
  }

  return (
    <div className={styles.app}>
      <MenuBar menus={menus} workspaceName={t('app.unknownWorkspace')} />
      <div className={styles.appBody}>
        <main className={styles.mainContent}>
          <EntityGraph
            entities={entities}
            relationships={relationships}
            onNodeClick={(id) => setSelectedId(id)}
          />
        </main>
        <Sidebar
          entities={entities}
          selectedId={selectedId}
          onSelect={handleSelect}
        />
      </div>
      <StatusBar slots={statusSlots} />
      <SettingsModal open={settingsOpen} onClose={() => setSettingsOpen(false)} />
      <OpenProjectModal
        open={openProjectOpen}
        onClose={() => setOpenProjectOpen(false)}
        onOpen={handleOpenProject}
      />
      <OpenWorkspaceModal
        open={openWorkspaceOpen}
        onClose={() => setOpenWorkspaceOpen(false)}
        onOpen={handleOpenWorkspace}
      />
      <SaveWorkspaceModal
        open={saveWorkspaceOpen}
        onClose={() => setSaveWorkspaceOpen(false)}
      />
    </div>
  );
}

export default App;
