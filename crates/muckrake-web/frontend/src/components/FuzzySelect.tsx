import { useState, useRef, useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Search } from 'lucide-react';
import Fuse from 'fuse.js';
import * as styles from '../styles/fuzzySelect.css';

export interface FuzzyOption<T = string> {
  id: T;
  label: string;
  icon?: React.ReactNode;
}

interface Props<T = string> {
  options: FuzzyOption<T>[];
  value?: T[];
  onChange?: (ids: T[]) => void;
  placeholder?: string;
  searchPlaceholder?: string;
}

export function FuzzySelect<T extends string = string>({
  options,
  value = [],
  onChange,
  placeholder = 'Filter...',
  searchPlaceholder = 'Type to search...',
}: Props<T>) {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const [query, setQuery] = useState('');
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listboxId = 'fuzzy-select-listbox';

  const fuse = useMemo(
    () =>
      new Fuse(options, {
        keys: ['label'],
        threshold: 0.4,
        includeScore: true,
      }),
    [options]
  );

  const filteredOptions = useMemo(() => {
    if (!query.trim()) return options;
    return fuse.search(query).map((result) => result.item);
  }, [fuse, query, options]);

  const selectedOptions = options.filter((o) => value.includes(o.id));

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setIsOpen(false);
        setQuery('');
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  useEffect(() => {
    if (isOpen && inputRef.current) {
      inputRef.current.focus();
    }
  }, [isOpen]);

  const handleSelect = (id: T) => {
    const isSelected = value.includes(id);
    const next = isSelected ? value.filter((v) => v !== id) : [...value, id];
    onChange?.(next);
  };

  const displayText = selectedOptions.length === 0
    ? placeholder
    : selectedOptions.length === 1
      ? selectedOptions[0].label
      : t('sidebar.filter.selected', { count: selectedOptions.length });

  return (
    <div className={styles.container} ref={containerRef}>
      {isOpen && (
        <div className={styles.dropdown} role="dialog" aria-label={placeholder}>
          <input
            ref={inputRef}
            type="text"
            className={styles.searchInput}
            placeholder={searchPlaceholder}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            role="combobox"
            aria-expanded={isOpen}
            aria-controls={listboxId}
            aria-autocomplete="list"
          />
          <ul
            id={listboxId}
            className={styles.optionsList}
            role="listbox"
            aria-multiselectable="true"
            aria-label={placeholder}
          >
            {filteredOptions.length === 0 ? (
              <li className={styles.noResults} role="option" aria-disabled="true">
                {t('fuzzySelect.noResults')}
              </li>
            ) : (
              filteredOptions.map((option) => (
                <li
                  key={option.id}
                  role="option"
                  aria-selected={value.includes(option.id)}
                >
                  <button
                    className={`${styles.option} ${value.includes(option.id) ? styles.optionSelected : ''}`}
                    onClick={() => handleSelect(option.id)}
                  >
                    {option.icon && <span aria-hidden="true">{option.icon}</span>}
                    {option.label}
                  </button>
                </li>
              ))
            )}
          </ul>
        </div>
      )}
      <button
        className={`${styles.iconTrigger} ${value.length > 0 ? styles.iconTriggerActive : ''}`}
        onClick={() => setIsOpen(!isOpen)}
        title={displayText}
        aria-label={displayText}
        aria-haspopup="listbox"
        aria-expanded={isOpen}
      >
        <Search size={14} aria-hidden="true" />
        {value.length > 0 && (
          <span className={styles.badge} aria-hidden="true">{value.length}</span>
        )}
      </button>
    </div>
  );
}
