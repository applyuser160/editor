export interface SemanticTokensLegend {
  tokenTypes: string[];
  tokenModifiers: string[];
}

export interface SemanticTokensResult {
  resultId?: string;
  data: Uint32Array;
}

/**
 * LSP's standard semantic token order. Servers normally return their own
 * legend during initialize; this keeps the provider useful while that
 * response is still in flight.
 */
export const DEFAULT_SEMANTIC_TOKEN_LEGEND: SemanticTokensLegend = {
  tokenTypes: [
    "namespace",
    "type",
    "class",
    "enum",
    "interface",
    "struct",
    "typeParameter",
    "parameter",
    "variable",
    "property",
    "enumMember",
    "event",
    "function",
    "method",
    "macro",
    "keyword",
    "modifier",
    "comment",
    "string",
    "number",
    "regexp",
    "operator",
  ],
  tokenModifiers: [
    "declaration",
    "definition",
    "readonly",
    "static",
    "deprecated",
    "abstract",
    "async",
    "modification",
    "documentation",
    "defaultLibrary",
  ],
};

export function isSemanticTokensLegend(
  value: unknown,
): value is SemanticTokensLegend {
  if (!value || typeof value !== "object") return false;
  const legend = value as Partial<SemanticTokensLegend>;
  return (
    Array.isArray(legend.tokenTypes) &&
    legend.tokenTypes.every((tokenType) => typeof tokenType === "string") &&
    Array.isArray(legend.tokenModifiers) &&
    legend.tokenModifiers.every((modifier) => typeof modifier === "string")
  );
}

/** Convert an LSP full semantic-token response to Monaco's representation. */
export function toMonacoSemanticTokens(
  value: unknown,
): SemanticTokensResult | null {
  if (!value || typeof value !== "object") return null;
  const response = value as { resultId?: unknown; data?: unknown };
  if (!Array.isArray(response.data) || response.data.length % 5 !== 0) {
    return null;
  }

  const data = response.data.map((token) => Number(token));
  if (
    data.some(
      (token) => !Number.isInteger(token) || token < 0 || token > 0xffffffff,
    )
  ) {
    return null;
  }

  return {
    data: new Uint32Array(data),
    ...(typeof response.resultId === "string"
      ? { resultId: response.resultId }
      : {}),
  };
}
