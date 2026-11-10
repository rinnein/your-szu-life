import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import type { Plugin } from "vite";

export type Repository = { fullName: string; url: string };

export type FetchedContributor = {
  login: string;
  url: string;
  avatarUrl: string;
};

export type ContributorSnapshot = {
  repository: Repository;
  contributors: Array<{ login: string; url: string }>;
  truncated: boolean;
};

export type AvatarFile = { bytes: Uint8Array; extension: string };

export type FetchLike = (input: string | URL | Request, init?: RequestInit) => Promise<Response>;

const GITHUB_HOST = "github.com";
const DEFAULT_PAGE_SIZE = 100;
const DEFAULT_MAX_PAGES = 10;
const DEFAULT_AVATAR_SIZE = 96;
const DEFAULT_MAX_AVATAR_BYTES = 512 * 1024;
const DEFAULT_AVATAR_CONCURRENCY = 8;
const REQUEST_TIMEOUT_MS = 20_000;
// GitHub 登录名只会是小写字母、数字和连字符；其它值不做头像落盘，避免非法文件路径。
const LOGIN_PATTERN = /^[A-Za-z0-9-]+$/;
const IMAGE_EXTENSIONS: Record<string, string> = {
  "image/png": "png",
  "image/jpeg": "jpg",
  "image/webp": "webp",
  "image/gif": "gif",
  "image/svg+xml": "svg",
};

function messageOf(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function parseRepository(value: unknown): Repository | null {
  let raw: string | undefined;
  if (typeof value === "string") {
    raw = value;
  } else if (
    value !== null &&
    typeof value === "object" &&
    typeof (value as { url?: unknown }).url === "string"
  ) {
    raw = (value as { url: string }).url;
  }
  if (!raw) return null;

  const normalized = raw
    .trim()
    .replace(/^git\+/, "")
    .replace(/\.git$/, "")
    .replace(/\/+$/, "");
  const shorthand = /^([A-Za-z0-9-]+)\/([A-Za-z0-9._-]+)$/.exec(normalized);
  if (shorthand) {
    return {
      fullName: `${shorthand[1]}/${shorthand[2]}`,
      url: `https://${GITHUB_HOST}/${shorthand[1]}/${shorthand[2]}`,
    };
  }

  let url: URL;
  try {
    url = new URL(normalized);
  } catch {
    return null;
  }
  if (url.hostname !== GITHUB_HOST || url.search || url.hash) return null;
  const parts = url.pathname.split("/").filter(Boolean);
  if (parts.length !== 2) return null;
  const [owner, repo] = parts;
  if (!/^[A-Za-z0-9-]+$/.test(owner) || !/^[A-Za-z0-9._-]+$/.test(repo)) return null;
  if (repo === "." || repo === "..") return null;
  return { fullName: `${owner}/${repo}`, url: `https://${GITHUB_HOST}/${owner}/${repo}` };
}

function githubHeaders(token?: string, accept = "application/vnd.github+json") {
  const headers: Record<string, string> = {
    Accept: accept,
    "X-GitHub-Api-Version": "2022-11-28",
    "User-Agent": "your-szu-life-build",
  };
  if (token) headers.Authorization = `Bearer ${token}`;
  return headers;
}

export async function fetchContributors(options: {
  repository: Repository;
  fetchImpl?: FetchLike;
  token?: string;
  pageSize?: number;
  maxPages?: number;
}): Promise<{ contributors: FetchedContributor[]; truncated: boolean }> {
  const fetchImpl = options.fetchImpl ?? fetch;
  const pageSize = options.pageSize ?? DEFAULT_PAGE_SIZE;
  const maxPages = options.maxPages ?? DEFAULT_MAX_PAGES;
  const contributors: FetchedContributor[] = [];
  const seen = new Set<string>();
  let truncated = false;

  for (let page = 1; page <= maxPages; page++) {
    const url = `https://api.github.com/repos/${options.repository.fullName}/contributors?per_page=${pageSize}&page=${page}`;
    const response = await fetchImpl(url, {
      headers: githubHeaders(options.token),
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    });
    if (response.status === 204) break;
    if (!response.ok) throw new Error(`GitHub 贡献者接口返回 ${response.status}`);
    const entries: unknown = await response.json();
    if (!Array.isArray(entries)) throw new Error("GitHub 贡献者响应无效");
    if (entries.length > pageSize) throw new Error("GitHub 单页贡献者数量超限");

    for (const entry of entries) {
      const login = (entry as { login?: unknown })?.login;
      const avatarUrl = (entry as { avatar_url?: unknown })?.avatar_url;
      if (typeof login !== "string" || !login || login.length > 100) continue;
      if (typeof avatarUrl !== "string" || !avatarUrl) continue;
      if (seen.has(login)) continue;
      seen.add(login);
      contributors.push({ login, url: `https://${GITHUB_HOST}/${login}`, avatarUrl });
    }

    if (entries.length < pageSize) break;
    if (page === maxPages) {
      truncated = true;
      break;
    }
  }

  return { contributors, truncated };
}

export async function downloadAvatars(
  contributors: FetchedContributor[],
  options: {
    fetchImpl?: FetchLike;
    size?: number;
    maxBytes?: number;
    concurrency?: number;
  } = {},
): Promise<Map<string, AvatarFile>> {
  const fetchImpl = options.fetchImpl ?? fetch;
  const size = options.size ?? DEFAULT_AVATAR_SIZE;
  const maxBytes = options.maxBytes ?? DEFAULT_MAX_AVATAR_BYTES;
  const concurrency = options.concurrency ?? DEFAULT_AVATAR_CONCURRENCY;
  const candidates = contributors.filter((contributor) => LOGIN_PATTERN.test(contributor.login));
  const avatars = new Map<string, AvatarFile>();
  let cursor = 0;

  const worker = async () => {
    while (cursor < candidates.length) {
      const contributor = candidates[cursor++];
      try {
        const url = new URL(contributor.avatarUrl);
        url.searchParams.set("s", String(size));
        const response = await fetchImpl(url, {
          signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
        });
        if (!response.ok) continue;
        const type = (response.headers.get("content-type") ?? "")
          .split(";")[0]
          ?.trim()
          .toLowerCase();
        const extension = IMAGE_EXTENSIONS[type];
        if (!extension) continue;
        const bytes = new Uint8Array(await response.arrayBuffer());
        if (bytes.byteLength > maxBytes) continue;
        avatars.set(contributor.login, { bytes, extension });
      } catch {
        // 单个头像失败只影响该行显示，整份列表仍然可用。
      }
    }
  };

  await Promise.all(Array.from({ length: Math.min(concurrency, candidates.length) }, worker));
  return avatars;
}

export async function writeContributorSnapshot(
  dir: string,
  snapshot: ContributorSnapshot,
  avatars: Map<string, AvatarFile>,
) {
  await rm(dir, { recursive: true, force: true });
  await mkdir(join(dir, "avatars"), { recursive: true });
  await writeFile(join(dir, "contributors.json"), JSON.stringify(snapshot, null, 2) + "\n");
  for (const [login, avatar] of avatars) {
    if (!LOGIN_PATTERN.test(login)) continue;
    await writeFile(join(dir, "avatars", `${login}.${avatar.extension}`), avatar.bytes);
  }
}

async function readPackageRepository(): Promise<Repository | null> {
  try {
    const pkg = JSON.parse(await readFile(resolve("package.json"), "utf8")) as {
      repository?: unknown;
    };
    return parseRepository(pkg.repository);
  } catch {
    return null;
  }
}

export function githubContributorsSnapshot(
  options: {
    dir?: string;
    repository?: Repository | null;
    fetchImpl?: FetchLike;
    token?: string;
  } = {},
): Plugin {
  const dir = options.dir ?? resolve("src/generated/contributors");
  return {
    name: "szu-github-contributors",
    apply: "build",
    async buildStart() {
      try {
        const repository =
          options.repository !== undefined ? options.repository : await readPackageRepository();
        if (!repository) throw new Error("package.json 未配置可用的 GitHub repository");
        const token = options.token ?? process.env.GITHUB_TOKEN ?? process.env.GH_TOKEN;
        const { contributors, truncated } = await fetchContributors({
          repository,
          fetchImpl: options.fetchImpl,
          token,
        });
        const avatars = await downloadAvatars(contributors, { fetchImpl: options.fetchImpl });
        await writeContributorSnapshot(
          dir,
          {
            repository,
            contributors: contributors.map(({ login, url }) => ({ login, url })),
            truncated,
          },
          avatars,
        );
        if (avatars.size < contributors.length) {
          this.warn(
            `贡献者头像下载不完整（${avatars.size}/${contributors.length}），缺失的头像将显示首字母。`,
          );
        }
      } catch (error) {
        await rm(dir, { recursive: true, force: true }).catch(() => {});
        this.warn(`构建期贡献者快照生成失败，将回退到运行时联网获取：${messageOf(error)}`);
      }
    },
  };
}
