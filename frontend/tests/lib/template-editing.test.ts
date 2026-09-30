import { describe, expect, test } from 'bun:test';
import {
  allowedActions,
  isValidTemplateId,
  slugifyTemplateId,
  sourceLabel,
  uniqueTemplateId,
} from '../../src/lib/template-editing';

describe('template ids', () => {
  test('slugifies names into safe ids', () => {
    expect(slugifyTemplateId('Client Discovery Call!')).toBe('client_discovery_call');
    expect(slugifyTemplateId('  ../Étude  ')).toBe('tude');
    expect(slugifyTemplateId('!!!')).toBe('');
  });

  test('limits length to 64 without trailing separator', () => {
    const id = slugifyTemplateId('word '.repeat(30));
    expect(id.length).toBeLessThanOrEqual(64);
    expect(id.endsWith('_')).toBe(false);
    expect(isValidTemplateId(id)).toBe(true);
  });

  test('validates ids', () => {
    expect(isValidTemplateId('my-tpl_2')).toBe(true);
    for (const bad of ['', 'A', 'a/b', '../x', 'a b', 'a'.repeat(65)]) {
      expect(isValidTemplateId(bad)).toBe(false);
    }
  });

  test('uniqueTemplateId avoids collisions', () => {
    expect(uniqueTemplateId('Daily Standup (copy)', [])).toBe('daily_standup_copy');
    expect(uniqueTemplateId('Standup', ['standup'])).toBe('standup_2');
    expect(uniqueTemplateId('Standup', ['standup', 'standup_2'])).toBe('standup_3');
    expect(uniqueTemplateId('???', [])).toBe('');
  });

  test('uniqueTemplateId keeps the 64 char limit', () => {
    const long = 'a'.repeat(80);
    const first = uniqueTemplateId(long, []);
    const second = uniqueTemplateId(long, [first]);
    expect(first.length).toBe(64);
    expect(second.length).toBe(64);
    expect(second).not.toBe(first);
    expect(isValidTemplateId(second)).toBe(true);
  });
});

describe('template actions', () => {
  test('maps source to allowed actions', () => {
    expect(allowedActions('custom')).toEqual({ edit: true, duplicate: true, delete: true, restore: false });
    expect(allowedActions('builtin')).toEqual({ edit: true, duplicate: true, delete: false, restore: false });
    expect(allowedActions('overridden')).toEqual({ edit: true, duplicate: true, delete: false, restore: true });
  });

  test('labels', () => {
    expect(sourceLabel('custom')).toBe('Custom');
    expect(sourceLabel('builtin')).toBe('Built-in');
    expect(sourceLabel('overridden')).toBe('Edited');
  });
});
