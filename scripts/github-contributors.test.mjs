// @vitest-environment node
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import {
  downloadAvatars,
  fetchContributors,
  githubContributorsSnapshot,
  parseRepository,
  writeContributorSnapshot,
} from "./github-contributors.ts";

const repository = {
  fullName: "rinnein/your-szu-life",
  url: "https://github.com/rinnein/your-szu-life",
};

const temporaryDirs = [];

function tempDir() {
  const dir = mkdtempSync(join(tmpdir(), "szu-contributors-"));
  temporaryDirs.push(dir);
  return dir;
}

function jsonResponse(body, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function imageResponse(bytes = new Uint8Array([1, 2, 3]), type = "image/png") {
  return new Response(bytes, { status: 200, headers: { "content-type": type } });
}

function avatarUrlFor(login) {
  return `https://avatars.githubusercontent.com/u/${login}`;
}

function githubContributor(login) {
  return { login, avatar_url: avatarUrlFor(login) };
}

afterEach(() => {
  for (const dir of temporaryDirs.splice(0)) rmSync(dir, { recursive: true, force: true });
});

describe("parseRepository", () => {
  it("parses a git+https package repository URL", () => {
    expect(parseRepository({ url: "git+https://github.com/rinnein/your-szu-life.git" })).toEqual(
      repository,
    );
  });

  it("parses owner/repo shorthand and plain GitHub URLs", () => {
    expect(parseRepository("rinnein/your-szu-life")).toEqual(repository);
    expect(parseRepository("https://github.com/rinnein/your-szu-life")).toEqual(repository);
  });

  it("rejects missing and non-GitHub repositories", () => {
    expect(parseRepository(undefined)).toBeNull();
    expect(parseRepository({ url: "https://gitlab.com/rinnein/your-szu-life" })).toBeNull();
    expect(parseRepository("owner/repo/extra")).toBeNull();
  });
});

describe("fetchContributors", () => {
  it("paginates, dedupes by login and builds GitHub profile URLs", async () => {
    const fetchImpl = vi.fn(async (input) => {
      const page = new URL(String(input)).searchParams.get("page");
      if (page === "1") return jsonResponse([githubContributor("alice"), githubContributor("bob")]);
      if (page === "2") return jsonResponse([githubContributor("bob"), githubContributor("carol")]);
      return jsonResponse([]);
    });

    const result = await fetchContributors({ repository, fetchImpl, pageSize: 2 });

    expect(result.contributors).toEqual([
      { login: "alice", url: "https://github.com/alice", avatarUrl: avatarUrlFor("alice") },
      { login: "bob", url: "https://github.com/bob", avatarUrl: avatarUrlFor("bob") },
      { login: "carol", url: "https://github.com/carol", avatarUrl: avatarUrlFor("carol") },
    ]);
    expect(result.truncated).toBe(false);
    expect(fetchImpl).toHaveBeenCalledTimes(3);
  });

  it("marks the list truncated when the page limit is filled", async () => {
    const fetchImpl = vi.fn(async () =>
      jsonResponse([githubContributor("alice"), githubContributor("bob")]),
    );

    const result = await fetchContributors({ repository, fetchImpl, pageSize: 2, maxPages: 2 });

    expect(result.contributors).toHaveLength(2);
    expect(result.truncated).toBe(true);
    expect(fetchImpl).toHaveBeenCalledTimes(2);
  });

  it("sends authenticated GitHub API headers", async () => {
    const fetchImpl = vi.fn(async () => jsonResponse([githubContributor("alice")]));

    await fetchContributors({ repository, fetchImpl, token: "test-token" });

    const init = fetchImpl.mock.calls[0]?.[1];
    expect(init.headers).toMatchObject({
      Accept: "application/vnd.github+json",
      "X-GitHub-Api-Version": "2022-11-28",
      Authorization: "Bearer test-token",
    });
  });

  it("throws with the HTTP status on API errors", async () => {
    const fetchImpl = vi.fn(async () => jsonResponse({ message: "rate limited" }, 403));

    await expect(fetchContributors({ repository, fetchImpl })).rejects.toThrow("403");
  });
});

describe("downloadAvatars", () => {
  it("requests 96px avatars and skips unsafe logins or failed downloads", async () => {
    const fetchImpl = vi.fn(async (input) => {
      if (String(input).includes("skip-me")) return new Response("error", { status: 500 });
      return imageResponse();
    });

    const avatars = await downloadAvatars(
      [
        { login: "alice", url: "https://github.com/alice", avatarUrl: avatarUrlFor("alice") },
        { login: "bad/login", url: "https://github.com/bad/login", avatarUrl: avatarUrlFor("bad") },
        { login: "skip-me", url: "https://github.com/skip-me", avatarUrl: avatarUrlFor("skip-me") },
      ],
      { fetchImpl },
    );

    expect(avatars.get("alice")).toMatchObject({ extension: "png" });
    expect(avatars.has("bad/login")).toBe(false);
    expect(avatars.has("skip-me")).toBe(false);
    expect(fetchImpl).toHaveBeenCalledTimes(2);

    const requested = new URL(String(fetchImpl.mock.calls[0]?.[0]));
    expect(requested.searchParams.get("s")).toBe("96");
  });

  it("rejects non-image responses and oversized files", async () => {
    const avatars = await downloadAvatars(
      [
        { login: "html", url: "https://github.com/html", avatarUrl: avatarUrlFor("html") },
        { login: "huge", url: "https://github.com/huge", avatarUrl: avatarUrlFor("huge") },
      ],
      {
        fetchImpl: async (input) =>
          String(input).includes("/u/html")
            ? new Response("<html>", { headers: { "content-type": "text/html" } })
            : imageResponse(new Uint8Array(32)),
        maxBytes: 16,
      },
    );

    expect(avatars.size).toBe(0);
  });
});

describe("writeContributorSnapshot", () => {
  it("replaces stale avatar files and writes the camelCase snapshot", async () => {
    const dir = tempDir();
    mkdirSync(join(dir, "avatars"), { recursive: true });
    writeFileSync(join(dir, "avatars", "stale.png"), "old");

    await writeContributorSnapshot(
      dir,
      {
        repository,
        contributors: [{ login: "alice", url: "https://github.com/alice" }],
        truncated: false,
      },
      new Map([["alice", { bytes: new Uint8Array([9, 8, 7]), extension: "png" }]]),
    );

    expect(JSON.parse(readFileSync(join(dir, "contributors.json"), "utf8"))).toEqual({
      repository,
      contributors: [{ login: "alice", url: "https://github.com/alice" }],
      truncated: false,
    });
    expect(readdirSync(join(dir, "avatars"))).toEqual(["alice.png"]);
  });
});

describe("githubContributorsSnapshot", () => {
  it("runs only during builds and writes list plus avatars", async () => {
    const dir = tempDir();
    const fetchImpl = vi.fn(async (input) =>
      String(input).includes("api.github.com")
        ? jsonResponse([githubContributor("alice")])
        : imageResponse(),
    );
    const plugin = githubContributorsSnapshot({ dir, repository, fetchImpl });
    const warn = vi.fn();

    expect(plugin.apply).toBe("build");
    await plugin.buildStart.call({ warn }, {});

    expect(warn).not.toHaveBeenCalled();
    expect(JSON.parse(readFileSync(join(dir, "contributors.json"), "utf8"))).toEqual({
      repository,
      contributors: [{ login: "alice", url: "https://github.com/alice" }],
      truncated: false,
    });
    expect(readdirSync(join(dir, "avatars"))).toEqual(["alice.png"]);
  });

  it("clears generated data and warns instead of failing the build", async () => {
    const dir = tempDir();
    mkdirSync(join(dir, "avatars"), { recursive: true });
    writeFileSync(join(dir, "contributors.json"), "{}");

    const plugin = githubContributorsSnapshot({
      dir,
      repository,
      fetchImpl: async () => jsonResponse({ message: "rate limited" }, 403),
    });
    const warn = vi.fn();

    await plugin.buildStart.call({ warn }, {});

    expect(warn).toHaveBeenCalledWith(expect.stringContaining("回退到运行时联网获取"));
    expect(existsSync(dir)).toBe(false);
  });
});
