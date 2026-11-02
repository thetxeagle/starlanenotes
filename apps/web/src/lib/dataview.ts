export interface DataviewDocument {
  id: string;
  title: string;
  human_identifier?: string | null;
  maintenance_user_id?: string | null;
  maintenance_group_id?: string | null;
  content?: string;
  space_id?: string;
  space_name?: string;
  tags?: string[];
  properties?: Record<string, string | number | boolean | null | undefined>;
  created_at?: string;
  updated_at?: string;
}

export type DataviewOperator = "contains" | "equals" | "fuzzy" | "is" | "is_not";

export type DataviewExpression =
  | { type: "predicate"; field?: string; operator: DataviewOperator; value: string }
  | { type: "not"; expression: DataviewExpression }
  | { type: "and" | "or"; left: DataviewExpression; right: DataviewExpression };

export interface DataviewQuery {
  source: string;
  expression: DataviewExpression | null;
}

type TokenType = "word" | "quoted" | "colon" | "equals" | "tilde" | "lparen" | "rparen";

interface Token {
  type: TokenType;
  value: string;
}

export function parseDataviewQuery(source: string): DataviewQuery {
  const tokens = tokenize(source);
  const parser = new DataviewParser(tokens);
  return {
    source,
    expression: parser.parseExpression(),
  };
}

export function filterDataviewDocuments(
  documents: DataviewDocument[],
  query: string | DataviewQuery,
  limit = 50,
): DataviewDocument[] {
  const parsed = typeof query === "string" ? parseDataviewQuery(query) : query;
  if (!parsed.expression) {
    return documents.slice(0, limit);
  }
  return documents
    .filter((document) => evaluateDataviewExpression(document, parsed.expression!))
    .slice(0, limit);
}

export function evaluateDataviewExpression(
  document: DataviewDocument,
  expression: DataviewExpression,
): boolean {
  switch (expression.type) {
    case "and":
      return (
        evaluateDataviewExpression(document, expression.left) &&
        evaluateDataviewExpression(document, expression.right)
      );
    case "or":
      return (
        evaluateDataviewExpression(document, expression.left) ||
        evaluateDataviewExpression(document, expression.right)
      );
    case "not":
      return !evaluateDataviewExpression(document, expression.expression);
    case "predicate":
      return evaluatePredicate(document, expression);
  }
}

function tokenize(source: string): Token[] {
  const tokens: Token[] = [];
  let index = 0;
  while (index < source.length) {
    const char = source[index];
    if (/\s/.test(char)) {
      index += 1;
      continue;
    }
    if (char === '"') {
      const start = index + 1;
      index += 1;
      let value = "";
      while (index < source.length) {
        if (source[index] === "\\" && index + 1 < source.length) {
          value += source[index + 1];
          index += 2;
          continue;
        }
        if (source[index] === '"') break;
        value += source[index];
        index += 1;
      }
      tokens.push({ type: "quoted", value: value || source.slice(start, index) });
      index += source[index] === '"' ? 1 : 0;
      continue;
    }
    if (char === ":") {
      tokens.push({ type: "colon", value: char });
      index += 1;
      continue;
    }
    if (char === "=") {
      tokens.push({ type: "equals", value: char });
      index += 1;
      continue;
    }
    if (char === "~") {
      tokens.push({ type: "tilde", value: char });
      index += 1;
      continue;
    }
    if (char === "(") {
      tokens.push({ type: "lparen", value: char });
      index += 1;
      continue;
    }
    if (char === ")") {
      tokens.push({ type: "rparen", value: char });
      index += 1;
      continue;
    }

    let value = "";
    while (
      index < source.length &&
      !/\s/.test(source[index]) &&
      ![":", "=", "~", "(", ")"].includes(source[index])
    ) {
      value += source[index];
      index += 1;
    }
    tokens.push({ type: "word", value });
  }
  return tokens;
}

class DataviewParser {
  private index = 0;

  constructor(private readonly tokens: Token[]) {}

  parseExpression(): DataviewExpression | null {
    if (this.tokens.length === 0) return null;
    return this.parseOr();
  }

  private parseOr(): DataviewExpression {
    let expression = this.parseAnd();
    while (this.matchWord("or")) {
      expression = { type: "or", left: expression, right: this.parseAnd() };
    }
    return expression;
  }

  private parseAnd(): DataviewExpression {
    let expression = this.parseUnary();
    while (this.hasMore() && !this.peekType("rparen") && !this.peekWord("or")) {
      if (this.peekWord("and")) this.index += 1;
      expression = { type: "and", left: expression, right: this.parseUnary() };
    }
    return expression;
  }

  private parseUnary(): DataviewExpression {
    if (this.matchWord("not")) {
      return { type: "not", expression: this.parseUnary() };
    }
    if (this.matchType("lparen")) {
      const expression = this.parseOr();
      this.matchType("rparen");
      return expression;
    }
    return this.parsePredicate();
  }

  private parsePredicate(): DataviewExpression {
    const first = this.consumeValue();
    if (!first) return { type: "predicate", operator: "contains", value: "" };

    if (this.matchType("colon")) {
      return {
        type: "predicate",
        field: first,
        operator: "contains",
        value: this.consumeValue() ?? "",
      };
    }
    if (this.matchType("equals")) {
      return {
        type: "predicate",
        field: first,
        operator: "equals",
        value: this.consumeValue() ?? "",
      };
    }
    if (this.matchType("tilde")) {
      return {
        type: "predicate",
        field: first,
        operator: "fuzzy",
        value: this.consumeValue() ?? "",
      };
    }
    if (this.matchWord("is")) {
      const isNot = this.matchWord("not");
      return {
        type: "predicate",
        field: first,
        operator: isNot ? "is_not" : "is",
        value: this.consumeValue() ?? "",
      };
    }

    return {
      type: "predicate",
      operator: "contains",
      value: first,
    };
  }

  private consumeValue(): string | null {
    const token = this.tokens[this.index];
    if (!token || (token.type !== "word" && token.type !== "quoted")) return null;
    this.index += 1;
    return token.value;
  }

  private hasMore(): boolean {
    return this.index < this.tokens.length;
  }

  private peekType(type: TokenType): boolean {
    return this.tokens[this.index]?.type === type;
  }

  private matchType(type: TokenType): boolean {
    if (!this.peekType(type)) return false;
    this.index += 1;
    return true;
  }

  private peekWord(value: string): boolean {
    const token = this.tokens[this.index];
    return token?.type === "word" && token.value.toLowerCase() === value;
  }

  private matchWord(value: string): boolean {
    if (!this.peekWord(value)) return false;
    this.index += 1;
    return true;
  }
}

function evaluatePredicate(
  document: DataviewDocument,
  predicate: Extract<DataviewExpression, { type: "predicate" }>,
): boolean {
  const field = predicate.field
    ? ["page.identifier", "page.maintenance_user", "page.maintenance_group"].includes(
        predicate.field.trim().toLowerCase(),
      )
      ? predicate.field.trim().toLowerCase()
      : normalizeFieldName(predicate.field)
    : undefined;
  if (field === "page.maintenance_user" || field === "page.maintenance_group") {
    const actual = valuesForField(document, field)[0] ?? "";
    if (predicate.operator === "is_not") return actual !== predicate.value;
    if (predicate.operator === "is" || predicate.operator === "equals")
      return actual === predicate.value;
    if (predicate.operator === "fuzzy") return fuzzyIncludes(actual, predicate.value);
    return predicate.value.includes("*")
      ? wildcardMatches(actual, predicate.value)
      : actual.includes(predicate.value);
  }
  const expected = normalizeComparable(predicate.value);
  const values = field ? valuesForField(document, field) : valuesForFreeText(document);

  if (predicate.operator === "is_not") {
    return values.every((value) => !valueMatches(value, expected, "is"));
  }
  return values.some((value) => valueMatches(value, expected, predicate.operator));
}

function valuesForFreeText(document: DataviewDocument): string[] {
  return [document.title, document.content ?? ""];
}

function valuesForField(document: DataviewDocument, field: string): string[] {
  if (field === "page.identifier") return [document.human_identifier ?? ""];
  if (field === "page.maintenance_user") return [document.maintenance_user_id ?? ""];
  if (field === "page.maintenance_group") return [document.maintenance_group_id ?? ""];
  if (field === "title" || field === "name") return [document.title];
  if (field === "content" || field === "text") return [document.content ?? ""];
  if (field === "space" || field === "spaceid") {
    return [document.space_id ?? "", document.space_name ?? ""];
  }
  if (field === "tag" || field === "tags") return document.tags ?? [];
  if (field === "created" || field === "createdat") return [document.created_at ?? ""];
  if (field === "updated" || field === "updatedat") return [document.updated_at ?? ""];

  const properties = document.properties ?? {};
  const matching = Object.entries(properties).find(([key]) => normalizeFieldName(key) === field);
  if (!matching) return [];
  const value = matching[1];
  return Array.isArray(value) ? value.map(String) : [String(value ?? "")];
}

function valueMatches(value: string, expected: string, operator: DataviewOperator): boolean {
  const actual = normalizeComparable(value);
  if (operator === "equals" || operator === "is") return actual === expected;
  if (operator === "fuzzy") return fuzzyIncludes(actual, expected);
  if (expected.includes("*")) return wildcardMatches(actual, expected);
  return actual.includes(expected);
}

function normalizeComparable(value: string): string {
  const trimmed = value.trim().replace(/^#/, "").toLowerCase();
  const date = normalizeDate(trimmed);
  return date ?? trimmed;
}

function normalizeFieldName(value: string): string {
  return value
    .trim()
    .replace(/^property[.:]/i, "")
    .replace(/^#/, "tag")
    .toLowerCase()
    .replace(/[^a-z0-9]/g, "");
}

function normalizeDate(value: string): string | null {
  const match = value.match(/^(\d{4})[-/](\d{1,2})[-/](\d{1,2})$/);
  if (!match) return null;
  const [, year, month, day] = match;
  return `${year}-${month.padStart(2, "0")}-${day.padStart(2, "0")}`;
}

function wildcardMatches(value: string, pattern: string): boolean {
  const escaped = pattern
    .split("*")
    .map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
    .join(".*");
  return new RegExp(`^${escaped}$`).test(value);
}

function fuzzyIncludes(value: string, expected: string): boolean {
  let cursor = 0;
  for (const char of expected) {
    cursor = value.indexOf(char, cursor);
    if (cursor === -1) return false;
    cursor += 1;
  }
  return true;
}
