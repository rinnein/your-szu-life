// PR 构建签名指令解析。
//
// 仓库 owner 只能在 PR 会话评论里用「独占一行」的固定命令表态，前缀可省略：
//   prod-sign:y / prod-sign:yes / y / yes  → 使用仓库正式签名
//   prod-sign:n / prod-sign:no  / n / no   → 使用一次性签名
//
// 其它任何写法（「确定」「签名」「prod-sign:yes please」「prod-sign:maybe」）
// 都不算表态，避免自然语言被误读成授权。大小写不敏感，允许行首空白、
// 冒号两侧空格与行尾空白。
//
// 自检：node .ci/pr-signing-command.mjs --selftest

const COMMAND = /^[ \t]*(?:prod-sign[ \t]*:[ \t]*)?(y|yes|n|no)[ \t]*\r?$/im;

/**
 * @param {string | null | undefined} body 评论正文
 * @returns {'y' | 'n' | null} 首个合法命令；没有合法命令时为 null
 */
export function parseSignCommand(body) {
  if (typeof body !== "string") return null;
  const match = COMMAND.exec(body);
  if (!match) return null;
  return /^y/i.test(match[1]) ? "y" : "n";
}

if (process.argv.includes("--selftest")) {
  const assert = (actual, expected) => {
    if (actual !== expected) {
      throw new Error(`prod-sign 解析自检失败：期望 ${expected}，实际 ${actual}`);
    }
  };
  assert(parseSignCommand("prod-sign:y"), "y");
  assert(parseSignCommand("prod-sign:yes"), "y");
  assert(parseSignCommand("  prod-sign : YES  "), "y");
  assert(parseSignCommand("prod-sign:y\r\n"), "y");
  assert(parseSignCommand("先说明一句\n\nprod-sign:yes\n谢谢"), "y");
  assert(parseSignCommand("prod-sign:n"), "n");
  assert(parseSignCommand("prod-sign:No"), "n");
  assert(parseSignCommand("prod-sign:no"), "n");
  // 前缀可省略，但同样必须独占一行。
  assert(parseSignCommand("y"), "y");
  assert(parseSignCommand(" YES "), "y");
  assert(parseSignCommand("N"), "n");
  assert(parseSignCommand("no"), "n");
  assert(parseSignCommand("先说明一句\n\nn\n谢谢"), "n");
  assert(parseSignCommand("确定"), null);
  assert(parseSignCommand("签名"), null);
  assert(parseSignCommand("prod-sign"), null);
  assert(parseSignCommand("prod-sign:maybe"), null);
  assert(parseSignCommand("prod-sign:yes please"), null);
  assert(parseSignCommand("prod-signy:yes"), null);
  assert(parseSignCommand("yes please"), null);
  assert(parseSignCommand("yo"), null);
  assert(parseSignCommand(""), null);
  assert(parseSignCommand(null), null);
  console.log("[pr-signing-command] 自检通过");
}
