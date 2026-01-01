import { useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { MenuBar } from './components/MenuBar';
import { Sidebar } from './components/Sidebar';
import { StatusBar } from './components/StatusBar';
import { EntityGraph } from './components/Graph';
import { useEntities, useRelationships } from './hooks';
import * as styles from './styles/app.css';

function App() {
  const { t } = useTranslation();
  const { entities, loading, error } = useEntities();
  const [selectedId, setSelectedId] = useState<string>();

  const entityIds = useMemo(() => entities.map((e) => e.id), [entities]);
  const { relationships } = useRelationships(entityIds);

  const menus = useMemo(() => [
    {
      label: t('menu.file.label'),
      items: [
        { label: t('menu.file.newProject'), shortcut: 'Ctrl+N' },
        { label: t('menu.file.openProject'), shortcut: 'Ctrl+O' },
        { separator: true as const },
        { label: t('menu.file.save'), shortcut: 'Ctrl+S', disabled: true },
        { label: t('menu.file.export'), shortcut: 'Ctrl+Shift+E' },
        { separator: true as const },
        { label: t('menu.file.exit'), shortcut: 'Alt+F4' },
      ],
    },
    {
      label: t('menu.edit.label'),
      items: [
        { label: t('menu.edit.undo'), shortcut: 'Ctrl+Z', disabled: true },
        { label: t('menu.edit.redo'), shortcut: 'Ctrl+Y', disabled: true },
        { separator: true as const },
        { label: t('menu.edit.addEntity'), shortcut: 'Ctrl+E' },
        { label: t('menu.edit.addRelationship'), shortcut: 'Ctrl+R' },
        { separator: true as const },
        { label: t('menu.edit.find'), shortcut: 'Ctrl+F' },
      ],
    },
    {
      label: t('menu.view.label'),
      items: [
        { label: t('menu.view.zoomIn'), shortcut: 'Ctrl+=' },
        { label: t('menu.view.zoomOut'), shortcut: 'Ctrl+-' },
        { label: t('menu.view.fitToScreen'), shortcut: 'Ctrl+0' },
        { separator: true as const },
        { label: t('menu.view.showGrid') },
        { label: t('menu.view.showLabels') },
      ],
    },
    {
      label: t('menu.help.label'),
      items: [
        { label: t('menu.help.documentation') },
        { label: t('menu.help.keyboardShortcuts'), shortcut: 'Ctrl+?' },
        { separator: true as const },
        { label: t('menu.help.about') },
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
        <Sidebar
          entities={entities}
          selectedId={selectedId}
          onSelect={handleSelect}
        />
        <main className={styles.mainContent}>
          <EntityGraph
            entities={entities}
            relationships={relationships}
            onNodeClick={(id) => setSelectedId(id)}
          />
        </main>
      </div>
      <StatusBar slots={statusSlots} />
    </div>
  );
}

export default App;
