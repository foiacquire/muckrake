import { useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Folder, File, ChevronDown, ChevronRight } from 'lucide-react';
import type { Entity, EntityType } from '../types';
import * as styles from '../styles/sidebar.css';
import { getEntityColor } from '../utils/colors';
import { FuzzySelect, type FuzzyOption } from './FuzzySelect';
import { useProjects } from '../hooks';

type SidebarTab = 'entities' | 'projects';

interface Props {
  entities: Entity[];
  selectedId?: string;
  onSelect?: (id: string) => void;
}

function formatTypeLabel(type: string): string {
  const formatted = type.replace(/_/g, ' ');
  return formatted.charAt(0).toUpperCase() + formatted.slice(1) + 's';
}

export function Sidebar({ entities, selectedId, onSelect }: Props) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<SidebarTab>('entities');
  const [collapsedSections, setCollapsedSections] = useState<Set<EntityType>>(new Set());
  const [filterEntityIds, setFilterEntityIds] = useState<string[]>([]);
  const [collapsedProjects, setCollapsedProjects] = useState<Set<string>>(new Set());
  const { projects } = useProjects();

  const toggleProject = (projectId: string) => {
    setCollapsedProjects((prev) => {
      const next = new Set(prev);
      if (next.has(projectId)) {
        next.delete(projectId);
      } else {
        next.add(projectId);
      }
      return next;
    });
  };

  const toggleSection = (type: EntityType) => {
    setCollapsedSections((prev) => {
      const next = new Set(prev);
      if (next.has(type)) {
        next.delete(type);
      } else {
        next.add(type);
      }
      return next;
    });
  };

  const { entityTypes, groupedEntities } = useMemo(() => {
    const groups: Record<EntityType, Entity[]> = {};
    const types: EntityType[] = [];

    for (const entity of entities) {
      const type = entity.type;
      if (!groups[type]) {
        groups[type] = [];
        types.push(type);
      }
      groups[type].push(entity);
    }

    types.sort();
    return { entityTypes: types, groupedEntities: groups };
  }, [entities]);

  const filteredGroups = useMemo(() => {
    if (filterEntityIds.length === 0) return groupedEntities;

    const filterSet = new Set(filterEntityIds);
    const result: Record<EntityType, Entity[]> = {};

    for (const type of entityTypes) {
      result[type] = groupedEntities[type].filter((e) => filterSet.has(e.id));
    }

    return result;
  }, [groupedEntities, entityTypes, filterEntityIds]);

  const searchOptions: FuzzyOption<string>[] = useMemo(
    () =>
      entities.map((e) => ({
        id: e.id,
        label: e.canonical_name,
        icon: (
          <span
            style={{
              width: 8,
              height: 8,
              borderRadius: '50%',
              backgroundColor: getEntityColor(e.type),
              display: 'inline-block',
            }}
          />
        ),
      })),
    [entities]
  );

  return (
    <aside className={styles.sidebar} role="complementary" aria-label={t('sidebar.filter.placeholder')}>
      <div className={styles.tabBar} role="tablist">
        <button
          role="tab"
          aria-selected={activeTab === 'entities'}
          className={`${styles.tab} ${activeTab === 'entities' ? styles.tabActive : ''}`}
          onClick={() => setActiveTab('entities')}
        >
          {t('sidebar.tabs.entities')}
        </button>
        <button
          role="tab"
          aria-selected={activeTab === 'projects'}
          className={`${styles.tab} ${activeTab === 'projects' ? styles.tabActive : ''}`}
          onClick={() => setActiveTab('projects')}
        >
          {t('sidebar.tabs.projects')}
        </button>
      </div>

      {activeTab === 'entities' && (
        <div className={styles.tabContent}>
          <div className={styles.actionBar} role="toolbar" aria-label={t('sidebar.filter.placeholder')}>
            <FuzzySelect
              options={searchOptions}
              value={filterEntityIds}
              onChange={setFilterEntityIds}
              placeholder={t('sidebar.filter.placeholder')}
              searchPlaceholder={t('sidebar.filter.searchPlaceholder')}
            />
          </div>
          <nav className={styles.sidebarContent} role="navigation" aria-label={t('sidebar.filter.placeholder')}>
            {entityTypes.map((type) => {
              const typeEntities = filteredGroups[type] ?? [];
              const allTypeEntities = groupedEntities[type] ?? [];
              const isCollapsed = collapsedSections.has(type);
              const sectionId = `section-${type}`;
              const typeLabel = formatTypeLabel(type);

              return (
                <div key={type} className={styles.sidebarSection} role="region" aria-labelledby={sectionId}>
                  <button
                    id={sectionId}
                    className={styles.sectionHeader}
                    onClick={() => toggleSection(type)}
                    aria-expanded={!isCollapsed}
                    aria-controls={`${sectionId}-content`}
                  >
                    <span
                      className={`${styles.sectionToggle} ${isCollapsed ? styles.sectionToggleCollapsed : ''}`}
                      aria-hidden="true"
                    >
                      ▼
                    </span>
                    <span className={styles.sectionTitle}>{typeLabel}</span>
                    <span className={styles.sectionCount} aria-label={`${allTypeEntities.length} ${typeLabel}`}>
                      {allTypeEntities.length}
                    </span>
                    <span
                      className={styles.entityDot}
                      style={{ backgroundColor: getEntityColor(type) }}
                      aria-hidden="true"
                    />
                  </button>
                  <ul
                    id={`${sectionId}-content`}
                    className={`${styles.sectionContent} ${isCollapsed ? styles.sectionContentCollapsed : ''}`}
                    role="listbox"
                    aria-label={typeLabel}
                  >
                    {typeEntities.length === 0 ? (
                      <li className={styles.emptyState} role="option" aria-disabled="true">
                        {filterEntityIds.length > 0 ? t('sidebar.emptyState.noMatches') : t('sidebar.emptyState.none')}
                      </li>
                    ) : (
                      typeEntities.map((entity) => (
                        <li key={entity.id} role="option" aria-selected={selectedId === entity.id}>
                          <button
                            className={`${styles.treeItemRow} ${selectedId === entity.id ? styles.treeItemRowSelected : ''}`}
                            onClick={() => onSelect?.(entity.id)}
                          >
                            <span className={styles.treeLabel}>{entity.canonical_name}</span>
                          </button>
                        </li>
                      ))
                    )}
                  </ul>
                </div>
              );
            })}
          </nav>
        </div>
      )}

      {activeTab === 'projects' && (
        <div className={styles.tabContent}>
          <div className={styles.sidebarContent}>
            {projects.length === 0 ? (
              <div className={styles.emptyState}>
                {t('sidebar.projects.noProjects')}
              </div>
            ) : (
              projects.map((project) => {
                const isCollapsed = collapsedProjects.has(project.id);
                return (
                  <div key={project.id} className={styles.sidebarSection}>
                    <button
                      className={styles.sectionHeader}
                      onClick={() => toggleProject(project.id)}
                      aria-expanded={!isCollapsed}
                    >
                      <span className={styles.sectionToggle} aria-hidden="true">
                        {isCollapsed ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
                      </span>
                      <Folder size={14} style={{ flexShrink: 0 }} />
                      <span className={styles.sectionTitle}>{project.name}</span>
                      <span className={styles.sectionCount}>{project.files.length}</span>
                    </button>
                    {!isCollapsed && (
                      <ul className={styles.sectionContent}>
                        {project.files.length === 0 ? (
                          <li className={styles.emptyState}>
                            {t('sidebar.projects.noFiles')}
                          </li>
                        ) : (
                          project.files.map((file) => (
                            <li key={file.path}>
                              <button className={styles.treeItemRow}>
                                {file.is_dir ? (
                                  <Folder size={14} />
                                ) : (
                                  <File size={14} />
                                )}
                                <span className={styles.treeLabel}>{file.name}</span>
                              </button>
                            </li>
                          ))
                        )}
                      </ul>
                    )}
                  </div>
                );
              })
            )}
          </div>
        </div>
      )}
    </aside>
  );
}
