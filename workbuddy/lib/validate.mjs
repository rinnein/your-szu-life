// Validate the WorkBuddy expert package against the open-platform contract and
// the extra invariants this integration relies on.

import { readFile, stat } from "node:fs/promises";
import path from "node:path";

import { walkFiles } from "./zip.mjs";

const CATEGORY_IDS = new Set([
  "01-ProductDesign",
  "02-Engineering",
  "03-GameSpatial",
  "04-DataAI",
  "05-MarketingGrowth",
  "06-ContentCreative",
  "07-SalesCommerce",
  "08-FinanceInvestment",
  "09-OperationsHR",
  "10-ProjectQuality",
  "11-SecurityCompliance",
  "12-IndustryConsultant",
  "13-TencentZone",
  "14-WorldWise",
  "15-Education",
]);

const REQUIRED_REFERENCES = [
  "install-linux.md",
  "install-windows.md",
  "tool-workflows.md",
  "troubleshooting.md",
];

function localized(value, field, errors, required = true) {
  if (!value || typeof value !== "object") {
    if (required) errors.push(`${field} 必须是 {zh,en} 对象`);
    return;
  }
  for (const language of ["zh", "en"]) {
    if (typeof value[language] !== "string" || !value[language].trim()) {
      if (required) errors.push(`${field}.${language} 不能为空`);
    }
  }
}

function parseFrontmatter(text) {
  const match = String(text).match(/^---\r?\n([\s\S]*?)\r?\n---/);
  if (!match) return null;
  const fields = {};
  for (const line of match[1].split(/\r?\n/)) {
    if (!line.trim() || line.trim().startsWith("#")) continue;
    const colon = line.indexOf(":");
    if (colon <= 0) continue;
    const key = line.slice(0, colon).trim();
    let value = line.slice(colon + 1).trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    fields[key] = value;
  }
  return fields;
}

function pngSize(buffer) {
  if (buffer.length < 24) return null;
  const signature = buffer.subarray(0, 8);
  const expected = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  if (!signature.equals(expected)) return null;
  if (buffer.subarray(12, 16).toString("ascii") !== "IHDR") return null;
  return {
    width: buffer.readUInt32BE(16),
    height: buffer.readUInt32BE(20),
  };
}

async function fileExists(target) {
  try {
    await stat(target);
    return true;
  } catch {
    return false;
  }
}

export async function validatePackage(root, options = {}) {
  const { checkSecrets = true } = options;
  const errors = [];
  const warnings = [];
  const manifestPath = path.join(root, ".codebuddy-plugin", "plugin.json");

  let manifest;
  try {
    manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  } catch (error) {
    return { ok: false, errors: [`无法读取 plugin.json: ${String(error)}`], warnings };
  }

  if (
    !/^[a-z0-9][a-z0-9-]*[a-z0-9]$/.test(manifest.name ?? "") ||
    (manifest.name ?? "").length < 2
  ) {
    errors.push("name 必须是小写 kebab-case，至少 2 个字符");
  }
  if (manifest.plugin !== manifest.name) {
    errors.push("plugin 必须与 name 一致");
  }
  if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(manifest.version ?? "")) {
    errors.push("version 必须是语义化版本");
  }
  if (typeof manifest.description !== "string" || !manifest.description.trim()) {
    errors.push("description 不能为空");
  }
  if (!manifest.author?.name || !manifest.author?.email) {
    errors.push("author.name / author.email 不能为空");
  }
  if (manifest.expertType !== "agent") {
    errors.push('expertType 必须是 "agent"');
  }
  if (typeof manifest.agentName !== "string" || !manifest.agentName) {
    errors.push("agentName 不能为空");
  }

  const agentRelative = `agents/${manifest.agentName}.md`;
  const agentPath = path.join(root, agentRelative);
  if (!(await fileExists(agentPath))) {
    errors.push(`缺少 Agent 文件 ${agentRelative}`);
  } else {
    const agentText = await readFile(agentPath, "utf8");
    const frontmatter = parseFrontmatter(agentText);
    if (!frontmatter) {
      errors.push("Agent 文件缺少 YAML frontmatter");
    } else {
      if (frontmatter.name !== manifest.agentName) {
        errors.push("Agent frontmatter.name 必须与 agentName/文件名一致");
      }
      if (!frontmatter.description) {
        errors.push("Agent frontmatter.description 不能为空");
      }
      if (frontmatter.tools) {
        errors.push("Agent frontmatter 不允许写 tools（工具权限由 WorkBuddy 分配）");
      }
    }
  }
  if (!Array.isArray(manifest.agents) || !manifest.agents.includes(`./${agentRelative}`)) {
    errors.push(`agents 必须包含 ./${agentRelative}`);
  }

  localized(manifest.displayName, "displayName", errors);
  localized(manifest.profession, "profession", errors);
  localized(manifest.displayDescription, "displayDescription", errors);
  localized(manifest.defaultInitPrompt, "defaultInitPrompt", errors);

  const descriptionZh = manifest.displayDescription?.zh ?? "";
  const descriptionLength = [...descriptionZh].length;
  if (descriptionLength < 40 || descriptionLength > 50) {
    errors.push(
      `displayDescription.zh 必须为 40–50 字，当前 ${descriptionLength} 字：${descriptionZh}`,
    );
  }

  if (!CATEGORY_IDS.has(manifest.categoryId)) {
    errors.push(`categoryId 不在官方行业分类中：${manifest.categoryId}`);
  }

  if (!Array.isArray(manifest.tags) || manifest.tags.length !== 3) {
    errors.push("tags 必须恰好 3 个");
  } else {
    manifest.tags.forEach((tag, index) => localized(tag, `tags[${index}]`, errors));
  }

  if (!Array.isArray(manifest.quickPrompts) || manifest.quickPrompts.length !== 3) {
    errors.push("quickPrompts 必须恰好 3 个");
  } else {
    manifest.quickPrompts.forEach((prompt, index) =>
      localized(prompt, `quickPrompts[${index}]`, errors),
    );
    const first = manifest.quickPrompts[0];
    const initial = manifest.defaultInitPrompt;
    if (first?.zh !== initial?.zh || first?.en !== initial?.en) {
      errors.push("defaultInitPrompt 必须与 quickPrompts 第一条完全一致");
    }
  }

  if (manifest.dependencies?.mcpServers !== "./.mcp.json") {
    errors.push('dependencies.mcpServers 必须指向 "./.mcp.json"');
  }

  // Avatar: relative path, PNG 512x512, <=500KB.
  const avatarPath = path.join(root, manifest.avatar ?? "");
  if (!manifest.avatar || !(await fileExists(avatarPath))) {
    errors.push(`头像不存在：${manifest.avatar}`);
  } else {
    const info = await stat(avatarPath);
    if (info.size > 500 * 1024) {
      errors.push(`头像超过 500KB（${info.size} bytes）`);
    }
    const size = pngSize(await readFile(avatarPath));
    if (!size || size.width !== 512 || size.height !== 512) {
      errors.push(
        `头像必须是 512×512 PNG，当前 ${size ? `${size.width}×${size.height}` : "无法识别"}`,
      );
    }
  }

  // .mcp.json and its display icon.
  let mcpConfig;
  try {
    mcpConfig = JSON.parse(await readFile(path.join(root, ".mcp.json"), "utf8"));
  } catch (error) {
    errors.push(`无法读取 .mcp.json: ${String(error)}`);
  }
  if (mcpConfig) {
    const servers = mcpConfig.mcpServers;
    if (!servers || typeof servers !== "object" || Array.isArray(servers)) {
      errors.push(".mcp.json 缺少 mcpServers 对象");
    } else {
      const server = servers[manifest.name];
      if (!server) {
        errors.push(`.mcp.json 缺少名为 ${manifest.name} 的服务`);
      } else {
        if (server.type !== "stdio") errors.push(".mcp.json 服务类型必须是 stdio");
        if (server.command !== "node") errors.push(".mcp.json command 必须是 node");
        if (
          !Array.isArray(server.args) ||
          !server.args.some((arg) => String(arg).includes("szu-life.mjs"))
        ) {
          errors.push(".mcp.json args 必须包含 szu-life.mjs");
        }
        const icon = server["x-workbuddy"]?.icon;
        if (!icon || !(await fileExists(path.join(root, icon)))) {
          errors.push(".mcp.json x-workbuddy.icon 不存在");
        }
        if (server["x-workbuddy"]?.auth?.type !== "none") {
          errors.push('.mcp.json x-workbuddy.auth.type 必须是 "none"');
        }
        localized(server["x-workbuddy"]?.displayName, "x-workbuddy.displayName", errors);
        localized(server["x-workbuddy"]?.description, "x-workbuddy.description", errors);
      }
    }
  }

  // Bundled skill and references.
  const skillRelative = manifest.skills?.[0];
  if (!skillRelative) {
    errors.push("skills 必须包含 ./skills/your-szu-life");
  } else {
    const skillDir = path.join(root, skillRelative);
    const skillPath = path.join(skillDir, "SKILL.md");
    if (!(await fileExists(skillPath))) {
      errors.push("缺少 skills/your-szu-life/SKILL.md");
    } else {
      const frontmatter = parseFrontmatter(await readFile(skillPath, "utf8"));
      for (const field of [
        "name",
        "display_name",
        "display_name_en",
        "description",
        "description_zh",
        "description_en",
        "version",
        "author",
      ]) {
        if (!frontmatter?.[field]) {
          errors.push(`SKILL.md frontmatter 缺少 ${field}`);
        }
      }
      if (frontmatter?.name !== "your-szu-life") {
        errors.push("SKILL.md name 必须是 your-szu-life");
      }
    }
    for (const reference of REQUIRED_REFERENCES) {
      if (!(await fileExists(path.join(skillDir, "references", reference)))) {
        errors.push(`缺少 references/${reference}`);
      }
    }
  }

  for (const required of ["README.md", "bin/szu-life.mjs", "bin/szu-life", "bin/szu-life.cmd"]) {
    if (!(await fileExists(path.join(root, required)))) {
      errors.push(`缺少 ${required}`);
    }
  }

  // The bridge version must match the manifest version.
  const constantsPath = path.join(root, "bin/lib/constants.mjs");
  if (await fileExists(constantsPath)) {
    const constants = await readFile(constantsPath, "utf8");
    const match = constants.match(/EXPERT_VERSION\s*=\s*"([^"]+)"/);
    if (match && match[1] !== manifest.version) {
      errors.push(
        `bin/lib/constants.mjs EXPERT_VERSION=${match[1]} 与 plugin.json version=${manifest.version} 不一致`,
      );
    }
  }

  if (checkSecrets) {
    const files = await walkFiles(root, ".");
    for (const relative of files) {
      if (!/\.(md|json|mjs|cjs|js|sh|cmd|txt)$/i.test(relative)) continue;
      const text = await readFile(path.join(root, relative), "utf8");
      if (/\bsk-[A-Za-z0-9]{20,}\b/.test(text)) {
        errors.push(`疑似明文 API Key：${relative}`);
      }
      if (/\bghp_[A-Za-z0-9]{30,}\b/.test(text)) {
        errors.push(`疑似明文 GitHub Token：${relative}`);
      }
      if (/\bAKIA[0-9A-Z]{16}\b/.test(text)) {
        errors.push(`疑似明文 AWS Key：${relative}`);
      }
      if (/-----BEGIN [A-Z ]*PRIVATE KEY-----/.test(text)) {
        errors.push(`疑似私钥：${relative}`);
      }
    }
  }

  return {
    ok: errors.length === 0,
    errors,
    warnings,
    manifest,
  };
}
