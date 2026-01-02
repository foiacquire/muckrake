import { describe, it, expect, beforeEach } from 'vitest';
import {
  isRecentProjectsEnabled,
  setRecentProjectsEnabled,
  getRecentProjects,
  addRecentProject,
  removeRecentProject,
  clearRecentProjects,
  getStorageKeys,
} from './recentProjects';

describe('Recent Projects - Security Requirements', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  describe('CRITICAL: Feature must be opt-in only', () => {
    it('is disabled by default on fresh browser', () => {
      expect(isRecentProjectsEnabled()).toBe(false);
    });

    it('is disabled when localStorage is empty', () => {
      expect(localStorage.length).toBe(0);
      expect(isRecentProjectsEnabled()).toBe(false);
    });

    it('is disabled when enabled key does not exist', () => {
      expect(localStorage.getItem(getStorageKeys().enabled)).toBeNull();
      expect(isRecentProjectsEnabled()).toBe(false);
    });

    it('is disabled when enabled key is "false"', () => {
      localStorage.setItem(getStorageKeys().enabled, 'false');
      expect(isRecentProjectsEnabled()).toBe(false);
    });

    it('is disabled when enabled key is empty string', () => {
      localStorage.setItem(getStorageKeys().enabled, '');
      expect(isRecentProjectsEnabled()).toBe(false);
    });

    it('is disabled when enabled key is "0"', () => {
      localStorage.setItem(getStorageKeys().enabled, '0');
      expect(isRecentProjectsEnabled()).toBe(false);
    });

    it('is disabled when enabled key is "null"', () => {
      localStorage.setItem(getStorageKeys().enabled, 'null');
      expect(isRecentProjectsEnabled()).toBe(false);
    });

    it('is disabled when enabled key is "undefined"', () => {
      localStorage.setItem(getStorageKeys().enabled, 'undefined');
      expect(isRecentProjectsEnabled()).toBe(false);
    });

    it('is disabled when enabled key is any random string', () => {
      localStorage.setItem(getStorageKeys().enabled, 'yes');
      expect(isRecentProjectsEnabled()).toBe(false);
      localStorage.setItem(getStorageKeys().enabled, 'enabled');
      expect(isRecentProjectsEnabled()).toBe(false);
      localStorage.setItem(getStorageKeys().enabled, '1');
      expect(isRecentProjectsEnabled()).toBe(false);
    });

    it('is ONLY enabled when key is exactly "true"', () => {
      localStorage.setItem(getStorageKeys().enabled, 'true');
      expect(isRecentProjectsEnabled()).toBe(true);
    });

    it('is disabled when key is "TRUE" (case sensitive)', () => {
      localStorage.setItem(getStorageKeys().enabled, 'TRUE');
      expect(isRecentProjectsEnabled()).toBe(false);
    });

    it('is disabled when key is " true " (with spaces)', () => {
      localStorage.setItem(getStorageKeys().enabled, ' true ');
      expect(isRecentProjectsEnabled()).toBe(false);
    });
  });

  describe('CRITICAL: No data storage when disabled', () => {
    it('does not store any data when adding project while disabled', () => {
      expect(isRecentProjectsEnabled()).toBe(false);

      addRecentProject({ id: 'test-1', name: 'Test Project' });

      expect(localStorage.getItem(getStorageKeys().projects)).toBeNull();
    });

    it('returns empty array when getting projects while disabled', () => {
      expect(getRecentProjects()).toEqual([]);
    });

    it('returns empty array even if projects data somehow exists but feature disabled', () => {
      // Simulate data that might have been left over
      localStorage.setItem(getStorageKeys().projects, JSON.stringify([
        { id: 'old-1', name: 'Old Project', lastOpened: '2024-01-01' }
      ]));

      expect(isRecentProjectsEnabled()).toBe(false);
      expect(getRecentProjects()).toEqual([]);
    });

    it('does not create any localStorage keys when feature is disabled', () => {
      addRecentProject({ id: 'test-1', name: 'Test' });
      addRecentProject({ id: 'test-2', name: 'Test 2' });
      removeRecentProject('test-1');

      // Should have zero keys
      expect(localStorage.length).toBe(0);
    });
  });

  describe('CRITICAL: Disabling clears all data immediately', () => {
    it('clears projects data when disabled', () => {
      // Enable and add some projects
      setRecentProjectsEnabled(true);
      addRecentProject({ id: 'test-1', name: 'Test 1' });
      addRecentProject({ id: 'test-2', name: 'Test 2' });

      expect(getRecentProjects().length).toBe(2);
      expect(localStorage.getItem(getStorageKeys().projects)).not.toBeNull();

      // Disable - should clear immediately
      setRecentProjectsEnabled(false);

      expect(localStorage.getItem(getStorageKeys().projects)).toBeNull();
      expect(localStorage.getItem(getStorageKeys().enabled)).toBeNull();
    });

    it('removes the enabled key when disabled', () => {
      setRecentProjectsEnabled(true);
      expect(localStorage.getItem(getStorageKeys().enabled)).toBe('true');

      setRecentProjectsEnabled(false);
      expect(localStorage.getItem(getStorageKeys().enabled)).toBeNull();
    });

    it('leaves no trace after disabling', () => {
      setRecentProjectsEnabled(true);
      addRecentProject({ id: 'secret-project', name: 'Secret Investigation' });

      setRecentProjectsEnabled(false);

      // Verify absolutely no keys remain
      const keys = getStorageKeys();
      expect(localStorage.getItem(keys.enabled)).toBeNull();
      expect(localStorage.getItem(keys.projects)).toBeNull();
    });
  });

  describe('Enabled behavior', () => {
    beforeEach(() => {
      setRecentProjectsEnabled(true);
    });

    it('stores projects when enabled', () => {
      addRecentProject({ id: 'test-1', name: 'Test Project' });

      const projects = getRecentProjects();
      expect(projects.length).toBe(1);
      expect(projects[0].id).toBe('test-1');
      expect(projects[0].name).toBe('Test Project');
    });

    it('adds lastOpened timestamp', () => {
      addRecentProject({ id: 'test-1', name: 'Test Project' });

      const projects = getRecentProjects();
      expect(projects[0].lastOpened).toBeDefined();
      expect(new Date(projects[0].lastOpened).getTime()).toBeGreaterThan(0);
    });

    it('moves existing project to top when re-added', () => {
      addRecentProject({ id: 'test-1', name: 'First' });
      addRecentProject({ id: 'test-2', name: 'Second' });
      addRecentProject({ id: 'test-1', name: 'First Updated' });

      const projects = getRecentProjects();
      expect(projects.length).toBe(2);
      expect(projects[0].id).toBe('test-1');
      expect(projects[0].name).toBe('First Updated');
      expect(projects[1].id).toBe('test-2');
    });

    it('limits to 10 projects', () => {
      for (let i = 0; i < 15; i++) {
        addRecentProject({ id: `test-${i}`, name: `Project ${i}` });
      }

      const projects = getRecentProjects();
      expect(projects.length).toBe(10);
      // Most recent should be first
      expect(projects[0].id).toBe('test-14');
    });

    it('removes specific project', () => {
      addRecentProject({ id: 'test-1', name: 'First' });
      addRecentProject({ id: 'test-2', name: 'Second' });

      removeRecentProject('test-1');

      const projects = getRecentProjects();
      expect(projects.length).toBe(1);
      expect(projects[0].id).toBe('test-2');
    });

    it('clears all projects', () => {
      addRecentProject({ id: 'test-1', name: 'First' });
      addRecentProject({ id: 'test-2', name: 'Second' });

      clearRecentProjects();

      expect(getRecentProjects()).toEqual([]);
    });
  });

  describe('Data integrity', () => {
    it('handles corrupted JSON gracefully', () => {
      setRecentProjectsEnabled(true);
      localStorage.setItem(getStorageKeys().projects, 'not valid json');

      expect(getRecentProjects()).toEqual([]);
    });

    it('handles non-array JSON gracefully', () => {
      setRecentProjectsEnabled(true);
      localStorage.setItem(getStorageKeys().projects, '{"not": "an array"}');

      expect(getRecentProjects()).toEqual([]);
    });
  });
});
