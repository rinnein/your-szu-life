export interface CompileOptions {
  version: string;
  enableAsan: boolean;
  enableLsan: boolean;
}
export const standards: Record<string, readonly string[]> = {
  c: ["c99", "c11", "c17"],
  cpp: ["c++98", "c++03", "c++11", "c++14", "c++17", "c++20"],
};
export function compileOptions(
  language: string,
  saved?: CompileOptions | null,
): CompileOptions | null {
  if (!standards[language]) return null;
  const enableAsan = saved?.enableAsan ?? false;
  return {
    version:
      saved?.version && standards[language].includes(saved.version)
        ? saved.version
        : language === "c"
          ? "c17"
          : "c++20",
    enableAsan,
    enableLsan: enableAsan && (saved?.enableLsan ?? false),
  };
}
