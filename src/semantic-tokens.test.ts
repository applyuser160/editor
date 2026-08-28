import { describe, expect, it } from "vitest";
import {
  DEFAULT_SEMANTIC_TOKEN_LEGEND,
  isSemanticTokensLegend,
  toMonacoSemanticTokens,
} from "./semantic-tokens";

describe("semantic token utilities", () => {
  it("provides the standard fallback legend", () => {
    expect(DEFAULT_SEMANTIC_TOKEN_LEGEND.tokenTypes).toContain("function");
    expect(DEFAULT_SEMANTIC_TOKEN_LEGEND.tokenModifiers).toContain("readonly");
  });

  it("validates a server-provided legend", () => {
    expect(
      isSemanticTokensLegend({
        tokenTypes: ["class", "function"],
        tokenModifiers: ["declaration"],
      }),
    ).toBe(true);
    expect(isSemanticTokensLegend({ tokenTypes: ["class"] })).toBe(false);
    expect(isSemanticTokensLegend(null)).toBe(false);
  });

  it("converts a valid LSP full response to Monaco data", () => {
    const result = toMonacoSemanticTokens({
      resultId: "42",
      data: [0, 0, 6, 12, 1],
    });

    expect(result?.resultId).toBe("42");
    expect(result?.data).toBeInstanceOf(Uint32Array);
    expect(Array.from(result?.data ?? [])).toEqual([0, 0, 6, 12, 1]);
  });

  it("rejects malformed or unsafe LSP responses", () => {
    expect(toMonacoSemanticTokens({ data: [0, 0, 1] })).toBeNull();
    expect(toMonacoSemanticTokens({ data: [0, 0, 1, 0, -1] })).toBeNull();
    expect(toMonacoSemanticTokens({ data: [0, 0, 1, 0, 1.5] })).toBeNull();
    expect(toMonacoSemanticTokens(null)).toBeNull();
  });
});
