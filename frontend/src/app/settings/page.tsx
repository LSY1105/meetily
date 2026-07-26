'use client';

import React, { useState, useEffect, useLayoutEffect, useRef, useMemo } from 'react';
import { ArrowLeft, Settings2, Mic, Database as DatabaseIcon, SparkleIcon, FlaskConical, Search } from 'lucide-react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { motion } from 'framer-motion';
import { TranscriptSettings } from '@/components/TranscriptSettings';
import { RecordingSettings } from '@/components/RecordingSettings';
import { PreferenceSettings } from '@/components/PreferenceSettings';
import { SummaryModelSettings } from '@/components/SummaryModelSettings';
import { BetaSettings } from '@/components/BetaSettings';
import { useConfig } from '@/contexts/ConfigContext';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Input } from '@/components/ui/input';
import { useTranslations } from 'next-intl';

// Tabs configuration ordered by likely-use frequency.
// Value is the i18n key suffix (with two values aliased inside the render map below).
const TAB_DEFS = [
  { value: 'recording', icon: Mic },
  { value: 'Transcriptionmodels', icon: DatabaseIcon },
  { value: 'summaryModels', icon: SparkleIcon },
  { value: 'general', icon: Settings2 },
  { value: 'beta', icon: FlaskConical },
] as const;

function tabLabelKey(value: string): string {
  if (value === 'Transcriptionmodels') return 'transcript';
  if (value === 'summaryModels') return 'summary';
  return value;
}

export default function SettingsPage() {
  const router = useRouter();
  const { transcriptModelConfig, setTranscriptModelConfig } = useConfig();
  const tSettings = useTranslations('settings');

  const [activeTab, setActiveTab] = useState('recording');
  const [query, setQuery] = useState('');
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [underlineStyle, setUnderlineStyle] = useState({ left: 0, width: 0 });

  useEffect(() => {
    const loadTranscriptConfig = async () => {
      try {
        const config = await invoke('api_get_transcript_config') as any;
        if (config) {
          setTranscriptModelConfig({
            provider: config.provider || 'localWhisper',
            model: config.model || 'large-v3',
            apiKey: config.apiKey || null
          });
        }
      } catch (error) {
        console.error('Failed to load transcript config:', error);
      }
    };
    loadTranscriptConfig();
  }, [setTranscriptModelConfig]);

  const filteredTabs = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return TAB_DEFS;
    return TAB_DEFS.filter((tab) =>
      tSettings(`tabs.${tabLabelKey(tab.value)}`).toLowerCase().includes(q)
    );
  }, [query]);

  useLayoutEffect(() => {
    const activeIndex = filteredTabs.findIndex((tab) => tab.value === activeTab);
    const activeTabElement = tabRefs.current[activeIndex];
    if (activeTabElement) {
      const { offsetLeft, offsetWidth } = activeTabElement;
      setUnderlineStyle({ left: offsetLeft, width: offsetWidth });
    }
  }, [activeTab, filteredTabs]);

  return (
    <div className="h-screen bg-gray-50 flex flex-col">
      {/* Fixed Header */}
      <div className="sticky top-0 z-10 bg-gray-50 border-b border-gray-200">
        <div className="max-w-6xl mx-auto px-8 py-6">
          <div className="flex items-center gap-4">
            <button
              onClick={() => router.back()}
              className="flex items-center gap-2 text-gray-600 hover:text-gray-900 transition-colors"
            >
              <ArrowLeft className="w-5 h-5" />
              <span>{tSettings('shell.back')}</span>
            </button>
            <h1 className="text-3xl font-bold">{tSettings('title')}</h1>
          </div>
        </div>
      </div>

      {/* Scrollable Content */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-6xl mx-auto p-8 pt-6">
          <Tabs value={activeTab} onValueChange={setActiveTab}>
            {/* Search box */}
            <div className="relative max-w-md mb-4">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400 pointer-events-none" />
              <Input
                type="text"
                placeholder={tSettings('shell.search_placeholder')}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                className="pl-9"
              />
            </div>

            {filteredTabs.length === 0 ? (
              <p className="text-sm text-gray-500 py-4">{tSettings('shell.no_results')}</p>
            ) : (
              <TabsList className="bg-transparent relative rounded-none border-b border-gray-200 p-0 h-auto">
                {filteredTabs.map((tab) => {
                  const Icon = tab.icon;
                  const filteredIndex = filteredTabs.findIndex((t) => t.value === tab.value);
                  return (
                    <TabsTrigger
                      key={tab.value}
                      value={tab.value}
                      ref={(el) => { tabRefs.current[filteredIndex] = el }}
                      className="flex items-center gap-2 px-6 py-4 bg-transparent rounded-none border-0 data-[state=active]:bg-transparent data-[state=active]:text-blue-600 data-[state=active]:shadow-none text-gray-600 hover:text-gray-900 relative z-10"
                    >
                      <Icon className="w-4 h-4" />
                      {tSettings(`tabs.${tabLabelKey(tab.value)}`)}
                    </TabsTrigger>
                  );
                })}

                <motion.div
                  className="absolute bottom-0 z-20 h-0.5 bg-blue-600"
                  layoutId="underline"
                  style={{ left: underlineStyle.left, width: underlineStyle.width }}
                  transition={{ type: 'spring', stiffness: 400, damping: 40 }}
                />
              </TabsList>
            )}

            <TabsContent value="general">
              <PreferenceSettings />
            </TabsContent>
            <TabsContent value="recording">
              <RecordingSettings />
            </TabsContent>
            <TabsContent value="Transcriptionmodels">
              <TranscriptSettings
                transcriptModelConfig={transcriptModelConfig}
                setTranscriptModelConfig={setTranscriptModelConfig}
              />
            </TabsContent>
            <TabsContent value="summaryModels">
              <SummaryModelSettings />
            </TabsContent>
            <TabsContent value="beta" className="mt-6">
              <BetaSettings />
            </TabsContent>
          </Tabs>
        </div>
      </div>
    </div>
  );
};
