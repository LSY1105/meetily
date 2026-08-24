// lib/punctuateCJK.ts
//
// ponytail: rule-based CJK punctuation inserter. The streaming emit
// path strips punctuation tokens (sherpa-onnx streaming partials do
// not commit them until endpoint), so the rendered text arrives as a
// continuous CJK run like "你好今天天气很好". We insert clause-level
// punctuation on the rendering side. Must be a pure string transform
// that NEVER mutates already-emitted characters — otherwise the
// next streaming tick fails `text.starts_with(prev)` and the iFlytek
// delta path falls into the correction branch. So we only ever add
// characters between existing characters; we never delete or move
// them.
//
// ponytail: this is now a client-side fallback. When the backend
// sherpa-onnx OfflinePunctuation model is loaded (see
// `worker.rs` start_streaming_task), punctuation is applied at emit
// time and arrives in `segment.text` already punctuated — this
// function's first guard (the "text already contains punctuation"
// early return) makes it a no-op pass-through. When the punctuator
// model is missing or failed to load, the frontend applies this
// rule-based version instead. Either way, hotword highlighting
// (`wrapHotwords`) runs AFTER punctuateCJK so the punctuation-
// agnostic hotword matcher sees punctuated text.

// Question particles that turn a declarative run into a question.
const QUESTION_PARTICLES = new Set(['吗', '呢', '吧']);

// Soft sentence-final particles that read like an ending without a
// strong question mark.
const SOFT_FINAL_PARTICLES = new Set([
    '啊', '呀', '哦', '哈', '嘛', '耶', '咯', '喔', '嗯', '唉',
]);

// Conjunctions / connectives — prepend a Chinese comma.
const CONJUNCTIONS = new Set([
    '但是', '然后', '所以', '不过', '如果', '因为', '于是',
    '可是', '并且', '而且', '甚至', '其实', '另外', '因此',
    '既然', '虽然', '然而', '不过', '只是', '只要', '即使',
]);

// Coordinating conjunctions that read like enumerations — prepend 、.
const LIST_CONJUNCTIONS = new Set(['和', '或', '跟', '与']);

const SENTENCE_END_PUNCT = new Set(['。', '？', '！', '…', '，', '、']);

function isCJK(ch: string): boolean {
    const code = ch.charCodeAt(0);
    // CJK Unified Ideographs + ext A/B + Hiragana/Katakana full block
    return (code >= 0x4e00 && code <= 0x9fff)
        || (code >= 0x3400 && code <= 0x4dbf)
        || (code >= 0xf900 && code <= 0xfaff)
        || (code >= 0x3040 && code <= 0x30ff);
}

function isLatinOrDigit(ch: string): boolean {
    const code = ch.charCodeAt(0);
    return (code >= 0x30 && code <= 0x39)
        || (code >= 0x41 && code <= 0x5a)
        || (code >= 0x61 && code <= 0x7a);
}

function endsWithSentencePunct(s: string): boolean {
    if (s.length === 0) return false;
    return SENTENCE_END_PUNCT.has(s[s.length - 1]);
}

/**
 * Insert clause-boundary CJK punctuation into a continuous CJK run.
 *
 * Streaming invariant: this function only ADDS characters between
 * existing characters; it never deletes, swaps, or moves anything.
 * So even if the streaming emit's `strip_prefix` check ran on this
 * function's output, it would still find the original characters in
 * order.
 *
 * @param text  The raw streaming hypothesis (no punctuation).
 * @returns     The same text with ，。？！ inserted at clause boundaries.
 */
export function punctuateCJK(text: string): string {
    if (!text) return text;
    // ponytail: skip short fragments. A 1-3 character partial mid-
    // stream is almost never a complete clause; inserting punctuation
    // here would create fake boundaries that disappear when the next
    // Mid arrives (e.g. "你好" → "你好。" → "你好今天" → we'd have
    // to delete the dot, which is forbidden by the streaming invariant).
    if (text.length < 4) return text;

    // ponytail: never re-punctuate text that already contains
    // punctuation. This guards against double-application and against
    // cases where the user / earlier pipeline already inserted dots.
    for (const ch of text) {
        if (SENTENCE_END_PUNCT.has(ch)) return text;
    }

    let out = '';
    const n = text.length;

    for (let i = 0; i < n; i++) {
        const ch = text[i];

        // Single-char list conjunction: "和", "或", "跟", "与" — insert 、 before.
        if (LIST_CONJUNCTIONS.has(ch)) {
            const prev = out[out.length - 1];
            const next = text[i + 1];
            // ponytail: only when sandwiched between two CJK chars
            // and the previous char is not already punctuation and
            // we are not at the very start (avoid "，和...").
            if (prev && isCJK(prev) && next && isCJK(next)) {
                out += '、';
            }
        }

        // Two-char conjunctions (longest match).
        if (i + 1 < n) {
            const pair = text.substring(i, i + 2);
            if (CONJUNCTIONS.has(pair)) {
                const prev = out[out.length - 1];
                const next = text[i + 2];
                if (prev && isCJK(prev) && next && isCJK(next)) {
                    out += '，';
                    // ponytail: emit the conjunction itself, then
                    // continue. We never insert punctuation at the
                    // start of the sentence ("但是..." stays
                    // unprefixed).
                }
            }
        }

        out += ch;

        // Sentence-final punctuation: if the run ends with a particle,
        // append the matching punctuation.
        const isLast = i === n - 1;
        if (isLast && isCJK(ch)) {
            if (QUESTION_PARTICLES.has(ch)) {
                out += '？';
            } else if (SOFT_FINAL_PARTICLES.has(ch)) {
                out += '。';
            } else if (!endsWithSentencePunct(out)) {
                // ponytail: complete declarative runs get a period
                // too, so the user always sees a terminal glyph.
                out += '。';
            }
        }
    }

    // ponytail: scrub the trailing "。" when the original text was
    // just an enumerative / mid-sentence fragment ending in a
    // particle that the model would extend on the next Mid. We
    // detect this by checking whether the *unpunctuated* source
    // text ended in a particle inside a longer sentence. The
    // cheapest signal: if the original text length is short
    // (< 6 chars) and ends with a particle, do not add the period
    // — let the next Mid extend it.
    if (out.endsWith('。') || out.endsWith('？')) {
        const origEndsInParticle =
            QUESTION_PARTICLES.has(text[text.length - 1])
            || SOFT_FINAL_PARTICLES.has(text[text.length - 1]);
        if (origEndsInParticle && text.length < 6) {
            // ponytail: streaming safety — short fragments ending in
            // a particle are likely the middle of a longer clause,
            // not a complete sentence. Strip the period we just
            // added.
            out = out.substring(0, out.length - 1);
        }
    }

    return out;
}

// ponytail: minimal sanity check; no test runner.
if (typeof window === 'undefined') {
    // @ts-ignore — module-shape check that runs under node only.
    if (typeof console !== 'undefined') {
        console.assert(punctuateCJK('你好今天天气很好') === '你好，今天天气很好。');
        console.assert(punctuateCJK('你吃饭了吗') === '你吃饭了吗？');
        console.assert(punctuateCJK('但是我觉得其实可以') === '但是，我觉得，其实可以。');
        console.assert(punctuateCJK('苹果和香蕉') === '苹果和香蕉');
        console.assert(punctuateCJK('好') === '好');
        console.assert(punctuateCJK('你好。') === '你好。'); // already punctuated, pass through
        console.assert(punctuateCJK('hello world') === 'hello world');
    }
}