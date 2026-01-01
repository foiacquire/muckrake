import type { Entity, Relationship } from './types';

const API_BASE = '/api';

async function fetchJSON<T>(url: string, options?: RequestInit): Promise<T> {
  const response = await fetch(url, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...options?.headers,
    },
  });

  if (!response.ok) {
    const error = await response.text();
    throw new Error(error || response.statusText);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return response.json();
}

export const api = {
  entities: {
    list: (type?: string) =>
      fetchJSON<Entity[]>(`${API_BASE}/entities${type ? `?type=${type}` : ''}`),

    get: (id: string) => fetchJSON<Entity>(`${API_BASE}/entities/${id}`),

    search: (query: string) =>
      fetchJSON<Entity[]>(`${API_BASE}/entities/search?q=${encodeURIComponent(query)}`),

    create: (entity: Omit<Entity, 'id' | 'created_at' | 'updated_at'>) =>
      fetchJSON<Entity>(`${API_BASE}/entities`, {
        method: 'POST',
        body: JSON.stringify(entity),
      }),

    update: (id: string, entity: Partial<Entity>) =>
      fetchJSON<Entity>(`${API_BASE}/entities/${id}`, {
        method: 'PUT',
        body: JSON.stringify(entity),
      }),

    delete: (id: string) =>
      fetchJSON<void>(`${API_BASE}/entities/${id}`, { method: 'DELETE' }),
  },

  relationships: {
    get: (id: string) => fetchJSON<Relationship>(`${API_BASE}/relationships/${id}`),

    forEntity: (entityId: string) =>
      fetchJSON<Relationship[]>(`${API_BASE}/relationships/entity/${entityId}`),

    create: (rel: Omit<Relationship, 'id' | 'created_at'>) =>
      fetchJSON<Relationship>(`${API_BASE}/relationships`, {
        method: 'POST',
        body: JSON.stringify(rel),
      }),

    delete: (id: string) =>
      fetchJSON<void>(`${API_BASE}/relationships/${id}`, { method: 'DELETE' }),
  },
};
