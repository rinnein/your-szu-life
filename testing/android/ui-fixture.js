(() => {
  const params = new URLSearchParams(location.search),
    scenario = params.get("scenario") || "home";
  const profile = {
    studentId: "2026000000",
    campusCardNumber: "123456",
    name: "界面测试账户",
    hasPassword: true,
  };
  let auth =
    scenario === "auth"
      ? "unauthenticated"
      : { authenticated: { studentId: profile.studentId, name: profile.name } };
  const stepIds = ["auth", "storage", "dormitory", "appearance", "local_model", "ai", "mcp"];
  const isStep = stepIds.includes(scenario);
  const steps = stepIds.map((id, i) => ({
    id,
    status: isStep && i >= stepIds.indexOf(scenario) ? "pending" : "configured",
    skippable: ["dormitory", "appearance", "local_model", "ai", "mcp"].includes(id),
    revision: 1,
  }));
  const campuses = [
    {
      id: "yuehai",
      name: "粤海校区",
      areas: [
        {
          id: "south",
          name: "南区宿舍",
          campusId: "yuehai",
          provider: "test",
          buildings: Array.from({ length: 24 }, (_, i) => ({
            id: String(i + 1),
            name: "测试宿舍楼 " + (i + 1) + " 栋（1–12层）",
          })),
        },
        {
          id: "north",
          name: "北区宿舍",
          campusId: "yuehai",
          provider: "test",
          buildings: [{ id: "29", name: "测试宿舍楼29栋（1–12层）" }],
        },
      ],
    },
    {
      id: "lishan",
      name: "丽山校区",
      areas: [
        {
          id: "west",
          name: "西区宿舍",
          campusId: "lishan",
          provider: "test",
          buildings: [{ id: "30", name: "测试宿舍楼30栋（1–12层）" }],
        },
      ],
    },
  ];
  const callbacks = new Map(),
    listeners = new Map();
  let id = 1;
  const errors = [];
  window.addEventListener("error", (e) => errors.push(e.message));
  window.addEventListener("unhandledrejection", (e) => errors.push(String(e.reason)));
  const emit = (name, payload) => {
    for (const [key, e] of listeners)
      if (e.name === name) callbacks.get(e.handler)?.({ event: name, id: key, payload });
  };
  window.__TAURI_OS_PLUGIN_INTERNALS__ = {
    platform: "android",
    os_type: "android",
    version: "11",
    arch: "aarch64",
    family: "unix",
    eol: "\n",
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: (_n, k) => listeners.delete(k) };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    transformCallback(fn, once = false) {
      const k = id++;
      callbacks.set(
        k,
        once
          ? (v) => {
              callbacks.delete(k);
              return fn(v);
            }
          : fn,
      );
      return k;
    },
    unregisterCallback(k) {
      callbacks.delete(k);
    },
    runCallback(k, v) {
      callbacks.get(k)?.(v);
    },
    convertFileSrc(p) {
      return p;
    },
    async invoke(cmd, args = {}) {
      if (cmd === "plugin:event|listen") {
        const k = id++;
        listeners.set(k, { name: args.event, handler: args.handler });
        return k;
      }
      if (cmd === "plugin:event|unlisten") {
        listeners.delete(args.eventId);
        return;
      }
      if (cmd === "plugin:event|emit") {
        emit(args.event, args.payload);
        return;
      }
      if (cmd === "get_auth_state") return auth;
      if (cmd === "get_account_profile") return scenario === "auth" ? null : profile;
      if (cmd === "get_user_data")
        return scenario === "auth" ? null : { studentId: profile.studentId, name: profile.name };
      if (cmd === "get_onboarding_state")
        return { steps, needsSetup: isStep, currentStep: isStep ? scenario : null };
      if (cmd === "complete_onboarding_step" || cmd === "skip_onboarding_step") {
        steps.find((s) => s.id === args.id).status = "configured";
        return {
          steps,
          needsSetup: steps.some((s) => s.status === "pending"),
          currentStep: steps.find((s) => s.status === "pending")?.id ?? null,
        };
      }
      if (cmd === "get_electricity_options") return campuses;
      if (cmd === "get_electricity_settings")
        return {
          campusId: "yuehai",
          areaId: "south",
          buildingId: "1",
          buildingName: "测试宿舍楼 1 栋（1–12层）",
          roomName: "601",
        };
      if (cmd === "get_appearance")
        return {
          mode: params.get("dark") ? "dark" : "light",
          seed: "#5d5fef",
          variant: "tonal-spot",
          seedSource: "default",
          autoWallpaper: false,
          wallpaperPollMinutes: 15,
        };
      if (cmd === "get_wallpaper_seed") return { seed: null };
      if (cmd === "get_app_content_snapshot")
        return {
          revision: 1,
          version: "0.13.6",
          configuration: { status: "ready", message: null },
          repository: null,
          repositoryMessage: null,
          refreshing: false,
          nextAutomaticCheckAt: null,
          announcements: {
            data: {
              announcements: [
                {
                  id: "fixture",
                  title: "界面测试公告：用于检查长标题在窄屏下的排版",
                  mode: "silent",
                  desc: "这是一条测试公告。",
                },
              ],
            },
            error: null,
          },
          update: { data: null, error: null },
          updateAvailable: false,
        };
      if (cmd === "subscribe_todos") {
        args.channel.onmessage?.({
          type: "snapshot",
          data: [
            {
              id: "test-1",
              kind: "oj-quiz",
              title: "这是一个很长的测试待办标题，用于检查窄屏排版和截断",
              subtitle: "界面回归测试",
              dueAtMs: Date.now() + 86400000,
              route: "/study/oj",
              urgent: false,
            },
          ],
        });
        return;
      }
      if (cmd.startsWith("subscribe_")) throw Error("No external subscriptions in UI fixture");
      if (cmd === "get_oj_snapshot")
        return { desktop: false, sites: [], companionEnabled: false, companionPort: 27121 };
      if (cmd === "get_file_manager_state")
        return {
          platform: "android",
          defaultBase: "/test",
          config: {
            selection: { kind: "androidTree", uri: "content://test/tree/fixture" },
            relativeBase: "",
            displayRoot: "界面测试目录",
            selected: "界面测试目录",
            viaDefault: false,
            chosenAt: 0,
          },
          base: "界面测试目录",
          usingDefault: false,
          access: { state: "granted", detail: null },
          modules: [{ id: "recordings", label: "录像", path: "界面测试目录/录像" }],
          canUseDefault: false,
        };
      if (cmd === "get_mcp_server_status")
        return { running: false, enabled: false, port: null, preferredPort: 53901, endpoint: null };
      if (cmd === "get_notification_settings") return { enabled: true, electricity: true };
      if (cmd.endsWith("_model_status"))
        return { status: "missing", revision: "test", sha256: "", bytes: 0 };
      if (cmd === "get_uooc_ai_connections")
        return {
          main: { baseUrl: "", hasKey: false },
          jev: { baseUrl: "https://api.typesafe.ai/v1", hasKey: false },
        };
      if (cmd === "get_ai_config")
        return { provider: "openai", baseUrl: "", apiKey: "", model: "", enabled: false };
      if (cmd.startsWith("consume_") || cmd === "plugin:deep-link|get_current") return null;
      if (cmd.includes("debug") || cmd.startsWith("is_")) return false;
      return null;
    },
  };
  window.__szuUiFixture = { scenario, errors };
})();
