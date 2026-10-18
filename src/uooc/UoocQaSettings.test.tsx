/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "@heroui/react";
import { UoocQaSettings, type EmbeddingVectorStatus, type QaAccountStatus } from "./UoocQaSettings";
import { getQaAuthClient } from "./qaAuthClient";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("../app-content/external", () => ({ openPublicUrl: vi.fn() }));
vi.mock("./qaAuthClient", () => ({
  normalizeQaBaseUrl: (url?: string) => url || "http://localhost:4321",
  getQaAuthClient: vi.fn(),
}));

const initialStatus: EmbeddingVectorStatus = {
  status: "empty",
  cachedCount: 0,
  totalDimension: 1536,
  updatedAtMs: 1000,
  message: "尚未下载题库向量",
};

const downloadedStatus: EmbeddingVectorStatus = {
  status: "ready",
  cachedCount: 88,
  totalDimension: 1536,
  updatedAtMs: 2000,
  message: "本地已缓存 88 条题目向量",
};

const anonAccount: QaAccountStatus = {
  apiKey: "uooc_anon_12345",
  isAnonymous: true,
  userId: "anon_user_1",
  userName: "Anonymous",
  permissionLevel: "anonymous",
  canOverwrite: false,
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  Element.prototype.getAnimations = () => [];
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_uooc_embedding_status") return initialStatus;
    if (command === "get_uooc_qa_account_status") return anonAccount;
    if (command === "download_uooc_qa_vectors") return downloadedStatus;
    if (command === "sync_uooc_qa_bank") return 5;
    if (command === "save_uooc_qa_api_key") return undefined;
    if (command === "get_uooc_local_bank_questions") return [];
    if (command === "search_uooc_qa_questions")
      return [
        {
          questionId: "q101",
          tid: "201",
          cid: "301",
          stem: "关于定积分中值定理，以下表述正确的是？",
          questionType: "single",
          similarity: 0.965,
          answers: ["B"],
          source: "本地题库向量检索",
        },
      ];
    return undefined;
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("renders initial status and downloads uooc qa vectors", async () => {
  render(<UoocQaSettings />);
  expect(await screen.findByText("尚未下载题库向量")).toBeInTheDocument();
  expect(await screen.findByText("匿名账户（读+新增）")).toBeInTheDocument();

  const downloadBtn = screen.getByRole("button", { name: "下载 UOOC 标记题库向量" });
  fireEvent.click(downloadBtn);

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("download_uooc_qa_vectors", {
      serverUrl: "http://localhost:4321",
    }),
  );
  expect(await screen.findByText("题库向量库已就绪")).toBeInTheDocument();
  expect(screen.getAllByText(/88 条题目向量/).length).toBeGreaterThan(0);
});

it("syncs confirmed questions with backend", async () => {
  const successSpy = vi.spyOn(toast, "success");
  render(<UoocQaSettings />);
  const syncBtn = await screen.findByRole("button", { name: "同步已确认题目至服务端" });
  fireEvent.click(syncBtn);

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("sync_uooc_qa_bank", {
      serverUrl: "http://localhost:4321",
    }),
  );
  await waitFor(() =>
    expect(successSpy).toHaveBeenCalledWith("已成功与后端比对并将 5 道缺失题目同步上传"),
  );
});

it("saves custom api key", async () => {
  const successSpy = vi.spyOn(toast, "success");
  render(<UoocQaSettings />);
  const keyInput = await screen.findByLabelText("题库 API Key");
  fireEvent.change(keyInput, { target: { value: "uooc_custom_secret_key" } });

  const saveKeyBtn = screen.getByRole("button", { name: "保存 Key" });
  fireEvent.click(saveKeyBtn);

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_uooc_qa_api_key", {
      apiKey: "uooc_custom_secret_key",
    }),
  );
  await waitFor(() => expect(successSpy).toHaveBeenCalledWith("题库 API Key 保存成功"));
});

it("searches questions using local embedding calculation", async () => {
  render(<UoocQaSettings />);
  const input = await screen.findByPlaceholderText(/输入题干文本或关键词/);
  fireEvent.change(input, { target: { value: "积分中值定理" } });

  const searchBtn = screen.getByRole("button", { name: "向量搜索" });
  expect(searchBtn).toBeEnabled();
  fireEvent.click(searchBtn);

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("search_uooc_qa_questions", {
      query: "积分中值定理",
      serverUrl: "http://localhost:4321",
    }),
  );

  expect(await screen.findByText("关于定积分中值定理，以下表述正确的是？")).toBeInTheDocument();
  expect(screen.getByText("相似度 96.5%")).toBeInTheDocument();
  expect(screen.getByText("单选题")).toBeInTheDocument();
  expect(screen.getByText(/标准答案：/)).toBeInTheDocument();
});

it("initiates device authorization and polls until completion", async () => {
  let pollAttempts = 0;
  const mockCode = vi.fn().mockResolvedValue({
    data: {
      device_code: "dev_code_123",
      user_code: "EFGH-5678",
      verification_uri: "http://localhost:4321/device",
      verification_uri_complete: "http://localhost:4321/device?user_code=EFGH-5678",
      interval: 0.05,
    },
    error: null,
  });
  const mockToken = vi.fn().mockImplementation(async () => {
    pollAttempts++;
    if (pollAttempts === 1) {
      return { data: null, error: { error: "authorization_pending" } };
    }
    return { data: { access_token: "mock_tok_888" }, error: null };
  });
  const mockApiKeyCreate = vi.fn().mockResolvedValue({
    data: { key: "uooc_device_obtained_key" },
    error: null,
  });

  vi.mocked(getQaAuthClient).mockReturnValue({
    device: {
      code: mockCode,
      token: mockToken,
    },
    apiKey: {
      create: mockApiKeyCreate,
    },
  } as unknown as ReturnType<typeof getQaAuthClient>);

  const successSpy = vi.spyOn(toast, "success");
  render(<UoocQaSettings />);
  const deviceAuthBtn = await screen.findByRole("button", { name: /通过设备码登录/ });
  fireEvent.click(deviceAuthBtn);

  expect(await screen.findByText("EFGH-5678")).toBeInTheDocument();
  expect(screen.getByText("在浏览器中打开授权页")).toBeInTheDocument();

  await waitFor(
    () => {
      expect(successSpy).toHaveBeenCalledWith("设备码授权登录成功！已自动获取并绑定专属 API Key");
    },
    { timeout: 3000 },
  );
  expect(invoke).toHaveBeenCalledWith("save_uooc_qa_api_key", {
    apiKey: "uooc_device_obtained_key",
  });
});

it("performs manual full questions upload", async () => {
  const successSpy = vi.spyOn(toast, "success");
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_uooc_embedding_status") return initialStatus;
    if (command === "get_uooc_qa_account_status") return anonAccount;
    if (command === "upload_all_uooc_qa_bank") return 42;
    return undefined;
  });

  render(<UoocQaSettings />);
  const fullUploadBtn = await screen.findByRole("button", { name: "手动全题目上传" });
  fireEvent.click(fullUploadBtn);

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("upload_all_uooc_qa_bank", {
      serverUrl: "http://localhost:4321",
    }),
  );
  await waitFor(() =>
    expect(successSpy).toHaveBeenCalledWith("已成功全量上传 42 道本地题目至题库共享平台"),
  );
});

it("shows toast.danger when operation fails", async () => {
  const dangerSpy = vi.spyOn(toast, "danger");
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_uooc_embedding_status") return initialStatus;
    if (command === "get_uooc_qa_account_status") return anonAccount;
    if (command === "sync_uooc_qa_bank") throw "服务连接超时";
    return undefined;
  });

  render(<UoocQaSettings />);
  const syncBtn = await screen.findByRole("button", { name: "同步已确认题目至服务端" });
  fireEvent.click(syncBtn);

  await waitFor(() =>
    expect(dangerSpy).toHaveBeenCalledWith("同步题库失败", { description: "服务连接超时" }),
  );
});

it("browses local question bank with annotated sync status and filters", async () => {
  const mockQuestions = [
    {
      cid: "course_1",
      tid: "task_100",
      questionId: "q_2001",
      questionType: "single",
      stem: "关于定积分与微积分基本定理",
      options: [
        { id: "A", text: "连续必可积" },
        { id: "B", text: "可积必连续" },
      ],
      blanks: 0,
      correctAnswers: ["连续必可积"],
      uploaded: true,
      updatedAtMs: 1700000000000,
    },
    {
      cid: "course_2",
      tid: "task_200",
      questionId: "q_2002",
      questionType: "multiple",
      stem: "人工智能的主要研究分支包括",
      options: [
        { id: "A", text: "机器学习" },
        { id: "B", text: "计算机视觉" },
      ],
      blanks: 0,
      correctAnswers: ["机器学习", "计算机视觉"],
      uploaded: false,
      updatedAtMs: 1700001000000,
    },
  ];

  const mockCourses = [
    {
      id: "course_1",
      title: "高等数学（微积分）",
      semester: "秋季",
      organization: "深圳大学",
      weeks: "5/7",
    },
    {
      id: "course_2",
      title: "人工智能概论",
      semester: "秋季",
      organization: "深圳大学",
      weeks: "5/7",
    },
  ];

  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_uooc_embedding_status") return initialStatus;
    if (command === "get_uooc_qa_account_status") return anonAccount;
    if (command === "get_uooc_local_bank_questions") return mockQuestions;
    if (command === "build_uooc_local_bank") return 2;
    if (command === "get_uooc_course_title") {
      const cid = (args as { cid?: string })?.cid;
      if (cid === "course_1") return "高等数学（微积分）";
      if (cid === "course_2") return "人工智能概论";
      return `课程 ${cid}`;
    }
    return undefined;
  });

  render(<UoocQaSettings courses={mockCourses} />);

  // 1. Verify course-grouped entries and summary
  expect(await screen.findByText("本地题库内容浏览")).toBeInTheDocument();
  expect(screen.getByText("全部课程")).toBeInTheDocument();
  expect(await screen.findByText("高等数学（微积分）")).toBeInTheDocument();
  expect(await screen.findByText("人工智能概论")).toBeInTheDocument();

  // Summary counts
  expect(screen.getByText(/全部收录: 2 道/)).toBeInTheDocument();
  expect(screen.getByText(/已同步至服务端: 1 道/)).toBeInTheDocument();
  expect(screen.getByText(/待同步（仅本地）: 1 道/)).toBeInTheDocument();

  // 2. Click "全部课程" to open modal
  const viewAllBtn = screen.getByRole("button", { name: "浏览全部题目" });
  fireEvent.click(viewAllBtn);

  // In modal: verify questions and sync status badges
  expect(await screen.findByText("全部课程题库")).toBeInTheDocument();
  expect(screen.getByText("关于定积分与微积分基本定理")).toBeInTheDocument();
  expect(screen.getByText("人工智能的主要研究分支包括")).toBeInTheDocument();

  // Annotated sync statuses inside modal
  expect(screen.getByText("已同步至服务端")).toBeInTheDocument();
  expect(screen.getByText("待同步（仅本地）")).toBeInTheDocument();

  // Option keys in standard answers
  // The answer includes real option key "A" and text "连续必可积"
  expect(screen.getAllByText("A").length).toBeGreaterThan(0);
  expect(screen.getAllByText("连续必可积").length).toBeGreaterThan(0);

  // 题单选择器呈现
  expect(screen.getByLabelText("选择题单")).toBeInTheDocument();

  // Filter by status: click "已同步" inside modal
  const filterSyncedBtn = screen.getByRole("button", { name: /^已同步 \(1\)/ });
  fireEvent.click(filterSyncedBtn);
  expect(screen.getByText("关于定积分与微积分基本定理")).toBeInTheDocument();
  expect(screen.queryByText("人工智能的主要研究分支包括")).not.toBeInTheDocument();

  // Close modal to return to main view
  const closeBtn = screen.getByRole("button", { name: "关闭" });
  fireEvent.click(closeBtn);

  // Test rebuilding bank button
  const successSpy = vi.spyOn(toast, "success");
  const rebuildBtn = await screen.findByRole("button", { name: "重新构建题库" });
  fireEvent.click(rebuildBtn);

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("build_uooc_local_bank"));
  await waitFor(() =>
    expect(successSpy).toHaveBeenCalledWith("本地题库已重新构建完成，共收录 2 道题目"),
  );
});

it("allows clearing local question bank when debugEnabled is true", async () => {
  const successSpy = vi.spyOn(toast, "success");
  const mockQuestions = [
    {
      cid: "course_1",
      tid: "task_100",
      questionId: "q_2001",
      questionType: "single",
      stem: "关于定积分与微积分基本定理",
      options: [
        { id: "A", text: "连续必可积" },
        { id: "B", text: "可积必连续" },
      ],
      blanks: 0,
      correctAnswers: ["连续必可积"],
      uploaded: true,
      updatedAtMs: 1710000000000,
    },
  ];

  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_uooc_embedding_status") return initialStatus;
    if (command === "get_uooc_qa_account_status") return anonAccount;
    if (command === "get_uooc_local_bank_questions") return mockQuestions;
    if (command === "clear_uooc_local_bank") return 1;
    return undefined;
  });

  render(<UoocQaSettings debugEnabled={true} />);

  const clearBtn = await screen.findByRole("button", { name: "清空本地题库" });
  expect(clearBtn).toBeInTheDocument();
  fireEvent.click(clearBtn);

  expect(await screen.findByText("确认清空本地题库？")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "确认清空" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("clear_uooc_local_bank"));
  await waitFor(() => expect(successSpy).toHaveBeenCalledWith("已清空本地题库（共清除 1 道题目）"));
});
