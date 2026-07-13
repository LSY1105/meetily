import { describe, it, expect } from "vitest";
import { wer, tokenize, editDistance } from "../wer";

describe("wer", () => {
  it("returns 0 for identical inputs", () => {
    const r = wer("hello world", "hello world");
    expect(r.wer).toBe(0);
    expect(r.cer).toBe(0);
    expect(r.hits).toBe(2);
  });

  it("counts a single substitution", () => {
    const r = wer("hello world", "hello there");
    expect(r.wer).toBeCloseTo(0.5, 5);
    expect(r.substitutions).toBe(1);
  });

  it("counts deletions", () => {
    const r = wer("the quick brown fox", "quick brown fox");
    expect(r.deletions).toBe(1);
    expect(r.wer).toBeCloseTo(1 / 4, 5);
  });

  it("counts insertions", () => {
    const r = wer("quick fox", "the quick brown fox");
    expect(r.insertions).toBe(2);
    expect(r.wer).toBeGreaterThan(0);
  });

  it("handles empty reference", () => {
    const r = wer("", "anything");
    expect(r.wer).toBe(1);
  });

  it("normalizes punctuation and case", () => {
    const r = wer("Hello, World!", "hello world");
    expect(r.wer).toBe(0);
  });

  it("handles chinese characters (1 substitution)", () => {
    const r = wer("今天 天气 很好", "今天 天气 不太好");
    expect(r.substitutions).toBe(1);
    expect(r.wer).toBeCloseTo(1 / 3, 5);
  });
});

describe("tokenize", () => {
  it("collapses whitespace and lowercases", () => {
    expect(tokenize("  Hello,   WORLD! ")).toEqual(["hello", "world"]);
  });
});

describe("editDistance", () => {
  it("computes Levenshtein distance", () => {
    expect(editDistance(["a", "b", "c"], ["a", "c"])).toBe(1);
    expect(editDistance([], ["x"])).toBe(1);
    expect(editDistance(["a", "b"], ["a", "b"])).toBe(0);
  });
});
