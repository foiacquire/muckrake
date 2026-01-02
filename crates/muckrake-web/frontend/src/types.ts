export type EntityType = string;

export type RelationType =
  | 'owns'
  | 'controls'
  | 'shareholders'
  | 'employs'
  | 'employed_by'
  | 'director_of'
  | 'officer_of'
  | 'member_of'
  | 'parent_of'
  | 'child_of'
  | 'spouse_of'
  | 'sibling_of'
  | 'relative_of'
  | 'located_at'
  | 'headquartered_at'
  | 'registered_at'
  | 'participated_in'
  | 'organized_by'
  | 'mentioned_in'
  | 'authored_by'
  | 'signed_by'
  | 'associated_with'
  | 'linked_to';

export interface Entity {
  id: string;
  canonical_name: string;
  type: EntityType;
  data: Record<string, unknown>;
  confidence?: number;
  created_at: string;
  updated_at: string;
}

export interface Relationship {
  id: string;
  source_id: string;
  target_id: string;
  relation_type: RelationType;
  confidence?: number;
  start_date?: string;
  end_date?: string;
  role?: string;
  notes?: string;
  created_at: string;
}

export interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  is_hidden?: boolean;
}

export interface Project {
  id: string;
  name: string;
  path: string;
  files: FileEntry[];
}

export interface ServerConfig {
  allow_remote_workspaces: boolean;
}

// Colors are defined in styles/theme.css.ts (entityColors)
