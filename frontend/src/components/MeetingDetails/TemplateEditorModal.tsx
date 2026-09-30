'use client';

import { useState } from 'react';
import { toast } from 'sonner';
import type { FullTemplate, TemplateInfo } from '@/hooks/meeting-details/useTemplates';
import { allowedActions, sourceLabel, uniqueTemplateId } from '@/lib/template-editing';
import type { TemplateSource } from '@/lib/template-editing';

type Format = 'paragraph' | 'list' | 'table' | 'string';

interface SectionDraft {
  title: string;
  instruction: string;
  format: Format;
  item_format?: string;
  example_item_format?: string;
}

type EditorMode =
  | { kind: 'new' }
  | { kind: 'edit'; id: string; source: TemplateSource }
  | { kind: 'duplicate'; id: string };

interface TemplateEditorModalProps {
  open: boolean;
  onClose: () => void;
  availableTemplates: TemplateInfo[];
  onSave: (templateId: string, templateJson: string) => Promise<string>;
  onDelete: (templateId: string) => Promise<void>;
  onRestore: (templateId: string) => Promise<void>;
  onLoadTemplate: (templateId: string) => Promise<FullTemplate>;
}

const emptySection = (): SectionDraft => ({ title: '', instruction: '', format: 'list' });

const SOURCE_BADGE_CLASS: Record<TemplateSource, string> = {
  custom: 'bg-blue-50 text-blue-700',
  builtin: 'bg-gray-100 text-gray-600',
  overridden: 'bg-amber-50 text-amber-700',
};

const errorMessage = (e: unknown, fallback: string) => (typeof e === 'string' ? e : fallback);

export function TemplateEditorModal({
  open,
  onClose,
  availableTemplates,
  onSave,
  onDelete,
  onRestore,
  onLoadTemplate,
}: TemplateEditorModalProps) {
  // null = showing the template list, otherwise the form
  const [mode, setMode] = useState<EditorMode | null>(null);
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [sections, setSections] = useState<SectionDraft[]>([emptySection()]);
  const [saving, setSaving] = useState(false);
  const [loadingId, setLoadingId] = useState<string | null>(null);

  if (!open) return null;

  const closeAll = () => {
    setMode(null);
    onClose();
  };

  const startNew = () => {
    setName('');
    setDescription('');
    setSections([emptySection()]);
    setMode({ kind: 'new' });
  };

  const loadInto = async (t: TemplateInfo, next: (t: TemplateInfo) => EditorMode, nameSuffix = '') => {
    setLoadingId(t.id);
    try {
      const full = await onLoadTemplate(t.id);
      setName(`${full.name}${nameSuffix}`);
      setDescription(full.description);
      setSections(
        full.sections.length > 0
          ? full.sections.map((s) => ({
              title: s.title,
              instruction: s.instruction,
              format: s.format,
              item_format: s.item_format,
              example_item_format: s.example_item_format,
            }))
          : [emptySection()]
      );
      setMode(next(t));
    } catch (e) {
      toast.error(errorMessage(e, 'Failed to load template'));
    } finally {
      setLoadingId(null);
    }
  };

  const startEdit = (t: TemplateInfo) => loadInto(t, (x) => ({ kind: 'edit', id: x.id, source: x.source }));
  const startDuplicate = (t: TemplateInfo) => loadInto(t, (x) => ({ kind: 'duplicate', id: x.id }), ' (copy)');

  const updateSection = (i: number, patch: Partial<SectionDraft>) => {
    setSections((prev) => prev.map((s, idx) => (idx === i ? { ...s, ...patch } : s)));
  };

  const moveSection = (i: number, delta: -1 | 1) => {
    setSections((prev) => {
      const j = i + delta;
      if (j < 0 || j >= prev.length) return prev;
      const next = [...prev];
      [next[i], next[j]] = [next[j], next[i]];
      return next;
    });
  };

  const save = async () => {
    if (!mode) return;
    if (!name.trim()) return toast.error('Template name is required');
    if (!description.trim()) return toast.error('Template description is required');
    const cleaned = sections
      .map((s) => ({ ...s, title: s.title.trim(), instruction: s.instruction.trim() }))
      .filter((s) => s.title && s.instruction);
    if (cleaned.length === 0) return toast.error('Add at least one section with a title and instruction');

    const template = {
      name: name.trim(),
      description: description.trim(),
      sections: cleaned.map((s) => {
        const out: Record<string, string> = { title: s.title, instruction: s.instruction, format: s.format };
        if (s.item_format && s.item_format.trim()) out.item_format = s.item_format.trim();
        if (s.example_item_format && s.example_item_format.trim()) {
          out.example_item_format = s.example_item_format.trim();
        }
        return out;
      }),
    };

    // Edit keeps the id (overwrites in place / creates the override of a built-in);
    // new and duplicate get a fresh id that never collides with an existing one.
    const id =
      mode.kind === 'edit' ? mode.id : uniqueTemplateId(name, availableTemplates.map((t) => t.id));
    if (!id) return toast.error('Template name must contain letters or numbers');

    setSaving(true);
    try {
      await onSave(id, JSON.stringify(template, null, 2));
      toast.success(mode.kind === 'edit' ? `Template "${name.trim()}" updated` : `Template "${name.trim()}" created`);
      setMode(null);
    } catch (e) {
      toast.error(errorMessage(e, 'Failed to save template'));
    } finally {
      setSaving(false);
    }
  };

  const del = async (t: TemplateInfo) => {
    if (!window.confirm(`Delete template "${t.name}"? This cannot be undone.`)) return;
    try {
      await onDelete(t.id);
      toast.success(`Deleted "${t.name}"`);
    } catch (e) {
      toast.error(errorMessage(e, 'Failed to delete'));
    }
  };

  const restore = async (t: TemplateInfo) => {
    if (!window.confirm(`Restore the default version of "${t.name}"? Your edits will be discarded.`)) return;
    try {
      await onRestore(t.id);
      toast.success(`Restored default for "${t.name}"`);
    } catch (e) {
      toast.error(errorMessage(e, 'Failed to restore default'));
    }
  };

  const title =
    mode === null ? 'Summary templates' : mode.kind === 'edit' ? 'Edit template' : mode.kind === 'duplicate' ? 'Duplicate template' : 'New template';
  const cta = mode?.kind === 'edit' ? 'Save changes' : 'Create';
  const editingBuiltin = mode?.kind === 'edit' && (mode.source === 'builtin' || mode.source === 'overridden');

  const actionBtn = 'rounded px-2 py-1 text-xs hover:bg-gray-100';

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 p-4" onClick={closeAll}>
      <div
        className="flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-xl bg-white shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-gray-100 px-5 py-3">
          <h2 className="text-lg font-semibold text-gray-800">{title}</h2>
          <button onClick={closeAll} className="rounded px-2 py-1 text-gray-500 hover:bg-gray-100">✕</button>
        </div>

        <div className="flex-1 overflow-y-auto px-5 py-4">
          {mode === null ? (
            <div>
              <div className="mb-3 flex justify-end">
                <button
                  onClick={startNew}
                  className="rounded-lg bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700"
                >
                  ＋ New template
                </button>
              </div>
              <div className="space-y-1">
                {availableTemplates.map((t) => {
                  const actions = allowedActions(t.source);
                  return (
                    <div key={t.id} className="flex items-center justify-between rounded-lg px-2 py-1.5 hover:bg-gray-50">
                      <div className="min-w-0">
                        <div className="flex items-center gap-2">
                          <span className="truncate text-sm text-gray-800">{t.name}</span>
                          <span className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${SOURCE_BADGE_CLASS[t.source]}`}>
                            {sourceLabel(t.source)}
                          </span>
                        </div>
                        <div className="truncate text-xs text-gray-400">{t.description}</div>
                      </div>
                      <div className="ml-3 flex shrink-0 items-center gap-0.5">
                        {actions.edit && (
                          <button
                            onClick={() => startEdit(t)}
                            disabled={loadingId === t.id}
                            className={`${actionBtn} text-blue-600`}
                          >
                            Edit
                          </button>
                        )}
                        {actions.duplicate && (
                          <button
                            onClick={() => startDuplicate(t)}
                            disabled={loadingId === t.id}
                            className={`${actionBtn} text-gray-600`}
                          >
                            Duplicate
                          </button>
                        )}
                        {actions.restore && (
                          <button onClick={() => restore(t)} className={`${actionBtn} text-amber-700`}>
                            Restore default
                          </button>
                        )}
                        {actions.delete && (
                          <button onClick={() => del(t)} className={`${actionBtn} text-red-500 hover:bg-red-50`}>
                            Delete
                          </button>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              {editingBuiltin && (
                <p className="rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-800">
                  Saving creates your own version; you can restore the default anytime.
                </p>
              )}
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <div>
                  <label className="mb-1 block text-xs font-medium text-gray-600">Template name</label>
                  <input
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder="e.g. Client Discovery Call"
                    className="w-full rounded-lg border border-gray-200 px-2 py-1.5 text-sm focus:border-blue-400 focus:outline-none"
                  />
                </div>
                <div>
                  <label className="mb-1 block text-xs font-medium text-gray-600">Description</label>
                  <input
                    value={description}
                    onChange={(e) => setDescription(e.target.value)}
                    placeholder="What is this template for?"
                    className="w-full rounded-lg border border-gray-200 px-2 py-1.5 text-sm focus:border-blue-400 focus:outline-none"
                  />
                </div>
              </div>

              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <label className="text-xs font-medium text-gray-600">Sections</label>
                  <button
                    onClick={() => setSections((p) => [...p, emptySection()])}
                    className="rounded px-2 py-1 text-xs font-medium text-blue-600 hover:bg-blue-50"
                  >
                    ＋ Add section
                  </button>
                </div>

                {sections.map((s, i) => (
                  <div key={i} className="rounded-lg border border-gray-200 p-3">
                    <div className="mb-2 flex items-center gap-2">
                      <input
                        value={s.title}
                        onChange={(e) => updateSection(i, { title: e.target.value })}
                        placeholder="Section title (e.g. Action Items)"
                        className="flex-1 rounded border border-gray-200 px-2 py-1 text-sm focus:border-blue-400 focus:outline-none"
                      />
                      <select
                        value={s.format}
                        onChange={(e) => updateSection(i, { format: e.target.value as Format })}
                        className="rounded border border-gray-200 px-2 py-1 text-sm"
                        title="How the model should format this section"
                      >
                        <option value="list">List</option>
                        <option value="paragraph">Paragraph</option>
                        <option value="table">Table</option>
                        <option value="string">Single line</option>
                      </select>
                      <button
                        onClick={() => moveSection(i, -1)}
                        disabled={i === 0}
                        className="rounded px-1.5 py-1 text-xs text-gray-500 hover:bg-gray-100 disabled:opacity-30"
                        title="Move up"
                      >
                        ↑
                      </button>
                      <button
                        onClick={() => moveSection(i, 1)}
                        disabled={i === sections.length - 1}
                        className="rounded px-1.5 py-1 text-xs text-gray-500 hover:bg-gray-100 disabled:opacity-30"
                        title="Move down"
                      >
                        ↓
                      </button>
                      {sections.length > 1 && (
                        <button
                          onClick={() => setSections((p) => p.filter((_, idx) => idx !== i))}
                          className="rounded px-2 py-1 text-xs text-red-500 hover:bg-red-50"
                          title="Remove section"
                        >
                          ✕
                        </button>
                      )}
                    </div>
                    <textarea
                      value={s.instruction}
                      onChange={(e) => updateSection(i, { instruction: e.target.value })}
                      rows={2}
                      placeholder="Instruction for the AI — e.g. 'List concrete action items with an owner'"
                      className="w-full resize-none rounded border border-gray-200 px-2 py-1 text-sm focus:border-blue-400 focus:outline-none"
                    />
                    <input
                      value={s.item_format ?? ''}
                      onChange={(e) => updateSection(i, { item_format: e.target.value })}
                      placeholder="Item format (optional) — e.g. '- **Owner**: task (due date)'"
                      className="mt-2 w-full rounded border border-gray-200 px-2 py-1 text-xs focus:border-blue-400 focus:outline-none"
                    />
                  </div>
                ))}
              </div>

              <div className="flex justify-end gap-2 pt-1">
                <button onClick={() => setMode(null)} className="rounded-lg px-3 py-2 text-sm text-gray-600 hover:bg-gray-100">
                  Cancel
                </button>
                <button
                  onClick={save}
                  disabled={saving}
                  className="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white disabled:bg-gray-300"
                >
                  {saving ? 'Saving…' : cta}
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default TemplateEditorModal;
