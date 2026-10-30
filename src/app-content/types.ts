export type ContentError = {
  kind: "network" | "http" | "invalidData" | "tooLarge" | "rateLimited" | "unavailable";
  message: string;
  retryAt: number | null;
};

export type Announcement = {
  title: string;
  pic?: string | null;
  desc?: string;
  goto?: string;
};

export type UpdateInfo = {
  latest_ver: string;
  latest_ver_url: string;
  changelog: string;
  release_url?: string;
};

export type FeedSnapshot<T> = {
  data: T | null;
  lastAttemptAt: number | null;
  lastSuccessAt: number | null;
  source: "primary" | "fallback" | "github" | "cnb" | null;
  fromCache: boolean;
  error: ContentError | null;
};

export type Repository = { fullName: string; url: string };

export type AppContentSnapshot = {
  revision: number;
  version: string;
  configuration: {
    status: "ready" | "unconfigured" | "invalid";
    message: string | null;
  };
  repository: Repository | null;
  repositoryMessage: string | null;
  refreshing: boolean;
  nextAutomaticCheckAt: number | null;
  announcements: FeedSnapshot<{ announcements: Announcement[] }>;
  update: FeedSnapshot<UpdateInfo>;
  updateAvailable: boolean;
};

export type Contributor = { login: string; avatarUrl: string; url: string };

export type ContributorsSnapshot = {
  status: "unconfigured" | "ready" | "error" | "rateLimited";
  repository: Repository | null;
  contributors: Contributor[];
  truncated: boolean;
  fromCache: boolean;
  lastSuccessAt: number | null;
  error: ContentError | null;
  message: string | null;
};
