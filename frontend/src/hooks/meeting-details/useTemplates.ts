import { useState, useEffect, useCallback } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import type { TemplateSource } from '@/lib/template-editing';

export interface TemplateInfo {
  id: string;
  name: string;
  description: string;
  source: TemplateSource;
}

export interface FullTemplate {
  name: string;
  description: string;
  sections: Array<{
    title: string;
    instruction: string;
    format: 'paragraph' | 'list' | 'table' | 'string';
    item_format?: string;
    example_item_format?: string;
  }>;
}

const DEFAULT_TEMPLATE_ID = 'standard_meeting';

export function useTemplates() {
  const [availableTemplates, setAvailableTemplates] = useState<TemplateInfo[]>([]);
  const [selectedTemplate, setSelectedTemplate] = useState<string>(DEFAULT_TEMPLATE_ID);

  const refreshTemplates = useCallback(async () => {
    try {
      const templates = await invokeTauri('api_list_templates_detailed') as TemplateInfo[];
      setAvailableTemplates(templates);
      // Keep the selection if it still exists, otherwise fall back to the default.
      setSelectedTemplate((current) =>
        templates.some((t) => t.id === current) ? current : DEFAULT_TEMPLATE_ID
      );
      return templates;
    } catch (error) {
      console.error('Failed to fetch templates:', error);
      return [];
    }
  }, []);

  // Fetch available templates on mount
  useEffect(() => {
    void refreshTemplates();
  }, [refreshTemplates]);

  // Create or overwrite a custom template. `templateJson` must match the template schema.
  const saveCustomTemplate = useCallback(async (templateId: string, templateJson: string) => {
    const savedId = await invokeTauri('api_save_custom_template', {
      templateId,
      templateJson,
    }) as string;
    await refreshTemplates();
    return savedId;
  }, [refreshTemplates]);

  // Delete a custom template (built-ins are not deletable).
  const deleteCustomTemplate = useCallback(async (templateId: string) => {
    await invokeTauri('api_delete_custom_template', { templateId });
    await refreshTemplates();
  }, [refreshTemplates]);

  // Load the full template (all sections/fields) for editing or duplicating.
  const getTemplate = useCallback(async (templateId: string) => {
    return await invokeTauri('api_get_template', { templateId }) as FullTemplate;
  }, []);

  // Remove the user's override of a built-in template so the built-in applies again.
  const restoreTemplateDefault = useCallback(async (templateId: string) => {
    await invokeTauri('api_restore_template_default', { templateId });
    await refreshTemplates();
  }, [refreshTemplates]);

  const isCustomTemplate = useCallback(async (templateId: string): Promise<boolean> => {
    try {
      return await invokeTauri('api_is_custom_template', { templateId }) as boolean;
    } catch {
      return false;
    }
  }, []);

  // Handle template selection
  const handleTemplateSelection = useCallback((templateId: string, templateName: string) => {
    setSelectedTemplate(templateId);
    toast.success('Template selected', {
      description: `Using "${templateName}" template for summary generation`,
    });
  }, []);

  return {
    availableTemplates,
    selectedTemplate,
    handleTemplateSelection,
    refreshTemplates,
    saveCustomTemplate,
    deleteCustomTemplate,
    getTemplate,
    restoreTemplateDefault,
    isCustomTemplate,
  };
}
