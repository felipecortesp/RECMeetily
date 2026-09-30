export type TemplateSource = 'custom' | 'builtin' | 'overridden';

export const MAX_TEMPLATE_ID_LEN = 64;

export interface TemplateActions {
  edit: boolean;
  duplicate: boolean;
  delete: boolean;
  restore: boolean;
}

/** Which list actions are available for a template of the given source. */
export function allowedActions(source: TemplateSource): TemplateActions {
  return {
    edit: true,
    duplicate: true,
    delete: source === 'custom',
    restore: source === 'overridden',
  };
}

export function sourceLabel(source: TemplateSource): string {
  switch (source) {
    case 'custom':
      return 'Custom';
    case 'overridden':
      return 'Edited';
    default:
      return 'Built-in';
  }
}

/** Slug limited to [a-z0-9_-], at most 64 characters (same rule as the backend). */
export function slugifyTemplateId(name: string, maxLen = MAX_TEMPLATE_ID_LEN): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, maxLen)
    .replace(/_+$/g, '');
}

export function isValidTemplateId(id: string): boolean {
  return id.length > 0 && id.length <= MAX_TEMPLATE_ID_LEN && /^[a-z0-9_-]+$/.test(id);
}

/** Slug of `name` that does not collide with `existingIds` (appends _2, _3, ...). Empty if name has no usable characters. */
export function uniqueTemplateId(name: string, existingIds: Iterable<string>): string {
  const taken = new Set(existingIds);
  const base = slugifyTemplateId(name);
  if (!base) return '';
  if (!taken.has(base)) return base;
  for (let n = 2; ; n++) {
    const suffix = `_${n}`;
    const candidate = base.slice(0, MAX_TEMPLATE_ID_LEN - suffix.length) + suffix;
    if (!taken.has(candidate)) return candidate;
  }
}
