'use client';

import { useState, useRef, useEffect } from 'react';
import { useTranslations } from 'next-intl';
import { X, Send, Loader2, Globe, Search, MessageSquareText, ChevronDown, ChevronUp, KeyRound } from 'lucide-react';
import { askMeeting, setSearchKey, hasSearchKey, QaSource, QaScope } from '@/services/qaService';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';

interface QaMessage {
  role: 'user' | 'assistant';
  text: string;
  sources?: QaSource[];
  error?: boolean;
}

/** Splits an answer into text runs and [n] citation tokens for rendering. */
function parseCitations(text: string): Array<{ type: 'text' | 'cite'; value: string }> {
  const parts: Array<{ type: 'text' | 'cite'; value: string }> = [];
  const regex = /\[(\d+)\]/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = regex.exec(text)) !== null) {
    if (m.index > last) parts.push({ type: 'text', value: text.slice(last, m.index) });
    parts.push({ type: 'cite', value: m[1] });
    last = m.index + m[0].length;
  }
  if (last < text.length) parts.push({ type: 'text', value: text.slice(last) });
  return parts;
}

export function AskAiPanel({
  meetingId,
  onClose,
}: {
  meetingId: string;
  onClose: () => void;
}) {
  const t = useTranslations('summary');
  const [scope, setScope] = useState<QaScope>('meeting');
  const [useWeb, setUseWeb] = useState(false);
  const [input, setInput] = useState('');
  const [messages, setMessages] = useState<QaMessage[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [searchKeyConfigured, setSearchKeyConfigured] = useState<boolean | null>(null);
  const [searchKeyInput, setSearchKeyInput] = useState('');
  const [showSearchKeyInput, setShowSearchKeyInput] = useState(false);
  const [expandedSource, setExpandedSource] = useState<number | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    hasSearchKey().then(setSearchKeyConfigured).catch(() => setSearchKeyConfigured(false));
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, isLoading]);

  const handleSaveSearchKey = async () => {
    if (!searchKeyInput.trim()) return;
    try {
      await setSearchKey(searchKeyInput.trim());
      setSearchKeyConfigured(true);
      setShowSearchKeyInput(false);
      setSearchKeyInput('');
    } catch (err) {
      console.error('Failed to save search key:', err);
    }
  };

  const handleAsk = async () => {
    const question = input.trim();
    if (!question || isLoading) return;

    setInput('');
    setMessages((prev) => [...prev, { role: 'user', text: question }]);
    setIsLoading(true);

    try {
      const result = await askMeeting({
        question,
        scope,
        meetingId: scope === 'meeting' ? meetingId : undefined,
        useWeb,
      });
      setMessages((prev) => [
        ...prev,
        { role: 'assistant', text: result.answer, sources: result.sources },
      ]);
    } catch (err) {
      const message = typeof err === 'string' ? err : err instanceof Error ? err.message : String(err);
      setMessages((prev) => [...prev, { role: 'assistant', text: message, error: true }]);
    } finally {
      setIsLoading(false);
    }
  };

  const renderAnswer = (msg: QaMessage, msgIdx: number) => {
    const parts = parseCitations(msg.text);
    return (
      <div className="space-y-2">
        <div className="text-sm leading-relaxed whitespace-pre-wrap">
          {parts.map((p, i) =>
            p.type === 'text' ? (
              <span key={i}>{p.value}</span>
            ) : (
              <button
                key={i}
                onClick={() =>
                  setExpandedSource((prev) => (prev === Number(p.value) ? null : Number(p.value)))
                }
                className="inline-flex items-center justify-center min-w-[1.4rem] h-5 px-1 mx-0.5 rounded bg-blue-100 text-blue-700 text-xs font-medium hover:bg-blue-200 align-middle"
                title={msg.sources?.find((s) => s.index === Number(p.value))?.label}
              >
                {p.value}
              </button>
            )
          )}
        </div>

        {msg.sources && msg.sources.length > 0 && (
          <div className="space-y-1.5">
            <p className="text-xs text-gray-400">{t('qa_sources')}</p>
            {msg.sources.map((s) => (
              <div key={s.index} className="text-xs">
                <button
                  onClick={() =>
                    setExpandedSource((prev) => (prev === s.index ? null : s.index))
                  }
                  className={`flex items-center gap-1.5 text-left rounded px-1.5 py-1 w-full ${
                    expandedSource === s.index ? 'bg-blue-50' : 'hover:bg-gray-50'
                  }`}
                >
                  <span className="inline-flex items-center justify-center min-w-[1.3rem] h-4 px-1 rounded bg-gray-100 text-gray-600 font-medium">
                    {s.index}
                  </span>
                  {s.kind === 'web' ? (
                    <Globe className="h-3 w-3 text-gray-400 flex-shrink-0" />
                  ) : s.kind === 'summary' ? (
                    <MessageSquareText className="h-3 w-3 text-gray-400 flex-shrink-0" />
                  ) : (
                    <Search className="h-3 w-3 text-gray-400 flex-shrink-0" />
                  )}
                  <span className="truncate text-gray-600">{s.label}</span>
                  {expandedSource === s.index ? (
                    <ChevronUp className="h-3 w-3 ml-auto flex-shrink-0 text-gray-400" />
                  ) : (
                    <ChevronDown className="h-3 w-3 ml-auto flex-shrink-0 text-gray-400" />
                  )}
                </button>
                {expandedSource === s.index && (
                  <div className="ml-6 mt-1 p-2 rounded bg-gray-50 text-gray-600 whitespace-pre-wrap border border-gray-100">
                    {s.url ? (
                      <a
                        href={s.url}
                        target="_blank"
                        rel="noreferrer"
                        className="block text-blue-600 hover:underline mb-1 break-all"
                      >
                        {s.url}
                      </a>
                    ) : null}
                    {s.snippet}
                  </div>
                )}
              </div>
            ))}
          </div>
        )}

        {msg.error && (
          <div className="text-xs text-red-500">{t('qa_error_hint')}</div>
        )}

        {/* keeps msgIdx referenced for future per-message actions */}
        <span className="hidden">{msgIdx}</span>
      </div>
    );
  };

  return (
    <div className="flex flex-col h-full bg-white border-l border-gray-200 w-[360px] flex-shrink-0">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-gray-100">
        <h3 className="text-sm font-semibold text-gray-800">{t('qa_title')}</h3>
        <button onClick={onClose} className="p-1 rounded hover:bg-gray-100" aria-label="close">
          <X className="h-4 w-4 text-gray-500" />
        </button>
      </div>

      {/* Scope + web toggles */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-100">
        <div className="flex rounded-md border border-gray-200 overflow-hidden text-xs">
          <button
            onClick={() => setScope('meeting')}
            className={`px-2.5 py-1 ${scope === 'meeting' ? 'bg-gray-900 text-white' : 'bg-white text-gray-600 hover:bg-gray-50'}`}
          >
            {t('qa_scope_meeting')}
          </button>
          <button
            onClick={() => setScope('all')}
            className={`px-2.5 py-1 ${scope === 'all' ? 'bg-gray-900 text-white' : 'bg-white text-gray-600 hover:bg-gray-50'}`}
          >
            {t('qa_scope_all')}
          </button>
        </div>
        <button
          onClick={() => {
            if (!useWeb && searchKeyConfigured === false) {
              setShowSearchKeyInput(true);
            }
            setUseWeb((v) => !v);
          }}
          className={`flex items-center gap-1 px-2 py-1 rounded-md border text-xs ${
            useWeb ? 'bg-blue-50 border-blue-200 text-blue-700' : 'border-gray-200 text-gray-600 hover:bg-gray-50'
          }`}
          title={t('qa_web_toggle')}
        >
          <Globe className="h-3 w-3" />
          {t('qa_web')}
        </button>
        <button
          onClick={() => setShowSearchKeyInput((v) => !v)}
          className="p-1 rounded hover:bg-gray-100 ml-auto"
          title={t('qa_search_key')}
        >
          <KeyRound
            className={`h-3.5 w-3.5 ${searchKeyConfigured ? 'text-green-500' : 'text-gray-300'}`}
          />
        </button>
      </div>

      {/* Search key input (collapsible) */}
      {showSearchKeyInput && (
        <div className="px-3 py-2 border-b border-gray-100 bg-gray-50 space-y-1.5">
          <p className="text-xs text-gray-500">{t('qa_search_key_hint')}</p>
          <div className="flex gap-1.5">
            <Input
              value={searchKeyInput}
              onChange={(e) => setSearchKeyInput(e.target.value)}
              placeholder="tvly-..."
              className="h-7 text-xs"
              type="password"
            />
            <Button size="sm" className="h-7 text-xs px-2" onClick={handleSaveSearchKey}>
              {t('qa_save')}
            </Button>
          </div>
        </div>
      )}

      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-3 py-3 space-y-3">
        {messages.length === 0 && (
          <p className="text-xs text-gray-400 text-center mt-8">{t('qa_empty')}</p>
        )}
        {messages.map((msg, i) => (
          <div
            key={i}
            className={
              msg.role === 'user'
                ? 'ml-6 p-2.5 rounded-lg bg-blue-600 text-white text-sm whitespace-pre-wrap'
                : msg.error
                  ? 'mr-2 p-2.5 rounded-lg bg-red-50 border border-red-100'
                  : 'mr-2 p-2.5 rounded-lg bg-gray-50 border border-gray-100'
            }
          >
            {msg.role === 'user' ? msg.text : renderAnswer(msg, i)}
          </div>
        ))}
        {isLoading && (
          <div className="mr-2 p-2.5 rounded-lg bg-gray-50 border border-gray-100 flex items-center gap-2 text-sm text-gray-500">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            {t('qa_thinking')}
          </div>
        )}
        <div ref={bottomRef} />
      </div>

      {/* Input */}
      <div className="p-3 border-t border-gray-100">
        <div className="flex gap-1.5">
          <Input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.nativeEvent.isComposing) {
                e.preventDefault();
                handleAsk();
              }
            }}
            placeholder={t('qa_placeholder')}
            className="h-9 text-sm"
            disabled={isLoading}
          />
          <Button size="sm" className="h-9 px-2.5" onClick={handleAsk} disabled={isLoading || !input.trim()}>
            <Send className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  );
}
