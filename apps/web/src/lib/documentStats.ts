export interface DocumentStats {
  words: number;
  readingMinutes: number;
}

const WORDS_PER_MINUTE = 225;

export function getDocumentStats(content: string): DocumentStats {
  const plain = content
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/!\[[^\]]*]\([^)]*\)/g, " ")
    .replace(/\[[^\]]*]\([^)]*\)/g, " ")
    .replace(/[#>*_`~|[\](){}-]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  const words = plain ? plain.split(" ").filter(Boolean).length : 0;
  return {
    words,
    readingMinutes: Math.max(1, Math.ceil(words / WORDS_PER_MINUTE)),
  };
}
