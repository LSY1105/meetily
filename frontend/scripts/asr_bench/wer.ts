// WER (Word Error Rate) + CER (Character Error Rate) algorithm.
// Mirrors the jiwer contract so existing tooling can consume the output.

export interface NormalizeOptions {
  lowercase: boolean;
  removePunctuation: boolean;
  collapseWhitespace: boolean;
}

const DEFAULT_OPTIONS: NormalizeOptions = {
  lowercase: true,
  removePunctuation: true,
  collapseWhitespace: true,
};

const PUNCT_RE = /[!"#$%&'\(\)\*\+,\-\.\/:;<=>\?@\[\\\]^_`\{\|\}~]/g;

export function normalize(input: string, opts: NormalizeOptions = DEFAULT_OPTIONS): string {
  let s = input;
  if (opts.lowercase) s = s.toLowerCase();
  if (opts.removePunctuation) {
    s = s
      .replace(PUNCT_RE, ' ')
      .replace(/[\u3000-\u303f\uFF00-\uFFEF\u2026]/g, ' ');
  }
  if (opts.collapseWhitespace) s = s.replace(/\s+/g, ' ').trim();
  return s;
}

export function tokenize(input: string): string[] {
  return normalize(input).split(' ').filter((t) => t.length > 0);
}

export function characters(input: string): string[] {
  return Array.from(normalize(input, { ...DEFAULT_OPTIONS, removePunctuation: false }).replace(/\s+/g, ''));
}

export function editDistance(a: string[], b: string[]): number {
  const m = a.length;
  const n = b.length;
  if (m === 0) return n;
  if (n === 0) return m;
  let prev = new Array<number>(n + 1);
  let curr = new Array<number>(n + 1);
  for (let j = 0; j <= n; j++) prev[j] = j;
  for (let i = 1; i <= m; i++) {
    curr[0] = i;
    for (let j = 1; j <= n; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      curr[j] = Math.min(curr[j - 1] + 1, prev[j] + 1, prev[j - 1] + cost);
    }
    [prev, curr] = [curr, prev];
  }
  return prev[n];
}

export interface WERResult {
  wer: number;
  cer: number;
  substitutions: number;
  deletions: number;
  insertions: number;
  hits: number;
  refTokens: number;
  hypTokens: number;
}

export function wer(reference: string, hypothesis: string): WERResult {
  const ref = tokenize(reference);
  const hyp = tokenize(hypothesis);
  const subsDelsIns = computeAlignment(ref, hyp);
  const refChars = characters(reference);
  const hypChars = characters(hypothesis);
  const charDist = editDistance(refChars, hypChars);
  return {
    wer: ref.length === 0 ? (hyp.length === 0 ? 0 : 1) : subsDelsIns.errors / ref.length,
    cer: refChars.length === 0 ? (hypChars.length === 0 ? 0 : 1) : charDist / refChars.length,
    ...subsDelsIns,
    hits: subsDelsIns.hits,
    refTokens: ref.length,
    hypTokens: hyp.length,
  };
}

function computeAlignment(ref: string[], hyp: string[]): {
  errors: number;
  substitutions: number;
  deletions: number;
  insertions: number;
  hits: number;
} {
  const m = ref.length;
  const n = hyp.length;
  const dp: number[][] = Array.from({ length: m + 1 }, () => new Array<number>(n + 1).fill(0));
  const tb: string[][] = Array.from({ length: m + 1 }, () => new Array<string>(n + 1).fill(''));
  for (let i = 0; i <= m; i++) {
    dp[i][0] = i;
    tb[i][0] = 'D';
  }
  for (let j = 0; j <= n; j++) {
    dp[0][j] = j;
    tb[0][j] = 'I';
  }
  tb[0][0] = '';
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      if (ref[i - 1] === hyp[j - 1]) {
        dp[i][j] = dp[i - 1][j - 1];
        tb[i][j] = 'H';
      } else {
        const sub = dp[i - 1][j - 1] + 1;
        const del = dp[i - 1][j] + 1;
        const ins = dp[i][j - 1] + 1;
        if (sub <= del && sub <= ins) {
          dp[i][j] = sub;
          tb[i][j] = 'S';
        } else if (del <= ins) {
          dp[i][j] = del;
          tb[i][j] = 'D';
        } else {
          dp[i][j] = ins;
          tb[i][j] = 'I';
        }
      }
    }
  }
  let i = m;
  let j = n;
  let substitutions = 0;
  let deletions = 0;
  let insertions = 0;
  let hits = 0;
  while (i > 0 || j > 0) {
    const op = tb[i][j];
    if (op === 'H') { hits++; i--; j--; }
    else if (op === 'S') { substitutions++; i--; j--; }
    else if (op === 'D') { deletions++; i--; }
    else if (op === 'I') { insertions++; j--; }
    else break;
  }
  return { errors: substitutions + deletions + insertions, substitutions, deletions, insertions, hits };
}
