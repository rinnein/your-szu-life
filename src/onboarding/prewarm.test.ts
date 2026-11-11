import { beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { prewarmSsoModules } from "./prewarm";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("prewarmSsoModules", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("starts both module bootstraps without waiting for either", () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    prewarmSsoModules();

    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke).toHaveBeenCalledWith("get_lms_snapshot");
    expect(invoke).toHaveBeenCalledWith("get_jxxk_snapshot");
  });

  it("swallows failures so a failed prewarm never surfaces in the wizard", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("auth missing"));

    prewarmSsoModules();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(invoke).toHaveBeenCalledTimes(2);
  });
});
