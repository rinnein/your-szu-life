import type { Contributor, Repository } from "./types";

export type StaticContributors = {
  repository: Repository | null;
  contributors: Contributor[];
  truncated: boolean;
};

type GeneratedSnapshot = {
  repository?: Repository | null;
  contributors?: Array<{ login?: unknown; url?: unknown }>;
  truncated?: boolean;
};

export function mapStaticContributors(
  snapshot: GeneratedSnapshot,
  avatars: Record<string, string>,
): StaticContributors | null {
  if (!Array.isArray(snapshot.contributors)) return null;
  const avatarByLogin = Object.fromEntries(
    Object.entries(avatars).map(([path, url]) => [loginFromAvatarPath(path), url]),
  );
  return {
    repository: snapshot.repository ?? null,
    truncated: snapshot.truncated === true,
    contributors: snapshot.contributors.flatMap((entry) => {
      const login = entry?.login;
      const url = entry?.url;
      if (typeof login !== "string" || typeof url !== "string") return [];
      return [{ login, url, avatarUrl: avatarByLogin[login] ?? "" }];
    }),
  };
}

function loginFromAvatarPath(path: string) {
  const file = path.slice(path.lastIndexOf("/") + 1);
  const dot = file.lastIndexOf(".");
  return dot === -1 ? file : file.slice(0, dot);
}

// 构建期生成的目录可能不存在（开发模式或抓取失败），因此只能用 glob：
// 静态 JSON import 会让干净的 `tsc` 在生成文件缺失时直接报错。
const snapshots = import.meta.glob("../generated/contributors/contributors.json", {
  eager: true,
  import: "default",
}) as Record<string, GeneratedSnapshot>;

const avatarUrls = import.meta.glob("../generated/contributors/avatars/*", {
  eager: true,
  query: "?url",
  import: "default",
}) as Record<string, string>;

const generated = Object.values(snapshots)[0];

export const staticContributors = generated ? mapStaticContributors(generated, avatarUrls) : null;
