// @vitest-environment node
import { describe, expect, it } from "vite-plus/test";
import { mapStaticContributors } from "./contributors-static";
import type { StaticContributors } from "./contributors-static";

const repository = { fullName: "owner/repo", url: "https://github.com/owner/repo" };

describe("mapStaticContributors", () => {
  it("maps unhashed glob keys to the hashed asset URLs by login", () => {
    const result: StaticContributors | null = mapStaticContributors(
      {
        repository,
        contributors: [
          { login: "alice", url: "https://github.com/alice" },
          { login: "bob", url: "https://github.com/bob" },
        ],
        truncated: false,
      },
      {
        "../generated/contributors/avatars/alice.png": "/assets/alice-abc123.png",
      },
    );

    expect(result).toEqual({
      repository,
      truncated: false,
      contributors: [
        { login: "alice", url: "https://github.com/alice", avatarUrl: "/assets/alice-abc123.png" },
        { login: "bob", url: "https://github.com/bob", avatarUrl: "" },
      ],
    });
  });

  it("rejects malformed snapshots", () => {
    expect(mapStaticContributors({ contributors: "not-an-array" } as never, {})).toBeNull();
  });
});
