/**
 * Recent Projects Storage
 *
 * SECURITY: This feature is OPT-IN ONLY.
 * - Disabled by default to protect journalist privacy
 * - Data stored only in browser localStorage (never sent to server)
 * - Disabling immediately clears all stored data
 * - No data is stored unless explicitly enabled
 */

const STORAGE_KEY_ENABLED = 'muckrake_recent_projects_enabled';
const STORAGE_KEY_PROJECTS = 'muckrake_recent_projects';
const MAX_RECENT_PROJECTS = 10;

export interface RecentProject {
  id: string;
  name: string;
  lastOpened: string; // ISO date string
  filePath?: string;
}

/**
 * Check if recent projects feature is enabled.
 * SECURITY: Returns false by default - opt-in only.
 */
export function isRecentProjectsEnabled(): boolean {
  // Explicit check for 'true' string - anything else is disabled
  return localStorage.getItem(STORAGE_KEY_ENABLED) === 'true';
}

/**
 * Enable or disable the recent projects feature.
 * When disabled, immediately clears all stored project data.
 */
export function setRecentProjectsEnabled(enabled: boolean): void {
  if (enabled) {
    localStorage.setItem(STORAGE_KEY_ENABLED, 'true');
  } else {
    // SECURITY: Immediately clear all data when disabling
    localStorage.removeItem(STORAGE_KEY_ENABLED);
    localStorage.removeItem(STORAGE_KEY_PROJECTS);
  }
}

/**
 * Get the list of recent projects.
 * Returns empty array if feature is disabled.
 */
export function getRecentProjects(): RecentProject[] {
  // SECURITY: Never return data if feature is disabled
  if (!isRecentProjectsEnabled()) {
    return [];
  }

  const stored = localStorage.getItem(STORAGE_KEY_PROJECTS);
  if (!stored) {
    return [];
  }

  try {
    const parsed = JSON.parse(stored);
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed;
  } catch {
    return [];
  }
}

/**
 * Add a project to recent projects list.
 * Does nothing if feature is disabled.
 */
export function addRecentProject(project: Omit<RecentProject, 'lastOpened'>): void {
  // SECURITY: Never store data if feature is disabled
  if (!isRecentProjectsEnabled()) {
    return;
  }

  const current = getRecentProjects();
  const now = new Date().toISOString();

  // Remove existing entry for this project if present
  const filtered = current.filter(p => p.id !== project.id);

  // Add to front of list
  const updated: RecentProject[] = [
    { ...project, lastOpened: now },
    ...filtered,
  ].slice(0, MAX_RECENT_PROJECTS);

  localStorage.setItem(STORAGE_KEY_PROJECTS, JSON.stringify(updated));
}

/**
 * Remove a specific project from recent projects list.
 */
export function removeRecentProject(projectId: string): void {
  if (!isRecentProjectsEnabled()) {
    return;
  }

  const current = getRecentProjects();
  const filtered = current.filter(p => p.id !== projectId);
  localStorage.setItem(STORAGE_KEY_PROJECTS, JSON.stringify(filtered));
}

/**
 * Clear all recent projects.
 */
export function clearRecentProjects(): void {
  localStorage.removeItem(STORAGE_KEY_PROJECTS);
}

/**
 * Get the storage keys used by this module.
 * Useful for testing to verify no unexpected data is stored.
 */
export function getStorageKeys(): { enabled: string; projects: string } {
  return {
    enabled: STORAGE_KEY_ENABLED,
    projects: STORAGE_KEY_PROJECTS,
  };
}
