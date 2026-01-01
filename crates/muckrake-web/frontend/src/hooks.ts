import { useState, useEffect, useCallback } from 'react';
import { api } from './api';
import type { Entity, Relationship } from './types';

export function useEntities() {
  const [entities, setEntities] = useState<Entity[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const data = await api.entities.list();
      setEntities(data);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e : new Error(String(e)));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { entities, loading, error, refresh };
}

export function useRelationships(entityIds: string[]) {
  const [relationships, setRelationships] = useState<Relationship[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const refresh = useCallback(async () => {
    if (entityIds.length === 0) {
      setRelationships([]);
      setLoading(false);
      return;
    }

    setLoading(true);
    try {
      const allRels = await Promise.all(entityIds.map((id) => api.relationships.forEntity(id)));
      const uniqueRels = new Map<string, Relationship>();
      for (const rels of allRels) {
        for (const rel of rels) {
          uniqueRels.set(rel.id, rel);
        }
      }
      setRelationships(Array.from(uniqueRels.values()));
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e : new Error(String(e)));
    } finally {
      setLoading(false);
    }
  }, [entityIds.join(',')]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { relationships, loading, error, refresh };
}
