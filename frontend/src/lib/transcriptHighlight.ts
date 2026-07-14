/**
 * Transcript highlighting (Wave 14 PR-44a).
 *
 * Pure-function tokenizer + renderer. No React, no DOM-specific code in
 * the core; renderer emits an array of React-ready nodes for the caller
 * to splice into JSX.
 */
export type HighlightCategory = "number" | "date" | "url" | "proper" | "custom";

export interface HighlightConfig {
    enabled: boolean;
    /** User-supplied keywords, comma-separated. Case-insensitive. */
    customKeywords: string;
}

export interface HighlightToken {
    text: string;
    category: HighlightCategory | null;
}

export const DEFAULT_HIGHLIGHT_CONFIG: HighlightConfig = {
    enabled: true,
    customKeywords: "",
};

interface CompiledPattern {
    category: HighlightCategory;
    regex: RegExp;
}

const PATTERNS: CompiledPattern[] = [
    { category: "url", regex: /https?:\/\/[A-Za-z0-9._\/?=&%-]+|[A-Za-z0-9_.+-]+@[A-Za-z0-9-]+\.[A-Za-z0-9.-]+/g },
    { category: "date", regex: /\d{4}[-/年]\d{1,2}[-/月]\d{1,2}(?:日)?|\d{1,2}[-/月]\d{1,2}(?:日)?|明天|今天|后天|下周[一二三四五六日天]?|上周[一二三四五六日天]?|本周[一二三四五六日天]?/g },
    { category: "number", regex: /(?:[¥$€£]\s?)?\d+(?:[,.]\d+)*(?:%|万|亿|千|百|十)?|\d+\.\d+|百分之\d+/g },
    { category: "proper", regex: /\b[A-Z][a-zA-Z]+(?:\s+[A-Z][a-zA-Z]+)*\b|\b[A-Z]{2,}\b/g },
];

function buildCustomRegex(keywords: string): RegExp | null {
    const parts = keywords
        .split(",")
        .map((w) => w.trim())
        .filter(Boolean)
        .map((w) => w.replace(/[-\/\\^$*+?.()|[\\]{}]/g, "\\$&"));
    if (parts.length === 0) return null;
    return new RegExp("\\b(?:" + parts.join("|") + ")\\b", "gi");
}

interface Span {
    start: number;
    end: number;
    category: HighlightCategory;
}

function collectSpans(text: string, config: HighlightConfig): Span[] {
    const spans: Span[] = [];
    for (const p of PATTERNS) {
        p.regex.lastIndex = 0;
        let m: RegExpExecArray | null;
        while ((m = p.regex.exec(text)) !== null) {
            if (m[0].length === 0) {
                p.regex.lastIndex++;
                continue;
            }
            spans.push({ start: m.index, end: m.index + m[0].length, category: p.category });
        }
    }
    if (config.customKeywords) {
        const re = buildCustomRegex(config.customKeywords);
        if (re) {
            re.lastIndex = 0;
            let m: RegExpExecArray | null;
            while ((m = re.exec(text)) !== null) {
                if (m[0].length === 0) {
                    re.lastIndex++;
                    continue;
                }
                spans.push({ start: m.index, end: m.index + m[0].length, category: "custom" });
            }
        }
    }
    spans.sort((a, b) => (a.start - b.start) || (b.end - a.end));
    const filtered: Span[] = [];
    let lastEnd = -1;
    for (const s of spans) {
        if (s.start >= lastEnd) {
            filtered.push(s);
            lastEnd = s.end;
        }
    }
    return filtered;
}

export function tokenize(text: string, config: HighlightConfig): HighlightToken[] {
    if (!config.enabled) return [{ text, category: null }];
    const spans = collectSpans(text, config);
    if (spans.length === 0) return [{ text, category: null }];
    const tokens: HighlightToken[] = [];
    let cursor = 0;
    for (const span of spans) {
        if (span.start > cursor) {
            tokens.push({ text: text.slice(cursor, span.start), category: null });
        }
        tokens.push({ text: text.slice(span.start, span.end), category: span.category });
        cursor = span.end;
    }
    if (cursor < text.length) {
        tokens.push({ text: text.slice(cursor), category: null });
    }
    return tokens;
}

export const HIGHLIGHT_CLASSES: Record<HighlightCategory, string> = {
    number: "bg-amber-100 text-amber-900 rounded px-0.5",
    date: "bg-emerald-100 text-emerald-900 rounded px-0.5",
    url: "bg-sky-100 text-sky-900 underline rounded px-0.5",
    proper: "bg-violet-100 text-violet-900 rounded px-0.5",
    custom: "bg-rose-100 text-rose-900 rounded px-0.5",
};
