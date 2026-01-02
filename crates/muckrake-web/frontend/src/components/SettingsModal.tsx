import { useState, useEffect, useRef, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { X, Shield, Globe, Database, AlertTriangle, Search, ChevronUp, ChevronDown, GripVertical } from 'lucide-react';
import * as styles from '../styles/settingsModal.css';
import { isRecentProjectsEnabled, setRecentProjectsEnabled } from '../storage/recentProjects';

type NetworkMode = 'tor_snowflake' | 'tor_obfs4' | 'tor_direct' | 'direct_unsafe';
type TorClient = 'tor' | 'arti' | 'tor_browser' | 'existing_proxy';

interface TorClientConfig {
  client: TorClient;
  enabled: boolean;
}

interface NetworkSettings {
  mode: NetworkMode;
  torClients: TorClientConfig[];
  socksPort: number;
  allowOnion: boolean;
  bypassDomains: string[];
}

interface Settings {
  network: NetworkSettings;
}

const defaultSettings: Settings = {
  network: {
    mode: 'tor_snowflake',
    torClients: [
      { client: 'tor', enabled: true },
      { client: 'arti', enabled: true },
      { client: 'tor_browser', enabled: false },
      { client: 'existing_proxy', enabled: false },
    ],
    socksPort: 9150,
    allowOnion: true,
    bypassDomains: [],
  },
};

interface Props {
  open: boolean;
  onClose: () => void;
}

type Tab = 'network' | 'storage' | 'general';

interface SettingItem {
  id: string;
  tab: Tab;
  section: string;
  keywords: string[];
}

const clientLabels: Record<TorClient, string> = {
  tor: 'System Tor',
  arti: 'Arti',
  tor_browser: 'Tor Browser Proxy',
  existing_proxy: 'Custom Proxy',
};

export function SettingsModal({ open, onClose }: Props) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<Tab>('network');
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [bypassInput, setBypassInput] = useState('');
  const [searchQuery, setSearchQuery] = useState('');
  const [recentProjectsEnabled, setRecentProjectsEnabledState] = useState(false);
  const [showRecentProjectsWarning, setShowRecentProjectsWarning] = useState(false);
  const modalRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  // Load recent projects setting from localStorage on mount
  useEffect(() => {
    setRecentProjectsEnabledState(isRecentProjectsEnabled());
  }, [open]);

  const settingsIndex: SettingItem[] = useMemo(() => [
    { id: 'network-mode', tab: 'network', section: 'mode', keywords: ['connection', 'mode', 'tor', 'snowflake', 'obfs4', 'direct', 'unsafe', 'privacy', 'anonymous'] },
    { id: 'network-provider', tab: 'network', section: 'torProvider', keywords: ['tor', 'connection', 'arti', 'browser', 'proxy', 'provider'] },
    { id: 'network-port', tab: 'network', section: 'proxy', keywords: ['port', 'socks', 'proxy', '9150', '9050'] },
    { id: 'network-onion', tab: 'network', section: 'proxy', keywords: ['onion', '.onion', 'hidden', 'service', 'dark'] },
    { id: 'network-bypass', tab: 'network', section: 'bypass', keywords: ['bypass', 'domain', 'whitelist', 'exception', 'direct', 'skip'] },
    { id: 'general-recent', tab: 'general', section: 'recentProjects', keywords: ['recent', 'projects', 'history', 'remember', 'open'] },
  ], []);

  const handleRecentProjectsToggle = () => {
    if (recentProjectsEnabled) {
      // Disabling - immediately clear data
      setRecentProjectsEnabled(false);
      setRecentProjectsEnabledState(false);
    } else {
      // Enabling - show warning first
      setShowRecentProjectsWarning(true);
    }
  };

  const confirmEnableRecentProjects = () => {
    setRecentProjectsEnabled(true);
    setRecentProjectsEnabledState(true);
    setShowRecentProjectsWarning(false);
  };

  const cancelEnableRecentProjects = () => {
    setShowRecentProjectsWarning(false);
  };

  const matchedSettings = useMemo(() => {
    if (!searchQuery.trim()) return null;
    const q = searchQuery.toLowerCase();
    return settingsIndex.filter((item) =>
      item.keywords.some((kw) => kw.includes(q)) ||
      item.section.toLowerCase().includes(q)
    );
  }, [searchQuery, settingsIndex]);

  const visibleSections = useMemo(() => {
    if (!matchedSettings) return null;
    return new Set(matchedSettings.map((m) => m.section));
  }, [matchedSettings]);

  const shouldShowSection = (section: string) => !visibleSections || visibleSections.has(section);

  const filteredTab = useMemo(() => {
    if (!matchedSettings || matchedSettings.length === 0) return activeTab;
    const tabs = new Set(matchedSettings.map((m) => m.tab));
    if (!tabs.has(activeTab) && tabs.size > 0) return Array.from(tabs)[0];
    return activeTab;
  }, [matchedSettings, activeTab]);

  useEffect(() => {
    if (matchedSettings && matchedSettings.length > 0) {
      const firstTab = matchedSettings[0].tab;
      if (firstTab !== activeTab) setActiveTab(firstTab);
    }
  }, [matchedSettings]);

  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        searchQuery ? setSearchQuery('') : onClose();
      }
    };
    if (open) {
      document.addEventListener('keydown', handleEscape);
      searchRef.current?.focus();
      return () => document.removeEventListener('keydown', handleEscape);
    }
  }, [open, onClose, searchQuery]);

  const handleOverlayClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) onClose();
  };

  const updateNetwork = <K extends keyof NetworkSettings>(key: K, value: NetworkSettings[K]) => {
    setSettings((prev) => ({ ...prev, network: { ...prev.network, [key]: value } }));
  };

  const moveClient = (index: number, direction: -1 | 1) => {
    const newIndex = index + direction;
    if (newIndex < 0 || newIndex >= settings.network.torClients.length) return;
    const clients = [...settings.network.torClients];
    [clients[index], clients[newIndex]] = [clients[newIndex], clients[index]];
    updateNetwork('torClients', clients);
  };

  const toggleClient = (index: number) => {
    const clients = [...settings.network.torClients];
    clients[index] = { ...clients[index], enabled: !clients[index].enabled };
    updateNetwork('torClients', clients);
  };

  const handleAddBypassDomain = () => {
    const domain = bypassInput.trim().toLowerCase();
    if (domain && !settings.network.bypassDomains.includes(domain)) {
      updateNetwork('bypassDomains', [...settings.network.bypassDomains, domain]);
      setBypassInput('');
    }
  };

  const handleRemoveBypassDomain = (domain: string) => {
    updateNetwork('bypassDomains', settings.network.bypassDomains.filter((d) => d !== domain));
  };

  const handleSave = () => {
    console.log('Settings saved:', settings);
    onClose();
  };

  if (!open) return null;

  const tabs: { id: Tab; label: string; icon: React.ReactNode }[] = [
    { id: 'network', label: t('settings.tabs.network'), icon: <Shield size={14} /> },
    { id: 'storage', label: t('settings.tabs.storage'), icon: <Database size={14} /> },
    { id: 'general', label: t('settings.tabs.general'), icon: <Globe size={14} /> },
  ];

  const hasNetworkResults = !matchedSettings || matchedSettings.some((m) => m.tab === 'network');
  const hasStorageResults = !matchedSettings || matchedSettings.some((m) => m.tab === 'storage');
  const hasGeneralResults = !matchedSettings || matchedSettings.some((m) => m.tab === 'general');

  return (
    <div className={styles.overlay} onClick={handleOverlayClick} role="dialog" aria-modal="true">
      <div className={styles.modal} ref={modalRef}>
        <div className={styles.header}>
          <h2 className={styles.title}>{t('settings.title')}</h2>
          <div className={styles.searchContainer}>
            <Search size={14} className={styles.searchIcon} />
            <input
              ref={searchRef}
              type="text"
              className={styles.searchInput}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={t('settings.search')}
            />
            {searchQuery && (
              <button className={styles.searchClear} onClick={() => setSearchQuery('')}>
                <X size={12} />
              </button>
            )}
          </div>
          <button className={styles.closeButton} onClick={onClose} aria-label={t('settings.close')}>
            <X size={16} />
          </button>
        </div>

        <div className={styles.body}>
          <nav className={styles.sidebar}>
            {tabs.map((tab) => {
              const hasResults = tab.id === 'network' ? hasNetworkResults :
                                tab.id === 'storage' ? hasStorageResults : hasGeneralResults;
              return (
                <button
                  key={tab.id}
                  className={`${styles.sidebarButton} ${filteredTab === tab.id ? styles.sidebarButtonActive : ''} ${!hasResults ? styles.sidebarButtonDimmed : ''}`}
                  onClick={() => setActiveTab(tab.id)}
                  disabled={!hasResults}
                >
                  {tab.icon}
                  {tab.label}
                </button>
              );
            })}
          </nav>

          <div className={styles.content}>
            {filteredTab === 'network' && hasNetworkResults && (
              <>
                {shouldShowSection('mode') && (
                  <section className={styles.section}>
                    <h3 className={styles.sectionTitle}>{t('settings.network.mode.title')}</h3>
                    <div className={styles.row}>
                      <span className={styles.label}>{t('settings.network.mode.label')}</span>
                      <select
                        className={styles.select}
                        value={settings.network.mode}
                        onChange={(e) => updateNetwork('mode', e.target.value as NetworkMode)}
                      >
                        <option value="tor_snowflake">{t('settings.network.mode.options.torSnowflake')}</option>
                        <option value="tor_obfs4">{t('settings.network.mode.options.torObfs4')}</option>
                        <option value="tor_direct">{t('settings.network.mode.options.torDirect')}</option>
                        <option value="direct_unsafe">{t('settings.network.mode.options.directUnsafe')}</option>
                      </select>
                    </div>
                    {settings.network.mode === 'direct_unsafe' && (
                      <div className={styles.warningBanner}>
                        <AlertTriangle size={14} />
                        <span>{t('settings.network.mode.unsafeWarning')}</span>
                      </div>
                    )}
                  </section>
                )}

                {shouldShowSection('torProvider') && (
                  <section className={styles.section}>
                    <h3 className={styles.sectionTitle}>{t('settings.network.torProvider.title')}</h3>
                    <div className={styles.fallbackChain}>
                      {settings.network.torClients.map((cfg, i) => (
                        <div
                          key={cfg.client}
                          className={`${styles.fallbackItem} ${!cfg.enabled ? styles.fallbackItemDisabled : ''}`}
                        >
                          <span className={styles.fallbackHandle}><GripVertical size={12} /></span>
                          <label className={styles.checkbox}>
                            <input
                              type="checkbox"
                              className={styles.checkboxInput}
                              checked={cfg.enabled}
                              onChange={() => toggleClient(i)}
                            />
                            <span className={styles.fallbackLabel}>
                              {t(`settings.network.torProvider.options.${cfg.client}`) || clientLabels[cfg.client]}
                            </span>
                          </label>
                          <div className={styles.fallbackControls}>
                            <button
                              className={styles.fallbackButton}
                              onClick={() => moveClient(i, -1)}
                              disabled={i === 0}
                              aria-label="Move up"
                            >
                              <ChevronUp size={14} />
                            </button>
                            <button
                              className={styles.fallbackButton}
                              onClick={() => moveClient(i, 1)}
                              disabled={i === settings.network.torClients.length - 1}
                              aria-label="Move down"
                            >
                              <ChevronDown size={14} />
                            </button>
                          </div>
                        </div>
                      ))}
                    </div>
                    <p className={styles.hint} style={{ marginTop: '4px' }}>
                      {t('settings.network.torProvider.hint')}
                    </p>
                  </section>
                )}

                {shouldShowSection('proxy') && (
                  <section className={styles.section}>
                    <h3 className={styles.sectionTitle}>{t('settings.network.proxy.title')}</h3>
                    <div className={styles.row}>
                      <span className={styles.label}>{t('settings.network.proxy.portLabel')}</span>
                      <input
                        type="number"
                        className={styles.inputSmall}
                        value={settings.network.socksPort}
                        onChange={(e) => updateNetwork('socksPort', parseInt(e.target.value, 10) || 9150)}
                        min={1024}
                        max={65535}
                      />
                    </div>
                    <div className={styles.row}>
                      <label className={styles.checkbox}>
                        <input
                          type="checkbox"
                          className={styles.checkboxInput}
                          checked={settings.network.allowOnion}
                          onChange={(e) => updateNetwork('allowOnion', e.target.checked)}
                        />
                        {t('settings.network.proxy.allowOnion')}
                      </label>
                    </div>
                  </section>
                )}

                {shouldShowSection('bypass') && (
                  <section className={styles.section}>
                    <h3 className={styles.sectionTitle}>{t('settings.network.bypass.title')}</h3>
                    <div className={styles.rowStacked}>
                      <div className={styles.inlineInput}>
                        <input
                          type="text"
                          className={styles.input}
                          value={bypassInput}
                          onChange={(e) => setBypassInput(e.target.value)}
                          onKeyDown={(e) => e.key === 'Enter' && handleAddBypassDomain()}
                          placeholder="internal.example.com"
                          style={{ flex: 1 }}
                        />
                        <button
                          className={styles.buttonSmall}
                          onClick={handleAddBypassDomain}
                          disabled={!bypassInput.trim()}
                        >
                          {t('settings.network.bypass.add')}
                        </button>
                      </div>
                      {settings.network.bypassDomains.length > 0 && (
                        <div className={styles.tagList}>
                          {settings.network.bypassDomains.map((domain) => (
                            <span key={domain} className={styles.tag}>
                              {domain}
                              <button
                                className={styles.tagRemove}
                                onClick={() => handleRemoveBypassDomain(domain)}
                              >
                                <X size={10} />
                              </button>
                            </span>
                          ))}
                        </div>
                      )}
                    </div>
                  </section>
                )}
              </>
            )}

            {filteredTab === 'storage' && hasStorageResults && (
              <div className={styles.emptyState}>{t('settings.storage.comingSoon')}</div>
            )}

            {filteredTab === 'general' && hasGeneralResults && (
              <>
                {shouldShowSection('recentProjects') && (
                  <section className={styles.section}>
                    <h3 className={styles.sectionTitle}>{t('settings.general.recentProjects.title')}</h3>
                    <div className={styles.row}>
                      <label className={styles.checkbox}>
                        <input
                          type="checkbox"
                          className={styles.checkboxInput}
                          checked={recentProjectsEnabled}
                          onChange={handleRecentProjectsToggle}
                        />
                        {t('settings.general.recentProjects.label')}
                      </label>
                    </div>
                    <p className={styles.hint}>{t('settings.general.recentProjects.hint')}</p>
                  </section>
                )}
              </>
            )}

            {showRecentProjectsWarning && (
              <div className={styles.warningOverlay}>
                <div className={styles.warningDialog}>
                  <div className={styles.warningHeader}>
                    <AlertTriangle size={20} className={styles.warningIcon} />
                    <h3 className={styles.warningTitle}>{t('settings.general.recentProjects.warning.title')}</h3>
                  </div>
                  <p className={styles.warningMessage}>
                    {t('settings.general.recentProjects.warning.message')}
                  </p>
                  <div className={styles.warningActions}>
                    <button className={styles.buttonSecondary} onClick={cancelEnableRecentProjects}>
                      {t('settings.general.recentProjects.warning.cancel')}
                    </button>
                    <button className={styles.buttonDanger} onClick={confirmEnableRecentProjects}>
                      {t('settings.general.recentProjects.warning.confirm')}
                    </button>
                  </div>
                </div>
              </div>
            )}

            {matchedSettings && matchedSettings.length === 0 && (
              <div className={styles.emptyState}>{t('settings.noResults')}</div>
            )}
          </div>
        </div>

        <div className={styles.footer}>
          <button className={styles.buttonSecondary} onClick={onClose}>
            {t('settings.cancel')}
          </button>
          <button className={styles.buttonPrimary} onClick={handleSave}>
            {t('settings.save')}
          </button>
        </div>
      </div>
    </div>
  );
}
